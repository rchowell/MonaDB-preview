mod config;
mod db;
mod error;
mod k8s;
mod models;
mod routes;
mod services;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::routing::{get, post};
use axum::Router;
use sqlx::PgPool;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

use crate::config::Config;
use crate::k8s::K8sProvisioner;

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub pool: PgPool,
    pub provisioner: Arc<K8sProvisioner>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let config = Config::from_env();
    let pool = db::connect(&config).await?;
    let provisioner = Arc::new(K8sProvisioner::new(config.clone()).await);
    let state = AppState {
        config: config.clone(),
        pool: pool.clone(),
        provisioner: provisioner.clone(),
    };

    tokio::spawn(sleep_loop(state.clone()));

    let app = Router::new()
        .route("/healthz", get(routes::healthz))
        .route("/databases", get(routes::list_databases).post(routes::create_database))
        .route("/databases/{id}", get(routes::get_database))
        .route("/internal/routing/{hostname}", get(routes::routing))
        .route("/internal/activity/{id}", post(routes::activity))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = config.listen_addr.parse()?;
    info!("mona-api listening on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn sleep_loop(state: AppState) {
    let interval = Duration::from_secs(state.config.sleep_poll_seconds);
    loop {
        tokio::time::sleep(interval).await;
        match services::sleep_idle_databases(&state.pool, &state.config, &state.provisioner).await
        {
            Ok(0) => {}
            Ok(n) => info!("scaled {n} idle database(s) to zero"),
            Err(err) => error!(error = %err, "idle sleeper failed"),
        }
    }
}

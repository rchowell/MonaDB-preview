mod config;
mod lease;
mod preamble;
mod routes;
mod tenant;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::routing::get;
use axum::Router;
use monadb::server::connection::handle_connection_with_buf;
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use tracing::{debug, error, info, warn};
use tracing_subscriber::EnvFilter;

use crate::config::Config;
use crate::preamble::read_preamble;
use crate::tenant::TenantRegistry;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let config = Config::from_env();
    std::fs::create_dir_all(&config.data_dir)?;

    let tenants = Arc::new(TenantRegistry::new(
        config.data_dir.clone(),
        config.lru_max_tenants,
    ));

    let gateway_addr: SocketAddr = config.gateway_listen.parse()?;
    let health_addr: SocketAddr = config.health_listen.parse()?;

    let app = Router::new()
        .route("/healthz", get(routes::healthz))
        .route("/internal/handles", get(routes::list_handles))
        .layer(TraceLayer::new_for_http())
        .with_state(tenants.clone());

    info!(
        gateway = %gateway_addr,
        health = %health_addr,
        data_dir = %config.data_dir.display(),
        lru_max_tenants = config.lru_max_tenants,
        "mona-gateway starting"
    );

    let mongo = {
        let tenants = tenants.clone();
        tokio::spawn(async move {
            if let Err(err) = serve_mongo(gateway_addr, tenants).await {
                error!(error = %err, "mongo accept loop exited");
            }
        })
    };

    let health_listener = TcpListener::bind(health_addr).await?;
    let health_server = axum::serve(health_listener, app);

    tokio::select! {
        result = health_server => {
            result?;
        }
        _ = mongo => {
            return Err("mongo accept loop ended unexpectedly".into());
        }
    }

    Ok(())
}

async fn serve_mongo(
    addr: SocketAddr,
    tenants: Arc<TenantRegistry>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let listener = TcpListener::bind(addr).await?;
    info!(%addr, "gateway mongo listening");

    loop {
        let (stream, peer) = listener.accept().await?;
        let tenants = tenants.clone();
        tokio::spawn(async move {
            if let Err(err) = handle_client(stream, peer, tenants).await {
                warn!(error = %err, %peer, "gateway connection error");
            }
        });
    }
}

async fn handle_client(
    mut stream: tokio::net::TcpStream,
    peer: SocketAddr,
    tenants: Arc<TenantRegistry>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (db_id, leftover) = match read_preamble(&mut stream).await {
        Ok(v) => v,
        Err(err) => {
            warn!(error = %err, %peer, "preamble failed");
            return Ok(());
        }
    };

    let guard = tenants.acquire(&db_id).await?;
    let state = guard.state();
    debug!(db_id = %db_id, %peer, "gateway connection routed");
    // Keep guard alive for the connection lifetime.
    let result = handle_connection_with_buf(stream, state, leftover).await;
    drop(guard);
    result.map_err(|err| err.into())
}

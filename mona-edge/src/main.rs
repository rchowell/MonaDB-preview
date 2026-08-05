mod config;
mod control_plane;
mod proxy;

use std::net::SocketAddr;

use axum::routing::get;
use axum::Router;
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::EnvFilter;

use crate::config::Config;
use crate::control_plane::ControlPlane;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("failed to install rustls crypto provider");

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let config = Config::from_env();
    let tls_config = proxy::load_tls_config(&config.tls_cert, &config.tls_key)?;
    let control_plane = ControlPlane::new(config.control_plane_url.clone());

    let edge_addr: SocketAddr = config.edge_listen.parse()?;
    let health_addr: SocketAddr = config.health_listen.parse()?;

    let health = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .layer(TraceLayer::new_for_http());

    info!(
        edge = %edge_addr,
        health = %health_addr,
        control_plane = %config.control_plane_url,
        "mona-edge starting"
    );

    let proxy = tokio::spawn(async move {
        if let Err(err) = proxy::serve(edge_addr, tls_config, control_plane).await {
            tracing::error!(error = %err, "edge proxy exited");
        }
    });

    let health_listener = tokio::net::TcpListener::bind(health_addr).await?;
    let health_server = axum::serve(health_listener, health);

    tokio::select! {
        result = health_server => {
            result?;
        }
        _ = proxy => {
            return Err("edge proxy task ended unexpectedly".into());
        }
    }

    Ok(())
}

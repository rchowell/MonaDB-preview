use std::fs::File;
use std::io::BufReader;
use std::net::SocketAddr;
use std::sync::Arc;

use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::ServerConfig;
use thiserror::Error;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::LazyConfigAcceptor;
use tracing::{error, info, warn};

use crate::control_plane::ControlPlane;

#[derive(Debug, Error)]
pub enum TlsConfigError {
    #[error("failed to read TLS file {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("no certificates found in {0}")]
    NoCerts(String),
    #[error("no private key found in {0}")]
    NoKey(String),
    #[error("failed to build TLS config: {0}")]
    Build(#[from] rustls::Error),
}

pub fn load_tls_config(cert_path: &str, key_path: &str) -> Result<Arc<ServerConfig>, TlsConfigError> {
    let certs = load_certs(cert_path)?;
    let key = load_key(key_path)?;

    let mut config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)?;
    config.alpn_protocols.clear();

    Ok(Arc::new(config))
}

fn load_certs(path: &str) -> Result<Vec<CertificateDer<'static>>, TlsConfigError> {
    let file = File::open(path).map_err(|source| TlsConfigError::Io {
        path: path.to_string(),
        source,
    })?;
    let mut reader = BufReader::new(file);
    let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| TlsConfigError::Io {
            path: path.to_string(),
            source,
        })?;
    if certs.is_empty() {
        return Err(TlsConfigError::NoCerts(path.to_string()));
    }
    Ok(certs)
}

fn load_key(path: &str) -> Result<PrivateKeyDer<'static>, TlsConfigError> {
    let file = File::open(path).map_err(|source| TlsConfigError::Io {
        path: path.to_string(),
        source,
    })?;
    let mut reader = BufReader::new(file);
    rustls_pemfile::private_key(&mut reader)
        .map_err(|source| TlsConfigError::Io {
            path: path.to_string(),
            source,
        })?
        .ok_or_else(|| TlsConfigError::NoKey(path.to_string()))
}

pub async fn serve(
    listen_addr: SocketAddr,
    tls_config: Arc<ServerConfig>,
    control_plane: ControlPlane,
) -> Result<(), std::io::Error> {
    let listener = TcpListener::bind(listen_addr).await?;
    info!(%listen_addr, "edge proxy listening");

    loop {
        let (stream, peer) = listener.accept().await?;
        let tls_config = tls_config.clone();
        let control_plane = control_plane.clone();
        tokio::spawn(async move {
            if let Err(err) = handle_client(stream, peer, tls_config, control_plane).await {
                warn!(error = %err, %peer, "connection closed with error");
            }
        });
    }
}

async fn handle_client(
    stream: TcpStream,
    peer: SocketAddr,
    tls_config: Arc<ServerConfig>,
    control_plane: ControlPlane,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let acceptor = LazyConfigAcceptor::new(rustls::server::Acceptor::default(), stream);
    let start = match tokio::time::timeout(std::time::Duration::from_secs(30), acceptor).await {
        Ok(Ok(start)) => start,
        Ok(Err(err)) => {
            error!(error = %err, %peer, "TLS accept failed");
            return Err(err.into());
        }
        Err(_) => {
            error!(%peer, "TLS handshake timed out");
            return Err("TLS handshake timed out".into());
        }
    };

    let hostname = start
        .client_hello()
        .server_name()
        .map(|name| name.to_string());

    let mut client = match tokio::time::timeout(
        std::time::Duration::from_secs(30),
        start.into_stream(tls_config),
    )
    .await
    {
        Ok(Ok(stream)) => stream,
        Ok(Err(err)) => {
            error!(error = %err, %peer, "TLS handshake failed");
            return Err(err.into());
        }
        Err(_) => {
            error!(%peer, "TLS handshake timed out");
            return Err("TLS handshake timed out".into());
        }
    };

    let Some(hostname) = hostname else {
        warn!(%peer, "missing SNI");
        let _ = client.shutdown().await;
        return Ok(());
    };

    let routing = match control_plane.resolve_backend(&hostname).await {
        Ok(routing) => routing,
        Err(err) => {
            error!(error = %err, %hostname, "routing failed");
            let _ = client.shutdown().await;
            return Ok(());
        }
    };

    info!(
        %hostname,
        backend = %format!("{}:{}", routing.backend_host, routing.backend_port),
        db = %routing.id,
        "route"
    );
    control_plane.touch_activity(&routing.id).await;

    let mut backend =
        match TcpStream::connect((routing.backend_host.as_str(), routing.backend_port as u16)).await
        {
            Ok(stream) => stream,
            Err(err) => {
                error!(error = %err, %hostname, "backend connect failed");
                let _ = client.shutdown().await;
                return Ok(());
            }
        };

    // Tenant handoff for shared gateway: MONA <db_id>\n then MongoDB wire bytes.
    let preamble = format!("MONA {}\n", routing.id);
    if let Err(err) = backend.write_all(preamble.as_bytes()).await {
        error!(error = %err, db = %routing.id, "failed to write gateway preamble");
        let _ = client.shutdown().await;
        let _ = backend.shutdown().await;
        return Ok(());
    }

    let _ = tokio::io::copy_bidirectional(&mut client, &mut backend).await;
    let _ = client.shutdown().await;
    let _ = backend.shutdown().await;
    Ok(())
}

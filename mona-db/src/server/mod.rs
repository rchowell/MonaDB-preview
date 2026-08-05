pub mod connection;

use std::net::SocketAddr;
use std::sync::Arc;

use tokio::net::TcpListener;

use crate::cursor::CursorRegistry;
use crate::error::Result;
use crate::storage::CollectionRegistry;

const DEFAULT_PORT: u16 = 27017;

pub struct AppState {
    pub registry: CollectionRegistry,
    pub cursors: CursorRegistry,
}

pub async fn run(
    addr: Option<SocketAddr>,
    registry: CollectionRegistry,
    cursors: CursorRegistry,
) -> Result<()> {
    let addr = addr.unwrap_or_else(|| SocketAddr::from(([0, 0, 0, 0], DEFAULT_PORT)));
    let listener = TcpListener::bind(addr).await?;
    let bound_addr = listener.local_addr()?;
    let state = Arc::new(AppState {
        registry,
        cursors,
    });

    println!("MonaDB listening on {bound_addr}");

    loop {
        let (stream, peer) = listener.accept().await?;
        println!("connection from {peer}");

        let state = state.clone();
        tokio::spawn(async move {
            if let Err(error) = connection::handle_connection(stream, state).await {
                eprintln!("connection error from {peer}: {error}");
            }
        });
    }
}

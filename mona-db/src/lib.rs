pub mod commands;
pub mod cursor;
pub mod error;
pub mod predicate;
pub mod server;
pub mod storage;
pub mod wire;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use slatedb::object_store::local::LocalFileSystem;
use slatedb::object_store::ObjectStore;

use crate::cursor::CursorRegistry;
use crate::error::Result;
use crate::storage::CollectionRegistry;

/// Open a local filesystem object store rooted at `data_dir`.
pub fn open_object_store(data_dir: &Path) -> Result<Arc<dyn ObjectStore>> {
    std::fs::create_dir_all(data_dir).map_err(error::Error::Io)?;
    let store = LocalFileSystem::new_with_prefix(data_dir).map_err(|error| {
        error::Error::Storage(format!("failed to open data directory: {error}"))
    })?;
    Ok(Arc::new(store))
}

/// Run a single-tenant MonaDB server (one data root).
pub async fn run(addr: Option<SocketAddr>, data_dir: PathBuf) -> Result<()> {
    let object_store = open_object_store(&data_dir)?;
    let registry = CollectionRegistry::new(object_store, "monadb");
    let cursors = CursorRegistry::new();
    server::run(addr, registry, cursors).await
}

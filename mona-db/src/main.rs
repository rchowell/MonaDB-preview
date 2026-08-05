mod commands;
mod cursor;
mod error;
mod server;
mod storage;
mod wire;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use slatedb::object_store::local::LocalFileSystem;

#[tokio::main]
async fn main() -> error::Result<()> {
    let config = parse_config();
    let object_store = open_object_store(&config.data_dir)?;
    let registry = storage::CollectionRegistry::new(object_store, "monadb");
    let cursors = cursor::CursorRegistry::new();
    server::run(parse_addr(), registry, cursors).await
}

struct Config {
    data_dir: PathBuf,
}

fn parse_config() -> Config {
    let mut data_dir = PathBuf::from("./monadb-data");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--data-dir" => {
                let value = args.next().expect("--data-dir requires a value");
                data_dir = PathBuf::from(value);
            }
            "--addr" => {
                args.next().expect("--addr requires a value");
            }
            _ => {}
        }
    }

    if let Ok(value) = std::env::var("MONADB_DATA_DIR") {
        data_dir = PathBuf::from(value);
    }

    Config { data_dir }
}

fn parse_addr() -> Option<SocketAddr> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--addr" {
            let value = args.next().expect("--addr requires a value");
            return Some(value.parse().expect("invalid --addr value"));
        }
    }

    std::env::var("MONADB_ADDR")
        .ok()
        .map(|value| value.parse().expect("invalid MONADB_ADDR"))
}

fn open_object_store(data_dir: &std::path::Path) -> error::Result<Arc<dyn slatedb::object_store::ObjectStore>> {
    std::fs::create_dir_all(data_dir).map_err(error::Error::Io)?;
    let store = LocalFileSystem::new_with_prefix(data_dir).map_err(|error| {
        error::Error::Storage(format!("failed to open data directory: {error}"))
    })?;
    Ok(Arc::new(store))
}

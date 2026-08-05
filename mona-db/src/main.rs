use std::net::SocketAddr;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> monadb::error::Result<()> {
    let config = parse_config();
    monadb::run(parse_addr(), config.data_dir).await
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
                let _ = args.next().expect("--addr requires a value");
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

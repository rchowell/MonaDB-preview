use std::env;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    pub gateway_listen: String,
    pub health_listen: String,
    pub data_dir: PathBuf,
    pub lru_max_tenants: usize,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            gateway_listen: env::var("GATEWAY_LISTEN").unwrap_or_else(|_| "0.0.0.0:27017".into()),
            health_listen: env::var("HEALTH_LISTEN").unwrap_or_else(|_| "0.0.0.0:8080".into()),
            data_dir: PathBuf::from(
                env::var("DATA_DIR").unwrap_or_else(|_| "/data".into()),
            ),
            lru_max_tenants: env_usize("LRU_MAX_TENANTS", 32),
        }
    }
}

fn env_usize(key: &str, default: usize) -> usize {
    env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

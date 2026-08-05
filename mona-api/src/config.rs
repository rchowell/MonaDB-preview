use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub edge_domain: String,
    pub gateway_service_host: String,
    pub gateway_service_port: u16,
    pub k8s_namespace: String,
    pub idle_timeout_seconds: u64,
    pub sleep_poll_seconds: u64,
    pub listen_addr: String,
}

impl Config {
    pub fn from_env() -> Self {
        let raw_url = env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://mona:mona@localhost:5432/mona".into());
        Self {
            database_url: normalize_database_url(&raw_url),
            edge_domain: env::var("EDGE_DOMAIN").unwrap_or_else(|_| "mona.localhost".into()),
            gateway_service_host: env::var("GATEWAY_SERVICE_HOST")
                .unwrap_or_else(|_| "mona-gateway.mona.svc.cluster.local".into()),
            gateway_service_port: env_u16("GATEWAY_SERVICE_PORT", 27017),
            k8s_namespace: env::var("K8S_NAMESPACE").unwrap_or_else(|_| "mona".into()),
            idle_timeout_seconds: env_u64("IDLE_TIMEOUT_SECONDS", 300),
            sleep_poll_seconds: env_u64("SLEEP_POLL_SECONDS", 30),
            listen_addr: env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8000".into()),
        }
    }
}

fn normalize_database_url(url: &str) -> String {
    url.replace("postgresql+asyncpg://", "postgres://")
        .replace("postgresql://", "postgres://")
}

fn env_u64(key: &str, default: u64) -> u64 {
    env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn env_u16(key: &str, default: u16) -> u16 {
    env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

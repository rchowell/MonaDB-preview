use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub edge_listen: String,
    pub health_listen: String,
    pub control_plane_url: String,
    pub tls_cert: String,
    pub tls_key: String,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            edge_listen: env::var("EDGE_LISTEN").unwrap_or_else(|_| "0.0.0.0:27017".into()),
            health_listen: env::var("HEALTH_LISTEN").unwrap_or_else(|_| "0.0.0.0:8080".into()),
            control_plane_url: env::var("CONTROL_PLANE_URL")
                .unwrap_or_else(|_| "http://mona-api.mona.svc.cluster.local:8000".into()),
            tls_cert: env::var("TLS_CERT").unwrap_or_else(|_| "/certs/tls.crt".into()),
            tls_key: env::var("TLS_KEY").unwrap_or_else(|_| "/certs/tls.key".into()),
        }
    }
}

use reqwest::Client;
use serde::Deserialize;
use thiserror::Error;
use tracing::warn;

#[derive(Clone)]
pub struct ControlPlane {
    client: Client,
    base_url: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingInfo {
    pub id: String,
    pub backend_host: String,
    pub backend_port: i32,
}

#[derive(Debug, Error)]
pub enum ControlPlaneError {
    #[error("control plane request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("control plane returned {status} for {hostname}")]
    BadStatus {
        status: reqwest::StatusCode,
        hostname: String,
    },
}

impl ControlPlane {
    pub fn new(base_url: impl Into<String>) -> Self {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        Self {
            client: Client::new(),
            base_url,
        }
    }

    pub async fn resolve_backend(&self, hostname: &str) -> Result<RoutingInfo, ControlPlaneError> {
        let url = format!("{}/internal/routing/{hostname}", self.base_url);
        let response = self
            .client
            .get(&url)
            .timeout(std::time::Duration::from_secs(120))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(ControlPlaneError::BadStatus {
                status: response.status(),
                hostname: hostname.to_string(),
            });
        }

        Ok(response.json().await?)
    }

    pub async fn touch_activity(&self, db_id: &str) {
        let url = format!("{}/internal/activity/{db_id}", self.base_url);
        if let Err(err) = self
            .client
            .post(&url)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
        {
            warn!(error = %err, db_id, "failed to touch activity");
        }
    }
}

use chrono::{DateTime, Utc};
use sqlx::FromRow;

use crate::models::DatabaseStatus;

/// Postgres row for the `databases` table (not part of the public OpenAPI surface).
#[derive(Debug, Clone, FromRow)]
pub struct DatabaseRow {
    pub id: String,
    pub name: String,
    pub status: String,
    /// Legacy per-DB K8s name; unused after shared-gateway cutover.
    #[allow(dead_code)]
    pub k8s_name: String,
    #[allow(dead_code)]
    pub last_active_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

impl DatabaseRow {
    pub fn status_enum(&self) -> Result<DatabaseStatus, String> {
        self.status
            .parse::<DatabaseStatus>()
            .map_err(|err| err.to_string())
    }
}

/// Internal edge routing payload (not part of the public OpenAPI surface).
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingResponse {
    pub id: String,
    pub backend_host: String,
    pub backend_port: i32,
    pub status: DatabaseStatus,
}

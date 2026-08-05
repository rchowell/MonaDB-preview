use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseStatus {
    Pending,
    Ready,
    Sleeping,
    Error,
}

impl DatabaseStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Ready => "ready",
            Self::Sleeping => "sleeping",
            Self::Error => "error",
        }
    }
}

impl std::fmt::Display for DatabaseStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for DatabaseStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "ready" => Ok(Self::Ready),
            "sleeping" => Ok(Self::Sleeping),
            "error" => Ok(Self::Error),
            other => Err(format!("invalid database status: {other}")),
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct DatabaseRow {
    pub id: String,
    pub name: String,
    pub status: String,
    pub k8s_name: String,
    #[allow(dead_code)]
    pub last_active_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

impl DatabaseRow {
    pub fn status_enum(&self) -> Result<DatabaseStatus, String> {
        self.status.parse()
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateDatabaseRequest {
    pub name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseResponse {
    pub id: String,
    pub name: String,
    pub hostname: String,
    pub connection_string: String,
    pub status: DatabaseStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingResponse {
    pub id: String,
    pub backend_host: String,
    pub backend_port: i32,
    pub status: DatabaseStatus,
}

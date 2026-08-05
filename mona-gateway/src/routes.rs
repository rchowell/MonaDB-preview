use std::sync::Arc;

use axum::extract::State;
use axum::Json;

use crate::tenant::{HandleInfo, TenantRegistry};

pub async fn healthz() -> &'static str {
    "ok"
}

pub async fn list_handles(State(tenants): State<Arc<TenantRegistry>>) -> Json<Vec<HandleInfo>> {
    Json(tenants.list_handles().await)
}

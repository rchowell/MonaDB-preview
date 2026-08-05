use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;

use crate::error::AppError;
use crate::models::{CreateDatabaseRequest, Database, UpdateDatabaseRequest};
use crate::row::RoutingResponse;
use crate::services;
use crate::AppState;

pub async fn healthz() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

pub async fn list_databases(
    State(state): State<AppState>,
) -> Result<Json<Vec<Database>>, AppError> {
    let rows = services::list_databases(&state.pool, &state.config).await?;
    Ok(Json(rows))
}

pub async fn create_database(
    State(state): State<AppState>,
    Json(body): Json<CreateDatabaseRequest>,
) -> Result<(StatusCode, Json<Database>), AppError> {
    let row = services::create_database(&state.pool, &state.config, &body.name).await?;
    Ok((StatusCode::CREATED, Json(row)))
}

pub async fn get_database(
    State(state): State<AppState>,
    Path(db_id): Path<String>,
) -> Result<Json<Database>, AppError> {
    match services::get_database(&state.pool, &state.config, &db_id).await? {
        Some(row) => Ok(Json(row)),
        None => Err(AppError::NotFound(format!("database {db_id} not found"))),
    }
}

pub async fn update_database(
    State(state): State<AppState>,
    Path(db_id): Path<String>,
    Json(body): Json<UpdateDatabaseRequest>,
) -> Result<Json<Database>, AppError> {
    match services::update_database(&state.pool, &state.config, &db_id, &body.name).await? {
        Some(row) => Ok(Json(row)),
        None => Err(AppError::NotFound(format!("database {db_id} not found"))),
    }
}

pub async fn delete_database(
    State(state): State<AppState>,
    Path(db_id): Path<String>,
) -> Result<StatusCode, AppError> {
    if services::delete_database(&state.pool, &db_id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound(format!("database {db_id} not found")))
    }
}

pub async fn routing(
    State(state): State<AppState>,
    Path(hostname): Path<String>,
) -> Result<Json<RoutingResponse>, AppError> {
    match services::resolve_routing(&state.pool, &state.config, &hostname).await? {
        Some(row) => Ok(Json(row)),
        None => Err(AppError::NotFound(format!("no route for {hostname}"))),
    }
}

pub async fn activity(
    State(state): State<AppState>,
    Path(db_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    if services::touch_activity(&state.pool, &db_id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound(format!("database {db_id} not found")))
    }
}

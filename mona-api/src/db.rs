use chrono::{DateTime, Utc};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

use crate::config::Config;
use crate::error::AppError;
use crate::models::DatabaseStatus;
use crate::row::DatabaseRow;

pub async fn connect(config: &Config) -> Result<PgPool, sqlx::Error> {
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.database_url)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

pub async fn list_databases(pool: &PgPool) -> Result<Vec<DatabaseRow>, sqlx::Error> {
    sqlx::query_as::<_, DatabaseRow>(
        r#"
        SELECT id, name, status, k8s_name, last_active_at, created_at
        FROM databases
        ORDER BY created_at DESC
        "#,
    )
    .fetch_all(pool)
    .await
}

pub async fn get_database(pool: &PgPool, id: &str) -> Result<Option<DatabaseRow>, sqlx::Error> {
    sqlx::query_as::<_, DatabaseRow>(
        r#"
        SELECT id, name, status, k8s_name, last_active_at, created_at
        FROM databases
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn insert_database(
    pool: &PgPool,
    id: &str,
    name: &str,
    status: DatabaseStatus,
    k8s_name: &str,
    now: DateTime<Utc>,
) -> Result<DatabaseRow, sqlx::Error> {
    sqlx::query_as::<_, DatabaseRow>(
        r#"
        INSERT INTO databases (id, name, status, k8s_name, last_active_at, created_at)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id, name, status, k8s_name, last_active_at, created_at
        "#,
    )
    .bind(id)
    .bind(name)
    .bind(status.as_str())
    .bind(k8s_name)
    .bind(now)
    .bind(now)
    .fetch_one(pool)
    .await
}

pub async fn update_database_name(
    pool: &PgPool,
    id: &str,
    name: &str,
) -> Result<Option<DatabaseRow>, sqlx::Error> {
    sqlx::query_as::<_, DatabaseRow>(
        r#"
        UPDATE databases
        SET name = $2
        WHERE id = $1
        RETURNING id, name, status, k8s_name, last_active_at, created_at
        "#,
    )
    .bind(id)
    .bind(name)
    .fetch_optional(pool)
    .await
}

pub async fn delete_database(pool: &PgPool, id: &str) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM databases WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn update_status(
    pool: &PgPool,
    id: &str,
    status: DatabaseStatus,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE databases SET status = $2 WHERE id = $1")
        .bind(id)
        .bind(status.as_str())
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn update_status_and_activity(
    pool: &PgPool,
    id: &str,
    status: DatabaseStatus,
    last_active_at: DateTime<Utc>,
) -> Result<DatabaseRow, sqlx::Error> {
    sqlx::query_as::<_, DatabaseRow>(
        r#"
        UPDATE databases
        SET status = $2, last_active_at = $3
        WHERE id = $1
        RETURNING id, name, status, k8s_name, last_active_at, created_at
        "#,
    )
    .bind(id)
    .bind(status.as_str())
    .bind(last_active_at)
    .fetch_one(pool)
    .await
}

pub async fn touch_activity(
    pool: &PgPool,
    id: &str,
    last_active_at: DateTime<Utc>,
    promote_sleeping: bool,
) -> Result<Option<DatabaseRow>, sqlx::Error> {
    if promote_sleeping {
        sqlx::query_as::<_, DatabaseRow>(
            r#"
            UPDATE databases
            SET last_active_at = $2,
                status = CASE WHEN status = 'sleeping' THEN 'ready' ELSE status END
            WHERE id = $1
            RETURNING id, name, status, k8s_name, last_active_at, created_at
            "#,
        )
        .bind(id)
        .bind(last_active_at)
        .fetch_optional(pool)
        .await
    } else {
        sqlx::query_as::<_, DatabaseRow>(
            r#"
            UPDATE databases
            SET last_active_at = $2
            WHERE id = $1
            RETURNING id, name, status, k8s_name, last_active_at, created_at
            "#,
        )
        .bind(id)
        .bind(last_active_at)
        .fetch_optional(pool)
        .await
    }
}

pub async fn list_idle_ready(
    pool: &PgPool,
    cutoff: DateTime<Utc>,
) -> Result<Vec<DatabaseRow>, sqlx::Error> {
    sqlx::query_as::<_, DatabaseRow>(
        r#"
        SELECT id, name, status, k8s_name, last_active_at, created_at
        FROM databases
        WHERE status = 'ready' AND last_active_at < $1
        "#,
    )
    .bind(cutoff)
    .fetch_all(pool)
    .await
}

pub fn require_status(row: &DatabaseRow) -> Result<DatabaseStatus, AppError> {
    row.status_enum()
        .map_err(AppError::Internal)
}

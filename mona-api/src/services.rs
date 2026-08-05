use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use rand::Rng;
use sqlx::PgPool;

use crate::config::Config;
use crate::db;
use crate::error::AppError;
use crate::k8s::K8sProvisioner;
use crate::models::{DatabaseResponse, DatabaseRow, DatabaseStatus, RoutingResponse};

fn new_id() -> String {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::rng();
    (0..8)
        .map(|_| ALPHABET[rng.random_range(0..ALPHABET.len())] as char)
        .collect()
}

pub fn hostname_for(db_id: &str, config: &Config) -> String {
    format!("db-{db_id}.{}", config.edge_domain)
}

pub fn connection_string_for(db_id: &str, config: &Config) -> String {
    let host = hostname_for(db_id, config);
    format!("mongodb://{host}:27017/?tls=true&tlsAllowInvalidCertificates=true")
}

pub fn to_response(row: &DatabaseRow, config: &Config) -> Result<DatabaseResponse, AppError> {
    Ok(DatabaseResponse {
        id: row.id.clone(),
        name: row.name.clone(),
        hostname: hostname_for(&row.id, config),
        connection_string: connection_string_for(&row.id, config),
        status: db::require_status(row)?,
        created_at: row.created_at,
    })
}

pub async fn list_databases(
    pool: &PgPool,
    config: &Config,
) -> Result<Vec<DatabaseResponse>, AppError> {
    let rows = db::list_databases(pool).await?;
    rows.iter().map(|row| to_response(row, config)).collect()
}

pub async fn get_database(
    pool: &PgPool,
    config: &Config,
    db_id: &str,
) -> Result<Option<DatabaseResponse>, AppError> {
    match db::get_database(pool, db_id).await? {
        Some(row) => Ok(Some(to_response(&row, config)?)),
        None => Ok(None),
    }
}

pub async fn create_database(
    pool: &PgPool,
    config: &Config,
    provisioner: &K8sProvisioner,
    name: &str,
) -> Result<DatabaseResponse, AppError> {
    let name = name.trim();
    if name.is_empty() || name.len() > 64 {
        return Err(AppError::BadRequest(
            "name must be between 1 and 64 characters".into(),
        ));
    }

    let db_id = new_id();
    let k8s_name = format!("mona-db-{db_id}");
    let now = Utc::now();
    let row = db::insert_database(
        pool,
        &db_id,
        name,
        DatabaseStatus::Pending,
        &k8s_name,
        now,
    )
    .await?;

    match provision_ready(provisioner, &db_id, &k8s_name).await {
        Ok(()) => {
            let row = db::update_status_and_activity(
                pool,
                &db_id,
                DatabaseStatus::Ready,
                Utc::now(),
            )
            .await?;
            to_response(&row, config)
        }
        Err(err) => {
            let _ = db::update_status(pool, &db_id, DatabaseStatus::Error).await;
            let _ = row;
            Err(err)
        }
    }
}

async fn provision_ready(
    provisioner: &K8sProvisioner,
    db_id: &str,
    k8s_name: &str,
) -> Result<(), AppError> {
    provisioner.provision_database(db_id, k8s_name).await?;
    provisioner.scale(k8s_name, 1).await?;
    provisioner
        .wait_ready(k8s_name, Duration::from_secs(120))
        .await
}

pub fn parse_db_id_from_hostname(hostname: &str, config: &Config) -> Option<String> {
    let host = hostname.split(':').next()?.to_ascii_lowercase();
    let suffix = format!(".{}", config.edge_domain);
    if !host.starts_with("db-") || !host.ends_with(&suffix) {
        return None;
    }
    let id = &host["db-".len()..host.len() - suffix.len()];
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
}

pub async fn resolve_routing(
    pool: &PgPool,
    config: &Config,
    provisioner: &K8sProvisioner,
    hostname: &str,
) -> Result<Option<RoutingResponse>, AppError> {
    let Some(db_id) = parse_db_id_from_hostname(hostname, config) else {
        return Ok(None);
    };
    let Some(row) = db::get_database(pool, &db_id).await? else {
        return Ok(None);
    };

    let status = db::require_status(&row)?;
    let row = if status != DatabaseStatus::Ready {
        db::update_status(pool, &db_id, DatabaseStatus::Pending).await?;
        provisioner.scale(&row.k8s_name, 1).await?;
        provisioner
            .wait_ready(&row.k8s_name, Duration::from_secs(120))
            .await?;
        db::update_status_and_activity(pool, &db_id, DatabaseStatus::Ready, Utc::now()).await?
    } else {
        db::update_status_and_activity(pool, &db_id, DatabaseStatus::Ready, Utc::now()).await?
    };

    Ok(Some(RoutingResponse {
        id: row.id.clone(),
        backend_host: provisioner.service_host(&row.k8s_name),
        backend_port: 27017,
        status: db::require_status(&row)?,
    }))
}

pub async fn touch_activity(pool: &PgPool, db_id: &str) -> Result<bool, AppError> {
    let row = db::touch_activity(pool, db_id, Utc::now(), true).await?;
    Ok(row.is_some())
}

pub async fn sleep_idle_databases(
    pool: &PgPool,
    config: &Config,
    provisioner: &K8sProvisioner,
) -> Result<usize, AppError> {
    let cutoff = Utc::now() - ChronoDuration::seconds(config.idle_timeout_seconds as i64);
    let rows = db::list_idle_ready(pool, cutoff).await?;
    let mut slept = 0usize;
    for row in rows {
        provisioner.scale(&row.k8s_name, 0).await?;
        db::update_status(pool, &row.id, DatabaseStatus::Sleeping).await?;
        slept += 1;
    }
    Ok(slept)
}

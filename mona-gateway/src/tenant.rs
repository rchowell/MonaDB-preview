use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use monadb::cursor::CursorRegistry;
use monadb::server::AppState;
use monadb::storage::CollectionRegistry;
use monadb::{open_object_store, error::Error as MonaError};
use serde::Serialize;
use tokio::sync::Mutex;
use tracing::{debug, info, warn, Instrument};

use crate::lease::{LeaseGuard, LocalLease, WriterLease};

struct TenantEntry {
    state: Arc<AppState>,
    _lease: LeaseGuard,
    last_used: Instant,
    active_connections: usize,
}

pub struct TenantRegistry {
    data_dir: PathBuf,
    max_tenants: usize,
    lease: LocalLease,
    inner: Mutex<HashMap<String, TenantEntry>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandleInfo {
    pub db_id: String,
    pub active_connections: usize,
    pub open_collections: usize,
    pub open_cursors: usize,
    pub idle_secs: u64,
}

impl TenantRegistry {
    pub fn new(data_dir: PathBuf, max_tenants: usize) -> Self {
        Self {
            data_dir,
            max_tenants: max_tenants.max(1),
            lease: LocalLease,
            inner: Mutex::new(HashMap::new()),
        }
    }

    pub async fn acquire(self: &Arc<Self>, db_id: &str) -> Result<TenantGuard, MonaError> {
        let started = Instant::now();
        let span = tracing::info_span!("tenant.acquire", db_id);

        async {
            self.evict_if_needed(db_id).await?;

            let mut map = self.inner.lock().await;
            if let Some(entry) = map.get_mut(db_id) {
                entry.last_used = Instant::now();
                entry.active_connections += 1;
                debug!(
                    db_id,
                    cold = false,
                    active_connections = entry.active_connections,
                    elapsed_ms = started.elapsed().as_secs_f64() * 1000.0,
                    "tenant acquire"
                );
                return Ok(TenantGuard {
                    db_id: db_id.to_string(),
                    state: entry.state.clone(),
                    registry: Arc::clone(self),
                });
            }
            drop(map);

            let lease_started = Instant::now();
            let lease = self
                .lease
                .acquire(db_id)
                .await
                .map_err(|err| MonaError::Storage(err.to_string()))?;
            let lease_ms = lease_started.elapsed().as_secs_f64() * 1000.0;

            let open_started = Instant::now();
            let tenant_dir = self.data_dir.join(db_id);
            let object_store = open_object_store(&tenant_dir)?;
            // data_root "monadb" mirrors single-tenant layout under /data/{db_id}/monadb/...
            let registry = CollectionRegistry::new(object_store, "monadb");
            let cursors = CursorRegistry::new();
            let state = Arc::new(AppState { registry, cursors });
            let open_ms = open_started.elapsed().as_secs_f64() * 1000.0;

            let mut map = self.inner.lock().await;
            if let Some(entry) = map.get_mut(db_id) {
                // Lost the race; reuse existing and drop our unused open.
                let _ = state.registry.close_all().await;
                entry.last_used = Instant::now();
                entry.active_connections += 1;
                debug!(
                    db_id,
                    cold = false,
                    raced = true,
                    lease_ms,
                    open_ms,
                    elapsed_ms = started.elapsed().as_secs_f64() * 1000.0,
                    "tenant acquire"
                );
                return Ok(TenantGuard {
                    db_id: db_id.to_string(),
                    state: entry.state.clone(),
                    registry: Arc::clone(self),
                });
            }

            map.insert(
                db_id.to_string(),
                TenantEntry {
                    state: state.clone(),
                    _lease: lease,
                    last_used: Instant::now(),
                    active_connections: 1,
                },
            );
            let tenants = map.len();
            info!(
                db_id,
                cold = true,
                lease_ms,
                open_ms,
                tenants,
                elapsed_ms = started.elapsed().as_secs_f64() * 1000.0,
                "tenant acquire"
            );

            Ok(TenantGuard {
                db_id: db_id.to_string(),
                state,
                registry: Arc::clone(self),
            })
        }
        .instrument(span)
        .await
    }

    async fn release(&self, db_id: &str) {
        let mut map = self.inner.lock().await;
        if let Some(entry) = map.get_mut(db_id) {
            entry.active_connections = entry.active_connections.saturating_sub(1);
            entry.last_used = Instant::now();
        }
    }

    async fn evict_if_needed(&self, keep_db_id: &str) -> Result<(), MonaError> {
        loop {
            let victim = {
                let map = self.inner.lock().await;
                if map.len() < self.max_tenants || map.contains_key(keep_db_id) {
                    return Ok(());
                }
                map.iter()
                    .filter(|(id, e)| {
                        id.as_str() != keep_db_id
                            && e.active_connections == 0
                            // cursors pin snapshots; skip tenants with open cursors
                    })
                    .min_by_key(|(_, e)| e.last_used)
                    .map(|(id, _)| id.clone())
            };

            let Some(victim_id) = victim else {
                // All other tenants busy; allow temporary over-capacity.
                warn!("LRU full but no idle tenant to evict");
                return Ok(());
            };

            // Re-check cursor pin under lock before close.
            let entry = {
                let mut map = self.inner.lock().await;
                let Some(entry) = map.get(&victim_id) else {
                    continue;
                };
                if entry.active_connections > 0 {
                    continue;
                }
                if !entry.state.cursors.is_empty().await {
                    continue;
                }
                map.remove(&victim_id)
            };

            if let Some(entry) = entry {
                let started = Instant::now();
                entry.state.cursors.clear().await;
                if let Err(err) = entry.state.registry.close_all().await {
                    warn!(db_id = %victim_id, error = %err, "failed to close tenant");
                    return Err(err);
                }
                info!(
                    db_id = %victim_id,
                    close_ms = started.elapsed().as_secs_f64() * 1000.0,
                    "evicted tenant"
                );
            }
        }
    }

    pub async fn list_handles(&self) -> Vec<HandleInfo> {
        let map = self.inner.lock().await;
        let mut out = Vec::with_capacity(map.len());
        for (db_id, entry) in map.iter() {
            out.push(HandleInfo {
                db_id: db_id.clone(),
                active_connections: entry.active_connections,
                open_collections: entry.state.registry.open_count().await,
                open_cursors: entry.state.cursors.len().await,
                idle_secs: entry.last_used.elapsed().as_secs(),
            });
        }
        out.sort_by(|a, b| a.db_id.cmp(&b.db_id));
        out
    }
}

pub struct TenantGuard {
    db_id: String,
    state: Arc<AppState>,
    registry: Arc<TenantRegistry>,
}

impl TenantGuard {
    pub fn state(&self) -> Arc<AppState> {
        self.state.clone()
    }
}

impl Drop for TenantGuard {
    fn drop(&mut self) {
        let db_id = self.db_id.clone();
        let registry = Arc::clone(&self.registry);
        tokio::spawn(async move {
            registry.release(&db_id).await;
        });
    }
}

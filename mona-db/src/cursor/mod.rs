use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use bson::Document;
use slatedb::DbSnapshot;
use tokio::sync::Mutex;

/// Default MongoDB-style batch size when `batchSize` is omitted or zero.
pub const DEFAULT_BATCH_SIZE: i32 = 101;

/// Server-side cursor state for batched `find` / `getMore`.
pub struct CursorState {
    pub ns: String,
    pub db: String,
    pub collection: String,
    pub snapshot: Arc<DbSnapshot>,
    pub last_key: Option<Vec<u8>>,
    pub limit_remaining: Option<i32>,
    pub default_batch_size: i32,
    /// Top-level equality filter applied on each batch (`None` = match all).
    pub equality: Option<Document>,
}

pub struct CursorRegistry {
    next_id: AtomicU64,
    cursors: Mutex<HashMap<i64, CursorState>>,
}

impl CursorRegistry {
    pub fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            cursors: Mutex::new(HashMap::new()),
        }
    }

    pub async fn register(&self, state: CursorState) -> i64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) as i64;
        self.cursors.lock().await.insert(id, state);
        id
    }

    pub async fn get(&self, id: i64) -> Option<CursorState> {
        self.cursors.lock().await.get(&id).cloned()
    }

    pub async fn take(&self, id: i64) -> Option<CursorState> {
        self.cursors.lock().await.remove(&id)
    }

    pub async fn kill(&self, ids: &[i64]) -> (Vec<i64>, Vec<i64>) {
        let mut killed = Vec::new();
        let mut not_found = Vec::new();
        let mut cursors = self.cursors.lock().await;

        for id in ids {
            if cursors.remove(id).is_some() {
                killed.push(*id);
            } else {
                not_found.push(*id);
            }
        }

        (killed, not_found)
    }

    pub async fn update(&self, id: i64, last_key: Option<Vec<u8>>, limit_remaining: Option<i32>) {
        if let Some(state) = self.cursors.lock().await.get_mut(&id) {
            state.last_key = last_key;
            state.limit_remaining = limit_remaining;
        }
    }
}

impl CursorState {
    pub fn new(
        db: String,
        collection: String,
        snapshot: Arc<DbSnapshot>,
        last_key: Option<Vec<u8>>,
        limit_remaining: Option<i32>,
        default_batch_size: i32,
        equality: Option<Document>,
    ) -> Self {
        let ns = format!("{db}.{collection}");
        Self {
            ns,
            db,
            collection,
            snapshot,
            last_key,
            limit_remaining,
            default_batch_size,
            equality,
        }
    }
}

// CursorState holds Arc<DbSnapshot> — clone for get().
impl Clone for CursorState {
    fn clone(&self) -> Self {
        Self {
            ns: self.ns.clone(),
            db: self.db.clone(),
            collection: self.collection.clone(),
            snapshot: self.snapshot.clone(),
            last_key: self.last_key.clone(),
            limit_remaining: self.limit_remaining,
            default_batch_size: self.default_batch_size,
            equality: self.equality.clone(),
        }
    }
}

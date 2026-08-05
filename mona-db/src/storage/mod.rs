mod id;
mod scan;

use std::collections::HashMap;
use std::sync::Arc;

use bson::{Bson, Document};
use slatedb::{Db, DbSnapshot, Error as SlateDbError};
use slatedb::object_store::ObjectStore;
use tokio::sync::RwLock;

use crate::error::{Error, Result};

pub use id::{encode_id, ensure_id};
pub use scan::scan_batch;

/// Lazily opens one SlateDB `Db` per MongoDB collection (`{data_root}/{db}/{coll}`).
pub struct CollectionRegistry {
    object_store: Arc<dyn ObjectStore>,
    data_root: String,
    open: RwLock<HashMap<(String, String), Arc<Db>>>,
}

impl CollectionRegistry {
    pub fn new(object_store: Arc<dyn ObjectStore>, data_root: impl Into<String>) -> Self {
        Self {
            object_store,
            data_root: data_root.into(),
            open: RwLock::new(HashMap::new()),
        }
    }

    fn collection_path(&self, db: &str, coll: &str) -> String {
        format!("{}/{}/{}", self.data_root, db, coll)
    }

    async fn db_for(&self, db: &str, coll: &str) -> Result<Arc<Db>> {
        let key = (db.to_string(), coll.to_string());
        {
            let open = self.open.read().await;
            if let Some(db_handle) = open.get(&key) {
                return Ok(db_handle.clone());
            }
        }

        let path = self.collection_path(db, coll);
        let db_handle = Db::builder(path, self.object_store.clone())
            .build()
            .await
            .map_err(map_slate_error)?;
        let db_handle = Arc::new(db_handle);

        self.open
            .write()
            .await
            .insert(key, db_handle.clone());

        Ok(db_handle)
    }

    pub async fn insert(&self, db: &str, coll: &str, mut doc: Document) -> Result<Bson> {
        let id = ensure_id(&mut doc);
        let key = encode_id(&id)?;
        let value = bson::to_vec(&doc)?;

        let coll_db = self.db_for(db, coll).await?;
        coll_db
            .put(&key, &value)
            .await
            .map_err(map_slate_error)?;

        Ok(id)
    }

    pub async fn get(&self, db: &str, coll: &str, id: &Bson) -> Result<Option<Document>> {
        let key = encode_id(id)?;
        let coll_db = self.db_for(db, coll).await?;
        let value = coll_db.get(&key).await.map_err(map_slate_error)?;

        match value {
            None => Ok(None),
            Some(bytes) => Ok(Some(bson::from_slice(&bytes)?)),
        }
    }

    pub async fn snapshot(&self, db: &str, coll: &str) -> Result<Arc<DbSnapshot>> {
        let coll_db = self.db_for(db, coll).await?;
        coll_db.snapshot().await.map_err(map_slate_error)
    }

    /// Scan all documents in a collection, optionally capped by `limit`.
    pub async fn scan(&self, db: &str, coll: &str, limit: Option<i32>) -> Result<Vec<Document>> {
        let coll_db = self.db_for(db, coll).await?;
        let mut iter = coll_db.scan(..).await.map_err(map_slate_error)?;
        let max = limit.map(|n| n.max(0) as usize);
        let mut docs = Vec::new();

        while let Some(kv) = iter.next().await.map_err(map_slate_error)? {
            let doc = bson::from_slice(kv.value.as_ref())?;
            docs.push(doc);
            if let Some(max) = max {
                if docs.len() >= max {
                    break;
                }
            }
        }

        Ok(docs)
    }
}

fn map_slate_error(error: SlateDbError) -> Error {
    Error::Storage(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bson::doc;
    use slatedb::object_store::memory::InMemory;

    async fn registry() -> CollectionRegistry {
        CollectionRegistry::new(Arc::new(InMemory::new()), "monadb-test")
    }

    #[tokio::test]
    async fn insert_and_get_round_trip() {
        let registry = registry().await;
        let id = registry
            .insert("test", "users", doc! { "name": "alice", "score": 10 })
            .await
            .unwrap();

        let doc = registry.get("test", "users", &id).await.unwrap().unwrap();
        assert_eq!(doc.get_str("name"), Ok("alice"));
        assert_eq!(doc.get_i32("score"), Ok(10));
        assert_eq!(doc.get("_id"), Some(&id));
    }

    #[tokio::test]
    async fn get_missing_returns_none() {
        let registry = registry().await;
        let missing = registry
            .get("test", "users", &Bson::String("nobody".into()))
            .await
            .unwrap();
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn scan_returns_all_documents() {
        let registry = registry().await;
        registry
            .insert("test", "users", doc! { "name": "alice" })
            .await
            .unwrap();
        registry
            .insert("test", "users", doc! { "name": "bob" })
            .await
            .unwrap();

        let docs = registry.scan("test", "users", None).await.unwrap();
        assert_eq!(docs.len(), 2);
    }

    #[tokio::test]
    async fn scan_respects_limit() {
        let registry = registry().await;
        registry
            .insert("test", "users", doc! { "name": "alice" })
            .await
            .unwrap();
        registry
            .insert("test", "users", doc! { "name": "bob" })
            .await
            .unwrap();

        let docs = registry.scan("test", "users", Some(1)).await.unwrap();
        assert_eq!(docs.len(), 1);
    }

    #[tokio::test]
    async fn persistence_across_registry_reopen() {
        let store = Arc::new(InMemory::new());
        let root = "persist-test";

        let registry = CollectionRegistry::new(store.clone(), root);
        let id = registry
            .insert("test", "items", doc! { "_id": "item-1", "v": 1 })
            .await
            .unwrap();

        // Simulate restart: new registry on same object store + root.
        let registry2 = CollectionRegistry::new(store, root);
        let doc = registry2.get("test", "items", &id).await.unwrap().unwrap();
        assert_eq!(doc.get_str("_id"), Ok("item-1"));
        assert_eq!(doc.get_i32("v"), Ok(1));
    }
}

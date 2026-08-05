use bson::{Bson, Document};

use crate::commands::filter::QueryFilter;
use crate::cursor::{CursorRegistry, CursorState, DEFAULT_BATCH_SIZE};
use crate::error::{Error, Result};
use crate::storage::{scan_batch, CollectionRegistry};

/// Parsed `find` command with equality and supported `$` operators.
#[derive(Debug, Clone, PartialEq)]
pub struct FindCmd {
    pub db: String,
    pub collection: String,
    pub(crate) filter: QueryFilter,
    pub limit: Option<i32>,
    pub skip: i32,
    pub batch_size: i32,
}

impl FindCmd {
    pub fn from_document(doc: Document) -> Result<Self> {
        let db = get_db(&doc)?;
        let collection = get_string(&doc, "find")?;

        let filter = match doc.get("filter") {
            Some(Bson::Document(filter)) => filter.clone(),
            None => Document::new(),
            Some(_) => {
                return Err(Error::CommandParse(
                    "field 'filter' must be a document".into(),
                ));
            }
        };

        Ok(Self {
            db,
            collection,
            filter: QueryFilter::from_query(&filter)?,
            limit: parse_limit(&doc)?,
            skip: parse_skip(&doc)?,
            batch_size: parse_batch_size(&doc)?,
        })
    }

    pub async fn execute(
        &self,
        registry: &CollectionRegistry,
        cursors: &CursorRegistry,
    ) -> Result<Document> {
        let ns = format!("{}.{}", self.db, self.collection);

        match &self.filter {
            QueryFilter::ById(id) => {
                let doc = registry.get(&self.db, &self.collection, id).await?;
                let mut docs = Vec::new();
                if let Some(doc) = doc {
                    docs.push(doc);
                }
                if let Some(limit) = self.limit {
                    docs.truncate(limit.max(0) as usize);
                }
                let first_batch: Vec<Bson> = docs.into_iter().map(Bson::Document).collect();
                Ok(cursor_reply(ns, 0, first_batch, true))
            }
            QueryFilter::Expr(pred) if pred.extract_id_eq().is_some() => {
                let id = pred.extract_id_eq().expect("_id present");
                let mut docs = Vec::new();
                if let Some(doc) = registry.get(&self.db, &self.collection, id).await? {
                    if pred.matches(&doc) {
                        docs.push(doc);
                    }
                }
                if let Some(limit) = self.limit {
                    docs.truncate(limit.max(0) as usize);
                }
                let first_batch: Vec<Bson> = docs.into_iter().map(Bson::Document).collect();
                Ok(cursor_reply(ns, 0, first_batch, true))
            }
            QueryFilter::All | QueryFilter::Expr(_) => {
                let predicate = self.filter.predicate().cloned();
                let snapshot = registry.snapshot(&self.db, &self.collection).await?;
                let batch = scan_batch(
                    &snapshot,
                    None,
                    self.skip,
                    self.batch_size,
                    self.limit,
                    predicate.as_ref(),
                )
                .await?;

                let first_batch: Vec<Bson> = batch.docs.into_iter().map(Bson::Document).collect();

                if batch.exhausted {
                    return Ok(cursor_reply(ns, 0, first_batch, true));
                }

                let cursor_id = cursors
                    .register(CursorState::new(
                        self.db.clone(),
                        self.collection.clone(),
                        snapshot,
                        batch.last_key,
                        batch.limit_remaining,
                        self.batch_size,
                        predicate,
                    ))
                    .await;

                Ok(cursor_reply(ns, cursor_id, first_batch, true))
            }
        }
    }
}

pub fn cursor_reply(ns: String, cursor_id: i64, batch: Vec<Bson>, first_batch: bool) -> Document {
    let mut cursor = Document::new();
    cursor.insert("id", cursor_id);
    cursor.insert("ns", ns);
    if first_batch {
        cursor.insert("firstBatch", Bson::Array(batch));
    } else {
        cursor.insert("nextBatch", Bson::Array(batch));
    }

    bson::doc! {
        "ok": 1.0,
        "cursor": cursor,
    }
}

fn parse_limit(doc: &Document) -> Result<Option<i32>> {
    match doc.get("limit") {
        None => Ok(None),
        Some(Bson::Int32(value)) => Ok(Some(*value)),
        Some(Bson::Int64(value)) => {
            i32::try_from(*value)
                .map(Some)
                .map_err(|_| Error::CommandParse("find limit out of range".into()))
        }
        Some(Bson::Double(value)) => Ok(Some(*value as i32)),
        Some(_) => Err(Error::CommandParse("field 'limit' must be a number".into())),
    }
}

fn parse_skip(doc: &Document) -> Result<i32> {
    match doc.get("skip") {
        None => Ok(0),
        Some(Bson::Int32(value)) => Ok(*value),
        Some(Bson::Int64(value)) => i32::try_from(*value)
            .map_err(|_| Error::CommandParse("find skip out of range".into())),
        Some(Bson::Double(value)) => Ok(*value as i32),
        Some(_) => Err(Error::CommandParse("field 'skip' must be a number".into())),
    }
}

fn parse_batch_size(doc: &Document) -> Result<i32> {
    match doc.get("batchSize") {
        None => Ok(DEFAULT_BATCH_SIZE),
        Some(Bson::Int32(value)) => Ok(if *value == 0 {
            DEFAULT_BATCH_SIZE
        } else {
            *value
        }),
        Some(Bson::Int64(value)) => {
            let value = i32::try_from(*value)
                .map_err(|_| Error::CommandParse("find batchSize out of range".into()))?;
            Ok(if value == 0 { DEFAULT_BATCH_SIZE } else { value })
        }
        Some(Bson::Double(value)) => {
            let value = *value as i32;
            Ok(if value == 0 { DEFAULT_BATCH_SIZE } else { value })
        }
        Some(_) => Err(Error::CommandParse("field 'batchSize' must be a number".into())),
    }
}

fn get_db(doc: &Document) -> Result<String> {
    get_string(doc, "$db")
}

fn get_string(doc: &Document, key: &str) -> Result<String> {
    match doc.get(key) {
        Some(Bson::String(value)) => Ok(value.clone()),
        Some(_) => Err(Error::CommandParse(format!(
            "field '{key}' must be a string"
        ))),
        None => Err(Error::CommandParse(format!("missing field '{key}'"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bson::doc;
    use slatedb::object_store::memory::InMemory;
    use std::sync::Arc;

    #[test]
    fn parses_find_by_id() {
        let cmd = FindCmd::from_document(doc! {
            "find": "users",
            "$db": "test",
            "filter": { "_id": "alice" }
        })
        .unwrap();

        assert_eq!(cmd.db, "test");
        assert_eq!(cmd.collection, "users");
        assert_eq!(cmd.filter, QueryFilter::ById(Bson::String("alice".into())));
    }

    #[test]
    fn parses_empty_filter() {
        let cmd = FindCmd::from_document(doc! {
            "find": "users",
            "$db": "test",
            "filter": {}
        })
        .unwrap();

        assert_eq!(cmd.filter, QueryFilter::All);
        assert_eq!(cmd.batch_size, DEFAULT_BATCH_SIZE);
    }

    #[test]
    fn parses_equality_filter() {
        let cmd = FindCmd::from_document(doc! {
            "find": "users",
            "$db": "test",
            "filter": { "name": "alice" }
        })
        .unwrap();

        assert!(matches!(cmd.filter, QueryFilter::Expr(_)));
    }

    #[test]
    fn parses_comparison_filter() {
        let cmd = FindCmd::from_document(doc! {
            "find": "users",
            "$db": "test",
            "filter": { "score": { "$gt": 10 } }
        })
        .unwrap();

        assert!(matches!(cmd.filter, QueryFilter::Expr(_)));
    }

    #[test]
    fn parses_batch_size_and_skip() {
        let cmd = FindCmd::from_document(doc! {
            "find": "users",
            "$db": "test",
            "filter": {},
            "batchSize": 5,
            "skip": 2
        })
        .unwrap();

        assert_eq!(cmd.batch_size, 5);
        assert_eq!(cmd.skip, 2);
    }

    #[test]
    fn rejects_unsupported_operator_filter() {
        let err = FindCmd::from_document(doc! {
            "find": "users",
            "$db": "test",
            "filter": { "score": { "$mod": [2, 0] } }
        })
        .unwrap_err();
        assert!(err.to_string().contains("unsupported field operator"));
    }

    #[tokio::test]
    async fn find_by_name_equality() {
        let registry = CollectionRegistry::new(Arc::new(InMemory::new()), "find-eq-test");
        let cursors = CursorRegistry::new();
        registry
            .insert("test", "users", doc! { "_id": "a", "name": "alice" })
            .await
            .unwrap();
        registry
            .insert("test", "users", doc! { "_id": "b", "name": "bob" })
            .await
            .unwrap();

        let cmd = FindCmd::from_document(doc! {
            "find": "users",
            "$db": "test",
            "filter": { "name": "bob" }
        })
        .unwrap();

        let body = cmd.execute(&registry, &cursors).await.unwrap();
        let batch = body
            .get_document("cursor")
            .unwrap()
            .get_array("firstBatch")
            .unwrap();
        assert_eq!(batch.len(), 1);
        assert_eq!(
            batch[0].as_document().unwrap().get_str("name"),
            Ok("bob")
        );
    }

    #[tokio::test]
    async fn find_by_gt_and_or() {
        let registry = CollectionRegistry::new(Arc::new(InMemory::new()), "find-op-test");
        let cursors = CursorRegistry::new();
        for (id, name, score) in [
            ("a", "alice", 10),
            ("b", "bob", 20),
            ("c", "carol", 30),
        ] {
            registry
                .insert(
                    "test",
                    "users",
                    doc! { "_id": id, "name": name, "score": score },
                )
                .await
                .unwrap();
        }

        let cmd = FindCmd::from_document(doc! {
            "find": "users",
            "$db": "test",
            "filter": {
                "$or": [
                    { "score": { "$gt": 25 } },
                    { "name": "alice" }
                ]
            }
        })
        .unwrap();

        let body = cmd.execute(&registry, &cursors).await.unwrap();
        let batch = body
            .get_document("cursor")
            .unwrap()
            .get_array("firstBatch")
            .unwrap();
        let names: Vec<&str> = batch
            .iter()
            .map(|b| b.as_document().unwrap().get_str("name").unwrap())
            .collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"alice"));
        assert!(names.contains(&"carol"));
    }
}

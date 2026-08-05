use bson::{doc, Bson, Document};

use crate::commands::filter::QueryFilter;
use crate::error::{Error, Result};
use crate::storage::CollectionRegistry;

/// Parsed write commands per docs/mongodb-wire-protocol.md#command-dispatch.
#[derive(Debug, Clone, PartialEq)]
pub enum WriteCommand {
    Insert(InsertCmd),
    Update(UpdateCmd),
    Delete(DeleteCmd),
}

#[derive(Debug, Clone, PartialEq)]
pub struct InsertCmd {
    pub db: String,
    pub collection: String,
    pub documents: Vec<Document>,
    pub ordered: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UpdateOp {
    pub query: Document,
    pub update: Document,
    pub upsert: bool,
    pub multi: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UpdateCmd {
    pub db: String,
    pub collection: String,
    pub updates: Vec<UpdateOp>,
    pub ordered: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeleteOp {
    pub query: Document,
    pub limit: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeleteCmd {
    pub db: String,
    pub collection: String,
    pub deletes: Vec<DeleteOp>,
    pub ordered: bool,
}

impl WriteCommand {
    pub fn from_document(doc: Document) -> Result<Self> {
        if doc.contains_key("insert") {
            return Ok(Self::Insert(InsertCmd::from_document(doc)?));
        }
        if doc.contains_key("update") {
            return Ok(Self::Update(UpdateCmd::from_document(doc)?));
        }
        if doc.contains_key("delete") {
            return Ok(Self::Delete(DeleteCmd::from_document(doc)?));
        }

        Err(Error::CommandParse(
            "document is not a write command".into(),
        ))
    }
}

impl InsertCmd {
    fn from_document(doc: Document) -> Result<Self> {
        let db = get_db(&doc)?;
        let collection = get_string(&doc, "insert")?;
        let documents = get_document_array(&doc, "documents")?;
        let ordered = get_bool(&doc, "ordered").unwrap_or(true);

        Ok(Self {
            db,
            collection,
            documents,
            ordered,
        })
    }
}

impl UpdateCmd {
    fn from_document(doc: Document) -> Result<Self> {
        let db = get_db(&doc)?;
        let collection = get_string(&doc, "update")?;
        let updates = get_update_ops(&doc)?;
        let ordered = get_bool(&doc, "ordered").unwrap_or(true);

        Ok(Self {
            db,
            collection,
            updates,
            ordered,
        })
    }

    pub async fn execute(&self, registry: &CollectionRegistry) -> Result<Document> {
        let mut n = 0i32;
        let mut n_modified = 0i32;
        let mut upserted = Vec::new();

        for (index, op) in self.updates.iter().enumerate() {
            let set_fields = parse_set_update(&op.update)?;
            let filter = QueryFilter::from_query(&op.query)?;
            let limit = if op.multi { None } else { Some(1) };
            let matches = filter
                .collect(registry, &self.db, &self.collection, limit)
                .await?;

            if matches.is_empty() {
                if op.upsert {
                    let QueryFilter::ById(id) = &filter else {
                        return Err(Error::CommandParse(
                            "upsert requires an '_id' equality query".into(),
                        ));
                    };
                    let mut doc = Document::new();
                    doc.insert("_id", id.clone());
                    apply_set(&mut doc, &set_fields)?;
                    registry.put(&self.db, &self.collection, doc).await?;
                    n += 1;
                    upserted.push(doc! {
                        "index": index as i32,
                        "_id": id.clone(),
                    });
                }
                continue;
            }

            for mut doc in matches {
                n += 1;
                let before = doc.clone();
                apply_set(&mut doc, &set_fields)?;
                if doc != before {
                    n_modified += 1;
                }
                registry.put(&self.db, &self.collection, doc).await?;
            }
        }

        let mut body = doc! {
            "ok": 1.0,
            "n": n,
            "nModified": n_modified,
        };
        if !upserted.is_empty() {
            body.insert("upserted", Bson::Array(upserted.into_iter().map(Bson::Document).collect()));
        }
        Ok(body)
    }
}

impl DeleteCmd {
    fn from_document(doc: Document) -> Result<Self> {
        let db = get_db(&doc)?;
        let collection = get_string(&doc, "delete")?;
        let deletes = get_delete_ops(&doc)?;
        let ordered = get_bool(&doc, "ordered").unwrap_or(true);

        Ok(Self {
            db,
            collection,
            deletes,
            ordered,
        })
    }

    pub async fn execute(&self, registry: &CollectionRegistry) -> Result<Document> {
        let mut n = 0i32;

        for op in &self.deletes {
            let filter = QueryFilter::from_query(&op.query)?;
            let limit = match op.limit {
                0 => None,
                1 => Some(1),
                other => {
                    return Err(Error::CommandParse(format!(
                        "unsupported delete limit: {other} (expected 0 or 1)"
                    )));
                }
            };
            let matches = filter
                .collect(registry, &self.db, &self.collection, limit)
                .await?;

            for doc in matches {
                let Some(id) = doc.get("_id") else {
                    return Err(Error::Storage(
                        "document missing _id during delete".into(),
                    ));
                };
                registry.delete(&self.db, &self.collection, id).await?;
                n += 1;
            }
        }

        Ok(doc! {
            "ok": 1.0,
            "n": n,
        })
    }
}

/// Extract `$set` fields; reject replacement docs and other operators.
fn parse_set_update(update: &Document) -> Result<Document> {
    if update.is_empty() {
        return Err(Error::CommandParse(
            "update document must contain '$set'".into(),
        ));
    }

    let mut set_fields = None;
    for (key, value) in update.iter() {
        if key.starts_with('$') {
            if key != "$set" {
                return Err(Error::CommandParse(format!(
                    "unsupported update operator: '{key}' (only '$set' is supported)"
                )));
            }
            let Bson::Document(fields) = value else {
                return Err(Error::CommandParse(
                    "field '$set' must be a document".into(),
                ));
            };
            set_fields = Some(fields.clone());
        } else {
            return Err(Error::CommandParse(
                "replacement updates are not supported; use '$set'".into(),
            ));
        }
    }

    set_fields.ok_or_else(|| {
        Error::CommandParse("update document must contain '$set'".into())
    })
}

fn apply_set(doc: &mut Document, set_fields: &Document) -> Result<()> {
    for (key, value) in set_fields.iter() {
        if key == "_id" {
            if doc.get("_id") != Some(value) {
                return Err(Error::CommandParse(
                    "modifying '_id' via '$set' is not allowed".into(),
                ));
            }
            continue;
        }
        doc.insert(key, value.clone());
    }
    Ok(())
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

fn get_bool(doc: &Document, key: &str) -> Option<bool> {
    match doc.get(key) {
        Some(Bson::Boolean(value)) => Some(*value),
        _ => None,
    }
}

fn get_document_array(doc: &Document, key: &str) -> Result<Vec<Document>> {
    let Some(value) = doc.get(key) else {
        return Err(Error::CommandParse(format!("missing field '{key}'")));
    };

    let Bson::Array(items) = value else {
        return Err(Error::CommandParse(format!(
            "field '{key}' must be an array"
        )));
    };

    items
        .iter()
        .map(|item| match item {
            Bson::Document(doc) => Ok(doc.clone()),
            _ => Err(Error::CommandParse(format!(
                "field '{key}' must contain documents"
            ))),
        })
        .collect()
}

fn get_update_ops(doc: &Document) -> Result<Vec<UpdateOp>> {
    let Some(value) = doc.get("updates") else {
        return Err(Error::CommandParse("missing field 'updates'".into()));
    };

    let Bson::Array(items) = value else {
        return Err(Error::CommandParse(
            "field 'updates' must be an array".into(),
        ));
    };

    items
        .iter()
        .map(|item| {
            let Bson::Document(update_doc) = item else {
                return Err(Error::CommandParse(
                    "field 'updates' must contain documents".into(),
                ));
            };

            let query = match update_doc.get("q") {
                Some(Bson::Document(doc)) => doc.clone(),
                _ => {
                    return Err(Error::CommandParse(
                        "update entry missing document field 'q'".into(),
                    ));
                }
            };

            let update = match update_doc.get("u") {
                Some(Bson::Document(doc)) => doc.clone(),
                _ => {
                    return Err(Error::CommandParse(
                        "update entry missing document field 'u'".into(),
                    ));
                }
            };

            Ok(UpdateOp {
                query,
                update,
                upsert: matches!(update_doc.get("upsert"), Some(Bson::Boolean(true))),
                multi: matches!(update_doc.get("multi"), Some(Bson::Boolean(true))),
            })
        })
        .collect()
}

fn get_delete_ops(doc: &Document) -> Result<Vec<DeleteOp>> {
    let Some(value) = doc.get("deletes") else {
        return Err(Error::CommandParse("missing field 'deletes'".into()));
    };

    let Bson::Array(items) = value else {
        return Err(Error::CommandParse(
            "field 'deletes' must be an array".into(),
        ));
    };

    items
        .iter()
        .map(|item| {
            let Bson::Document(delete_doc) = item else {
                return Err(Error::CommandParse(
                    "field 'deletes' must contain documents".into(),
                ));
            };

            let query = match delete_doc.get("q") {
                Some(Bson::Document(doc)) => doc.clone(),
                _ => {
                    return Err(Error::CommandParse(
                        "delete entry missing document field 'q'".into(),
                    ));
                }
            };

            let limit = match delete_doc.get("limit") {
                Some(Bson::Int32(value)) => *value,
                Some(Bson::Int64(value)) => i32::try_from(*value).map_err(|_| {
                    Error::CommandParse("delete limit out of range".into())
                })?,
                _ => 0,
            };

            Ok(DeleteOp { query, limit })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bson::doc;
    use slatedb::object_store::memory::InMemory;
    use std::sync::Arc;

    fn registry() -> CollectionRegistry {
        CollectionRegistry::new(Arc::new(InMemory::new()), "write-test")
    }

    #[test]
    fn parses_insert_command() {
        let cmd = WriteCommand::from_document(doc! {
            "insert": "users",
            "$db": "test",
            "documents": [{ "name": "alice" }],
            "ordered": true
        })
        .unwrap();

        assert_eq!(
            cmd,
            WriteCommand::Insert(InsertCmd {
                db: "test".into(),
                collection: "users".into(),
                documents: vec![doc! { "name": "alice" }],
                ordered: true,
            })
        );
    }

    #[test]
    fn parses_update_command() {
        let cmd = WriteCommand::from_document(doc! {
            "update": "users",
            "$db": "test",
            "updates": [{
                "q": { "_id": "alice" },
                "u": { "$set": { "active": true } },
                "upsert": false,
                "multi": false
            }]
        })
        .unwrap();

        assert!(matches!(cmd, WriteCommand::Update(_)));
    }

    #[test]
    fn parses_delete_command() {
        let cmd = WriteCommand::from_document(doc! {
            "delete": "users",
            "$db": "test",
            "deletes": [{
                "q": { "_id": "alice" },
                "limit": 1
            }]
        })
        .unwrap();

        assert!(matches!(cmd, WriteCommand::Delete(_)));
    }

    #[test]
    fn rejects_unsupported_update_operator() {
        let err = parse_set_update(&doc! { "$inc": { "score": 1 } }).unwrap_err();
        assert!(err.to_string().contains("unsupported update operator"));
    }

    #[test]
    fn rejects_replacement_update() {
        let err = parse_set_update(&doc! { "name": "bob" }).unwrap_err();
        assert!(err.to_string().contains("replacement updates are not supported"));
    }

    #[tokio::test]
    async fn update_one_by_id_sets_fields() {
        let registry = registry();
        registry
            .insert("test", "users", doc! { "_id": "alice", "score": 10 })
            .await
            .unwrap();

        let cmd = UpdateCmd::from_document(doc! {
            "update": "users",
            "$db": "test",
            "updates": [{
                "q": { "_id": "alice" },
                "u": { "$set": { "score": 99 } },
                "multi": false
            }]
        })
        .unwrap();

        let body = cmd.execute(&registry).await.unwrap();
        assert_eq!(body.get_i32("n"), Ok(1));
        assert_eq!(body.get_i32("nModified"), Ok(1));

        let doc = registry
            .get("test", "users", &Bson::String("alice".into()))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(doc.get_i32("score"), Ok(99));
    }

    #[tokio::test]
    async fn update_multi_empty_filter() {
        let registry = registry();
        registry
            .insert("test", "users", doc! { "_id": "a", "active": false })
            .await
            .unwrap();
        registry
            .insert("test", "users", doc! { "_id": "b", "active": false })
            .await
            .unwrap();

        let cmd = UpdateCmd::from_document(doc! {
            "update": "users",
            "$db": "test",
            "updates": [{
                "q": {},
                "u": { "$set": { "active": true } },
                "multi": true
            }]
        })
        .unwrap();

        let body = cmd.execute(&registry).await.unwrap();
        assert_eq!(body.get_i32("n"), Ok(2));
        assert_eq!(body.get_i32("nModified"), Ok(2));
    }

    #[tokio::test]
    async fn upsert_by_id_inserts_document() {
        let registry = registry();
        let cmd = UpdateCmd::from_document(doc! {
            "update": "users",
            "$db": "test",
            "updates": [{
                "q": { "_id": "carol" },
                "u": { "$set": { "score": 5 } },
                "upsert": true,
                "multi": false
            }]
        })
        .unwrap();

        let body = cmd.execute(&registry).await.unwrap();
        assert_eq!(body.get_i32("n"), Ok(1));
        assert_eq!(body.get_i32("nModified"), Ok(0));
        let upserted = body.get_array("upserted").unwrap();
        assert_eq!(upserted.len(), 1);

        let doc = registry
            .get("test", "users", &Bson::String("carol".into()))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(doc.get_i32("score"), Ok(5));
    }

    #[tokio::test]
    async fn delete_one_by_id() {
        let registry = registry();
        registry
            .insert("test", "users", doc! { "_id": "alice", "name": "Alice" })
            .await
            .unwrap();

        let cmd = DeleteCmd::from_document(doc! {
            "delete": "users",
            "$db": "test",
            "deletes": [{
                "q": { "_id": "alice" },
                "limit": 1
            }]
        })
        .unwrap();

        let body = cmd.execute(&registry).await.unwrap();
        assert_eq!(body.get_i32("n"), Ok(1));
        assert!(registry
            .get("test", "users", &Bson::String("alice".into()))
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn delete_many_empty_filter() {
        let registry = registry();
        registry
            .insert("test", "users", doc! { "_id": "a" })
            .await
            .unwrap();
        registry
            .insert("test", "users", doc! { "_id": "b" })
            .await
            .unwrap();

        let cmd = DeleteCmd::from_document(doc! {
            "delete": "users",
            "$db": "test",
            "deletes": [{
                "q": {},
                "limit": 0
            }]
        })
        .unwrap();

        let body = cmd.execute(&registry).await.unwrap();
        assert_eq!(body.get_i32("n"), Ok(2));
        assert!(registry.scan("test", "users", None).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn rejects_unsupported_field_operator_query() {
        let registry = registry();
        let cmd = UpdateCmd::from_document(doc! {
            "update": "users",
            "$db": "test",
            "updates": [{
                "q": { "score": { "$mod": [2, 0] } },
                "u": { "$set": { "active": true } }
            }]
        })
        .unwrap();

        let err = cmd.execute(&registry).await.unwrap_err();
        assert!(err.to_string().contains("unsupported field operator"));
    }

    #[tokio::test]
    async fn update_by_gt_operator() {
        let registry = registry();
        registry
            .insert("test", "users", doc! { "_id": "a", "score": 10 })
            .await
            .unwrap();
        registry
            .insert("test", "users", doc! { "_id": "b", "score": 30 })
            .await
            .unwrap();

        let cmd = UpdateCmd::from_document(doc! {
            "update": "users",
            "$db": "test",
            "updates": [{
                "q": { "score": { "$gt": 20 } },
                "u": { "$set": { "high": true } },
                "multi": true
            }]
        })
        .unwrap();

        let body = cmd.execute(&registry).await.unwrap();
        assert_eq!(body.get_i32("n"), Ok(1));
        let doc = registry
            .get("test", "users", &Bson::String("b".into()))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(doc.get_bool("high"), Ok(true));
    }

    #[tokio::test]
    async fn update_one_by_field_equality() {
        let registry = registry();
        registry
            .insert("test", "users", doc! { "_id": "a", "name": "alice", "score": 10 })
            .await
            .unwrap();
        registry
            .insert("test", "users", doc! { "_id": "b", "name": "bob", "score": 20 })
            .await
            .unwrap();

        let cmd = UpdateCmd::from_document(doc! {
            "update": "users",
            "$db": "test",
            "updates": [{
                "q": { "name": "bob" },
                "u": { "$set": { "score": 21 } },
                "multi": false
            }]
        })
        .unwrap();

        let body = cmd.execute(&registry).await.unwrap();
        assert_eq!(body.get_i32("n"), Ok(1));
        assert_eq!(body.get_i32("nModified"), Ok(1));

        let doc = registry
            .get("test", "users", &Bson::String("b".into()))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(doc.get_i32("score"), Ok(21));
    }

    #[tokio::test]
    async fn delete_one_by_field_equality() {
        let registry = registry();
        registry
            .insert("test", "users", doc! { "_id": "a", "name": "alice" })
            .await
            .unwrap();
        registry
            .insert("test", "users", doc! { "_id": "b", "name": "bob" })
            .await
            .unwrap();

        let cmd = DeleteCmd::from_document(doc! {
            "delete": "users",
            "$db": "test",
            "deletes": [{
                "q": { "name": "alice" },
                "limit": 1
            }]
        })
        .unwrap();

        let body = cmd.execute(&registry).await.unwrap();
        assert_eq!(body.get_i32("n"), Ok(1));
        assert!(registry
            .get("test", "users", &Bson::String("a".into()))
            .await
            .unwrap()
            .is_none());
        assert!(registry
            .get("test", "users", &Bson::String("b".into()))
            .await
            .unwrap()
            .is_some());
    }
}

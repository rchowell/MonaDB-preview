use bson::{Bson, Document};

use crate::cursor::{CursorRegistry, CursorState, DEFAULT_BATCH_SIZE};
use crate::error::{Error, Result};
use crate::storage::{scan_batch, CollectionRegistry};

/// Supported `find` filter shapes.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FindFilter {
    All,
    ById(Bson),
}

/// Parsed `find` command (empty filter or `_id` point lookup).
#[derive(Debug, Clone, PartialEq)]
pub struct FindCmd {
    pub db: String,
    pub collection: String,
    pub(crate) filter: FindFilter,
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

        let find_filter = if filter.is_empty() {
            FindFilter::All
        } else if filter.len() == 1 {
            match filter.get("_id") {
                Some(id) => FindFilter::ById(id.clone()),
                None => {
                    return Err(Error::CommandParse(
                        "unsupported find filter: only empty filter or '_id' equality is supported"
                            .into(),
                    ));
                }
            }
        } else {
            return Err(Error::CommandParse(
                "unsupported find filter: only empty filter or '_id' equality is supported".into(),
            ));
        };

        Ok(Self {
            db,
            collection,
            filter: find_filter,
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
            FindFilter::ById(id) => {
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
            FindFilter::All => {
                let snapshot = registry.snapshot(&self.db, &self.collection).await?;
                let batch = scan_batch(
                    &snapshot,
                    None,
                    self.skip,
                    self.batch_size,
                    self.limit,
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
        assert_eq!(cmd.filter, FindFilter::ById(Bson::String("alice".into())));
    }

    #[test]
    fn parses_empty_filter() {
        let cmd = FindCmd::from_document(doc! {
            "find": "users",
            "$db": "test",
            "filter": {}
        })
        .unwrap();

        assert_eq!(cmd.filter, FindFilter::All);
        assert_eq!(cmd.batch_size, DEFAULT_BATCH_SIZE);
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
    fn rejects_non_id_filter() {
        let err = FindCmd::from_document(doc! {
            "find": "users",
            "$db": "test",
            "filter": { "name": "alice" }
        })
        .unwrap_err();
        assert!(err.to_string().contains("unsupported find filter"));
    }
}

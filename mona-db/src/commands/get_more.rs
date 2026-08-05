use bson::{Bson, Document};

use crate::commands::find::cursor_reply;
use crate::cursor::{CursorRegistry, DEFAULT_BATCH_SIZE};
use crate::error::{Error, Result};
use crate::storage::scan_batch;

pub fn cursor_not_found_body(cursor_id: i64) -> Document {
    bson::doc! {
        "ok": 0,
        "errmsg": format!("cursor id {cursor_id} not found"),
        "code": 43,
        "codeName": "CursorNotFound"
    }
}

/// Parsed `getMore` command.
#[derive(Debug, Clone, PartialEq)]
pub struct GetMoreCmd {
    pub cursor_id: i64,
    pub db: String,
    pub collection: String,
    pub batch_size: Option<i32>,
}

impl GetMoreCmd {
    pub fn from_document(doc: Document) -> Result<Self> {
        let db = get_db(&doc)?;
        let collection = get_string(&doc, "collection")?;

        let cursor_id = match doc.get("getMore") {
            Some(Bson::Int64(value)) => *value,
            Some(Bson::Int32(value)) => *value as i64,
            Some(Bson::Double(value)) => *value as i64,
            Some(_) => {
                return Err(Error::CommandParse(
                    "field 'getMore' must be a cursor id".into(),
                ));
            }
            None => {
                return Err(Error::CommandParse("missing field 'getMore'".into()));
            }
        };

        Ok(Self {
            cursor_id,
            db,
            collection,
            batch_size: parse_optional_batch_size(&doc)?,
        })
    }

    pub async fn execute(&self, cursors: &CursorRegistry) -> Result<Document> {
        let Some(state) = cursors.get(self.cursor_id).await else {
            return Ok(cursor_not_found_body(self.cursor_id));
        };

        if state.db != self.db || state.collection != self.collection {
            return Ok(cursor_not_found_body(self.cursor_id));
        }

        let batch_size = self
            .batch_size
            .unwrap_or(state.default_batch_size)
            .max(0);

        let batch = scan_batch(
            &state.snapshot,
            state.last_key.as_deref(),
            0,
            batch_size,
            state.limit_remaining,
            state.predicate.as_ref(),
        )
        .await?;

        let next_batch: Vec<Bson> = batch.docs.into_iter().map(Bson::Document).collect();

        if batch.exhausted {
            cursors.take(self.cursor_id).await;
            return Ok(cursor_reply(state.ns, 0, next_batch, false));
        }

        cursors
            .update(self.cursor_id, batch.last_key, batch.limit_remaining)
            .await;

        Ok(cursor_reply(state.ns, self.cursor_id, next_batch, false))
    }
}

fn parse_optional_batch_size(doc: &Document) -> Result<Option<i32>> {
    match doc.get("batchSize") {
        None => Ok(None),
        Some(Bson::Int32(value)) => Ok(Some(if *value == 0 {
            DEFAULT_BATCH_SIZE
        } else {
            *value
        })),
        Some(Bson::Int64(value)) => {
            let value = i32::try_from(*value)
                .map_err(|_| Error::CommandParse("getMore batchSize out of range".into()))?;
            Ok(Some(if value == 0 {
                DEFAULT_BATCH_SIZE
            } else {
                value
            }))
        }
        Some(Bson::Double(value)) => {
            let value = *value as i32;
            Ok(Some(if value == 0 {
                DEFAULT_BATCH_SIZE
            } else {
                value
            }))
        }
        Some(_) => Err(Error::CommandParse(
            "field 'batchSize' must be a number".into(),
        )),
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
    fn parses_get_more() {
        let cmd = GetMoreCmd::from_document(doc! {
            "getMore": 42_i64,
            "collection": "users",
            "$db": "test",
            "batchSize": 10
        })
        .unwrap();

        assert_eq!(cmd.cursor_id, 42);
        assert_eq!(cmd.collection, "users");
        assert_eq!(cmd.batch_size, Some(10));
    }
}

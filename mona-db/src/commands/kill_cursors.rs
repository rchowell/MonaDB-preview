use bson::{Bson, Document};

use crate::cursor::CursorRegistry;
use crate::error::{Error, Result};

/// Parsed `killCursors` command.
#[derive(Debug, Clone, PartialEq)]
pub struct KillCursorsCmd {
    pub db: String,
    pub collection: String,
    pub cursor_ids: Vec<i64>,
}

impl KillCursorsCmd {
    pub fn from_document(doc: Document) -> Result<Self> {
        let db = get_db(&doc)?;
        let collection = get_string(&doc, "killCursors")?;
        let cursor_ids = parse_cursor_ids(&doc)?;

        Ok(Self {
            db,
            collection,
            cursor_ids,
        })
    }

    pub async fn execute(&self, cursors: &CursorRegistry) -> Result<Document> {
        let (killed, not_found) = cursors.kill(&self.cursor_ids).await;

        let cursors_killed: Vec<Bson> = killed.into_iter().map(Bson::Int64).collect();
        let cursors_not_found: Vec<Bson> = not_found.into_iter().map(Bson::Int64).collect();

        Ok(bson::doc! {
            "ok": 1.0,
            "cursorsKilled": cursors_killed,
            "cursorsNotFound": cursors_not_found,
        })
    }
}

fn parse_cursor_ids(doc: &Document) -> Result<Vec<i64>> {
    let Some(value) = doc.get("cursors") else {
        return Err(Error::CommandParse("missing field 'cursors'".into()));
    };

    let Bson::Array(items) = value else {
        return Err(Error::CommandParse("field 'cursors' must be an array".into()));
    };

    items
        .iter()
        .map(|item| match item {
            Bson::Int64(value) => Ok(*value),
            Bson::Int32(value) => Ok(*value as i64),
            Bson::Double(value) => Ok(*value as i64),
            _ => Err(Error::CommandParse(
                "field 'cursors' must contain numeric cursor ids".into(),
            )),
        })
        .collect()
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
    fn parses_kill_cursors() {
        let cmd = KillCursorsCmd::from_document(doc! {
            "killCursors": "users",
            "$db": "test",
            "cursors": [1_i64, 2_i64]
        })
        .unwrap();

        assert_eq!(cmd.collection, "users");
        assert_eq!(cmd.cursor_ids, vec![1, 2]);
    }
}

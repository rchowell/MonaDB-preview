use bson::{Bson, Document};

use crate::error::{Result, Error};

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
                "q": { "name": "alice" },
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
                "q": { "name": "alice" },
                "limit": 1
            }]
        })
        .unwrap();

        assert!(matches!(cmd, WriteCommand::Delete(_)));
    }
}

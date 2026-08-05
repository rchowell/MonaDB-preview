pub mod find;
pub mod get_more;
pub mod kill_cursors;
pub mod write;

use bson::{doc, Document};

use crate::cursor::CursorRegistry;
use crate::error::{Error, Result};
use crate::storage::CollectionRegistry;
use crate::wire::{Message, MsgHeader, OpMsg, OpReply, Response, Section, OP_MSG};

/// Build a command-not-found style error body.
pub fn command_not_found_body(command_name: &str) -> Document {
    doc! {
        "ok": 0,
        "errmsg": format!("no such command: '{command_name}'"),
        "code": 59,
        "codeName": "CommandNotFound"
    }
}

/// Build the `hello` handshake body per docs/mongodb-wire-protocol.md#maxwireversion.
pub fn hello_body() -> Document {
    doc! {
        "ok": 1.0,
        "helloOk": true,
        "ismaster": true,
        "maxBsonObjectSize": 16_777_216,
        "maxMessageSizeBytes": 48_000_000,
        "maxWriteBatchSize": 100_000,
        "maxWireVersion": 21,
        "minWireVersion": 0,
        "readOnly": false,
    }
}

/// Build a write-command acknowledgment body.
pub fn write_body(n: i32) -> Document {
    doc! {
        "ok": 1.0,
        "n": n
    }
}

/// Dispatch a decoded command document to a response body.
pub async fn dispatch_body(
    command: &Document,
    registry: &CollectionRegistry,
    cursors: &CursorRegistry,
) -> Result<Option<Document>> {
    if command.contains_key("hello")
        || command.contains_key("isMaster")
        || command.contains_key("ismaster")
    {
        return Ok(Some(hello_body()));
    }

    if command.contains_key("ping") {
        return Ok(Some(doc! { "ok": 1.0 }));
    }

    if command.contains_key("find") {
        let find_cmd = find::FindCmd::from_document(command.clone())?;
        return Ok(Some(find_cmd.execute(registry, cursors).await?));
    }

    if command.contains_key("getMore") {
        let get_more_cmd = get_more::GetMoreCmd::from_document(command.clone())?;
        return Ok(Some(get_more_cmd.execute(cursors).await?));
    }

    if command.contains_key("killCursors") {
        let kill_cmd = kill_cursors::KillCursorsCmd::from_document(command.clone())?;
        return Ok(Some(kill_cmd.execute(cursors).await?));
    }

    if let Ok(write_cmd) = write::WriteCommand::from_document(command.clone()) {
        match write_cmd {
            write::WriteCommand::Insert(cmd) => {
                let count = cmd.documents.len();
                for document in cmd.documents {
                    registry.insert(&cmd.db, &cmd.collection, document).await?;
                }
                return Ok(Some(write_body(count as i32)));
            }
            write::WriteCommand::Update(_) | write::WriteCommand::Delete(_) => {
                return Err(Error::CommandParse(
                    "update and delete are not supported yet".into(),
                ));
            }
        }
    }

    if let Some(name) = command.keys().next() {
        return Ok(Some(command_not_found_body(name)));
    }

    Ok(None)
}

/// Dispatch a decoded command document to a wire response.
pub async fn dispatch(
    request_id: i32,
    command: &Document,
    format: crate::wire::ResponseFormat,
    registry: &CollectionRegistry,
    cursors: &CursorRegistry,
) -> Result<Option<Response>> {
    let Some(body) = dispatch_body(command, registry, cursors).await? else {
        return Ok(None);
    };

    Ok(Some(match format {
        crate::wire::ResponseFormat::Msg => Response::Msg(Message::Msg {
            header: MsgHeader {
                message_length: 0,
                request_id: 0,
                response_to: request_id,
                op_code: OP_MSG,
            },
            body: OpMsg {
                flag_bits: 0,
                sections: vec![Section::Body(body)],
            },
        }),
        crate::wire::ResponseFormat::Reply => Response::Reply(OpReply::new(request_id, body)),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use slatedb::object_store::memory::InMemory;
    use std::sync::Arc;

    fn registry() -> CollectionRegistry {
        CollectionRegistry::new(Arc::new(InMemory::new()), "dispatch-test")
    }

    fn cursors() -> CursorRegistry {
        CursorRegistry::new()
    }

    async fn dispatch_body_doc(
        command: &Document,
        registry: &CollectionRegistry,
        cursors: &CursorRegistry,
    ) -> Document {
        dispatch_body(command, registry, cursors)
            .await
            .unwrap()
            .unwrap()
    }

    async fn insert_docs(registry: &CollectionRegistry, docs: Vec<Document>) {
        dispatch(
            1,
            &doc! {
                "insert": "users",
                "$db": "test",
                "documents": docs
            },
            crate::wire::ResponseFormat::Msg,
            registry,
            &cursors(),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn dispatches_hello() {
        let reply = dispatch(
            7,
            &doc! { "hello": 1, "$db": "admin" },
            crate::wire::ResponseFormat::Msg,
            &registry(),
            &cursors(),
        )
        .await
        .unwrap()
        .unwrap();
        let Response::Msg(message) = reply else {
            panic!("expected OP_MSG response");
        };
        let body = message.command_document().expect("response body");
        assert_eq!(body.get_f64("ok"), Ok(1.0));
        assert_eq!(body.get_i32("maxWireVersion"), Ok(21));
    }

    #[tokio::test]
    async fn dispatches_insert() {
        let registry = registry();
        let reply = dispatch(
            3,
            &doc! {
                "insert": "users",
                "$db": "test",
                "documents": [{ "x": 1 }, { "x": 2 }]
            },
            crate::wire::ResponseFormat::Msg,
            &registry,
            &cursors(),
        )
        .await
        .unwrap()
        .unwrap();

        let Response::Msg(message) = reply else {
            panic!("expected OP_MSG response");
        };
        let body = message.command_document().expect("response body");
        assert_eq!(body.get_i32("n"), Ok(2));
    }

    #[tokio::test]
    async fn dispatches_find_by_id() {
        let registry = registry();
        let cursors = cursors();
        dispatch(
            1,
            &doc! {
                "insert": "users",
                "$db": "test",
                "documents": [{ "_id": "alice", "name": "Alice" }]
            },
            crate::wire::ResponseFormat::Msg,
            &registry,
            &cursors,
        )
        .await
        .unwrap();

        let body = dispatch_body_doc(
            &doc! {
                "find": "users",
                "$db": "test",
                "filter": { "_id": "alice" }
            },
            &registry,
            &cursors,
        )
        .await;

        let cursor = body.get_document("cursor").unwrap();
        assert_eq!(cursor.get_i64("id"), Ok(0));
        let batch = cursor.get_array("firstBatch").unwrap();
        assert_eq!(batch.len(), 1);
        let doc = batch[0].as_document().unwrap();
        assert_eq!(doc.get_str("name"), Ok("Alice"));
    }

    #[tokio::test]
    async fn find_batches_with_cursor_and_get_more() {
        let registry = registry();
        let cursors = cursors();
        insert_docs(
            &registry,
            vec![
                doc! { "n": 1 },
                doc! { "n": 2 },
                doc! { "n": 3 },
                doc! { "n": 4 },
                doc! { "n": 5 },
            ],
        )
        .await;

        let first = dispatch_body_doc(
            &doc! {
                "find": "users",
                "$db": "test",
                "filter": {},
                "batchSize": 2
            },
            &registry,
            &cursors,
        )
        .await;
        let cursor = first.get_document("cursor").unwrap();
        let first_batch = cursor.get_array("firstBatch").unwrap();
        assert_eq!(first_batch.len(), 2);
        let cursor_id = cursor.get_i64("id").unwrap();
        assert_ne!(cursor_id, 0);

        let second = dispatch_body_doc(
            &doc! {
                "getMore": cursor_id,
                "collection": "users",
                "$db": "test",
                "batchSize": 2
            },
            &registry,
            &cursors,
        )
        .await;
        let cursor = second.get_document("cursor").unwrap();
        let next_batch = cursor.get_array("nextBatch").unwrap();
        assert_eq!(next_batch.len(), 2);
        assert_eq!(cursor.get_i64("id").unwrap(), cursor_id);

        let third = dispatch_body_doc(
            &doc! {
                "getMore": cursor_id,
                "collection": "users",
                "$db": "test",
                "batchSize": 2
            },
            &registry,
            &cursors,
        )
        .await;
        let cursor = third.get_document("cursor").unwrap();
        let next_batch = cursor.get_array("nextBatch").unwrap();
        assert_eq!(next_batch.len(), 1);
        assert_eq!(cursor.get_i64("id"), Ok(0));
    }

    #[tokio::test]
    async fn find_respects_limit_with_batches() {
        let registry = registry();
        let cursors = cursors();
        insert_docs(
            &registry,
            vec![
                doc! { "n": 1 },
                doc! { "n": 2 },
                doc! { "n": 3 },
                doc! { "n": 4 },
                doc! { "n": 5 },
            ],
        )
        .await;

        let first = dispatch_body_doc(
            &doc! {
                "find": "users",
                "$db": "test",
                "filter": {},
                "batchSize": 3,
                "limit": 4
            },
            &registry,
            &cursors,
        )
        .await;
        let cursor = first.get_document("cursor").unwrap();
        assert_eq!(cursor.get_array("firstBatch").unwrap().len(), 3);
        let cursor_id = cursor.get_i64("id").unwrap();
        assert_ne!(cursor_id, 0);

        let second = dispatch_body_doc(
            &doc! {
                "getMore": cursor_id,
                "collection": "users",
                "$db": "test",
                "batchSize": 1
            },
            &registry,
            &cursors,
        )
        .await;
        let cursor = second.get_document("cursor").unwrap();
        assert_eq!(cursor.get_array("nextBatch").unwrap().len(), 1);
        assert_eq!(cursor.get_i64("id"), Ok(0));
    }

    #[tokio::test]
    async fn kill_cursors_prevents_get_more() {
        let registry = registry();
        let cursors = cursors();
        insert_docs(
            &registry,
            vec![doc! { "n": 1 }, doc! { "n": 2 }, doc! { "n": 3 }],
        )
        .await;

        let first = dispatch_body_doc(
            &doc! {
                "find": "users",
                "$db": "test",
                "filter": {},
                "batchSize": 1
            },
            &registry,
            &cursors,
        )
        .await;
        let cursor_id = first.get_document("cursor").unwrap().get_i64("id").unwrap();

        let killed = dispatch_body_doc(
            &doc! {
                "killCursors": "users",
                "$db": "test",
                "cursors": [cursor_id]
            },
            &registry,
            &cursors,
        )
        .await;
        assert_eq!(killed.get_array("cursorsKilled").unwrap().len(), 1);

        let err = dispatch_body_doc(
            &doc! {
                "getMore": cursor_id,
                "collection": "users",
                "$db": "test"
            },
            &registry,
            &cursors,
        )
        .await;
        assert_eq!(err.get_i32("code"), Ok(43));
    }
}

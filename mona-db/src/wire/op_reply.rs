use bson::Document;
use bytes::{BufMut, BytesMut};

use crate::wire::header::MsgHeader;
use crate::wire::opcodes::OP_REPLY;

/// Legacy OP_REPLY per docs/mongodb-wire-protocol.md#op_reply-1.
#[derive(Debug, Clone, PartialEq)]
pub struct OpReply {
    pub request_id: i32,
    pub response_to: i32,
    pub documents: Vec<Document>,
}

impl OpReply {
    pub fn new(response_to: i32, document: Document) -> Self {
        Self {
            request_id: 0,
            response_to,
            documents: vec![document],
        }
    }

    pub fn encode(&self) -> BytesMut {
        let mut payload = BytesMut::new();
        payload.put_i32_le(0); // responseFlags
        payload.put_i64_le(0); // cursorID
        payload.put_i32_le(0); // startingFrom
        payload.put_i32_le(self.documents.len() as i32);

        for document in &self.documents {
            payload.put_slice(&bson::to_vec(document).expect("document should encode"));
        }

        let mut buf = BytesMut::with_capacity(MsgHeader::SIZE + payload.len());
        let header = MsgHeader {
            message_length: (MsgHeader::SIZE + payload.len()) as i32,
            request_id: self.request_id,
            response_to: self.response_to,
            op_code: OP_REPLY,
        };
        header.encode(&mut buf);
        buf.put(payload);
        buf
    }
}

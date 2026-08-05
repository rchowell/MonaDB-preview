pub mod bson_io;
pub mod header;
pub mod op_compressed;
pub mod op_msg;
pub mod op_query;
pub mod op_reply;
pub mod opcodes;

use bytes::{Buf, BufMut, BytesMut};

use crate::error::{Error, Result};

pub use header::MsgHeader;
pub use op_msg::{OpMsg, Section};
pub use op_query::OpQuery;
pub use op_reply::OpReply;
pub use opcodes::{OP_COMPRESSED, OP_MSG, OP_QUERY};

/// A fully decoded wire message.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    Msg {
        header: MsgHeader,
        body: OpMsg,
    },
    Query {
        header: MsgHeader,
        body: OpQuery,
    },
}

/// Encoded response for a client request.
#[derive(Debug, Clone, PartialEq)]
pub enum Response {
    Msg(Message),
    Reply(OpReply),
}

impl Message {
    /// Decode a complete message from a buffer that contains exactly one frame.
    pub fn decode(mut data: BytesMut) -> Result<Self> {
        let header = MsgHeader::decode(&mut data)?;
        match header.op_code {
            OP_MSG => {
                if data.remaining() < 4 {
                    return Err(Error::Incomplete {
                        needed: 4,
                        available: data.remaining(),
                    });
                }
                let flag_bits = data.get_u32_le();
                let body = OpMsg::decode(&mut data, flag_bits)?;
                Ok(Message::Msg { header, body })
            }
            OP_QUERY => {
                let body = OpQuery::decode(&mut data)?;
                Ok(Message::Query { header, body })
            }
            OP_COMPRESSED => Err(Error::UnsupportedOpcode(OP_COMPRESSED)),
            opcode => Err(Error::UnsupportedOpcode(opcode)),
        }
    }

    pub fn encode_msg(header: MsgHeader, body: OpMsg) -> BytesMut {
        let mut payload = BytesMut::new();
        body.encode(&mut payload);

        let mut buf = BytesMut::with_capacity(MsgHeader::SIZE + payload.len());
        let encoded_header = MsgHeader {
            message_length: (MsgHeader::SIZE + payload.len()) as i32,
            request_id: header.request_id,
            response_to: header.response_to,
            op_code: header.op_code,
        };
        encoded_header.encode(&mut buf);
        buf.put(payload);
        buf
    }

    pub fn request_id(&self) -> i32 {
        match self {
            Message::Msg { header, .. } | Message::Query { header, .. } => header.request_id,
        }
    }

    pub fn command_document(&self) -> Option<bson::Document> {
        match self {
            Message::Msg { body, .. } => body.body_document(),
            Message::Query { body, .. } => Some(body.query.clone()),
        }
    }

    pub fn more_to_come(&self) -> bool {
        match self {
            Message::Msg { body, .. } => body.more_to_come(),
            Message::Query { .. } => false,
        }
    }

    pub fn expects_reply(&self) -> ResponseFormat {
        match self {
            Message::Msg { .. } => ResponseFormat::Msg,
            Message::Query { .. } => ResponseFormat::Reply,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseFormat {
    Msg,
    Reply,
}

impl Response {
    pub fn encode(&self) -> BytesMut {
        match self {
            Response::Msg(message) => match message {
                Message::Msg { header, body } => Message::encode_msg(*header, body.clone()),
                Message::Query { .. } => {
                    panic!("OP_QUERY cannot be encoded as a response");
                }
            },
            Response::Reply(reply) => reply.encode(),
        }
    }
}

/// Returns how many bytes are needed to read a complete message, or None if incomplete.
pub fn frame_length_available(buf: &BytesMut) -> Result<Option<usize>> {
    if buf.len() < 4 {
        return Ok(None);
    }

    let message_length = i32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
    if message_length < MsgHeader::SIZE as i32 {
        return Err(Error::InvalidMessageLength(message_length));
    }

    let message_length = message_length as usize;
    if buf.len() < message_length {
        return Ok(None);
    }

    Ok(Some(message_length))
}

/// Split one complete message frame from the front of `buf`.
pub fn take_frame(buf: &mut BytesMut) -> Result<Option<BytesMut>> {
    let Some(message_length) = frame_length_available(buf)? else {
        return Ok(None);
    };

    Ok(Some(buf.split_to(message_length)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bson::doc;
    use op_msg::Section;

    fn sample_message() -> Message {
        Message::Msg {
            header: MsgHeader {
                message_length: 0,
                request_id: 42,
                response_to: 0,
                op_code: OP_MSG,
            },
            body: OpMsg {
                flag_bits: 0,
                sections: vec![Section::Body(doc! { "ping": 1, "$db": "admin" })],
            },
        }
    }

    #[test]
    fn message_round_trip() {
        let original = sample_message();
        let Message::Msg { header, body } = original.clone() else {
            panic!("expected OP_MSG");
        };
        let encoded = Message::encode_msg(header, body);
        let decoded = Message::decode(encoded).unwrap();
        assert_eq!(decoded.request_id(), 42);
        assert_eq!(
            decoded.command_document(),
            original.command_document()
        );
    }

    #[test]
    fn take_frame_waits_for_full_message() {
        let Message::Msg { header, body } = sample_message() else {
            panic!("expected OP_MSG");
        };
        let full = Message::encode_msg(header, body);
        let full_len = full.len();

        let mut partial = full.clone();
        partial.truncate(full_len - 1);
        assert!(take_frame(&mut partial).unwrap().is_none());

        let mut complete = full;
        let frame = take_frame(&mut complete).unwrap().unwrap();
        assert_eq!(frame.len(), full_len);
        assert!(complete.is_empty());
    }
}

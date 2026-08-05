use bytes::{Buf, BufMut, BytesMut};

use crate::error::{Result, Error};

/// Standard 16-byte message header per docs/mongodb-wire-protocol.md#standard-message-header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MsgHeader {
    pub message_length: i32,
    pub request_id: i32,
    pub response_to: i32,
    pub op_code: i32,
}

impl MsgHeader {
    pub const SIZE: usize = 16;

    pub fn decode(buf: &mut impl Buf) -> Result<Self> {
        if buf.remaining() < Self::SIZE {
            return Err(Error::Incomplete {
                needed: Self::SIZE,
                available: buf.remaining(),
            });
        }

        let message_length = buf.get_i32_le();
        if message_length < Self::SIZE as i32 {
            return Err(Error::InvalidMessageLength(message_length));
        }

        Ok(Self {
            message_length,
            request_id: buf.get_i32_le(),
            response_to: buf.get_i32_le(),
            op_code: buf.get_i32_le(),
        })
    }

    pub fn encode(&self, buf: &mut BytesMut) {
        buf.put_i32_le(self.message_length);
        buf.put_i32_le(self.request_id);
        buf.put_i32_le(self.response_to);
        buf.put_i32_le(self.op_code);
    }
}

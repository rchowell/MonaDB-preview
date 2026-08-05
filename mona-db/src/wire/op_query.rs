use bson::Document;
use bytes::Buf;

use crate::error::Result;
use crate::wire::bson_io::{read_bson_document, read_cstring};

/// Legacy OP_QUERY body per docs/mongodb-wire-protocol.md#op_query-2004.
#[derive(Debug, Clone, PartialEq)]
pub struct OpQuery {
    pub flags: i32,
    pub full_collection_name: String,
    pub number_to_skip: i32,
    pub number_to_return: i32,
    pub query: Document,
}

impl OpQuery {
    pub fn decode(mut body: impl Buf) -> Result<Self> {
        let flags = body.get_i32_le();
        let full_collection_name = read_cstring(&mut body)?;
        let number_to_skip = body.get_i32_le();
        let number_to_return = body.get_i32_le();
        let query = read_bson_document(&mut body)?;

        Ok(Self {
            flags,
            full_collection_name,
            number_to_skip,
            number_to_return,
            query,
        })
    }
}

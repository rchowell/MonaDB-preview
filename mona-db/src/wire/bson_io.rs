use bytes::Buf;

use crate::error::{Error, Result};

pub fn read_cstring(buf: &mut impl Buf) -> Result<String> {
    let mut bytes = Vec::new();
    while buf.has_remaining() {
        let byte = buf.get_u8();
        if byte == 0 {
            return String::from_utf8(bytes).map_err(|error| {
                Error::CommandParse(format!("invalid cstring utf-8: {error}"))
            });
        }
        bytes.push(byte);
    }

    Err(Error::Incomplete {
        needed: bytes.len() + 1,
        available: bytes.len(),
    })
}

pub fn read_bson_document(buf: &mut impl Buf) -> Result<bson::Document> {
    if buf.remaining() < 4 {
        return Err(Error::Incomplete {
            needed: 4,
            available: buf.remaining(),
        });
    }

    let doc_len = buf.get_i32_le();
    if doc_len < 5 {
        return Err(Error::CommandParse(format!(
            "invalid BSON document length: {doc_len}"
        )));
    }

    let doc_len = doc_len as usize;
    if buf.remaining() < doc_len - 4 {
        return Err(Error::Incomplete {
            needed: doc_len,
            available: buf.remaining() + 4,
        });
    }

    let mut doc_bytes = Vec::with_capacity(doc_len);
    doc_bytes.extend_from_slice(&(doc_len as i32).to_le_bytes());
    doc_bytes.extend_from_slice(&buf.copy_to_bytes(doc_len - 4));
    Ok(bson::Document::from_reader(&mut doc_bytes.as_slice())?)
}

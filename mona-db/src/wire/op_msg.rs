use bson::{Bson, Document};
use bytes::{Buf, BufMut, BytesMut};

use crate::error::{Error, Result};
use crate::wire::bson_io::{read_bson_document, read_cstring};

/// OP_MSG flag bits per docs/mongodb-wire-protocol.md#flag-bits.
pub const FLAG_CHECKSUM_PRESENT: u32 = 1 << 0;
pub const FLAG_MORE_TO_COME: u32 = 1 << 1;
#[allow(dead_code)]
pub const FLAG_EXHAUST_ALLOWED: u32 = 1 << 16;

const REQUIRED_FLAG_MASK: u32 = 0xFFFF;
const KNOWN_REQUIRED_FLAGS: u32 = FLAG_CHECKSUM_PRESENT | FLAG_MORE_TO_COME;

/// A parsed OP_MSG section.
#[derive(Debug, Clone, PartialEq)]
pub enum Section {
    /// Kind 0 — single BSON document (command body).
    Body(Document),
    /// Kind 1 — document sequence merged into the body at `identifier`.
    DocumentSequence {
        identifier: String,
        documents: Vec<Document>,
    },
}

/// Parsed OP_MSG message per docs/mongodb-wire-protocol.md#op_msg.
#[derive(Debug, Clone, PartialEq)]
pub struct OpMsg {
    pub flag_bits: u32,
    pub sections: Vec<Section>,
}

impl OpMsg {
    pub fn decode(body: &mut impl Buf, flag_bits: u32) -> Result<Self> {
        validate_required_flags(flag_bits)?;

        let mut sections = Vec::new();
        while body.has_remaining() {
            if body.remaining() == 4 && flag_bits & FLAG_CHECKSUM_PRESENT != 0 {
                // CRC-32C checksum deferred to phase 2; skip for now.
                body.advance(4);
                break;
            }

            let kind = body.get_u8();
            match kind {
                0 => {
                    let doc = read_bson_document(body)?;
                    sections.push(Section::Body(doc));
                }
                1 => sections.push(read_document_sequence(body)?),
                _ => return Err(Error::UnsupportedSectionKind(kind)),
            }
        }

        Ok(Self {
            flag_bits,
            sections,
        })
    }

    pub fn encode(&self, buf: &mut BytesMut) {
        buf.put_u32_le(self.flag_bits);
        for section in &self.sections {
            match section {
                Section::Body(doc) => {
                    buf.put_u8(0);
                    let encoded = bson::to_vec(doc).expect("document should encode");
                    buf.put_slice(&encoded);
                }
                Section::DocumentSequence {
                    identifier,
                    documents,
                } => {
                    buf.put_u8(1);
                    let mut payload = BytesMut::new();
                    payload.put_slice(identifier.as_bytes());
                    payload.put_u8(0);
                    for document in documents {
                        payload.put_slice(&bson::to_vec(document).expect("document should encode"));
                    }
                    buf.put_i32_le((payload.len() + 4) as i32);
                    buf.put_slice(&payload);
                }
            }
        }
    }

    pub fn body_document(&self) -> Option<Document> {
        merge_sections(&self.sections)
    }

    pub fn more_to_come(&self) -> bool {
        self.flag_bits & FLAG_MORE_TO_COME != 0
    }
}

fn read_document_sequence(body: &mut impl Buf) -> Result<Section> {
    if body.remaining() < 4 {
        return Err(Error::Incomplete {
            needed: 4,
            available: body.remaining(),
        });
    }

    let section_size = body.get_i32_le() as usize;
    if section_size < 5 {
        return Err(Error::CommandParse(format!(
            "invalid document sequence size: {section_size}"
        )));
    }

    if body.remaining() < section_size - 4 {
        return Err(Error::Incomplete {
            needed: section_size,
            available: body.remaining() + 4,
        });
    }

    let section_bytes = body.copy_to_bytes(section_size - 4);
    let mut section = section_bytes.as_ref();
    let identifier = read_cstring(&mut section)?;
    let mut documents = Vec::new();

    while section.has_remaining() {
        documents.push(read_bson_document(&mut section)?);
    }

    Ok(Section::DocumentSequence {
        identifier,
        documents,
    })
}

fn merge_sections(sections: &[Section]) -> Option<Document> {
    let mut body = sections.iter().find_map(|section| match section {
        Section::Body(doc) => Some(doc.clone()),
        Section::DocumentSequence { .. } => None,
    })?;

    for section in sections {
        if let Section::DocumentSequence {
            identifier,
            documents,
        } = section
        {
            body.insert(
                identifier.clone(),
                Bson::Array(documents.iter().cloned().map(Bson::Document).collect()),
            );
        }
    }

    Some(body)
}

fn validate_required_flags(flag_bits: u32) -> Result<()> {
    let required = flag_bits & REQUIRED_FLAG_MASK;
    let unknown = required & !KNOWN_REQUIRED_FLAGS;
    if unknown != 0 {
        return Err(Error::UnknownRequiredFlagBits(unknown));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bson::doc;

    #[test]
    fn round_trip_op_msg_with_body_section() {
        let original = OpMsg {
            flag_bits: 0,
            sections: vec![Section::Body(doc! { "ping": 1, "$db": "admin" })],
        };

        let mut encoded = BytesMut::new();
        original.encode(&mut encoded);

        let mut cursor = encoded.as_ref();
        let flag_bits = cursor.get_u32_le();
        let decoded = OpMsg::decode(&mut cursor, flag_bits).unwrap();

        assert_eq!(decoded, original);
    }

    #[test]
    fn merges_kind1_document_sequence_into_body() {
        let body = doc! { "insert": "users", "$db": "test" };
        let documents = vec![doc! { "name": "alice" }, doc! { "name": "bob" }];
        let mut encoded = BytesMut::new();
        encoded.put_u32_le(0);
        encoded.put_u8(0);
        encoded.put_slice(&bson::to_vec(&body).unwrap());
        encoded.put_u8(1);

        let mut sequence_payload = BytesMut::new();
        sequence_payload.put_slice(b"documents\0");
        for document in &documents {
            sequence_payload.put_slice(&bson::to_vec(document).unwrap());
        }
        encoded.put_i32_le((sequence_payload.len() + 4) as i32);
        encoded.put_slice(&sequence_payload);

        let mut cursor = encoded.as_ref();
        let flag_bits = cursor.get_u32_le();
        let decoded = OpMsg::decode(&mut cursor, flag_bits).unwrap();
        let merged = decoded.body_document().unwrap();

        assert_eq!(merged.get_str("insert"), Ok("users"));
        assert_eq!(merged.get_array("documents").unwrap().len(), 2);
    }

    #[test]
    fn rejects_unknown_required_flag_bits() {
        let mut buf = BytesMut::new();
        buf.put_u8(0);
        let doc = doc! { "ping": 1 };
        buf.put_slice(&bson::to_vec(&doc).unwrap());

        let err = OpMsg::decode(&mut buf.as_ref(), 1 << 2).unwrap_err();
        assert!(matches!(err, Error::UnknownRequiredFlagBits(0x0004)));
    }
}

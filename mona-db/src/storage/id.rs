use bson::{Bson, oid::ObjectId};

use crate::error::{Error, Result};

/// Canonical byte encoding for MongoDB `_id` values used as SlateDB keys.
pub fn encode_id(id: &Bson) -> Result<Vec<u8>> {
    match id {
        Bson::ObjectId(oid) => {
            let mut buf = vec![0x07];
            buf.extend_from_slice(&oid.bytes());
            Ok(buf)
        }
        Bson::String(s) => {
            let mut buf = vec![0x02];
            buf.extend_from_slice(s.as_bytes());
            Ok(buf)
        }
        Bson::Int32(n) => {
            let mut buf = vec![0x10];
            buf.extend_from_slice(&n.to_le_bytes());
            Ok(buf)
        }
        Bson::Int64(n) => {
            let mut buf = vec![0x12];
            buf.extend_from_slice(&n.to_le_bytes());
            Ok(buf)
        }
        Bson::Binary(binary) => {
            let mut buf = vec![0x05];
            buf.extend_from_slice(&binary.bytes);
            Ok(buf)
        }
        Bson::Boolean(b) => {
            Ok(vec![0x08, if *b { 1 } else { 0 }])
        }
        other => Err(Error::Storage(format!(
            "unsupported _id type for storage key: {}",
            other
        ))),
    }
}

/// Ensure the document has an `_id`; generate an ObjectId when missing.
pub fn ensure_id(doc: &mut bson::Document) -> Bson {
    if let Some(id) = doc.get("_id") {
        return id.clone();
    }
    let id = Bson::ObjectId(ObjectId::new());
    doc.insert("_id", id.clone());
    id
}

#[cfg(test)]
mod tests {
    use super::*;
    use bson::doc;

    #[test]
    fn encode_object_id_is_unique_per_value() {
        let a = encode_id(&Bson::ObjectId(ObjectId::new())).unwrap();
        let b = encode_id(&Bson::ObjectId(ObjectId::new())).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn encode_string_and_int_do_not_collide() {
        let s = encode_id(&Bson::String("10".into())).unwrap();
        let n = encode_id(&Bson::Int32(10)).unwrap();
        assert_ne!(s, n);
    }

    #[test]
    fn ensure_id_generates_when_missing() {
        let mut doc = doc! { "name": "alice" };
        let id = ensure_id(&mut doc);
        assert!(doc.contains_key("_id"));
        assert_eq!(doc.get("_id"), Some(&id));
    }
}

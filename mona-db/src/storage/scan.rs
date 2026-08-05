use std::ops::Bound;

use bson::Document;
use slatedb::DbSnapshot;

use crate::error::{Error, Result};

/// Top-level field equality (AND): every key in `equality` must equal the doc field.
pub fn document_matches_equality(doc: &Document, equality: &Document) -> bool {
    for (key, expected) in equality.iter() {
        if doc.get(key) != Some(expected) {
            return false;
        }
    }
    true
}

pub struct ScanBatch {
    pub docs: Vec<Document>,
    pub last_key: Option<Vec<u8>>,
    pub exhausted: bool,
    pub limit_remaining: Option<i32>,
}

/// Read up to `batch_size` matching documents from a snapshot, resuming after `after_key`.
/// Applies `skip` only when `after_key` is `None` (first batch).
/// When `equality` is set, only matching docs count toward skip/batch/limit; `last_key`
/// still advances through non-matching keys.
pub async fn scan_batch(
    snapshot: &DbSnapshot,
    after_key: Option<&[u8]>,
    skip: i32,
    batch_size: i32,
    limit_remaining: Option<i32>,
    equality: Option<&Document>,
) -> Result<ScanBatch> {
    let batch_cap = batch_size.max(0) as usize;
    if batch_cap == 0 {
        return Ok(ScanBatch {
            docs: Vec::new(),
            last_key: after_key.map(|key| key.to_vec()),
            exhausted: true,
            limit_remaining,
        });
    }

    if limit_remaining == Some(0) {
        return Ok(ScanBatch {
            docs: Vec::new(),
            last_key: after_key.map(|key| key.to_vec()),
            exhausted: true,
            limit_remaining: Some(0),
        });
    }

    let mut iter = match after_key {
        Some(key) => snapshot
            .scan((Bound::Excluded(key), Bound::Unbounded))
            .await
            .map_err(map_slate_error)?,
        None => snapshot.scan(..).await.map_err(map_slate_error)?,
    };

    let mut skip_remaining = skip.max(0) as usize;
    let mut docs = Vec::new();
    let mut last_key = after_key.map(|key| key.to_vec());
    let mut limit_left = limit_remaining;
    let mut iterator_exhausted = false;

    while docs.len() < batch_cap {
        if limit_left == Some(0) {
            break;
        }

        let Some(kv) = iter.next().await.map_err(map_slate_error)? else {
            iterator_exhausted = true;
            break;
        };

        last_key = Some(kv.key.to_vec());

        let doc = bson::from_slice(kv.value.as_ref())?;
        if let Some(eq) = equality {
            if !document_matches_equality(&doc, eq) {
                continue;
            }
        }

        if skip_remaining > 0 {
            skip_remaining -= 1;
            continue;
        }

        docs.push(doc);

        if let Some(remaining) = limit_left {
            let next = remaining - 1;
            limit_left = Some(next);
        }
    }

    let exhausted = iterator_exhausted || limit_left == Some(0);

    Ok(ScanBatch {
        docs,
        last_key,
        exhausted,
        limit_remaining: limit_left,
    })
}

fn map_slate_error(error: slatedb::Error) -> Error {
    Error::Storage(error.to_string())
}

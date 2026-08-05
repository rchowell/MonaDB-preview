//! Phase 2 compatibility hooks.
//!
//! Deferred per docs/mongodb-wire-protocol.md#implementation-notes:
//! - OP_COMPRESSED decompression (noop, snappy, zlib, zstd)
//! - OP_MSG section kind 1 document sequences
//! - CRC-32C checksum validation on plain TCP
//! - Legacy opcodes (OP_QUERY, OP_INSERT, OP_UPDATE, OP_DELETE, etc.)

#![allow(dead_code)]

use crate::error::{Result, Error};
use crate::wire::opcodes::OP_COMPRESSED;

/// Placeholder for OP_COMPRESSED handling.
pub fn decode_compressed(_payload: &[u8]) -> Result<()> {
    Err(Error::UnsupportedOpcode(OP_COMPRESSED))
}

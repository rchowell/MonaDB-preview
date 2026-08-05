# MongoDB Wire Protocol Reference

> **Source:** [MongoDB Wire Protocol — Database Manual](https://www.mongodb.com/docs/manual/reference/mongodb-wire-protocol/)  
> **Legacy opcodes:** [Legacy Opcodes](https://www.mongodb.com/docs/manual/legacy-opcodes/)  
> **License note:** The official MongoDB Wire Protocol Specification is licensed under [CC BY-NC-SA 3.0 US](https://creativecommons.org/licenses/by-nc-sa/3.0/us/).

This document is a reference for implementing MongoDB-compatible wire protocol handling in MonaDB. It covers the current protocol (MongoDB 5.1+) and legacy opcodes retained for interoperability context.

---

## Table of Contents

1. [Overview](#overview)
2. [Transport](#transport)
3. [Standard Message Header](#standard-message-header)
4. [Opcodes](#opcodes)
5. [OP_COMPRESSED](#op_compressed)
6. [OP_MSG](#op_msg)
7. [Request/Response Patterns](#requestresponse-patterns)
8. [Legacy Opcodes](#legacy-opcodes)
9. [Implementation Notes](#implementation-notes)

---

## Overview

The MongoDB Wire Protocol is a **socket-based, request-response protocol**. Clients communicate with `mongod` or `mongos` over a plain TCP/IP connection (optionally wrapped in TLS).

- **Current protocol (5.1+):** All requests and replies use `OP_MSG` (opcode `2013`). Messages may be wrapped in `OP_COMPRESSED` (opcode `2012`).
- **Legacy protocol (≤5.0):** Individual opcodes (`OP_QUERY`, `OP_INSERT`, etc.) were used directly. These were removed in MongoDB 5.1 except for limited `OP_QUERY` handshake support.

Message structures in this document use C-like `struct` notation. Types such as `int32`, `int64`, `uint8`, `uint32`, `cstring`, and `document` follow the [BSON specification](https://bsonspec.org/).

---

## Transport

### Connection

Clients connect via a standard TCP/IP socket. There is no application-level connection handshake in the wire protocol itself; authentication and capability negotiation happen via commands (`hello` / `isMaster`) after the TCP connection is established.

### Port

| Service | Default Port |
|---------|--------------|
| `mongod` | `27017` |
| `mongos` | `27017` |

The port is configurable.

### Byte Ordering

All multi-byte integers use **little-endian** byte order (least-significant byte first).

---

## Standard Message Header

Every wire message begins with a 16-byte header:

```c
struct MsgHeader {
    int32   messageLength;  // total message size in bytes, including this field
    int32   requestID;      // unique identifier for this message
    int32   responseTo;     // requestID from the original client request (responses only)
    int32   opCode;         // message type (see Opcodes)
}
```

| Field | Size | Description |
|-------|------|-------------|
| `messageLength` | 4 | Total message size in bytes, including the 4 bytes of this field. |
| `requestID` | 4 | Client- or server-generated ID that uniquely identifies the message. For client requests, the server echoes this value in `responseTo` on replies. |
| `responseTo` | 4 | On server replies, the `requestID` from the originating client request. Zero on client requests. |
| `opCode` | 4 | Opcode identifying the message type. |

### Reading a Message

1. Read 4 bytes → `messageLength`
2. Read `messageLength - 4` additional bytes (the rest of the header + body)
3. Parse `requestID`, `responseTo`, `opCode` from bytes 4–15
4. Dispatch on `opCode`

---

## Opcodes

| Opcode | Value | Status | Purpose |
|--------|-------|--------|---------|
| `OP_REPLY` | `1` | Legacy (removed 5.1) | Server reply to `OP_QUERY` / `OP_GET_MORE` |
| `OP_UPDATE` | `2001` | Legacy (removed 5.1) | Update documents |
| `OP_INSERT` | `2002` | Legacy (removed 5.1) | Insert documents |
| `RESERVED` | `2003` | — | Formerly `OP_GET_BY_OID` |
| `OP_QUERY` | `2004` | Legacy (removed 5.1)* | Query a collection |
| `OP_GET_MORE` | `2005` | Legacy (removed 5.1) | Fetch more cursor results |
| `OP_DELETE` | `2006` | Legacy (removed 5.1) | Delete documents |
| `OP_KILL_CURSORS` | `2007` | Legacy (removed 5.1) | Close server-side cursors |
| `OP_COMPRESSED` | `2012` | **Current** | Compression wrapper for any opcode |
| `OP_MSG` | `2013` | **Current** | Standard message format for all requests and replies |

\* `OP_QUERY` is still supported on MongoDB servers **only** for the `hello` and `isMaster` handshake commands.

---

## OP_COMPRESSED

Any opcode may be compressed and wrapped in an `OP_COMPRESSED` envelope.

```c
struct {
    MsgHeader header;            // standard message header (opCode = 2012)
    int32     originalOpcode;    // opcode of the wrapped message
    int32     uncompressedSize;  // size of decompressed payload (excludes MsgHeader)
    uint8     compressorId;      // compressor used
    char      compressedMessage[]; // compressed opcode body (excludes MsgHeader)
}
```

| Field | Description |
|-------|-------------|
| `originalOpcode` | Opcode of the inner message (typically `2013` for `OP_MSG`). |
| `uncompressedSize` | Byte length of the decompressed `compressedMessage` (does not include the inner `MsgHeader`). |
| `compressorId` | Compressor identifier (see table below). |
| `compressedMessage` | Compressed bytes of the inner opcode body (everything after the inner `MsgHeader`). |

### Compressor IDs

| ID | Handshake Name | Algorithm |
|----|----------------|-----------|
| `0` | `noop` | Uncompressed (testing only) |
| `1` | `snappy` | Snappy |
| `2` | `zlib` | zlib |
| `3` | `zstd` | Zstandard |
| `4`–`255` | — | Reserved |

### Processing

1. Receive `OP_COMPRESSED` message
2. Decompress `compressedMessage` using the algorithm identified by `compressorId`
3. Prepend a reconstructed `MsgHeader` (with `opCode = originalOpcode`) to the decompressed bytes
4. Parse the inner message as the corresponding opcode (usually `OP_MSG`)

Compression is negotiated during the `hello` handshake via the `compression` field.

---

## OP_MSG

`OP_MSG` (opcode `2013`) is the extensible message format used for **all** client requests and server replies in MongoDB 5.1+.

```c
OP_MSG {
    MsgHeader header;              // standard message header (opCode = 2013)
    uint32    flagBits;            // message flags (see below)
    Section   sections[];          // one or more sections
    optional<uint32> checksum;     // present when checksumPresent flag is set
}
```

### Flag Bits

`flagBits` is a 32-bit bitmask:

- **Bits 0–15 (required):** Parsers **MUST** error on unknown set bits.
- **Bits 16–31 (optional):** Parsers **MUST** ignore unknown set bits. Proxies **MUST** clear unknown optional bits before forwarding.

| Bit | Name | Req | Resp | Description |
|-----|------|-----|------|-------------|
| `0` | `checksumPresent` | ✓ | ✓ | Message ends with a 4-byte CRC-32C checksum. |
| `1` | `moreToCome` | ✓ | ✓ | Another message follows without further action from the receiver. Receiver **MUST NOT** send a reply until a message with `moreToCome = 0` is received. Requests with this bit set receive no reply. Server replies use this only when the request had `exhaustAllowed` set. |
| `16` | `exhaustAllowed` | ✓ | — | Client is prepared for multiple server replies via `moreToCome`. Server will not set `moreToCome` on replies unless this bit is set on the request. |

### Sections

Each section begins with a 1-byte `kind` identifier, followed by the section payload.

#### Kind 0 — Body

A single BSON document. The BSON document's leading `int32` size field determines the section size. This is the standard command request/reply body.

- All top-level field names **MUST** be unique within the body.

**Example request body** (find command):

```json
{
  "find": "collection",
  "filter": { "status": "active" },
  "limit": 10
}
```

**Example reply body:**

```json
{
  "cursor": {
    "id": { "$numberLong": "0" },
    "ns": "db.collection",
    "firstBatch": [ ... ]
  },
  "ok": 1
}
```

#### Kind 1 — Document Sequence

Used for bulk operations (e.g., batched inserts) to avoid duplicating large arrays in the body.

```
int32       size            // total section size in bytes
cstring     identifier      // field path this sequence replaces in the body
document[]  documents       // concatenated BSON objects, no separators
```

| Field | Description |
|-------|-------------|
| `size` | Total section size in bytes. |
| `identifier` | Field path (possibly nested) that this sequence replaces in the body section. **MUST NOT** also appear in the body. |
| `documents` | Zero or more BSON objects written back-to-back. Each object is limited to `maxBSONObjectSize`; the combined sequence is not. Reading stops after `size` bytes. |

Parsers **MAY** merge the sequence into the body as an array at the path given by `identifier`.

#### Kind 2 — Internal

Reserved for internal server use.

### Checksum

When the `checksumPresent` flag (bit 0) is set, the message ends with a **CRC-32C** checksum (Castagnoli polynomial, [RFC 4960 §6](https://tools.ietf.org/html/rfc4960#page-140)) covering all preceding bytes.

| Connection Type | Checksum Behavior |
|-----------------|-------------------|
| Plain TCP (no TLS) | `mongod` / `mongos` exchange messages **with** checksums |
| TLS/SSL | Checksums are **skipped** |
| Driver compatibility | Drivers ignore checksums if present |

### OP_MSG Wire Layout (typical)

```
┌──────────────────────────────────────────────┐
│ MsgHeader (16 bytes)                         │
├──────────────────────────────────────────────┤
│ flagBits (4 bytes)                           │
├──────────────────────────────────────────────┤
│ Section 0: kind=0 + BSON body                │
├──────────────────────────────────────────────┤
│ Section 1: kind=1 + document sequence (opt.) │
├──────────────────────────────────────────────┤
│ CRC-32C checksum (4 bytes, optional)         │
└──────────────────────────────────────────────┘
```

---

## Request/Response Patterns

### Standard Command (OP_MSG)

```
Client                                    Server
  │                                          │
  │  OP_MSG { body: { "find": ... } }        │
  │ ────────────────────────────────────────>│
  │                                          │
  │  OP_MSG { body: { "cursor": ..., ok: 1 }}│
  │ <────────────────────────────────────────│
  │                                          │
```

### Unacknowledged Write (`moreToCome`)

```
Client                                    Server
  │                                          │
  │  OP_MSG { moreToCome: 1, body: insert }  │
  │ ────────────────────────────────────────>│  (no reply)
  │  OP_MSG { moreToCome: 0, body: insert }  │
  │ ────────────────────────────────────────>│  (no reply)
  │                                          │
```

### Compressed Message

```
Client                                    Server
  │                                          │
  │  OP_COMPRESSED {                         │
  │    originalOpcode: 2013,                 │
  │    compressorId: 1 (snappy),             │
  │    compressedMessage: <OP_MSG bytes>     │
  │  }                                       │
  │ ────────────────────────────────────────>│
  │                                          │
  │  OP_COMPRESSED { <compressed OP_MSG> }   │
  │ <────────────────────────────────────────│
  │                                          │
```

### Exhaust Cursor (`exhaustAllowed` + `moreToCome`)

```
Client                                    Server
  │                                          │
  │  OP_MSG { exhaustAllowed: 1, find: ... } │
  │ ────────────────────────────────────────>│
  │                                          │
  │  OP_MSG { moreToCome: 1, batch 1 }       │
  │ <────────────────────────────────────────│
  │  OP_MSG { moreToCome: 1, batch 2 }       │
  │ <────────────────────────────────────────│
  │  OP_MSG { moreToCome: 0, batch 3 }       │
  │ <────────────────────────────────────────│
  │                                          │
```

---

## Legacy Opcodes

> **Deprecated** in MongoDB 5.0. **Removed** in MongoDB 5.1.  
> Starting in 5.1, only `OP_MSG` and `OP_COMPRESSED` are supported for sending requests.

Legacy messages share the standard `MsgHeader` and use `cstring` namespace fields in the format `dbname.collectionname`.

### OP_QUERY (2004)

Query a collection. Server responds with `OP_REPLY`.

```c
struct OP_QUERY {
    MsgHeader header;
    int32     flags;
    cstring   fullCollectionName;
    int32     numberToSkip;
    int32     numberToReturn;
    document  query;
    document  returnFieldsSelector;  // optional
}
```

**Query flags:**

| Bit | Name | Description |
|-----|------|-------------|
| `0` | — | Reserved, must be 0 |
| `1` | `TailableCursor` | Cursor stays open at end of data |
| `2` | `SlaveOk` | Allow reads from secondary |
| `3` | `OplogReplay` | Oplog replay optimization (auto in 4.4+) |
| `4` | `NoCursorTimeout` | Disable 10-minute idle cursor timeout |
| `5` | `AwaitData` | Block waiting for new data (with tailable) |
| `6` | `Exhaust` | Stream all results without client getMore |
| `7` | `Partial` | Return partial results if shards are down |

**`numberToReturn` semantics:**

| Value | Behavior |
|-------|----------|
| `0` | Server default batch size |
| Negative | Return that many documents and close cursor |
| `1` | Treated as `-1` (close cursor after batch) |

### OP_REPLY (1)

Server response to `OP_QUERY` or `OP_GET_MORE`.

```c
struct {
    MsgHeader header;
    int32     responseFlags;
    int64     cursorID;
    int32     startingFrom;
    int32     numberReturned;
    document  documents[];
}
```

**Response flags:**

| Bit | Name | Description |
|-----|------|-------------|
| `0` | `CursorNotFound` | Cursor ID invalid |
| `1` | `QueryFailure` | Query failed; results contain `$err` document |
| `2` | `ShardConfigStale` | mongos only; config update needed |
| `3` | `AwaitCapable` | Server supports `AwaitData` option |

`cursorID` of `0` means no further results. Non-zero values require `OP_GET_MORE` or `OP_KILL_CURSORS`.

### OP_GET_MORE (2005)

Fetch additional cursor results. Server responds with `OP_REPLY`.

```c
struct {
    MsgHeader header;
    int32     ZERO;               // must be 0
    cstring   fullCollectionName;
    int32     numberToReturn;
    int64     cursorID;
}
```

### OP_INSERT (2002)

Insert one or more documents. No server response.

```c
struct {
    MsgHeader header;
    int32     flags;
    cstring   fullCollectionName;
    document  documents[];
}
```

| Bit | Name | Description |
|-----|------|-------------|
| `0` | `ContinueOnError` | Continue bulk insert on individual failures |

### OP_UPDATE (2001)

Update documents. No server response.

```c
struct OP_UPDATE {
    MsgHeader header;
    int32     ZERO;               // must be 0
    cstring   fullCollectionName;
    int32     flags;
    document  selector;
    document  update;
}
```

| Bit | Name | Description |
|-----|------|-------------|
| `0` | `Upsert` | Insert if no match |
| `1` | `MultiUpdate` | Update all matching documents |

### OP_DELETE (2006)

Delete documents. No server response.

```c
struct {
    MsgHeader header;
    int32     ZERO;               // must be 0
    cstring   fullCollectionName;
    int32     flags;
    document  selector;
}
```

| Bit | Name | Description |
|-----|------|-------------|
| `0` | `SingleRemove` | Remove only the first matching document |

### OP_KILL_CURSORS (2007)

Close server-side cursors. No server response.

```c
struct {
    MsgHeader header;
    int32     ZERO;               // must be 0
    int32     numberOfCursorIDs;
    int64     cursorIDs[];
}
```

If a cursor is exhausted (`cursorID` returns `0`), killing it is unnecessary.

---

## Implementation Notes

### Minimum Viable Server (MonaDB)

For MongoDB 5.1+ driver compatibility, implement:

1. **TCP listener** on port 27017 (configurable)
2. **Message framing** via `messageLength` in `MsgHeader`
3. **`OP_MSG` parsing** — Kind 0 body sections at minimum; Kind 1 for bulk writes
4. **`OP_MSG` encoding** for replies with `ok: 1` / error documents
5. **`OP_COMPRESSED` decompression** — at least `noop` and `snappy` (most common)
6. **`hello` command** — required for driver handshake; advertise `maxWireVersion`, `compression`, etc.
7. **CRC-32C checksums** on plain TCP connections

### Command Dispatch

With `OP_MSG`, all operations are BSON commands in the body section. The first field name determines the command:

| First Field | Command |
|-------------|---------|
| `find` | Query documents |
| `insert` | Insert documents |
| `update` | Update documents |
| `delete` | Delete documents |
| `aggregate` | Aggregation pipeline |
| `hello` / `isMaster` | Handshake |
| `ping` | Health check |
| `getMore` | Fetch cursor batch |
| `killCursors` | Close cursors |

Database commands use `$db` in the body: `{ "listCollections": 1, "$db": "mydb" }`.

### Error Responses

Server errors are returned as `OP_MSG` with a body like:

```json
{
  "ok": 0,
  "errmsg": "command not found",
  "code": 59,
  "codeName": "CommandNotFound"
}
```

### `maxWireVersion`

Drivers use `maxWireVersion` from the `hello` response to select protocol features:

| Version | Notable Change |
|---------|----------------|
| 6 | `OP_MSG` introduced (MongoDB 3.6) |
| 7 | Retryable writes |
| 8 | Transactions |
| 13 | `OP_MSG` checksums (4.2+) |
| 21 | Versioned API (5.0) |
| 25 | `OP_QUERY` removed for commands (5.1) |

### Type Reference

| Type | Size | Description |
|------|------|-------------|
| `int32` | 4 | Signed 32-bit integer, little-endian |
| `int64` | 8 | Signed 64-bit integer, little-endian |
| `uint8` | 1 | Unsigned 8-bit integer |
| `uint32` | 4 | Unsigned 32-bit integer, little-endian |
| `cstring` | variable | UTF-8 string terminated by `\0` |
| `document` | variable | BSON document (leading `int32` size + elements + `\0`) |

---

## References

- [MongoDB Wire Protocol (official)](https://www.mongodb.com/docs/manual/reference/mongodb-wire-protocol/)
- [Legacy Opcodes (official)](https://www.mongodb.com/docs/manual/legacy-opcodes/)
- [BSON Specification](https://bsonspec.org/)
- [CRC-32C (Castagnoli) — RFC 4960](https://tools.ietf.org/html/rfc4960#page-140)
- [MongoDB `hello` Command](https://www.mongodb.com/docs/manual/reference/command/hello/)
- [MongoDB Error Codes](https://www.mongodb.com/docs/manual/reference/error-codes/)

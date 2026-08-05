#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("incomplete message: need {needed} bytes, have {available}")]
    Incomplete { needed: usize, available: usize },

    #[error("invalid message length: {0}")]
    InvalidMessageLength(i32),

    #[error("unsupported opcode: {0}")]
    UnsupportedOpcode(i32),

    #[error("unknown required OP_MSG flag bits: 0x{0:04x}")]
    UnknownRequiredFlagBits(u32),

    #[error("unsupported section kind: {0}")]
    UnsupportedSectionKind(u8),

    #[error("BSON decode error: {0}")]
    Bson(#[from] bson::de::Error),

    #[error("BSON encode error: {0}")]
    BsonEncode(#[from] bson::ser::Error),

    #[error("command parse error: {0}")]
    CommandParse(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("storage error: {0}")]
    Storage(String),
}

pub type Result<T> = std::result::Result<T, Error>;

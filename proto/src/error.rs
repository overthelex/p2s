use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProtoError {
    #[error("CBOR encoding failed: {0}")]
    Encode(String),

    #[error("CBOR decoding failed: {0}")]
    Decode(String),

    #[error("invalid signature")]
    InvalidSignature,

    #[error("address mismatch: expected {expected}, got {got}")]
    AddressMismatch { expected: String, got: String },

    #[error("invalid card status: {0}")]
    InvalidStatus(String),
}

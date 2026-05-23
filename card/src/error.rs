use thiserror::Error;

#[derive(Debug, Error)]
pub enum CardError {
    #[error("serialization error: {0}")]
    Serialization(#[from] p2s_proto::ProtoError),

    #[error("invalid signature: card content has been tampered with or wrong key used")]
    InvalidSignature,

    #[error("address mismatch: card address does not match BLAKE3(pubkey)")]
    AddressMismatch,

    #[error("pubkey length must be 32 bytes, got {0}")]
    InvalidPubkeyLength(usize),

    #[error("signature length must be 64 bytes, got {0}")]
    InvalidSignatureLength(usize),
}

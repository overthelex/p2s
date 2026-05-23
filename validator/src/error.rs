use thiserror::Error;

#[derive(Debug, Error)]
pub enum ValidatorError {
    #[error("serialization error: {0}")]
    Serialization(#[from] p2s_proto::ProtoError),

    #[error("invalid validator signature on mandate")]
    InvalidSignature,

    #[error("mandate has expired (expired_at={expired_at}, now={now})")]
    MandateExpired { expired_at: u64, now: u64 },

    #[error("mandate not yet valid (issued_at={issued_at}, now={now})")]
    MandateNotYetValid { issued_at: u64, now: u64 },

    #[error("unknown validator key: not in the trusted set")]
    UnknownValidatorKey,

    #[error("node pubkey length must be 32 bytes, got {0}")]
    InvalidNodePubkeyLength(usize),
}

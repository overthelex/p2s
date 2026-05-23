use serde::{Deserialize, Serialize};

/// The signable portion of a node mandate — everything except the validator's signature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MandateRecord {
    /// The node operator's Ed25519 public key (32 bytes).
    #[serde(with = "serde_bytes")]
    pub node_pubkey: Vec<u8>,

    /// Unix timestamp (seconds) when this mandate was issued.
    pub issued_at: u64,

    /// Unix timestamp (seconds) when this mandate expires.
    pub expires_at: u64,
}

/// A complete signed mandate: the record plus the validator's Ed25519 signature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedMandate {
    pub record: MandateRecord,

    /// Ed25519 signature by the validator over the canonical CBOR encoding of `record`.
    #[serde(with = "serde_bytes")]
    pub validator_sig: Vec<u8>,
}

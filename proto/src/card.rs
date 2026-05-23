use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CardStatus {
    Active,
    Revoked,
}

/// The signable portion of a card record — everything except the signature itself.
/// Fields use integer keys in CBOR for compact, deterministic encoding.
/// The canonical byte representation of this struct is what gets signed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardRecord {
    /// Publisher's Ed25519 public key (32 bytes).
    #[serde(with = "serde_bytes")]
    pub pubkey: Vec<u8>,

    /// Monotonic version counter. Higher seq wins on conflict.
    pub seq: u64,

    /// Active or revoked.
    pub status: CardStatus,

    /// The service's agent/MCP endpoint URL.
    pub endpoint: String,

    /// BLAKE3 hash of the full capability manifest (32 bytes).
    #[serde(with = "serde_bytes")]
    pub manifest_hash: Vec<u8>,

    /// The domain this card claims (verified via DNS/.well-known).
    pub domain: String,

    /// Optional human-readable name. Non-unique, guarantees nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// A complete signed card: the record plus its Ed25519 signature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedCard {
    pub record: CardRecord,

    /// Ed25519 signature over the canonical CBOR encoding of `record`.
    #[serde(with = "serde_bytes")]
    pub sig: Vec<u8>,
}

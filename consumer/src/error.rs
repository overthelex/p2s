use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConsumerError {
    #[error("card verification failed: {0}")]
    CardVerification(#[from] p2s_card::CardError),

    #[error("card address mismatch: record key does not match BLAKE3(pubkey)")]
    AddressMismatch,

    #[error("manifest hash mismatch: fetched manifest does not match card's manifest_hash")]
    ManifestHashMismatch,

    #[error("card is revoked")]
    CardRevoked,

    #[error("manifest fetch failed: {0}")]
    ManifestFetchFailed(String),
}

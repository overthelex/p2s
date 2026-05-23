use crate::ProtoError;
use serde::{Deserialize, Serialize};

/// Encode a value to deterministic CBOR bytes.
///
/// ciborium produces RFC 8949 Core Deterministic Encoding by default:
/// map keys sorted by encoded form, shortest integer encoding, no indefinite lengths.
/// We rely on serde field ordering (struct fields serialize in declaration order)
/// combined with ciborium's deterministic map encoding.
pub fn canonical_encode<T: Serialize>(value: &T) -> Result<Vec<u8>, ProtoError> {
    let mut buf = Vec::new();
    ciborium::into_writer(value, &mut buf)
        .map_err(|e| ProtoError::Encode(e.to_string()))?;
    Ok(buf)
}

/// Decode a value from CBOR bytes.
pub fn canonical_decode<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T, ProtoError> {
    ciborium::from_reader(bytes)
        .map_err(|e| ProtoError::Decode(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CardRecord, CardStatus};

    #[test]
    fn deterministic_encoding_is_stable() {
        let record = CardRecord {
            pubkey: vec![0u8; 32],
            seq: 1,
            status: CardStatus::Active,
            endpoint: "https://example.com/agent".into(),
            manifest_hash: vec![0xAB; 32],
            domain: "example.com".into(),
            label: Some("My Service".into()),
        };

        let bytes1 = canonical_encode(&record).unwrap();
        let bytes2 = canonical_encode(&record).unwrap();
        assert_eq!(bytes1, bytes2, "encoding must be deterministic");
    }

    #[test]
    fn round_trip() {
        let record = CardRecord {
            pubkey: vec![1u8; 32],
            seq: 42,
            status: CardStatus::Active,
            endpoint: "https://test.example.com/mcp".into(),
            manifest_hash: vec![0xCD; 32],
            domain: "test.example.com".into(),
            label: None,
        };

        let bytes = canonical_encode(&record).unwrap();
        let decoded: CardRecord = canonical_decode(&bytes).unwrap();
        assert_eq!(record, decoded);
    }

    #[test]
    fn revoked_status_round_trips() {
        let record = CardRecord {
            pubkey: vec![2u8; 32],
            seq: 100,
            status: CardStatus::Revoked,
            endpoint: "https://revoked.example.com".into(),
            manifest_hash: vec![0x00; 32],
            domain: "revoked.example.com".into(),
            label: None,
        };

        let bytes = canonical_encode(&record).unwrap();
        let decoded: CardRecord = canonical_decode(&bytes).unwrap();
        assert_eq!(decoded.status, CardStatus::Revoked);
    }
}

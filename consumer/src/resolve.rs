use crate::ConsumerError;
use p2s_card::{verify_card, compute_address, Address, CardStatus, SignedCard};

/// Full verification pipeline for a card fetched from the DHT.
///
/// 1. Verify signature against embedded pubkey (invariant §1.2)
/// 2. Verify record key matches BLAKE3(pubkey) (invariant §1.1)
/// 3. Check card is not revoked
pub fn verify_fetched_card(
    signed_card: &SignedCard,
    expected_address: &Address,
) -> Result<(), ConsumerError> {
    verify_card(signed_card)?;

    let computed_address = compute_address(&signed_card.record.pubkey);
    if &computed_address != expected_address {
        return Err(ConsumerError::AddressMismatch);
    }

    if signed_card.record.status == CardStatus::Revoked {
        return Err(ConsumerError::CardRevoked);
    }

    Ok(())
}

/// Verify a fetched manifest against the card's manifest_hash.
/// The full manifest is fetched from the card's endpoint on demand (§3.1).
pub fn verify_manifest(manifest_bytes: &[u8], expected_hash: &[u8]) -> Result<(), ConsumerError> {
    let computed_hash = blake3::hash(manifest_bytes);
    if computed_hash.as_bytes() != expected_hash {
        return Err(ConsumerError::ManifestHashMismatch);
    }
    Ok(())
}

/// Fetch the manifest from the card's endpoint and verify its hash.
pub async fn fetch_and_verify_manifest(
    signed_card: &SignedCard,
) -> Result<Vec<u8>, ConsumerError> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| ConsumerError::ManifestFetchFailed(e.to_string()))?;

    let response = client.get(&signed_card.record.endpoint).send().await
        .map_err(|e| ConsumerError::ManifestFetchFailed(e.to_string()))?;

    let manifest_bytes = response.bytes().await
        .map_err(|e| ConsumerError::ManifestFetchFailed(e.to_string()))?
        .to_vec();

    verify_manifest(&manifest_bytes, &signed_card.record.manifest_hash)?;

    Ok(manifest_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use p2s_card::{generate_keypair, sign_card, CardRecord};

    fn make_test_card() -> (SignedCard, Address) {
        let keypair = generate_keypair();
        let manifest = b"test manifest content";
        let manifest_hash = blake3::hash(manifest).as_bytes().to_vec();

        let record = CardRecord {
            pubkey: keypair.verifying_key.as_bytes().to_vec(),
            seq: 1,
            status: CardStatus::Active,
            endpoint: "https://example.com/manifest".into(),
            manifest_hash,
            domain: "example.com".into(),
            label: None,
        };

        let signed = sign_card(record, &keypair.signing_key).unwrap();
        let address = compute_address(&signed.record.pubkey);
        (signed, address)
    }

    #[test]
    fn valid_card_passes_verification() {
        let (card, address) = make_test_card();
        assert!(verify_fetched_card(&card, &address).is_ok());
    }

    #[test]
    fn wrong_address_fails() {
        let (card, _) = make_test_card();
        let wrong_address = [0xFFu8; 32];
        assert!(matches!(
            verify_fetched_card(&card, &wrong_address),
            Err(ConsumerError::AddressMismatch)
        ));
    }

    #[test]
    fn revoked_card_fails() {
        let keypair = generate_keypair();
        let record = CardRecord {
            pubkey: keypair.verifying_key.as_bytes().to_vec(),
            seq: 2,
            status: CardStatus::Revoked,
            endpoint: "https://example.com/manifest".into(),
            manifest_hash: blake3::hash(b"m").as_bytes().to_vec(),
            domain: "example.com".into(),
            label: None,
        };
        let signed = sign_card(record, &keypair.signing_key).unwrap();
        let address = compute_address(&signed.record.pubkey);
        assert!(matches!(
            verify_fetched_card(&signed, &address),
            Err(ConsumerError::CardRevoked)
        ));
    }

    #[test]
    fn manifest_hash_check_passes() {
        let manifest = b"the real manifest";
        let hash = blake3::hash(manifest).as_bytes().to_vec();
        assert!(verify_manifest(manifest, &hash).is_ok());
    }

    #[test]
    fn manifest_hash_mismatch_fails() {
        let manifest = b"the real manifest";
        let wrong_hash = blake3::hash(b"different").as_bytes().to_vec();
        assert!(matches!(
            verify_manifest(manifest, &wrong_hash),
            Err(ConsumerError::ManifestHashMismatch)
        ));
    }

    #[test]
    fn tampered_card_fails_consumer_verification() {
        let keypair = generate_keypair();
        let record = CardRecord {
            pubkey: keypair.verifying_key.as_bytes().to_vec(),
            seq: 1,
            status: CardStatus::Active,
            endpoint: "https://example.com/manifest".into(),
            manifest_hash: blake3::hash(b"m").as_bytes().to_vec(),
            domain: "example.com".into(),
            label: None,
        };
        let mut signed = sign_card(record, &keypair.signing_key).unwrap();
        let address = compute_address(&signed.record.pubkey);

        signed.record.endpoint = "https://evil.com/manifest".into();

        assert!(matches!(
            verify_fetched_card(&signed, &address),
            Err(ConsumerError::CardVerification(_))
        ));
    }
}

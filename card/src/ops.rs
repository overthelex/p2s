use crate::CardError;
use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey, Signature};
use p2s_proto::{canonical_encode, CardRecord, SignedCard};

/// 32-byte DHT address derived from a public key.
pub type Address = [u8; 32];

pub struct CardKeypair {
    pub signing_key: SigningKey,
    pub verifying_key: VerifyingKey,
}

/// Generate a new Ed25519 keypair for card publishing.
pub fn generate_keypair() -> CardKeypair {
    let mut csprng = rand::rngs::OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let verifying_key = signing_key.verifying_key();
    CardKeypair { signing_key, verifying_key }
}

/// Compute the DHT address for a public key: BLAKE3(pubkey).
/// Identity is a keypair, not a URL (invariant §1.1).
pub fn compute_address(pubkey: &[u8]) -> Address {
    blake3::hash(pubkey).into()
}

/// Sign a card record, producing a SignedCard.
/// The signature covers the canonical CBOR encoding of the record.
pub fn sign_card(record: CardRecord, signing_key: &SigningKey) -> Result<SignedCard, CardError> {
    let canonical_bytes = canonical_encode(&record)?;
    let signature = signing_key.sign(&canonical_bytes);
    Ok(SignedCard {
        record,
        sig: signature.to_bytes().to_vec(),
    })
}

/// Verify a signed card:
/// 1. Check signature over canonical CBOR of the record against the embedded pubkey.
/// 2. No node is trusted to vouch for card contents (invariant §1.2).
pub fn verify_card(signed_card: &SignedCard) -> Result<(), CardError> {
    let pubkey_bytes: &[u8] = &signed_card.record.pubkey;
    if pubkey_bytes.len() != 32 {
        return Err(CardError::InvalidPubkeyLength(pubkey_bytes.len()));
    }
    if signed_card.sig.len() != 64 {
        return Err(CardError::InvalidSignatureLength(signed_card.sig.len()));
    }

    let verifying_key = VerifyingKey::from_bytes(
        pubkey_bytes.try_into().unwrap()
    ).map_err(|_| CardError::InvalidSignature)?;

    let signature = Signature::from_bytes(
        signed_card.sig.as_slice().try_into().unwrap()
    );

    let canonical_bytes = canonical_encode(&signed_card.record)?;
    verifying_key.verify(&canonical_bytes, &signature)
        .map_err(|_| CardError::InvalidSignature)
}

/// Verify that a card's address matches BLAKE3(pubkey).
/// Use this when fetching a card by address from the DHT.
pub fn verify_address(signed_card: &SignedCard, expected_address: &Address) -> Result<(), CardError> {
    let computed = compute_address(&signed_card.record.pubkey);
    if &computed != expected_address {
        return Err(CardError::AddressMismatch);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use p2s_proto::CardStatus;

    fn make_test_record(keypair: &CardKeypair) -> CardRecord {
        CardRecord {
            pubkey: keypair.verifying_key.as_bytes().to_vec(),
            seq: 1,
            status: CardStatus::Active,
            endpoint: "https://example.com/agent".into(),
            manifest_hash: blake3::hash(b"test manifest").as_bytes().to_vec(),
            domain: "example.com".into(),
            label: Some("Test Service".into()),
        }
    }

    #[test]
    fn sign_and_verify_round_trip() {
        let keypair = generate_keypair();
        let record = make_test_record(&keypair);
        let signed = sign_card(record, &keypair.signing_key).unwrap();

        assert!(verify_card(&signed).is_ok());
    }

    #[test]
    fn tampered_card_fails_verification() {
        let keypair = generate_keypair();
        let record = make_test_record(&keypair);
        let mut signed = sign_card(record, &keypair.signing_key).unwrap();

        // Tamper with the endpoint
        signed.record.endpoint = "https://evil.com/agent".into();

        assert!(verify_card(&signed).is_err());
    }

    #[test]
    fn tampered_seq_fails_verification() {
        let keypair = generate_keypair();
        let record = make_test_record(&keypair);
        let mut signed = sign_card(record, &keypair.signing_key).unwrap();

        signed.record.seq = 999;

        assert!(verify_card(&signed).is_err());
    }

    #[test]
    fn wrong_key_fails_verification() {
        let keypair1 = generate_keypair();
        let keypair2 = generate_keypair();
        let record = make_test_record(&keypair1);
        let signed = sign_card(record, &keypair2.signing_key).unwrap();

        // Signature was made with keypair2 but pubkey in the record is keypair1's
        assert!(verify_card(&signed).is_err());
    }

    #[test]
    fn address_derivation_is_deterministic() {
        let keypair = generate_keypair();
        let pubkey = keypair.verifying_key.as_bytes();
        let addr1 = compute_address(pubkey);
        let addr2 = compute_address(pubkey);
        assert_eq!(addr1, addr2);
    }

    #[test]
    fn address_verification_works() {
        let keypair = generate_keypair();
        let record = make_test_record(&keypair);
        let signed = sign_card(record, &keypair.signing_key).unwrap();
        let address = compute_address(&signed.record.pubkey);

        assert!(verify_address(&signed, &address).is_ok());
    }

    #[test]
    fn address_verification_rejects_wrong_address() {
        let keypair = generate_keypair();
        let record = make_test_record(&keypair);
        let signed = sign_card(record, &keypair.signing_key).unwrap();
        let wrong_address = [0xFFu8; 32];

        assert!(verify_address(&signed, &wrong_address).is_err());
    }

    #[test]
    fn higher_seq_supersedes() {
        let keypair = generate_keypair();

        let record_v1 = make_test_record(&keypair);
        let mut record_v2 = record_v1.clone();
        record_v2.seq = 2;
        record_v2.endpoint = "https://example.com/agent/v2".into();

        let signed_v1 = sign_card(record_v1, &keypair.signing_key).unwrap();
        let signed_v2 = sign_card(record_v2, &keypair.signing_key).unwrap();

        assert!(verify_card(&signed_v1).is_ok());
        assert!(verify_card(&signed_v2).is_ok());
        assert!(signed_v2.record.seq > signed_v1.record.seq);
    }

    #[test]
    fn revocation_via_status_and_seq() {
        let keypair = generate_keypair();

        let active_record = make_test_record(&keypair);
        let mut revoked_record = active_record.clone();
        revoked_record.seq = 2;
        revoked_record.status = CardStatus::Revoked;

        let signed_revoked = sign_card(revoked_record, &keypair.signing_key).unwrap();

        assert!(verify_card(&signed_revoked).is_ok());
        assert_eq!(signed_revoked.record.status, CardStatus::Revoked);
        assert_eq!(signed_revoked.record.seq, 2);
    }

    #[test]
    fn invalid_pubkey_length_rejected() {
        let keypair = generate_keypair();
        let mut record = make_test_record(&keypair);
        record.pubkey = vec![0u8; 16]; // wrong length
        let signed = sign_card(record, &keypair.signing_key).unwrap();

        assert!(matches!(verify_card(&signed), Err(CardError::InvalidPubkeyLength(16))));
    }

    #[test]
    fn invalid_signature_length_rejected() {
        let keypair = generate_keypair();
        let record = make_test_record(&keypair);
        let mut signed = sign_card(record, &keypair.signing_key).unwrap();
        signed.sig = vec![0u8; 32]; // wrong length

        assert!(matches!(verify_card(&signed), Err(CardError::InvalidSignatureLength(32))));
    }
}

use crate::ValidatorError;
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use p2s_proto::{canonical_encode, MandateRecord, SignedMandate};

const MANDATE_TTL_SECS: u64 = 24 * 60 * 60; // 24 hours

/// The validator side: issues and renews mandates for node operators.
pub struct MandateIssuer {
    signing_key: SigningKey,
    verifying_key: VerifyingKey,
}

impl MandateIssuer {
    pub fn new(signing_key: SigningKey) -> Self {
        let verifying_key = signing_key.verifying_key();
        Self { signing_key, verifying_key }
    }

    pub fn verifying_key(&self) -> &VerifyingKey {
        &self.verifying_key
    }

    /// Issue a mandate for a node operator's public key.
    /// The mandate is valid for 24 hours from now.
    pub fn issue_mandate(&self, node_pubkey: &[u8]) -> Result<SignedMandate, ValidatorError> {
        if node_pubkey.len() != 32 {
            return Err(ValidatorError::InvalidNodePubkeyLength(node_pubkey.len()));
        }

        let now = current_unix_timestamp();
        let record = MandateRecord {
            node_pubkey: node_pubkey.to_vec(),
            issued_at: now,
            expires_at: now + MANDATE_TTL_SECS,
        };

        let canonical_bytes = canonical_encode(&record)?;
        let signature = self.signing_key.sign(&canonical_bytes);

        Ok(SignedMandate {
            record,
            validator_sig: signature.to_bytes().to_vec(),
        })
    }

    /// Renew a mandate: issue a fresh one with a new 24h window.
    /// Semantically identical to issue — renewal is just a new mandate.
    pub fn renew_mandate(&self, node_pubkey: &[u8]) -> Result<SignedMandate, ValidatorError> {
        self.issue_mandate(node_pubkey)
    }
}

fn current_unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before Unix epoch")
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_issuer() -> MandateIssuer {
        let mut csprng = rand::rngs::OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        MandateIssuer::new(signing_key)
    }

    #[test]
    fn issue_mandate_produces_valid_structure() {
        let issuer = test_issuer();
        let node_key = [0xABu8; 32];
        let mandate = issuer.issue_mandate(&node_key).unwrap();

        assert_eq!(mandate.record.node_pubkey, node_key.to_vec());
        assert!(mandate.record.expires_at > mandate.record.issued_at);
        assert_eq!(mandate.record.expires_at - mandate.record.issued_at, MANDATE_TTL_SECS);
        assert_eq!(mandate.validator_sig.len(), 64);
    }

    #[test]
    fn invalid_node_pubkey_length_rejected() {
        let issuer = test_issuer();
        let bad_key = [0u8; 16];
        assert!(matches!(
            issuer.issue_mandate(&bad_key),
            Err(ValidatorError::InvalidNodePubkeyLength(16))
        ));
    }

    #[test]
    fn renewal_produces_fresh_mandate() {
        let issuer = test_issuer();
        let node_key = [0xCDu8; 32];
        let m1 = issuer.issue_mandate(&node_key).unwrap();
        let m2 = issuer.renew_mandate(&node_key).unwrap();

        assert_eq!(m1.record.node_pubkey, m2.record.node_pubkey);
        assert!(m2.record.issued_at >= m1.record.issued_at);
    }
}

use crate::ValidatorError;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use p2s_proto::{canonical_encode, SignedMandate};

/// The node side: verifies mandates against trusted validator keys.
/// Supports multiple validator keys for future federation (§3.3).
pub struct MandateVerifier {
    trusted_keys: Vec<VerifyingKey>,
}

impl MandateVerifier {
    /// Create a verifier with one or more trusted validator public keys.
    pub fn new(trusted_keys: Vec<VerifyingKey>) -> Self {
        Self { trusted_keys }
    }

    /// Verify a mandate: check signature against any trusted validator key,
    /// and check temporal validity.
    pub fn verify(&self, mandate: &SignedMandate) -> Result<(), ValidatorError> {
        self.verify_at(mandate, current_unix_timestamp())
    }

    /// Verify a mandate at a specific point in time (useful for testing).
    pub fn verify_at(&self, mandate: &SignedMandate, now: u64) -> Result<(), ValidatorError> {
        self.verify_signature(mandate)?;
        self.verify_temporal(mandate, now)?;
        Ok(())
    }

    fn verify_signature(&self, mandate: &SignedMandate) -> Result<(), ValidatorError> {
        if mandate.validator_sig.len() != 64 {
            return Err(ValidatorError::InvalidSignature);
        }

        let canonical_bytes = canonical_encode(&mandate.record)?;
        let signature = Signature::from_bytes(
            mandate.validator_sig.as_slice().try_into().unwrap()
        );

        for key in &self.trusted_keys {
            if key.verify(&canonical_bytes, &signature).is_ok() {
                return Ok(());
            }
        }

        Err(ValidatorError::UnknownValidatorKey)
    }

    fn verify_temporal(&self, mandate: &SignedMandate, now: u64) -> Result<(), ValidatorError> {
        if now < mandate.record.issued_at {
            return Err(ValidatorError::MandateNotYetValid {
                issued_at: mandate.record.issued_at,
                now,
            });
        }
        if now >= mandate.record.expires_at {
            return Err(ValidatorError::MandateExpired {
                expired_at: mandate.record.expires_at,
                now,
            });
        }
        Ok(())
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
    use crate::MandateIssuer;
    use ed25519_dalek::SigningKey;

    fn setup() -> (MandateIssuer, MandateVerifier) {
        let mut csprng = rand::rngs::OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let verifying_key = signing_key.verifying_key();
        let issuer = MandateIssuer::new(signing_key);
        let verifier = MandateVerifier::new(vec![verifying_key]);
        (issuer, verifier)
    }

    #[test]
    fn valid_mandate_verifies() {
        let (issuer, verifier) = setup();
        let node_key = [0xABu8; 32];
        let mandate = issuer.issue_mandate(&node_key).unwrap();
        assert!(verifier.verify(&mandate).is_ok());
    }

    #[test]
    fn tampered_mandate_fails() {
        let (issuer, verifier) = setup();
        let node_key = [0xABu8; 32];
        let mut mandate = issuer.issue_mandate(&node_key).unwrap();
        mandate.record.expires_at += 9999;
        assert!(verifier.verify(&mandate).is_err());
    }

    #[test]
    fn wrong_validator_key_fails() {
        let (issuer, _) = setup();
        let other_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let wrong_verifier = MandateVerifier::new(vec![other_key.verifying_key()]);

        let node_key = [0xABu8; 32];
        let mandate = issuer.issue_mandate(&node_key).unwrap();
        assert!(matches!(
            wrong_verifier.verify(&mandate),
            Err(ValidatorError::UnknownValidatorKey)
        ));
    }

    #[test]
    fn expired_mandate_fails() {
        let (issuer, verifier) = setup();
        let node_key = [0xABu8; 32];
        let mandate = issuer.issue_mandate(&node_key).unwrap();
        let future = mandate.record.expires_at + 1;
        assert!(matches!(
            verifier.verify_at(&mandate, future),
            Err(ValidatorError::MandateExpired { .. })
        ));
    }

    #[test]
    fn not_yet_valid_mandate_fails() {
        let (issuer, verifier) = setup();
        let node_key = [0xABu8; 32];
        let mandate = issuer.issue_mandate(&node_key).unwrap();
        let past = mandate.record.issued_at - 1;
        assert!(matches!(
            verifier.verify_at(&mandate, past),
            Err(ValidatorError::MandateNotYetValid { .. })
        ));
    }

    #[test]
    fn multiple_trusted_keys_any_match() {
        let mut csprng = rand::rngs::OsRng;
        let key1 = SigningKey::generate(&mut csprng);
        let key2 = SigningKey::generate(&mut csprng);
        let issuer1 = MandateIssuer::new(key1);

        let verifier = MandateVerifier::new(vec![
            SigningKey::generate(&mut csprng).verifying_key(), // unrelated
            issuer1.verifying_key().clone(),                   // matches
        ]);

        let node_key = [0xEFu8; 32];
        let mandate = issuer1.issue_mandate(&node_key).unwrap();
        assert!(verifier.verify(&mandate).is_ok());

        // key2 mandate should fail since key2 is not in trusted set
        let issuer2 = MandateIssuer::new(key2);
        let mandate2 = issuer2.issue_mandate(&node_key).unwrap();
        assert!(matches!(
            verifier.verify(&mandate2),
            Err(ValidatorError::UnknownValidatorKey)
        ));
    }
}

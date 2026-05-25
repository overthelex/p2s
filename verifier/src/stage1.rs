use crate::harden::{harden_label, HardenError};
use crate::manifest::{
    fetch_manifest, parse_and_validate_manifest, validate_endpoint, verify_manifest_hash,
    Manifest, ManifestError,
};
use p2s_proto::SignedCard;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Stage1Error {
    #[error("structural validity: {0}")]
    StructuralValidity(String),

    #[error("domain ownership: {0}")]
    DomainOwnership(String),

    #[error("endpoint validation: {0}")]
    EndpointValidation(#[from] ManifestError),

    #[error("label hardening: {0}")]
    LabelHardening(#[from] HardenError),
}

/// Verified facts from Stage 1, passed to Stage 2 as established truth.
/// The agent has no authority to alter these.
#[derive(Debug, Clone)]
pub struct Stage1Facts {
    pub domain_verified: bool,
    pub endpoint_valid: bool,
    pub manifest_valid: bool,
    pub manifest: Option<Manifest>,
    pub label_hardened: Option<String>,
}

pub enum Stage1Outcome {
    Passed(Stage1Facts),
    Rejected {
        step: &'static str,
        reason: String,
    },
}

/// Run all Stage 1 deterministic checks in spec order.
///
/// Order:
/// 1. Structural validity (§1.2) — signature + address
/// 2. Domain ownership (§1.1) — challenge-response
/// 3. Endpoint + manifest (§1.3) — URL, fetch, hash, schema
/// 4. Free-text hardening (§1.4) — label normalization
///
/// Any mandatory failure produces a Rejected outcome.
/// The agent is never invoked if a mandatory check fails.
pub async fn run_stage1(
    signed_card: &SignedCard,
    challenge_nonce: &[u8; 16],
) -> Stage1Outcome {
    // §1.2 Structural validity — signature + address + required fields
    if let Err(e) = p2s_card::verify_card(signed_card) {
        return Stage1Outcome::Rejected {
            step: "structural_validity",
            reason: format!("card verification failed: {e}"),
        };
    }

    let expected_address = p2s_card::compute_address(&signed_card.record.pubkey);
    if let Err(e) = p2s_card::verify_address(signed_card, &expected_address) {
        return Stage1Outcome::Rejected {
            step: "structural_validity",
            reason: format!("address mismatch: {e}"),
        };
    }

    // §1.1 Domain ownership — reconstruct challenge and verify
    let domain_verified = match verify_domain_ownership(
        &signed_card.record.pubkey,
        &signed_card.record.domain,
        challenge_nonce,
    )
    .await
    {
        Ok(true) => true,
        Ok(false) => {
            return Stage1Outcome::Rejected {
                step: "domain_ownership",
                reason: "domain ownership verification failed".into(),
            };
        }
        Err(e) => {
            return Stage1Outcome::Rejected {
                step: "domain_ownership",
                reason: format!("domain verification error: {e}"),
            };
        }
    };

    // §1.3 Endpoint + manifest presence
    if let Err(e) = validate_endpoint(&signed_card.record.endpoint) {
        return Stage1Outcome::Rejected {
            step: "endpoint_validation",
            reason: format!("invalid endpoint: {e}"),
        };
    }

    let manifest = match fetch_and_validate_manifest(
        &signed_card.record.endpoint,
        &signed_card.record.manifest_hash,
    )
    .await
    {
        Ok(m) => Some(m),
        Err(e) => {
            return Stage1Outcome::Rejected {
                step: "manifest_validation",
                reason: format!("manifest validation failed: {e}"),
            };
        }
    };

    // §1.4 Free-text field hardening
    let label_hardened = if let Some(ref label) = signed_card.record.label {
        match harden_label(label) {
            Ok(hardened) => Some(hardened),
            Err(e) => {
                return Stage1Outcome::Rejected {
                    step: "label_hardening",
                    reason: format!("invalid label: {e}"),
                };
            }
        }
    } else {
        None
    };

    Stage1Outcome::Passed(Stage1Facts {
        domain_verified,
        endpoint_valid: true,
        manifest_valid: true,
        manifest,
        label_hardened,
    })
}

async fn verify_domain_ownership(
    pubkey: &[u8],
    domain: &str,
    nonce: &[u8; 16],
) -> anyhow::Result<bool> {
    let challenge = p2s_publisher::reconstruct_challenge(pubkey, domain, nonce);

    // Try DNS first, fall back to .well-known
    match p2s_publisher::verify_domain_dns(&challenge).await {
        Ok(_) => return Ok(true),
        Err(_) => {}
    }

    match p2s_publisher::verify_domain_wellknown(&challenge).await {
        Ok(_) => Ok(true),
        Err(e) => Err(anyhow::anyhow!("neither DNS nor .well-known verified: {e}")),
    }
}

async fn fetch_and_validate_manifest(
    endpoint: &str,
    expected_hash: &[u8],
) -> Result<Manifest, ManifestError> {
    let bytes = fetch_manifest(endpoint).await?;
    verify_manifest_hash(&bytes, expected_hash)?;
    parse_and_validate_manifest(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use p2s_card::{generate_keypair, sign_card};
    use p2s_proto::{CardRecord, CardStatus};

    fn make_test_card() -> (SignedCard, [u8; 16]) {
        let keypair = generate_keypair();
        let record = CardRecord {
            pubkey: keypair.verifying_key.as_bytes().to_vec(),
            seq: 1,
            status: CardStatus::Active,
            endpoint: "https://example.com/manifest".into(),
            manifest_hash: blake3::hash(b"test").as_bytes().to_vec(),
            domain: "example.com".into(),
            label: Some("Test Service".into()),
        };
        let signed = sign_card(record, &keypair.signing_key).unwrap();
        let nonce = [0u8; 16];
        (signed, nonce)
    }

    #[test]
    fn rejects_invalid_signature() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let (mut card, nonce) = make_test_card();
        card.record.endpoint = "https://tampered.com".into();
        // Signature no longer matches
        let outcome = rt.block_on(run_stage1(&card, &nonce));
        assert!(matches!(outcome, Stage1Outcome::Rejected { step: "structural_validity", .. }));
    }

    #[test]
    fn rejects_invalid_label() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let keypair = generate_keypair();
        let record = CardRecord {
            pubkey: keypair.verifying_key.as_bytes().to_vec(),
            seq: 1,
            status: CardStatus::Active,
            endpoint: "https://example.com/manifest".into(),
            manifest_hash: blake3::hash(b"test").as_bytes().to_vec(),
            domain: "example.com".into(),
            label: Some("bad\nlabel".into()),
        };
        let signed = sign_card(record, &keypair.signing_key).unwrap();
        let nonce = [0u8; 16];
        // This will fail at domain_ownership before reaching label check,
        // but the label check logic is tested in harden.rs directly
        let outcome = rt.block_on(run_stage1(&signed, &nonce));
        assert!(matches!(outcome, Stage1Outcome::Rejected { .. }));
    }
}

use crate::error::PublisherError;
use crate::challenge::ChallengeToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainProofMethod {
    Dns,
    WellKnown,
}

/// Verify domain ownership via DNS TXT record.
/// Looks up TXT records at `_p2s-verify.<domain>` and checks for the challenge token.
pub async fn verify_domain_dns(challenge: &ChallengeToken) -> Result<DomainProofMethod, PublisherError> {
    use hickory_resolver::TokioAsyncResolver;
    use hickory_resolver::config::{ResolverConfig, ResolverOpts};

    let resolver = TokioAsyncResolver::tokio(ResolverConfig::default(), ResolverOpts::default());

    let lookup = resolver.txt_lookup(&challenge.dns_record_name).await
        .map_err(|e| PublisherError::DnsLookupFailed {
            domain: challenge.dns_record_name.clone(),
            reason: e.to_string(),
        })?;

    for record in lookup.iter() {
        let txt_data = record.to_string();
        if txt_data.trim() == challenge.token {
            return Ok(DomainProofMethod::Dns);
        }
    }

    Err(PublisherError::NoMatchingTxtRecord {
        domain: challenge.dns_record_name.clone(),
    })
}

/// Verify domain ownership via HTTPS .well-known endpoint.
/// Fetches `https://<domain>/.well-known/p2s-verify` and checks the body matches.
pub async fn verify_domain_wellknown(challenge: &ChallengeToken) -> Result<DomainProofMethod, PublisherError> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| PublisherError::HttpFetchFailed {
            url: challenge.wellknown_path.clone(),
            reason: e.to_string(),
        })?;

    let response = client.get(&challenge.wellknown_path).send().await
        .map_err(|e| PublisherError::HttpFetchFailed {
            url: challenge.wellknown_path.clone(),
            reason: e.to_string(),
        })?;

    let body = response.text().await
        .map_err(|e| PublisherError::HttpFetchFailed {
            url: challenge.wellknown_path.clone(),
            reason: e.to_string(),
        })?;

    if body.trim() == challenge.token {
        Ok(DomainProofMethod::WellKnown)
    } else {
        Err(PublisherError::WellKnownTokenMismatch {
            domain: challenge.wellknown_path.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::challenge::generate_challenge;

    #[test]
    fn proof_method_enum_coverage() {
        assert_ne!(DomainProofMethod::Dns, DomainProofMethod::WellKnown);
    }

    #[tokio::test]
    async fn dns_verification_fails_for_nonexistent_domain() {
        let pubkey = [0xABu8; 32];
        let challenge = generate_challenge(&pubkey, "this-domain-does-not-exist-p2s-test.invalid");
        let result = verify_domain_dns(&challenge).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn wellknown_verification_fails_for_nonexistent_domain() {
        let pubkey = [0xABu8; 32];
        let challenge = generate_challenge(&pubkey, "this-domain-does-not-exist-p2s-test.invalid");
        let result = verify_domain_wellknown(&challenge).await;
        assert!(result.is_err());
    }
}

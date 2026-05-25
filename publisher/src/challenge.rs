/// A challenge token that the domain owner must place in DNS or .well-known.
/// Format: `p2s-verify=<hex(BLAKE3(pubkey || domain || nonce))>`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChallengeToken {
    pub token: String,
    pub nonce: [u8; 16],
    pub dns_record_name: String,
    pub wellknown_path: String,
}

/// Generate a domain-ownership challenge for a given pubkey and domain.
/// The publisher must place the token value as either:
///   - A DNS TXT record at `_p2s-verify.<domain>`
///   - An HTTPS response body at `https://<domain>/.well-known/p2s-verify`
pub fn generate_challenge(pubkey: &[u8], domain: &str) -> ChallengeToken {
    let mut nonce = [0u8; 16];
    use rand::RngCore;
    rand::rngs::OsRng.fill_bytes(&mut nonce);

    let token_hash = blake3::keyed_hash(
        &blake3::hash(b"p2s-domain-challenge").into(),
        &[pubkey, domain.as_bytes(), &nonce].concat(),
    );
    let token = format!("p2s-verify={}", hex_encode(token_hash.as_bytes()));

    ChallengeToken {
        token,
        nonce,
        dns_record_name: format!("_p2s-verify.{domain}"),
        wellknown_path: format!("https://{domain}/.well-known/p2s-verify"),
    }
}

/// Reconstruct a challenge token from a known nonce (for verification by the node).
/// The node receives the nonce from the publisher and recomputes the expected token.
pub fn reconstruct_challenge(pubkey: &[u8], domain: &str, nonce: &[u8; 16]) -> ChallengeToken {
    let token_hash = blake3::keyed_hash(
        &blake3::hash(b"p2s-domain-challenge").into(),
        &[pubkey, domain.as_bytes(), nonce.as_slice()].concat(),
    );
    let token = format!("p2s-verify={}", hex_encode(token_hash.as_bytes()));

    ChallengeToken {
        token,
        nonce: *nonce,
        dns_record_name: format!("_p2s-verify.{domain}"),
        wellknown_path: format!("https://{domain}/.well-known/p2s-verify"),
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn challenge_token_format() {
        let pubkey = [0xABu8; 32];
        let challenge = generate_challenge(&pubkey, "example.com");

        assert!(challenge.token.starts_with("p2s-verify="));
        // BLAKE3 output = 32 bytes = 64 hex chars, plus "p2s-verify=" prefix
        assert_eq!(challenge.token.len(), 11 + 64);
        assert_eq!(challenge.dns_record_name, "_p2s-verify.example.com");
        assert_eq!(challenge.wellknown_path, "https://example.com/.well-known/p2s-verify");
    }

    #[test]
    fn reconstruct_matches_generate() {
        let pubkey = [0xABu8; 32];
        let generated = generate_challenge(&pubkey, "example.com");
        let reconstructed = reconstruct_challenge(&pubkey, "example.com", &generated.nonce);
        assert_eq!(generated.token, reconstructed.token);
        assert_eq!(generated.dns_record_name, reconstructed.dns_record_name);
        assert_eq!(generated.wellknown_path, reconstructed.wellknown_path);
    }

    #[test]
    fn different_nonces_produce_different_tokens() {
        let pubkey = [0xABu8; 32];
        let c1 = generate_challenge(&pubkey, "example.com");
        let c2 = generate_challenge(&pubkey, "example.com");
        assert_ne!(c1.token, c2.token);
        assert_ne!(c1.nonce, c2.nonce);
    }

    #[test]
    fn different_domains_produce_different_tokens() {
        let pubkey = [0xABu8; 32];
        let c1 = generate_challenge(&pubkey, "a.com");
        let c2 = generate_challenge(&pubkey, "b.com");
        assert_ne!(c1.token, c2.token);
    }

    // ── reconstruct_challenge additional cases ────────────────────────────

    #[test]
    fn different_pubkeys_same_nonce_produce_different_tokens() {
        let nonce = [0x11u8; 16];
        let pubkey_a = [0xAAu8; 32];
        let pubkey_b = [0xBBu8; 32];

        let token_a = reconstruct_challenge(&pubkey_a, "example.com", &nonce).token;
        let token_b = reconstruct_challenge(&pubkey_b, "example.com", &nonce).token;

        assert_ne!(
            token_a, token_b,
            "different pubkeys must produce different tokens even with the same nonce"
        );
    }

    #[test]
    fn different_domains_same_nonce_produce_different_tokens() {
        let nonce = [0x22u8; 16];
        let pubkey = [0xCCu8; 32];

        let token_alpha = reconstruct_challenge(&pubkey, "alpha.example.com", &nonce).token;
        let token_beta = reconstruct_challenge(&pubkey, "beta.example.com", &nonce).token;

        assert_ne!(
            token_alpha, token_beta,
            "different domains must produce different tokens even with the same nonce"
        );
    }
}

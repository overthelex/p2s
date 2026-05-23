use thiserror::Error;

#[derive(Debug, Error)]
pub enum PublisherError {
    #[error("DNS lookup failed for {domain}: {reason}")]
    DnsLookupFailed { domain: String, reason: String },

    #[error("no matching TXT record found for {domain}")]
    NoMatchingTxtRecord { domain: String },

    #[error("HTTP fetch failed for {url}: {reason}")]
    HttpFetchFailed { url: String, reason: String },

    #[error("well-known token mismatch for {domain}")]
    WellKnownTokenMismatch { domain: String },

    #[error("domain verification failed: {0}")]
    VerificationFailed(String),
}

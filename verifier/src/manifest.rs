use crate::harden::harden_tool_description;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("endpoint is not a valid URL: {0}")]
    InvalidEndpointUrl(String),

    #[error("endpoint must use https scheme, got {0}")]
    InsecureScheme(String),

    #[error("endpoint must not contain userinfo")]
    EndpointHasUserinfo,

    #[error("manifest fetch failed: {0}")]
    FetchFailed(String),

    #[error("manifest is not valid JSON: {0}")]
    InvalidJson(String),

    #[error("manifest hash mismatch")]
    HashMismatch,

    #[error("manifest has no tools")]
    NoTools,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    pub tools: Vec<ToolEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolEntry {
    pub name: String,
    pub description: String,
}

/// Validate that an endpoint is a well-formed HTTPS URL.
pub fn validate_endpoint(endpoint: &str) -> Result<Url, ManifestError> {
    let url = Url::parse(endpoint)
        .map_err(|e| ManifestError::InvalidEndpointUrl(e.to_string()))?;

    if url.scheme() != "https" {
        return Err(ManifestError::InsecureScheme(url.scheme().to_string()));
    }

    if url.username() != "" || url.password().is_some() {
        return Err(ManifestError::EndpointHasUserinfo);
    }

    Ok(url)
}

/// Parse and validate a manifest from raw bytes.
/// Hardens tool descriptions as a side effect.
pub fn parse_and_validate_manifest(bytes: &[u8]) -> Result<Manifest, ManifestError> {
    let mut manifest: Manifest = serde_json::from_slice(bytes)
        .map_err(|e| ManifestError::InvalidJson(e.to_string()))?;

    if manifest.tools.is_empty() {
        return Err(ManifestError::NoTools);
    }

    for tool in &mut manifest.tools {
        tool.description = harden_tool_description(&tool.description);
    }

    Ok(manifest)
}

/// Verify that manifest bytes match the expected BLAKE3 hash.
pub fn verify_manifest_hash(bytes: &[u8], expected_hash: &[u8]) -> Result<(), ManifestError> {
    let computed = blake3::hash(bytes);
    if computed.as_bytes() != expected_hash {
        return Err(ManifestError::HashMismatch);
    }
    Ok(())
}

/// Fetch manifest bytes from the given endpoint URL.
pub async fn fetch_manifest(endpoint: &str) -> Result<Vec<u8>, ManifestError> {
    let _url = validate_endpoint(endpoint)?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| ManifestError::FetchFailed(e.to_string()))?;

    let response = client
        .get(endpoint)
        .send()
        .await
        .map_err(|e| ManifestError::FetchFailed(e.to_string()))?;

    if !response.status().is_success() {
        return Err(ManifestError::FetchFailed(format!(
            "HTTP {}",
            response.status()
        )));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| ManifestError::FetchFailed(e.to_string()))?;

    Ok(bytes.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_https_endpoint() {
        assert!(validate_endpoint("https://api.example.com/manifest").is_ok());
    }

    #[test]
    fn http_endpoint_rejected() {
        assert!(matches!(
            validate_endpoint("http://api.example.com/manifest"),
            Err(ManifestError::InsecureScheme(_))
        ));
    }

    #[test]
    fn javascript_endpoint_rejected() {
        assert!(matches!(
            validate_endpoint("javascript:alert(1)"),
            Err(ManifestError::InsecureScheme(_))
        ));
    }

    #[test]
    fn ftp_endpoint_rejected() {
        assert!(matches!(
            validate_endpoint("ftp://files.example.com/manifest"),
            Err(ManifestError::InsecureScheme(_))
        ));
    }

    #[test]
    fn endpoint_with_userinfo_rejected() {
        assert!(matches!(
            validate_endpoint("https://user:pass@example.com/manifest"),
            Err(ManifestError::EndpointHasUserinfo)
        ));
    }

    #[test]
    fn invalid_url_rejected() {
        assert!(matches!(
            validate_endpoint("not a url at all"),
            Err(ManifestError::InvalidEndpointUrl(_))
        ));
    }

    #[test]
    fn valid_manifest_parses() {
        let manifest_json = serde_json::json!({
            "name": "test-service",
            "version": "1.0.0",
            "tools": [
                {"name": "search", "description": "Search for documents"}
            ]
        });
        let bytes = serde_json::to_vec(&manifest_json).unwrap();
        let manifest = parse_and_validate_manifest(&bytes).unwrap();
        assert_eq!(manifest.name, "test-service");
        assert_eq!(manifest.tools.len(), 1);
        assert_eq!(manifest.tools[0].name, "search");
    }

    #[test]
    fn manifest_with_no_tools_rejected() {
        let manifest_json = serde_json::json!({
            "name": "empty-service",
            "version": "1.0.0",
            "tools": []
        });
        let bytes = serde_json::to_vec(&manifest_json).unwrap();
        assert!(matches!(
            parse_and_validate_manifest(&bytes),
            Err(ManifestError::NoTools)
        ));
    }

    #[test]
    fn manifest_tool_descriptions_hardened() {
        let long_desc = "x".repeat(1000);
        let manifest_json = serde_json::json!({
            "name": "test",
            "version": "1.0",
            "tools": [{"name": "t", "description": long_desc}]
        });
        let bytes = serde_json::to_vec(&manifest_json).unwrap();
        let manifest = parse_and_validate_manifest(&bytes).unwrap();
        assert!(manifest.tools[0].description.len() <= 512);
    }

    #[test]
    fn invalid_json_rejected() {
        assert!(matches!(
            parse_and_validate_manifest(b"not json"),
            Err(ManifestError::InvalidJson(_))
        ));
    }

    #[test]
    fn manifest_hash_matches() {
        let data = b"manifest content";
        let hash = blake3::hash(data);
        assert!(verify_manifest_hash(data, hash.as_bytes()).is_ok());
    }

    #[test]
    fn manifest_hash_mismatch() {
        let data = b"manifest content";
        let wrong_hash = blake3::hash(b"different");
        assert!(matches!(
            verify_manifest_hash(data, wrong_hash.as_bytes()),
            Err(ManifestError::HashMismatch)
        ));
    }
}

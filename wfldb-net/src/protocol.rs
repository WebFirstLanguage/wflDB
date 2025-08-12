//! Protocol definitions and utilities

use wfldb_core::*;

/// Protocol version
pub const PROTOCOL_VERSION: u8 = 1;

/// Maximum header size (prevents DoS)
pub const MAX_HEADER_SIZE: usize = 64 * 1024; // 64KB

/// Maximum body size for small objects
pub const MAX_SMALL_OBJECT_SIZE: usize = 64 * 1024 * 1024; // 64MB

/// Protocol error types
#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("Invalid frame format: {0}")]
    InvalidFrame(String),
    
    #[error("Header too large: {0} bytes (max: {1})")]
    HeaderTooLarge(usize, usize),
    
    #[error("Unsupported protocol version: {0}")]
    UnsupportedVersion(u8),
    
    #[error("Malformed header: {0}")]
    MalformedHeader(String),
    
    #[error("Missing required field: {0}")]
    MissingField(String),
}

/// Validate protocol frame
pub fn validate_frame(header_size: usize, protocol_version: u8) -> std::result::Result<(), ProtocolError> {
    if header_size > MAX_HEADER_SIZE {
        return Err(ProtocolError::HeaderTooLarge(header_size, MAX_HEADER_SIZE));
    }
    
    if protocol_version != PROTOCOL_VERSION {
        return Err(ProtocolError::UnsupportedVersion(protocol_version));
    }
    
    Ok(())
}

/// Protocol constants for field names
pub mod fields {
    pub const REQUEST_ID: &str = "request_id";
    pub const BUCKET: &str = "bucket";
    pub const KEY: &str = "key";
    pub const REQUEST_TYPE: &str = "request_type";
    pub const TIMESTAMP: &str = "timestamp";
    pub const NONCE: &str = "nonce";
    pub const CONTENT_LENGTH: &str = "content_length";
    pub const CONTENT_HASH: &str = "content_hash";
}

/// Canonical request builder for signature verification
pub struct CanonicalRequest {
    method: String,
    uri: String,
    query_string: String,
    headers: Vec<(String, String)>,
    payload_hash: String,
    timestamp: u64,
    nonce: String,
}

impl CanonicalRequest {
    pub fn new(method: &str, uri: &str) -> Self {
        CanonicalRequest {
            method: method.to_uppercase(),
            uri: uri.to_string(),
            query_string: String::new(),
            headers: Vec::new(),
            payload_hash: String::new(),
            timestamp: 0,
            nonce: String::new(),
        }
    }
    
    pub fn with_timestamp(mut self, timestamp: u64) -> Self {
        self.timestamp = timestamp;
        self
    }
    
    pub fn with_nonce(mut self, nonce: String) -> Self {
        self.nonce = nonce;
        self
    }
    
    pub fn with_payload_hash(mut self, hash: String) -> Self {
        self.payload_hash = hash;
        self
    }
    
    pub fn add_header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_lowercase(), value.to_string()));
        self
    }
    
    /// Build canonical string for signing
    pub fn build(&mut self) -> String {
        // Sort headers by name
        self.headers.sort_by(|a, b| a.0.cmp(&b.0));
        
        let mut canonical = Vec::new();
        canonical.push(self.method.clone());
        canonical.push(self.uri.clone());
        canonical.push(self.query_string.clone());
        
        // Canonical headers
        let headers_str = self.headers.iter()
            .map(|(name, value)| format!("{}:{}", name, value.trim()))
            .collect::<Vec<_>>()
            .join("\n");
        canonical.push(headers_str);
        
        // Signed headers list
        let signed_headers = self.headers.iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>()
            .join(";");
        canonical.push(signed_headers);
        
        canonical.push(self.payload_hash.clone());
        canonical.push(self.timestamp.to_string());
        canonical.push(self.nonce.clone());
        
        canonical.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_canonical_request() {
        let mut request = CanonicalRequest::new("PUT", "/v1/photos/cat.jpg")
            .with_timestamp(1234567890)
            .with_nonce("abc123".to_string())
            .with_payload_hash("sha256hash".to_string())
            .add_header("content-type", "image/jpeg")
            .add_header("x-wfldb-version", "1");
        
        let canonical = request.build();
        
        assert!(canonical.contains("PUT"));
        assert!(canonical.contains("/v1/photos/cat.jpg"));
        assert!(canonical.contains("content-type:image/jpeg"));
        assert!(canonical.contains("1234567890"));
        assert!(canonical.contains("abc123"));
    }
    
    #[test]
    fn test_frame_validation() {
        // Valid frame
        assert!(validate_frame(1024, PROTOCOL_VERSION).is_ok());
        
        // Header too large
        assert!(validate_frame(MAX_HEADER_SIZE + 1, PROTOCOL_VERSION).is_err());
        
        // Wrong version
        assert!(validate_frame(1024, PROTOCOL_VERSION + 1).is_err());
    }
}
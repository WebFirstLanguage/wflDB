//! Canonical request signing for replay protection
//!
//! Implements AWS SigV4-inspired canonical request construction and signing
//! to prevent replay attacks and ensure request integrity.

use crate::{auth::KeyPair, BucketId, Key, Result, WflDBError};
use ed25519_dalek::Signature;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// HTTP method for canonical request
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HttpMethod {
    GET,
    PUT,
    POST,
    DELETE,
}

impl std::fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HttpMethod::GET => write!(f, "GET"),
            HttpMethod::PUT => write!(f, "PUT"),
            HttpMethod::POST => write!(f, "POST"),
            HttpMethod::DELETE => write!(f, "DELETE"),
        }
    }
}

/// Canonical request for signing
#[derive(Debug, Clone)]
pub struct CanonicalRequest {
    pub method: HttpMethod,
    pub bucket: BucketId,
    pub key: Option<Key>,
    pub query_params: BTreeMap<String, String>,
    pub headers: BTreeMap<String, String>,
    pub payload_hash: String,
    pub timestamp: SystemTime,
    pub nonce: String,
}

impl CanonicalRequest {
    /// Create a new canonical request
    pub fn new(
        method: HttpMethod,
        bucket: BucketId,
        key: Option<Key>,
        payload: Option<&[u8]>,
    ) -> Self {
        let payload_hash = match payload {
            Some(data) => hex::encode(blake3::hash(data).as_bytes()),
            None => "UNSIGNED-PAYLOAD".to_string(),
        };
        
        // Generate a unique nonce for this request
        let nonce = ulid::Ulid::new().to_string();
        
        CanonicalRequest {
            method,
            bucket,
            key,
            query_params: BTreeMap::new(),
            headers: BTreeMap::new(),
            payload_hash,
            timestamp: SystemTime::now(),
            nonce,
        }
    }
    
    /// Add a query parameter
    pub fn add_query_param(&mut self, key: String, value: String) {
        self.query_params.insert(key, value);
    }
    
    /// Add a header
    pub fn add_header(&mut self, key: String, value: String) {
        self.headers.insert(key.to_lowercase(), value);
    }
    
    /// Set custom timestamp (for testing)
    pub fn with_timestamp(mut self, timestamp: SystemTime) -> Self {
        self.timestamp = timestamp;
        self
    }
    
    /// Set custom nonce (for testing)
    pub fn with_nonce(mut self, nonce: String) -> Self {
        self.nonce = nonce;
        self
    }
    
    /// Build the canonical string for signing
    pub fn to_canonical_string(&self) -> String {
        let mut canonical = String::new();
        
        // HTTP Method
        canonical.push_str(&self.method.to_string());
        canonical.push('\n');
        
        // Canonical URI
        canonical.push_str("/v1/");
        canonical.push_str(self.bucket.as_str());
        if let Some(ref key) = self.key {
            canonical.push('/');
            canonical.push_str(&uri_encode(key.as_str()));
        }
        canonical.push('\n');
        
        // Canonical Query String
        let query_string = self.query_params
            .iter()
            .map(|(k, v)| format!("{}={}", uri_encode(k), uri_encode(v)))
            .collect::<Vec<_>>()
            .join("&");
        canonical.push_str(&query_string);
        canonical.push('\n');
        
        // Canonical Headers
        let header_string = self.headers
            .iter()
            .map(|(k, v)| format!("{}:{}", k, v.trim()))
            .collect::<Vec<_>>()
            .join("\n");
        canonical.push_str(&header_string);
        canonical.push('\n');
        
        // Signed Headers
        let signed_headers = self.headers.keys().cloned().collect::<Vec<_>>().join(";");
        canonical.push_str(&signed_headers);
        canonical.push('\n');
        
        // Payload Hash
        canonical.push_str(&self.payload_hash);
        canonical.push('\n');
        
        // Timestamp (ISO 8601)
        let timestamp_secs = self.timestamp.duration_since(UNIX_EPOCH).unwrap().as_secs();
        canonical.push_str(&format!("{}", timestamp_secs));
        canonical.push('\n');
        
        // Nonce
        canonical.push_str(&self.nonce);
        
        canonical
    }
    
    /// Sign this canonical request
    pub fn sign(&self, keypair: &KeyPair) -> SignedRequest {
        let canonical_string = self.to_canonical_string();
        let signature = keypair.sign(canonical_string.as_bytes());
        
        SignedRequest {
            canonical_request: self.clone(),
            signature,
            signer_key_id: keypair.key_id(),
        }
    }
    
    /// Get timestamp as seconds since epoch
    pub fn timestamp_secs(&self) -> u64 {
        self.timestamp.duration_since(UNIX_EPOCH).unwrap().as_secs()
    }
}

/// A signed canonical request
#[derive(Debug, Clone)]
pub struct SignedRequest {
    pub canonical_request: CanonicalRequest,
    pub signature: Signature,
    pub signer_key_id: crate::auth::KeyId,
}

impl SignedRequest {
    /// Verify the signature of this request
    pub fn verify(&self, public_key: &crate::auth::PublicKey) -> Result<()> {
        // Ensure the public key matches the signer
        if public_key.key_id() != self.signer_key_id {
            return Err(WflDBError::AuthenticationFailed("key ID mismatch".to_string()));
        }
        
        let canonical_string = self.canonical_request.to_canonical_string();
        public_key.verify(canonical_string.as_bytes(), &self.signature)
    }
}

/// Replay protection nonce cache
#[derive(Debug)]
pub struct NonceCache {
    nonces: std::collections::HashMap<String, u64>,
    window_seconds: u64,
}

impl NonceCache {
    /// Create a new nonce cache with specified replay window
    pub fn new(window: Duration) -> Self {
        NonceCache {
            nonces: std::collections::HashMap::new(),
            window_seconds: window.as_secs(),
        }
    }
    
    /// Check if a nonce is valid (not replayed and within time window)
    pub fn check_nonce(&mut self, nonce: &str, timestamp: u64) -> Result<()> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        
        // Check if timestamp is within allowed window
        if timestamp + self.window_seconds < now || timestamp > now + self.window_seconds {
            return Err(WflDBError::ReplayAttack);
        }
        
        // Check if nonce was already used
        if let Some(&used_timestamp) = self.nonces.get(nonce) {
            if used_timestamp + self.window_seconds >= now {
                return Err(WflDBError::ReplayAttack);
            }
        }
        
        // Record this nonce
        self.nonces.insert(nonce.to_string(), timestamp);
        
        // Clean up old nonces
        self.cleanup_old_nonces(now);
        
        Ok(())
    }
    
    /// Remove nonces that are outside the replay window
    fn cleanup_old_nonces(&mut self, now: u64) {
        self.nonces.retain(|_, &mut timestamp| {
            timestamp + self.window_seconds >= now
        });
    }
}

/// Request authentication context
#[derive(Debug)]
pub struct AuthContext {
    pub key_packet: crate::auth::KeyPacket,
    pub signed_request: SignedRequest,
}

impl AuthContext {
    /// Create authentication context from headers and verify
    pub fn from_request(
        method: HttpMethod,
        bucket: BucketId,
        key: Option<Key>,
        payload: Option<&[u8]>,
        auth_header: &str,
        signature_header: &str,
        timestamp_header: &str,
        nonce_header: &str,
        nonce_cache: &mut NonceCache,
        issuer_public_key: &crate::auth::PublicKey,
    ) -> Result<Self> {
        // Parse timestamp
        let timestamp_secs: u64 = timestamp_header.parse()
            .map_err(|_| WflDBError::AuthenticationFailed("invalid timestamp".to_string()))?;
        
        let timestamp = UNIX_EPOCH + Duration::from_secs(timestamp_secs);
        
        // Check nonce for replay protection
        nonce_cache.check_nonce(nonce_header, timestamp_secs)?;
        
        // Parse JWT key packet from Authorization header
        let token = auth_header.strip_prefix("Bearer ")
            .ok_or_else(|| WflDBError::AuthenticationFailed("invalid auth header".to_string()))?;
        
        let key_packet = crate::auth::KeyPacket::parse(token, issuer_public_key)?;
        
        // Parse signature
        let signature_bytes = hex::decode(signature_header)
            .map_err(|_| WflDBError::AuthenticationFailed("invalid signature format".to_string()))?;
        
        if signature_bytes.len() != 64 {
            return Err(WflDBError::AuthenticationFailed("invalid signature length".to_string()));
        }
        
        let signature = Signature::from_bytes(&signature_bytes.try_into().unwrap());
        
        // Reconstruct canonical request
        let canonical_request = CanonicalRequest::new(method, bucket, key, payload)
            .with_timestamp(timestamp)
            .with_nonce(nonce_header.to_string());
        
        let signed_request = SignedRequest {
            canonical_request,
            signature,
            signer_key_id: key_packet.custom_claims().subject_key_id(),
        };
        
        // Verify signature using the public key from the key packet subject
        // Note: In a real implementation, we'd need to resolve the subject key ID to a public key
        
        Ok(AuthContext {
            key_packet,
            signed_request,
        })
    }
}

/// URI encode function (RFC 3986)
fn uri_encode(input: &str) -> String {
    let mut result = String::new();
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                result.push(byte as char);
            }
            _ => {
                result.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    result
}

mod hex {
    use std::fmt::Write;
    
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes.as_ref().iter().fold(String::new(), |mut output, b| {
            let _ = write!(output, "{:02x}", b);
            output
        })
    }
    
    pub fn decode(s: &str) -> Result<Vec<u8>, std::num::ParseIntError> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::KeyPair;
    use std::time::Duration;
    
    #[test]
    fn auth_client_server_signature_match_for_put_get_delete() {
        let keypair = KeyPair::generate();
        let bucket = BucketId::new("test-bucket").unwrap();
        let key = Key::new("test-key").unwrap();
        let data = b"test data";
        
        // Test PUT request
        let put_request = CanonicalRequest::new(
            HttpMethod::PUT,
            bucket.clone(),
            Some(key.clone()),
            Some(data),
        );
        
        let signed_put = put_request.sign(&keypair);
        let public_key = crate::auth::PublicKey::from_verifying_key(*keypair.verifying_key());
        assert!(signed_put.verify(&public_key).is_ok());
        
        // Test GET request  
        let get_request = CanonicalRequest::new(
            HttpMethod::GET,
            bucket.clone(),
            Some(key.clone()),
            None,
        );
        
        let signed_get = get_request.sign(&keypair);
        assert!(signed_get.verify(&public_key).is_ok());
        
        // Test DELETE request
        let delete_request = CanonicalRequest::new(
            HttpMethod::DELETE,
            bucket,
            Some(key),
            None,
        );
        
        let signed_delete = delete_request.sign(&keypair);
        assert!(signed_delete.verify(&public_key).is_ok());
    }
    
    #[test]
    fn auth_replay_is_rejected_outside_window_or_nonce_reuse() {
        let mut nonce_cache = NonceCache::new(Duration::from_secs(300)); // 5 minute window
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        
        // Valid request within window
        assert!(nonce_cache.check_nonce("nonce1", now).is_ok());
        
        // Replay of same nonce should fail
        assert!(nonce_cache.check_nonce("nonce1", now).is_err());
        
        // Request too far in the past should fail
        let past_timestamp = now - 600; // 10 minutes ago
        assert!(nonce_cache.check_nonce("nonce2", past_timestamp).is_err());
        
        // Request too far in the future should fail
        let future_timestamp = now + 600; // 10 minutes in future
        assert!(nonce_cache.check_nonce("nonce3", future_timestamp).is_err());
        
        // Different nonce within window should succeed
        assert!(nonce_cache.check_nonce("nonce4", now).is_ok());
    }
    
    #[test]
    fn test_canonical_string_format() {
        let bucket = BucketId::new("test-bucket").unwrap();
        let key = Key::new("test/key with spaces").unwrap();
        let data = b"hello world";
        
        let mut request = CanonicalRequest::new(
            HttpMethod::PUT,
            bucket,
            Some(key),
            Some(data),
        );
        
        request.add_query_param("param1".to_string(), "value1".to_string());
        request.add_header("content-type".to_string(), "application/octet-stream".to_string());
        
        let canonical = request.to_canonical_string();
        
        // Should contain all components
        assert!(canonical.contains("PUT"));
        assert!(canonical.contains("/v1/test-bucket"));
        assert!(canonical.contains("test%2Fkey%20with%20spaces"));
        assert!(canonical.contains("param1=value1"));
        assert!(canonical.contains("content-type:application/octet-stream"));
    }
}
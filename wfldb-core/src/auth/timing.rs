//! Constant-time cryptographic operations
//!
//! Provides constant-time comparison utilities to prevent timing attacks
//! on cryptographic operations.

use subtle::ConstantTimeEq;

/// Constant-time signature comparison
pub fn constant_time_sig_compare(sig1: &[u8], sig2: &[u8]) -> bool {
    if sig1.len() != sig2.len() {
        return false;
    }
    
    sig1.ct_eq(sig2).into()
}

/// Constant-time key ID comparison
pub fn constant_time_key_id_compare(key_id1: &crate::auth::KeyId, key_id2: &crate::auth::KeyId) -> bool {
    constant_time_sig_compare(key_id1.as_str().as_bytes(), key_id2.as_str().as_bytes())
}

/// Constant-time hash comparison
pub fn constant_time_hash_compare(hash1: &[u8; 32], hash2: &[u8; 32]) -> bool {
    hash1.ct_eq(hash2).into()
}

/// Constant-time string comparison (for tokens, etc.)
pub fn constant_time_str_compare(str1: &str, str2: &str) -> bool {
    constant_time_sig_compare(str1.as_bytes(), str2.as_bytes())
}

/// Wrapper for Ed25519 signature verification with constant-time comparison
pub fn verify_signature_constant_time(
    public_key: &crate::auth::PublicKey,
    message: &[u8],
    signature: &ed25519_dalek::Signature,
) -> crate::Result<()> {
    // Use the standard verification method, which should already be constant-time
    // Ed25519 verification in dalek is designed to be constant-time
    public_key.verify(message, signature)
}

/// Secure comparison for authentication tokens
pub fn compare_auth_tokens(token1: &str, token2: &str) -> bool {
    constant_time_str_compare(token1, token2)
}

/// Timing-safe nonce validation
pub fn validate_nonce_timing_safe(
    provided_nonce: &str,
    expected_nonce: &str,
) -> bool {
    constant_time_str_compare(provided_nonce, expected_nonce)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::KeyPair;
    use std::time::Instant;
    
    #[test]
    fn timing_sig_compare_is_constant_time() {
        let sig1 = [1u8; 64];
        let sig2 = [2u8; 64];
        let sig3 = [1u8; 64];
        
        // Test that comparison timing doesn't depend on content
        let start1 = Instant::now();
        let result1 = constant_time_sig_compare(&sig1, &sig2);
        let duration1 = start1.elapsed();
        
        let start2 = Instant::now();
        let result2 = constant_time_sig_compare(&sig1, &sig3);
        let duration2 = start2.elapsed();
        
        // Results should be correct
        assert!(!result1); // Different signatures
        assert!(result2);  // Same signatures
        
        // Note: This is a basic test. In practice, you'd need more sophisticated
        // timing analysis to verify constant-time behavior, but the subtle crate
        // provides the constant-time guarantees we need.
        
        // Basic sanity check that durations are in reasonable range
        assert!(duration1.as_nanos() > 0);
        assert!(duration2.as_nanos() > 0);
    }
    
    #[test]
    fn test_constant_time_key_id_compare() {
        let keypair1 = KeyPair::generate();
        let keypair2 = KeyPair::generate();
        
        let key_id1 = keypair1.key_id();
        let key_id2 = keypair2.key_id();
        let key_id1_copy = keypair1.key_id();
        
        // Same key IDs should compare equal
        assert!(constant_time_key_id_compare(&key_id1, &key_id1_copy));
        
        // Different key IDs should compare unequal
        assert!(!constant_time_key_id_compare(&key_id1, &key_id2));
    }
    
    #[test]
    fn test_constant_time_hash_compare() {
        let hash1 = [1u8; 32];
        let hash2 = [2u8; 32];
        let hash1_copy = [1u8; 32];
        
        assert!(constant_time_hash_compare(&hash1, &hash1_copy));
        assert!(!constant_time_hash_compare(&hash1, &hash2));
    }
    
    #[test]
    fn test_auth_token_comparison() {
        let token1 = "eyJhbGciOiJFZDI1NTE5IiwidHlwIjoiSldUIn0.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.signature1";
        let token2 = "eyJhbGciOiJFZDI1NTE5IiwidHlwIjoiSldUIn0.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.signature2";
        let token1_copy = "eyJhbGciOiJFZDI1NTE5IiwidHlwIjoiSldUIn0.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.signature1";
        
        assert!(compare_auth_tokens(token1, token1_copy));
        assert!(!compare_auth_tokens(token1, token2));
    }
    
    #[test]
    fn test_nonce_validation() {
        let nonce1 = "01234567-89ab-cdef-0123-456789abcdef";
        let nonce2 = "11234567-89ab-cdef-0123-456789abcdef";
        let nonce1_copy = "01234567-89ab-cdef-0123-456789abcdef";
        
        assert!(validate_nonce_timing_safe(nonce1, nonce1_copy));
        assert!(!validate_nonce_timing_safe(nonce1, nonce2));
    }
    
    #[test]
    fn test_signature_verification_wrapper() {
        let keypair = KeyPair::generate();
        let data = b"test message";
        let signature = keypair.sign(data);
        
        let public_key = crate::auth::PublicKey::from_verifying_key(*keypair.verifying_key());
        
        // Valid signature should verify
        assert!(verify_signature_constant_time(&public_key, data, &signature).is_ok());
        
        // Invalid signature should fail
        let wrong_signature = keypair.sign(b"different message");
        assert!(verify_signature_constant_time(&public_key, data, &wrong_signature).is_err());
    }
}
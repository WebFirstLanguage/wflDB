//! Ed25519 key management
//!
//! Provides key generation, storage, and cryptographic operations
//! using Ed25519 signatures for authentication.

use crate::{Result, WflDBError};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Ed25519 key pair for signing operations
#[derive(Clone)]
pub struct KeyPair {
    signing_key: SigningKey,
    verifying_key: VerifyingKey,
}

impl KeyPair {
    /// Generate a new Ed25519 key pair
    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        
        KeyPair {
            signing_key,
            verifying_key,
        }
    }
    
    /// Create key pair from signing key bytes
    pub fn from_signing_key_bytes(bytes: &[u8; 32]) -> Result<Self> {
        let signing_key = SigningKey::from_bytes(bytes);
        let verifying_key = signing_key.verifying_key();
        
        Ok(KeyPair {
            signing_key,
            verifying_key,
        })
    }
    
    /// Create key pair from signing key and verifying key bytes
    pub fn from_bytes(signing_bytes: &[u8; 32], verifying_bytes: &[u8; 32]) -> Result<Self> {
        let signing_key = SigningKey::from_bytes(signing_bytes);
        let verifying_key = VerifyingKey::from_bytes(verifying_bytes)
            .map_err(|e| WflDBError::AuthenticationFailed(format!("invalid verifying key: {}", e)))?;
        
        // Ensure the keys match
        if signing_key.verifying_key() != verifying_key {
            return Err(WflDBError::AuthenticationFailed("key pair mismatch".to_string()));
        }
        
        Ok(KeyPair {
            signing_key,
            verifying_key,
        })
    }
    
    /// Get the verifying key
    pub fn verifying_key(&self) -> &VerifyingKey {
        &self.verifying_key
    }
    
    /// Get signing key bytes (sensitive operation)
    pub fn signing_key_bytes(&self) -> [u8; 32] {
        self.signing_key.to_bytes()
    }
    
    /// Get verifying key bytes
    pub fn verifying_key_bytes(&self) -> [u8; 32] {
        self.verifying_key.to_bytes()
    }
    
    /// Sign data with this key pair
    pub fn sign(&self, data: &[u8]) -> Signature {
        self.signing_key.sign(data)
    }
    
    /// Get a unique identifier for this key (hash of public key)
    pub fn key_id(&self) -> KeyId {
        KeyId::from_verifying_key(&self.verifying_key)
    }
}

impl fmt::Debug for KeyPair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeyPair")
            .field("key_id", &self.key_id())
            .finish_non_exhaustive()
    }
}

/// Public key for verification operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicKey {
    verifying_key: VerifyingKey,
}

impl PublicKey {
    /// Create from verifying key
    pub fn from_verifying_key(verifying_key: VerifyingKey) -> Self {
        PublicKey { verifying_key }
    }
    
    /// Create from public key bytes
    pub fn from_bytes(bytes: &[u8; 32]) -> Result<Self> {
        let verifying_key = VerifyingKey::from_bytes(bytes)
            .map_err(|e| WflDBError::AuthenticationFailed(format!("invalid public key: {}", e)))?;
        
        Ok(PublicKey { verifying_key })
    }
    
    /// Get public key bytes
    pub fn to_bytes(&self) -> [u8; 32] {
        self.verifying_key.to_bytes()
    }
    
    /// Verify a signature against data
    pub fn verify(&self, data: &[u8], signature: &Signature) -> Result<()> {
        self.verifying_key
            .verify(data, signature)
            .map_err(|_| WflDBError::InvalidSignature)
    }
    
    /// Get a unique identifier for this key
    pub fn key_id(&self) -> KeyId {
        KeyId::from_verifying_key(&self.verifying_key)
    }
    
    /// Get the underlying verifying key
    pub fn verifying_key(&self) -> &VerifyingKey {
        &self.verifying_key
    }
}

impl Serialize for PublicKey {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_bytes(&self.verifying_key.to_bytes())
    }
}

impl<'de> Deserialize<'de> for PublicKey {
    fn deserialize<D>(deserializer: D) -> std::result::Result<PublicKey, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes: Vec<u8> = Deserialize::deserialize(deserializer)?;
        if bytes.len() != 32 {
            return Err(serde::de::Error::custom("invalid public key length"));
        }
        
        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(&bytes);
        
        PublicKey::from_bytes(&key_bytes)
            .map_err(|e| serde::de::Error::custom(format!("invalid public key: {}", e)))
    }
}

/// Unique identifier for a cryptographic key
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyId(String);

impl KeyId {
    /// Create key ID from verifying key (BLAKE3 hash of public key bytes)
    pub fn from_verifying_key(verifying_key: &VerifyingKey) -> Self {
        let hash = blake3::hash(&verifying_key.to_bytes());
        KeyId(hex::encode(&hash.as_bytes()[..16])) // Use first 16 bytes as hex
    }
    
    /// Create from string representation
    pub fn from_string(s: String) -> Self {
        KeyId(s)
    }
    
    /// Get string representation
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for KeyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

mod hex {
    use std::fmt::Write;
    
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes.as_ref().iter().fold(String::new(), |mut output, b| {
            let _ = write!(output, "{:02x}", b);
            output
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_key_pair_generation() {
        let keypair = KeyPair::generate();
        let key_id = keypair.key_id();
        
        // Key ID should be deterministic
        assert_eq!(key_id, keypair.key_id());
        
        // Should be able to sign and verify
        let data = b"test message";
        let signature = keypair.sign(data);
        
        let public_key = PublicKey::from_verifying_key(*keypair.verifying_key());
        assert!(public_key.verify(data, &signature).is_ok());
    }
    
    #[test]
    fn test_key_serialization() {
        let keypair = KeyPair::generate();
        let signing_bytes = keypair.signing_key_bytes();
        let verifying_bytes = keypair.verifying_key_bytes();
        
        // Reconstruct from bytes
        let reconstructed = KeyPair::from_bytes(&signing_bytes, &verifying_bytes).unwrap();
        
        // Should produce same signatures
        let data = b"test message";
        let sig1 = keypair.sign(data);
        let sig2 = reconstructed.sign(data);
        assert_eq!(sig1, sig2);
    }
    
    #[test]
    fn test_public_key_operations() {
        let keypair = KeyPair::generate();
        let public_key = PublicKey::from_verifying_key(*keypair.verifying_key());
        
        // Key IDs should match
        assert_eq!(keypair.key_id(), public_key.key_id());
        
        // Should verify signatures
        let data = b"test message";
        let signature = keypair.sign(data);
        assert!(public_key.verify(data, &signature).is_ok());
        
        // Should reject invalid signatures
        let bad_signature = keypair.sign(b"different message");
        assert!(public_key.verify(data, &bad_signature).is_err());
    }
}
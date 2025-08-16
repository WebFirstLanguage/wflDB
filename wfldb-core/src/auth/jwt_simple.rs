//! Simplified JWT implementation for key packets
//!
//! This is a simplified implementation that works correctly with jwt-simple

use crate::{auth::KeyId, Result, WflDBError};
use jwt_simple::prelude::*;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Custom claims for the JWT
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WflDBClaims {
    /// Subject key ID
    pub sub_key_id: String,
    /// Issuer key ID  
    pub iss_key_id: String,
    /// Permissions
    pub permissions: super::Permissions,
    /// Delegation chain
    pub delegation_chain: Vec<KeyId>,
}

/// Simple JWT key packet
#[derive(Debug, Clone)]
pub struct SimpleKeyPacket {
    token: String,
    claims: WflDBClaims,
}

impl SimpleKeyPacket {
    /// Create and sign a new key packet
    pub fn create(
        subject_key_id: KeyId,
        issuer_key_id: KeyId,
        permissions: super::Permissions,
        validity_duration: Duration,
        signing_key: &super::KeyPair,
    ) -> Result<Self> {
        // Convert to Ed25519 key for jwt-simple
        let signing_bytes = signing_key.signing_key_bytes();
        let verifying_bytes = signing_key.verifying_key_bytes();
        let mut full_key = [0u8; 64];
        full_key[..32].copy_from_slice(&signing_bytes);
        full_key[32..].copy_from_slice(&verifying_bytes);
        
        let key_pair = Ed25519KeyPair::from_bytes(&full_key)
            .map_err(|e| WflDBError::InvalidKeyPacket(format!("key conversion failed: {}", e)))?;
        
        let claims = WflDBClaims {
            sub_key_id: subject_key_id.as_str().to_string(),
            iss_key_id: issuer_key_id.as_str().to_string(),
            permissions,
            delegation_chain: vec![issuer_key_id],
        };
        
        let jwt_duration = jwt_simple::prelude::Duration::from_secs(validity_duration.as_secs());
        let jwt_claims = Claims::with_custom_claims(claims.clone(), jwt_duration);
        
        let token = key_pair
            .sign(jwt_claims)
            .map_err(|e| WflDBError::InvalidKeyPacket(format!("signing failed: {}", e)))?;
        
        Ok(SimpleKeyPacket { token, claims })
    }
    
    /// Parse and verify a JWT key packet
    pub fn parse(token: &str, verifying_key: &super::PublicKey) -> Result<Self> {
        let public_key = Ed25519PublicKey::from_bytes(&verifying_key.to_bytes())
            .map_err(|e| WflDBError::InvalidKeyPacket(format!("key conversion failed: {}", e)))?;
        
        let verified_claims = public_key
            .verify_token::<WflDBClaims>(token, None)
            .map_err(|e| WflDBError::InvalidKeyPacket(format!("verification failed: {}", e)))?;
        
        Ok(SimpleKeyPacket {
            token: token.to_string(),
            claims: verified_claims.custom,
        })
    }
    
    /// Get the token string
    pub fn token(&self) -> &str {
        &self.token
    }
    
    /// Get the custom claims
    pub fn claims(&self) -> &WflDBClaims {
        &self.claims
    }
    
    /// Get subject key ID
    pub fn subject_key_id(&self) -> KeyId {
        KeyId::from_string(self.claims.sub_key_id.clone())
    }
    
    /// Get issuer key ID
    pub fn issuer_key_id(&self) -> KeyId {
        KeyId::from_string(self.claims.iss_key_id.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{KeyPair, Permissions};
    use std::time::Duration;
    
    #[test]
    fn test_simple_jwt_roundtrip() {
        let keypair = KeyPair::generate();
        let permissions = Permissions::all();
        
        // Create packet
        let packet = SimpleKeyPacket::create(
            keypair.key_id(),
            keypair.key_id(),
            permissions,
            Duration::from_secs(3600),
            &keypair,
        ).unwrap();
        
        // Parse and verify
        let public_key = super::super::PublicKey::from_verifying_key(*keypair.verifying_key());
        let parsed_packet = SimpleKeyPacket::parse(packet.token(), &public_key).unwrap();
        
        // Claims should match
        assert_eq!(packet.subject_key_id(), parsed_packet.subject_key_id());
        assert_eq!(packet.claims().permissions.can_read, parsed_packet.claims().permissions.can_read);
    }
}
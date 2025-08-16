//! JWT key packet implementation
//!
//! Implements capability-based authorization using JWT tokens signed with Ed25519.
//! Key packets contain client permissions and can be delegated with restricted scopes.

use crate::{auth::KeyId, BucketId, Result, WflDBError};
use jwt_simple::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Permissions that can be granted in a key packet
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Permissions {
    /// Buckets this key can access (empty means all buckets)
    pub buckets: HashSet<String>,
    
    /// Whether this key can read objects
    pub can_read: bool,
    
    /// Whether this key can write objects
    pub can_write: bool,
    
    /// Whether this key can delete objects
    pub can_delete: bool,
    
    /// Whether this key can perform batch operations
    pub can_batch: bool,
    
    /// Whether this key can delegate permissions to other keys
    pub can_delegate: bool,
    
    /// Whether this key can revoke other keys (admin privilege)
    pub can_revoke: bool,
}

impl Permissions {
    /// Create permissions with all capabilities granted
    pub fn all() -> Self {
        Permissions {
            buckets: HashSet::new(),
            can_read: true,
            can_write: true,
            can_delete: true,
            can_batch: true,
            can_delegate: true,
            can_revoke: true,
        }
    }
    
    /// Create read-only permissions
    pub fn read_only() -> Self {
        Permissions {
            buckets: HashSet::new(),
            can_read: true,
            can_write: false,
            can_delete: false,
            can_batch: false,
            can_delegate: false,
            can_revoke: false,
        }
    }
    
    /// Create write permissions (read + write)
    pub fn read_write() -> Self {
        Permissions {
            buckets: HashSet::new(),
            can_read: true,
            can_write: true,
            can_delete: false,
            can_batch: false,
            can_delegate: false,
            can_revoke: false,
        }
    }
    
    /// Create permissions for specific buckets
    pub fn for_buckets(buckets: impl IntoIterator<Item = BucketId>) -> Self {
        let bucket_set = buckets.into_iter().map(|b| b.as_str().to_string()).collect();
        
        Permissions {
            buckets: bucket_set,
            can_read: true,
            can_write: true,
            can_delete: true,
            can_batch: true,
            can_delegate: false,
            can_revoke: false,
        }
    }
    
    /// Check if permissions allow access to a specific bucket
    pub fn allows_bucket(&self, bucket: &BucketId) -> bool {
        self.buckets.is_empty() || self.buckets.contains(bucket.as_str())
    }
    
    /// Check if this permission set is a subset of another (for delegation)
    pub fn is_subset_of(&self, other: &Permissions) -> bool {
        // Bucket restrictions must be same or more restrictive
        let bucket_check = if other.buckets.is_empty() {
            true // Other allows all buckets
        } else if self.buckets.is_empty() {
            false // Self allows all buckets but other is restricted
        } else {
            self.buckets.is_subset(&other.buckets)
        };
        
        bucket_check
            && (!self.can_read || other.can_read)
            && (!self.can_write || other.can_write)
            && (!self.can_delete || other.can_delete)
            && (!self.can_batch || other.can_batch)
            && (!self.can_delegate || other.can_delegate)
            && (!self.can_revoke || other.can_revoke)
    }
    
    /// Create intersection of two permission sets (most restrictive)
    pub fn intersect(&self, other: &Permissions) -> Permissions {
        let buckets = if self.buckets.is_empty() {
            other.buckets.clone()
        } else if other.buckets.is_empty() {
            self.buckets.clone()
        } else {
            self.buckets.intersection(&other.buckets).cloned().collect()
        };
        
        Permissions {
            buckets,
            can_read: self.can_read && other.can_read,
            can_write: self.can_write && other.can_write,
            can_delete: self.can_delete && other.can_delete,
            can_batch: self.can_batch && other.can_batch,
            can_delegate: self.can_delegate && other.can_delegate,
            can_revoke: self.can_revoke && other.can_revoke,
        }
    }
}

/// Custom claims in a JWT key packet
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomClaims {
    /// Permissions granted to this key
    pub permissions: Permissions,
    
    /// Optional delegation chain (for audit trail)
    pub delegation_chain: Vec<KeyId>,
}

/// Full claims structure for JWT key packet
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyPacketClaims {
    /// Subject (key ID of the key this packet authorizes)
    pub sub: String,
    
    /// Issuer (key ID of the key that signed this packet)
    pub iss: String,
    
    /// Custom claims
    #[serde(flatten)]
    pub custom: CustomClaims,
}

impl KeyPacketClaims {
    /// Create new key packet claims
    pub fn new(
        subject: KeyId,
        issuer: KeyId,
        permissions: Permissions,
        _validity_duration: Duration, // Will be handled by JWT library
    ) -> Self {
        KeyPacketClaims {
            sub: subject.as_str().to_string(),
            iss: issuer.as_str().to_string(),
            custom: CustomClaims {
                permissions,
                delegation_chain: vec![issuer],
            },
        }
    }
    
    /// Get subject key ID
    pub fn subject_key_id(&self) -> KeyId {
        KeyId::from_string(self.sub.clone())
    }
    
    /// Get issuer key ID
    pub fn issuer_key_id(&self) -> KeyId {
        KeyId::from_string(self.iss.clone())
    }
    
    /// Add a key to the delegation chain (for delegated tokens)
    pub fn add_to_delegation_chain(&mut self, delegator: KeyId) {
        self.custom.delegation_chain.push(delegator);
    }
}

/// JWT key packet for capability-based authorization
#[derive(Debug, Clone)]
pub struct KeyPacket {
    token: String,
    claims: KeyPacketClaims,
    issued_at: Option<u64>,
    expires_at: Option<u64>,
}

impl KeyPacket {
    /// Create and sign a new key packet
    pub fn create(
        custom_claims: KeyPacketClaims,
        signing_key: &crate::auth::KeyPair,
        validity_duration: Duration,
    ) -> Result<Self> {
        // Convert to Ed25519 key for jwt-simple
        // Try creating the key pair with concatenated private + public key (64 bytes total)
        let signing_bytes = signing_key.signing_key_bytes();
        let verifying_bytes = signing_key.verifying_key_bytes();
        let mut full_key = [0u8; 64];
        full_key[..32].copy_from_slice(&signing_bytes);
        full_key[32..].copy_from_slice(&verifying_bytes);
        
        let key_pair = Ed25519KeyPair::from_bytes(&full_key)
            .map_err(|e| WflDBError::InvalidKeyPacket(format!("key conversion failed: {}", e)))?;
        
        let jwt_duration = jwt_simple::prelude::Duration::from_secs(validity_duration.as_secs());
        let claims = Claims::with_custom_claims(custom_claims.clone(), jwt_duration);
        
        let token = key_pair
            .sign(claims)
            .map_err(|e| WflDBError::InvalidKeyPacket(format!("signing failed: {}", e)))?;
        
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        Ok(KeyPacket { 
            token, 
            claims: custom_claims,
            issued_at: Some(now),
            expires_at: Some(now + validity_duration.as_secs()),
        })
    }
    
    /// Parse and verify a JWT key packet
    pub fn parse(token: &str, verifying_key: &crate::auth::PublicKey) -> Result<Self> {
        // Convert to Ed25519 public key for jwt-simple
        let public_key = Ed25519PublicKey::from_bytes(&verifying_key.to_bytes())
            .map_err(|e| WflDBError::InvalidKeyPacket(format!("key conversion failed: {}", e)))?;
        
        let claims = public_key
            .verify_token::<KeyPacketClaims>(token, None)
            .map_err(|e| WflDBError::InvalidKeyPacket(format!("verification failed: {}", e)))?;
        
        // Extract the custom claims and reconstruct our structure
        let custom_claims_data = serde_json::to_value(&claims.custom)
            .map_err(|e| WflDBError::InvalidKeyPacket(format!("serialization error: {}", e)))?;
        
        let custom_claims: KeyPacketClaims = serde_json::from_value(custom_claims_data)
            .map_err(|e| WflDBError::InvalidKeyPacket(format!("deserialization error: {}", e)))?;
        
        Ok(KeyPacket {
            token: token.to_string(),
            claims: custom_claims,
            issued_at: claims.issued_at.map(|d| d.as_secs()),
            expires_at: claims.expires_at.map(|d| d.as_secs()),
        })
    }
    
    /// Get the token string
    pub fn token(&self) -> &str {
        &self.token
    }
    
    /// Get the custom claims
    pub fn custom_claims(&self) -> &KeyPacketClaims {
        &self.claims
    }
    
    /// Check if token is currently valid
    pub fn is_valid(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        match (self.issued_at, self.expires_at) {
            (Some(iat), Some(exp)) => now >= iat && now < exp,
            _ => false, // If we don't have timing info, consider invalid
        }
    }
    
    /// Check if this key packet allows a specific operation
    pub fn allows_operation(&self, bucket: &BucketId, operation: &Operation) -> bool {
        if !self.is_valid() {
            return false;
        }
        
        if !self.claims.custom.permissions.allows_bucket(bucket) {
            return false;
        }
        
        match operation {
            Operation::Read => self.claims.custom.permissions.can_read,
            Operation::Write => self.claims.custom.permissions.can_write,
            Operation::Delete => self.claims.custom.permissions.can_delete,
            Operation::Batch => self.claims.custom.permissions.can_batch,
            Operation::Delegate => self.claims.custom.permissions.can_delegate,
            Operation::Revoke => self.claims.custom.permissions.can_revoke,
        }
    }
    
    /// Create a delegated key packet with restricted permissions
    pub fn delegate(
        &self,
        target_key: KeyId,
        restricted_permissions: Permissions,
        validity_duration: Duration,
        delegating_key: &crate::auth::KeyPair,
    ) -> Result<KeyPacket> {
        if !self.claims.custom.permissions.can_delegate {
            return Err(WflDBError::InsufficientPermissions);
        }
        
        // Ensure delegated permissions are a subset of current permissions
        if !restricted_permissions.is_subset_of(&self.claims.custom.permissions) {
            return Err(WflDBError::AuthorizationFailed(
                "delegated permissions exceed current permissions".to_string(),
            ));
        }
        
        let mut new_claims = KeyPacketClaims::new(
            target_key,
            delegating_key.key_id(),
            restricted_permissions,
            validity_duration,
        );
        
        // Add delegation chain
        new_claims.custom.delegation_chain = self.claims.custom.delegation_chain.clone();
        new_claims.add_to_delegation_chain(delegating_key.key_id());
        
        KeyPacket::create(new_claims, delegating_key, validity_duration)
    }
}

/// Operations that can be performed on the system
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operation {
    Read,
    Write,
    Delete,
    Batch,
    Delegate,
    Revoke,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::KeyPair;
    use std::time::Duration;
    
    #[test]
    fn auth_jwt_ed25519_roundtrip_ok() {
        let keypair = KeyPair::generate();
        let permissions = Permissions::all();
        
        let claims = KeyPacketClaims::new(
            keypair.key_id(),
            keypair.key_id(),
            permissions,
            Duration::from_secs(3600),
        );
        
        // Create and sign packet
        let packet = KeyPacket::create(claims, &keypair, Duration::from_secs(3600)).unwrap();
        
        // Parse and verify packet
        let public_key = crate::auth::PublicKey::from_verifying_key(*keypair.verifying_key());
        let parsed_packet = KeyPacket::parse(packet.token(), &public_key).unwrap();
        
        // Claims should match
        assert_eq!(packet.custom_claims().sub, parsed_packet.custom_claims().sub);
        assert_eq!(packet.custom_claims().custom.permissions.can_read, parsed_packet.custom_claims().custom.permissions.can_read);
    }
    
    #[test] 
    fn auth_rejects_expired_or_future_nbf() {
        let keypair = KeyPair::generate();
        let permissions = Permissions::all();
        
        // Test expired token (0 duration)
        let expired_claims = KeyPacketClaims::new(
            keypair.key_id(),
            keypair.key_id(),
            permissions.clone(),
            Duration::from_secs(0),
        );
        
        let expired_packet = KeyPacket::create(expired_claims, &keypair, Duration::from_secs(0)).unwrap();
        
        // Sleep briefly to ensure expiration
        std::thread::sleep(Duration::from_millis(10));
        assert!(!expired_packet.is_valid());
        
        // Test valid token
        let valid_claims = KeyPacketClaims::new(
            keypair.key_id(),
            keypair.key_id(),
            permissions,
            Duration::from_secs(3600),
        );
        
        let valid_packet = KeyPacket::create(valid_claims, &keypair, Duration::from_secs(3600)).unwrap();
        assert!(valid_packet.is_valid());
    }
    
    #[test]
    fn test_permissions_subset() {
        let all_perms = Permissions::all();
        let read_only = Permissions::read_only();
        let bucket_specific = Permissions::for_buckets([BucketId::new("test").unwrap()]);
        
        assert!(read_only.is_subset_of(&all_perms));
        assert!(bucket_specific.is_subset_of(&all_perms));
        assert!(!all_perms.is_subset_of(&read_only));
    }
}
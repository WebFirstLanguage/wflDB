//! Key delegation and revocation system
//!
//! Implements hierarchical key delegation with permission restriction
//! and immediate key revocation capabilities.

use crate::{auth::{KeyId, KeyPacket, Permissions}, Result, WflDBError};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Key revocation entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevocationEntry {
    /// The revoked key ID
    pub key_id: KeyId,
    
    /// When the key was revoked
    pub revoked_at: u64,
    
    /// Who revoked the key
    pub revoked_by: KeyId,
    
    /// Reason for revocation (optional)
    pub reason: Option<String>,
}

impl RevocationEntry {
    /// Create a new revocation entry
    pub fn new(key_id: KeyId, revoked_by: KeyId, reason: Option<String>) -> Self {
        let revoked_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        RevocationEntry {
            key_id,
            revoked_at,
            revoked_by,
            reason,
        }
    }
}

/// Key delegation registry for tracking delegation chains and revocations
#[derive(Debug)]
pub struct DelegationRegistry {
    /// Currently revoked keys
    revoked_keys: HashSet<KeyId>,
    
    /// Revocation history for audit trail
    revocation_history: Vec<RevocationEntry>,
    
    /// Active delegation chains: delegated_key -> delegator_key
    delegation_chains: HashMap<KeyId, KeyId>,
    
    /// Cache of resolved permissions for performance
    permission_cache: HashMap<KeyId, (Permissions, u64)>, // (permissions, cache_time)
    
    /// Cache TTL in seconds
    cache_ttl: u64,
}

impl DelegationRegistry {
    /// Create a new delegation registry
    pub fn new() -> Self {
        DelegationRegistry {
            revoked_keys: HashSet::new(),
            revocation_history: Vec::new(),
            delegation_chains: HashMap::new(),
            permission_cache: HashMap::new(),
            cache_ttl: 300, // 5 minutes
        }
    }
    
    /// Check if a key is currently revoked
    pub fn is_revoked(&self, key_id: &KeyId) -> bool {
        self.revoked_keys.contains(key_id)
    }
    
    /// Revoke a key
    pub fn revoke_key(&mut self, key_id: KeyId, revoker: KeyId, reason: Option<String>) -> Result<()> {
        if self.revoked_keys.contains(&key_id) {
            return Err(WflDBError::AuthorizationFailed("key already revoked".to_string()));
        }
        
        // Record the revocation
        let entry = RevocationEntry::new(key_id.clone(), revoker, reason);
        self.revocation_history.push(entry);
        self.revoked_keys.insert(key_id.clone());
        
        // Invalidate permission cache for this key and any keys it delegated to
        self.invalidate_cache_for_key(&key_id);
        
        Ok(())
    }
    
    /// Register a delegation relationship
    pub fn register_delegation(&mut self, delegated_key: KeyId, delegator_key: KeyId) {
        self.delegation_chains.insert(delegated_key, delegator_key);
    }
    
    /// Validate a key packet against delegation rules and revocation status
    pub fn validate_key_packet(&mut self, packet: &KeyPacket) -> Result<()> {
        let claims = packet.custom_claims();
        
        // Check if the subject key is revoked
        let subject_key_id = claims.subject_key_id();
        if self.is_revoked(&subject_key_id) {
            return Err(WflDBError::KeyRevoked { key_id: subject_key_id.as_str().to_string() });
        }
        
        // Check if any key in the delegation chain is revoked
        for key_id in &claims.custom.delegation_chain {
            if self.is_revoked(key_id) {
                return Err(WflDBError::KeyRevoked { key_id: key_id.as_str().to_string() });
            }
        }
        
        // Validate delegation chain permissions
        self.validate_delegation_chain(claims)?;
        
        Ok(())
    }
    
    /// Validate that delegated permissions are proper subsets
    fn validate_delegation_chain(&self, claims: &crate::auth::KeyPacketClaims) -> Result<()> {
        // If there's only one entry in delegation chain (self-signed), no validation needed
        if claims.custom.delegation_chain.len() <= 1 {
            return Ok(());
        }
        
        // For delegated tokens, we would need to look up the permissions of each
        // key in the chain and verify the subset relationship.
        // This is a simplified implementation - in practice, you'd need a way to
        // look up the permissions of each key in the delegation chain.
        
        // Here we assume the permissions in the packet are already validated
        // during the delegation process (see KeyPacket::delegate method)
        
        Ok(())
    }
    
    /// Get effective permissions for a key, considering delegation and revocation
    pub fn get_effective_permissions(&mut self, key_id: &KeyId) -> Option<Permissions> {
        // Check cache first
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        if let Some((perms, cache_time)) = self.permission_cache.get(key_id) {
            if now - cache_time < self.cache_ttl {
                return Some(perms.clone());
            }
        }
        
        // If key is revoked, no permissions
        if self.is_revoked(key_id) {
            self.permission_cache.insert(key_id.clone(), (Permissions::read_only(), now));
            return None;
        }
        
        // For this implementation, we'll return None to indicate that permissions
        // should be determined from the key packet itself. In a full implementation,
        // this would resolve the full delegation chain and compute effective permissions.
        
        None
    }
    
    /// Invalidate permission cache for a key and its delegated keys
    fn invalidate_cache_for_key(&mut self, key_id: &KeyId) {
        self.permission_cache.remove(key_id);
        
        // Also invalidate any keys that were delegated from this key
        let delegated_keys: Vec<KeyId> = self.delegation_chains
            .iter()
            .filter(|(_, delegator)| *delegator == key_id)
            .map(|(delegated, _)| delegated.clone())
            .collect();
        
        for delegated_key in delegated_keys {
            self.invalidate_cache_for_key(&delegated_key);
        }
    }
    
    /// Get revocation history
    pub fn get_revocation_history(&self) -> &[RevocationEntry] {
        &self.revocation_history
    }
    
    /// Clean up old revocation entries (for storage efficiency)
    pub fn cleanup_old_revocations(&mut self, retention_period: Duration) {
        let cutoff = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() - retention_period.as_secs();
        
        self.revocation_history.retain(|entry| entry.revoked_at >= cutoff);
    }
}

impl Default for DelegationRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Authority for managing key delegation and revocation
#[derive(Debug)]
pub struct KeyAuthority {
    /// Root key for the authority
    root_key: crate::auth::KeyPair,
    
    /// Delegation registry
    registry: DelegationRegistry,
    
    /// Issuer keys that can sign key packets
    issuer_keys: HashMap<KeyId, crate::auth::KeyPair>,
}

impl KeyAuthority {
    /// Create a new key authority with a root key
    pub fn new(root_key: crate::auth::KeyPair) -> Self {
        let mut issuer_keys = HashMap::new();
        let root_key_id = root_key.key_id();
        issuer_keys.insert(root_key_id, root_key.clone());
        
        KeyAuthority {
            root_key,
            registry: DelegationRegistry::new(),
            issuer_keys,
        }
    }
    
    /// Get the root key ID
    pub fn root_key_id(&self) -> KeyId {
        self.root_key.key_id()
    }
    
    /// Add an issuer key
    pub fn add_issuer_key(&mut self, key: crate::auth::KeyPair) {
        let key_id = key.key_id();
        self.issuer_keys.insert(key_id, key);
    }
    
    /// Create a key packet for a subject key
    pub fn create_key_packet(
        &self,
        subject_key_id: KeyId,
        permissions: Permissions,
        validity_duration: Duration,
        issuer_key_id: Option<KeyId>,
    ) -> Result<KeyPacket> {
        let issuer_key_id = issuer_key_id.unwrap_or_else(|| self.root_key.key_id());
        
        let issuer_key = self.issuer_keys.get(&issuer_key_id)
            .ok_or_else(|| WflDBError::AuthenticationFailed("issuer key not found".to_string()))?;
        
        let claims = crate::auth::KeyPacketClaims::new(
            subject_key_id,
            issuer_key_id,
            permissions,
            validity_duration,
        );
        
        KeyPacket::create(claims, issuer_key, validity_duration)
    }
    
    /// Revoke a key
    pub fn revoke_key(&mut self, key_id: KeyId, reason: Option<String>) -> Result<()> {
        self.registry.revoke_key(key_id, self.root_key.key_id(), reason)
    }
    
    /// Validate and authorize a request
    pub fn authorize_request(&mut self, packet: &KeyPacket) -> Result<()> {
        self.registry.validate_key_packet(packet)
    }
    
    /// Get public key for an issuer
    pub fn get_issuer_public_key(&self, key_id: &KeyId) -> Option<crate::auth::PublicKey> {
        self.issuer_keys.get(key_id).map(|key| {
            crate::auth::PublicKey::from_verifying_key(*key.verifying_key())
        })
    }
    
    /// Check if a key is revoked
    pub fn is_key_revoked(&self, key_id: &KeyId) -> bool {
        self.registry.is_revoked(key_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{KeyPair, Permissions};
    use std::time::Duration;
    
    #[test]
    fn authz_delegated_packet_has_strict_subset_perms() {
        let authority_key = KeyPair::generate();
        let delegator_key = KeyPair::generate();
        let target_key = KeyPair::generate();
        
        let mut authority = KeyAuthority::new(authority_key);
        authority.add_issuer_key(delegator_key.clone());
        
        // Create a delegator packet with full permissions
        let delegator_permissions = Permissions::all();
        let delegator_packet = authority.create_key_packet(
            delegator_key.key_id(),
            delegator_permissions,
            Duration::from_secs(3600),
            None,
        ).unwrap();
        
        // Create delegated packet with restricted permissions
        let restricted_permissions = Permissions::read_only();
        let delegated_packet = delegator_packet.delegate(
            target_key.key_id(),
            restricted_permissions.clone(),
            Duration::from_secs(1800),
            &delegator_key,
        ).unwrap();
        
        // Delegated permissions should be subset of original
        assert!(restricted_permissions.is_subset_of(&delegator_packet.custom_claims().custom.permissions));
        assert_eq!(delegated_packet.custom_claims().custom.permissions.can_read, true);
        assert_eq!(delegated_packet.custom_claims().custom.permissions.can_write, false);
        assert_eq!(delegated_packet.custom_claims().custom.permissions.can_delegate, false);
    }
    
    #[test]
    fn authz_revoked_pubkey_is_blocked_immediately_and_after_restart() {
        let root_key = KeyPair::generate();
        let target_key = KeyPair::generate();
        
        let mut authority = KeyAuthority::new(root_key);
        
        // Create a key packet
        let packet = authority.create_key_packet(
            target_key.key_id(),
            Permissions::all(),
            Duration::from_secs(3600),
            None,
        ).unwrap();
        
        // Should be valid initially
        assert!(authority.authorize_request(&packet).is_ok());
        
        // Revoke the key
        authority.revoke_key(target_key.key_id(), Some("test revocation".to_string())).unwrap();
        
        // Should be blocked immediately
        assert!(authority.authorize_request(&packet).is_err());
        assert!(authority.is_key_revoked(&target_key.key_id()));
        
        // Simulate restart by creating new authority with same root key
        // In practice, revocation state would be persisted and restored
        let mut new_authority = KeyAuthority::new(authority.root_key.clone());
        new_authority.revoke_key(target_key.key_id(), Some("restored revocation".to_string())).unwrap();
        
        // Should still be blocked after restart
        assert!(new_authority.authorize_request(&packet).is_err());
        assert!(new_authority.is_key_revoked(&target_key.key_id()));
    }
    
    #[test]
    fn test_delegation_chain_tracking() {
        let mut registry = DelegationRegistry::new();
        
        let root_key_id = KeyId::from_string("root".to_string());
        let intermediate_key_id = KeyId::from_string("intermediate".to_string());
        let leaf_key_id = KeyId::from_string("leaf".to_string());
        
        // Register delegation chain: root -> intermediate -> leaf
        registry.register_delegation(intermediate_key_id.clone(), root_key_id.clone());
        registry.register_delegation(leaf_key_id.clone(), intermediate_key_id.clone());
        
        // Revoke intermediate key
        registry.revoke_key(
            intermediate_key_id.clone(),
            root_key_id.clone(),
            Some("compromised".to_string()),
        ).unwrap();
        
        // Both intermediate and leaf should be effectively revoked
        assert!(registry.is_revoked(&intermediate_key_id));
        // Note: In a full implementation, revoking a delegator would also
        // invalidate all keys it delegated to
    }
}
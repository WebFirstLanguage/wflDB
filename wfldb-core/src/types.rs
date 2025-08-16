//! Core data types for wflDB

use serde::{Deserialize, Serialize};
use std::time::SystemTime;

/// Unique bucket identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BucketId(String);

impl BucketId {
    /// Create a new bucket ID with validation
    pub fn new(name: &str) -> crate::Result<Self> {
        if name.is_empty() {
            return Err(crate::WflDBError::InvalidBucketName("empty name".to_string()));
        }
        
        // Validate bucket name (alphanumeric, hyphens, underscores only)
        if !name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
            return Err(crate::WflDBError::InvalidBucketName(
                format!("invalid characters in '{}'", name)
            ));
        }
        
        Ok(BucketId(name.to_string()))
    }
    
    /// Get the bucket name as a string slice
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for BucketId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Object key within a bucket
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Key(String);

impl Key {
    /// Create a new key with validation
    pub fn new(key: &str) -> crate::Result<Self> {
        if key.is_empty() {
            return Err(crate::WflDBError::InvalidKey("empty key".to_string()));
        }
        
        // Basic validation - no control characters
        if key.chars().any(|c| c.is_control()) {
            return Err(crate::WflDBError::InvalidKey(
                "control characters not allowed".to_string()
            ));
        }
        
        Ok(Key(key.to_string()))
    }
    
    /// Get the key as a string slice
    pub fn as_str(&self) -> &str {
        &self.0
    }
    
    /// Check if this key has the given prefix
    pub fn has_prefix(&self, prefix: &str) -> bool {
        self.0.starts_with(prefix)
    }
}

impl std::fmt::Display for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Version identifier using ULID for time-ordering
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Version(ulid::Ulid);

impl Version {
    /// Generate a new version with current timestamp
    pub fn new() -> Self {
        Version(ulid::Ulid::new())
    }
    
    /// Create version from ULID
    pub fn from_ulid(ulid: ulid::Ulid) -> Self {
        Version(ulid)
    }
    
    /// Get the underlying ULID
    pub fn as_ulid(&self) -> ulid::Ulid {
        self.0
    }
    
    /// Get timestamp component
    pub fn timestamp(&self) -> u64 {
        self.0.timestamp_ms()
    }
}

impl Default for Version {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Content hash for integrity verification
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    /// Create hash from data using BLAKE3
    pub fn new(data: &[u8]) -> Self {
        let hash = blake3::hash(data);
        ContentHash(hash.into())
    }
    
    /// Create from existing hash bytes
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        ContentHash(bytes)
    }
    
    /// Get hash as bytes
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
    
    /// Get hash as hex string
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

/// Chunk manifest for large objects
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkManifest {
    pub chunks: Vec<ContentHash>,
    pub chunk_size: u32,
    pub total_size: u64,
}

impl ChunkManifest {
    /// Create new chunk manifest
    pub fn new(chunks: Vec<ContentHash>, chunk_size: u32, total_size: u64) -> Self {
        ChunkManifest {
            chunks,
            chunk_size,
            total_size,
        }
    }
    
    /// Get number of chunks
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }
}

/// Object metadata stored in the primary LSM-tree
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectMetadata {
    pub size: u64,
    pub version: Version,
    pub content_hash: Option<ContentHash>,
    pub created_at: SystemTime,
    pub chunk_manifest: Option<ChunkManifest>,
}

impl ObjectMetadata {
    /// Create metadata for small inline object
    pub fn new_inline(size: u64, content_hash: ContentHash) -> Self {
        ObjectMetadata {
            size,
            version: Version::new(),
            content_hash: Some(content_hash),
            created_at: SystemTime::now(),
            chunk_manifest: None,
        }
    }
    
    /// Create metadata for large chunked object  
    pub fn new_chunked(chunk_manifest: ChunkManifest) -> Self {
        ObjectMetadata {
            size: chunk_manifest.total_size,
            version: Version::new(),
            content_hash: None, // Overall hash computed from manifest
            created_at: SystemTime::now(),
            chunk_manifest: Some(chunk_manifest),
        }
    }
    
    /// Check if this is a large object with chunks
    pub fn is_chunked(&self) -> bool {
        self.chunk_manifest.is_some()
    }
}

// Add hex dependency for ContentHash

/// Batch operation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchRequest {
    pub operations: Vec<BatchOperation>,
}

/// Individual batch operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BatchOperation {
    Put { key: Key, data: Vec<u8> },
    Delete { key: Key },
}

/// Batch operation response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchResponse {
    pub results: Vec<BatchResult>,
}

/// Result of individual batch operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BatchResult {
    Success,
    Error(String),
}

/// Multipart upload state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultipartUploadState {
    pub upload_id: String,
    pub bucket: BucketId,
    pub key: Key,
    pub parts: Vec<PartInfo>,
    pub created_at: SystemTime,
}

/// Information about an uploaded part
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartInfo {
    pub part_number: u32,
    pub size: u64,
    pub content_hash: ContentHash,
}

impl MultipartUploadState {
    /// Create new multipart upload
    pub fn new(upload_id: String, bucket: BucketId, key: Key) -> Self {
        MultipartUploadState {
            upload_id,
            bucket,
            key,
            parts: Vec::new(),
            created_at: SystemTime::now(),
        }
    }
    
    /// Add a part
    pub fn add_part(&mut self, part_number: u32, size: u64, hash: ContentHash) {
        self.parts.push(PartInfo {
            part_number,
            size,
            content_hash: hash,
        });
        // Keep parts sorted by part number
        self.parts.sort_by_key(|p| p.part_number);
    }
    
    /// Get total size
    pub fn total_size(&self) -> u64 {
        self.parts.iter().map(|p| p.size).sum()
    }
    
    /// Check if upload is complete
    pub fn is_complete(&self) -> bool {
        if self.parts.is_empty() {
            return false;
        }
        
        // Check that part numbers are sequential starting from 1
        for (i, part) in self.parts.iter().enumerate() {
            if part.part_number != (i as u32 + 1) {
                return false;
            }
        }
        
        true
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
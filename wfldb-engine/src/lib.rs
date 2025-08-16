//! Storage engine implementation using fjall

use fjall::{Config, Keyspace, PersistMode};
use std::path::Path;
use std::sync::Arc;
use wfldb_core::*;

pub mod bucket;
pub mod storage;

pub use bucket::*;
pub use storage::*;

/// Storage engine wrapping fjall keyspace
#[derive(Clone)]
pub struct StorageEngine {
    keyspace: Arc<Keyspace>,
    value_threshold: usize,
}

impl StorageEngine {
    /// Create new storage engine at the given path
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let config = Config::new(path);
        let keyspace = Arc::new(
            config
                .open()
                .map_err(|e| WflDBError::Storage(e.to_string()))?
        );
        
        Ok(StorageEngine {
            keyspace,
            value_threshold: 64 * 1024, // 64KB threshold for key-value separation
        })
    }
    
    /// Create temporary storage engine for testing
    #[cfg(any(test, feature = "test-utils"))]
    pub fn temp() -> Result<(Self, tempfile::TempDir)> {
        let temp_dir = tempfile::tempdir()
            .map_err(|e| WflDBError::Internal(e.to_string()))?;
        let engine = Self::new(temp_dir.path())?;
        Ok((engine, temp_dir))
    }
    
    /// Create or get bucket
    pub fn bucket(&self, bucket_id: &BucketId) -> Result<Bucket> {
        Bucket::new(self.clone(), bucket_id.clone())
    }
    
    /// Get the underlying keyspace
    pub(crate) fn keyspace(&self) -> &Keyspace {
        &self.keyspace
    }
    
    /// Get value separation threshold
    pub fn value_threshold(&self) -> usize {
        self.value_threshold
    }
    
    /// Persist all changes to disk
    pub fn persist(&self) -> Result<()> {
        self.keyspace
            .persist(PersistMode::SyncAll)
            .map_err(|e| WflDBError::Storage(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_storage_engine_creation() {
        let (engine, _temp) = StorageEngine::temp().unwrap();
        assert_eq!(engine.value_threshold(), 64 * 1024);
    }
    
    #[test]
    fn test_bucket_creation() {
        let (engine, _temp) = StorageEngine::temp().unwrap();
        let bucket_id = BucketId::new("test-bucket").unwrap();
        let bucket = engine.bucket(&bucket_id).unwrap();
        assert_eq!(bucket.id(), &bucket_id);
    }
}
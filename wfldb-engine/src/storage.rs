//! High-level storage operations

use wfldb_core::*;
use crate::{StorageEngine, Bucket};

/// High-level storage interface
pub struct Storage {
    engine: StorageEngine,
}

impl Storage {
    /// Create new storage instance
    pub fn new(engine: StorageEngine) -> Self {
        Storage { engine }
    }
    
    /// Put object with automatic size-based routing
    pub fn put_object(&self, bucket_id: &BucketId, key: &Key, data: &[u8]) -> Result<ObjectMetadata> {
        let bucket = self.engine.bucket(bucket_id)?;
        
        if data.len() <= self.engine.value_threshold() {
            bucket.put_small(key, data)
        } else {
            // Split large data into chunks
            let chunks = self.chunk_data(data);
            bucket.put_large(key, chunks)
        }
    }
    
    /// Get object data (small or large)
    pub fn get_object(&self, bucket_id: &BucketId, key: &Key) -> Result<Option<Vec<u8>>> {
        let bucket = self.engine.bucket(bucket_id)?;
        
        // First check metadata to determine if it's chunked
        match bucket.get_metadata(key)? {
            Some(metadata) => {
                if metadata.is_chunked() {
                    self.get_large_object(&bucket, &metadata)
                } else {
                    bucket.get_small(key)
                }
            }
            None => Ok(None),
        }
    }
    
    /// Get object metadata
    pub fn get_metadata(&self, bucket_id: &BucketId, key: &Key) -> Result<Option<ObjectMetadata>> {
        let bucket = self.engine.bucket(bucket_id)?;
        bucket.get_metadata(key)
    }
    
    /// Delete object
    pub fn delete_object(&self, bucket_id: &BucketId, key: &Key) -> Result<()> {
        let bucket = self.engine.bucket(bucket_id)?;
        bucket.delete(key)
    }
    
    /// List objects with prefix
    pub fn list_objects(&self, bucket_id: &BucketId, prefix: &str, limit: Option<usize>) -> Result<Vec<Key>> {
        let bucket = self.engine.bucket(bucket_id)?;
        bucket.scan_prefix(prefix, limit)
    }
    
    /// Get storage engine reference
    pub fn engine(&self) -> &StorageEngine {
        &self.engine
    }
    
    // Private helper methods
    
    fn chunk_data(&self, data: &[u8]) -> Vec<Vec<u8>> {
        const CHUNK_SIZE: usize = 4 * 1024 * 1024; // 4MB chunks
        
        data.chunks(CHUNK_SIZE)
            .map(|chunk| chunk.to_vec())
            .collect()
    }
    
    fn get_large_object(&self, bucket: &Bucket, metadata: &ObjectMetadata) -> Result<Option<Vec<u8>>> {
        let manifest = metadata.chunk_manifest.as_ref()
            .ok_or_else(|| WflDBError::Internal("Missing chunk manifest".to_string()))?;
        
        let mut data = Vec::with_capacity(metadata.size as usize);
        
        for chunk_hash in &manifest.chunks {
            match bucket.get_chunk(chunk_hash)? {
                Some(chunk_data) => data.extend(chunk_data),
                None => return Err(WflDBError::Internal(
                    format!("Missing chunk: {}", chunk_hash.to_hex())
                )),
            }
        }
        
        Ok(Some(data))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_storage_roundtrip() {
        let (engine, _temp) = StorageEngine::temp().unwrap();
        let storage = Storage::new(engine);
        
        let bucket_id = BucketId::new("test-bucket").unwrap();
        let key = Key::new("test-key").unwrap();
        let data = b"Hello, storage layer!";
        
        // Put object
        let metadata = storage.put_object(&bucket_id, &key, data).unwrap();
        assert_eq!(metadata.size, data.len() as u64);
        
        // Get object
        let retrieved = storage.get_object(&bucket_id, &key).unwrap().unwrap();
        assert_eq!(retrieved, data);
        
        // Get metadata
        let retrieved_metadata = storage.get_metadata(&bucket_id, &key).unwrap().unwrap();
        assert_eq!(retrieved_metadata.size, metadata.size);
        
        // List objects
        let keys = storage.list_objects(&bucket_id, "", None).unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].as_str(), "test-key");
    }
    
    #[tokio::test]
    async fn test_large_object_automatic_chunking() {
        let (engine, _temp) = StorageEngine::temp().unwrap();
        let storage = Storage::new(engine);
        
        let bucket_id = BucketId::new("test-bucket").unwrap();
        let key = Key::new("large-object").unwrap();
        
        // Create data larger than threshold (64KB)
        let data = vec![42u8; 128 * 1024]; // 128KB
        
        // Put object - should automatically chunk
        let metadata = storage.put_object(&bucket_id, &key, &data).unwrap();
        assert_eq!(metadata.size, data.len() as u64);
        assert!(metadata.is_chunked());
        
        // Get object - should automatically reassemble
        let retrieved = storage.get_object(&bucket_id, &key).unwrap().unwrap();
        assert_eq!(retrieved, data);
        assert_eq!(retrieved.len(), 128 * 1024);
    }
    
    #[tokio::test] 
    async fn test_delete_object() {
        let (engine, _temp) = StorageEngine::temp().unwrap();
        let storage = Storage::new(engine);
        
        let bucket_id = BucketId::new("test-bucket").unwrap();
        let key = Key::new("test-key").unwrap();
        let data = b"test data";
        
        // Put and verify
        storage.put_object(&bucket_id, &key, data).unwrap();
        assert!(storage.get_object(&bucket_id, &key).unwrap().is_some());
        
        // Delete and verify
        storage.delete_object(&bucket_id, &key).unwrap();
        assert!(storage.get_object(&bucket_id, &key).unwrap().is_none());
        assert!(storage.get_metadata(&bucket_id, &key).unwrap().is_none());
    }
}
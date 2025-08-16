//! Bucket abstraction over fjall partitions

use fjall::{Partition, PartitionCreateOptions};
use serde_json;
use std::sync::Arc;
use wfldb_core::*;
use crate::StorageEngine;

/// Bucket represents a multi-tenant boundary
pub struct Bucket {
    id: BucketId,
    pub(crate) main_partition: Arc<Partition>,
    engine: StorageEngine,
}

impl Bucket {
    /// Create or open bucket
    pub(crate) fn new(engine: StorageEngine, id: BucketId) -> Result<Self> {
        let partition_name = format!("{}_main", id.as_str());
        
        let main_partition = Arc::new(
            engine
                .keyspace()
                .open_partition(&partition_name, PartitionCreateOptions::default())
                .map_err(|e| WflDBError::Storage(e.to_string()))?
        );
        
        Ok(Bucket {
            id,
            main_partition,
            engine,
        })
    }
    
    /// Get bucket ID
    pub fn id(&self) -> &BucketId {
        &self.id
    }
    
    /// Put small object (stored inline in LSM-tree)
    pub fn put_small(&self, key: &Key, data: &[u8]) -> Result<ObjectMetadata> {
        if data.len() > self.engine.value_threshold() {
            return Err(WflDBError::Internal(
                "Data too large for small object storage".to_string()
            ));
        }
        
        let content_hash = ContentHash::new(data);
        let metadata = ObjectMetadata::new_inline(data.len() as u64, content_hash);
        
        let metadata_key = self.metadata_key(key);
        let data_key = self.data_key(key);
        
        // Store metadata and data
        let metadata_json = serde_json::to_vec(&metadata)
            .map_err(WflDBError::Serialization)?;
        
        self.main_partition
            .insert(&metadata_key, metadata_json)
            .map_err(|e| WflDBError::Storage(e.to_string()))?;
        
        self.main_partition
            .insert(&data_key, data)
            .map_err(|e| WflDBError::Storage(e.to_string()))?;
        
        self.engine.persist()?;
        
        Ok(metadata)
    }
    
    /// Get small object
    pub fn get_small(&self, key: &Key) -> Result<Option<Vec<u8>>> {
        let data_key = self.data_key(key);
        
        match self.main_partition.get(&data_key) {
            Ok(Some(data)) => Ok(Some(data.to_vec())),
            Ok(None) => Ok(None),
            Err(e) => Err(WflDBError::Storage(e.to_string())),
        }
    }
    
    /// Put large object (using value log for data, metadata in LSM-tree)
    pub fn put_large(&self, key: &Key, chunks: Vec<Vec<u8>>) -> Result<ObjectMetadata> {
        let mut chunk_hashes = Vec::new();
        let mut total_size = 0u64;
        let chunk_size = chunks.first().map(|c| c.len() as u32).unwrap_or(0);
        
        // Store each chunk in the value log using content-addressing with deduplication
        for chunk in chunks {
            let chunk_hash = ContentHash::new(&chunk);
            let chunk_key = self.chunk_key(&chunk_hash);
            let ref_key = self.chunk_ref_key(&chunk_hash);
            
            // Check if chunk already exists
            let existing_ref = self.main_partition.get(&ref_key)
                .map_err(|e| WflDBError::Storage(e.to_string()))?;
            
            if let Some(ref_data) = existing_ref {
                // Chunk exists, increment reference count
                let ref_count = u32::from_le_bytes(ref_data[0..4].try_into().unwrap());
                let new_ref_count = ref_count + 1;
                self.main_partition
                    .insert(&ref_key, &new_ref_count.to_le_bytes())
                    .map_err(|e| WflDBError::Storage(e.to_string()))?;
            } else {
                // New chunk, store it with reference count of 1
                self.main_partition
                    .insert(&chunk_key, &chunk)
                    .map_err(|e| WflDBError::Storage(e.to_string()))?;
                self.main_partition
                    .insert(&ref_key, &1u32.to_le_bytes())
                    .map_err(|e| WflDBError::Storage(e.to_string()))?;
            }
            
            chunk_hashes.push(chunk_hash);
            total_size += chunk.len() as u64;
        }
        
        let chunk_manifest = ChunkManifest::new(chunk_hashes, chunk_size, total_size);
        let metadata = ObjectMetadata::new_chunked(chunk_manifest);
        
        // Store metadata
        let metadata_key = self.metadata_key(key);
        let metadata_json = serde_json::to_vec(&metadata)
            .map_err(WflDBError::Serialization)?;
        
        self.main_partition
            .insert(&metadata_key, metadata_json)
            .map_err(|e| WflDBError::Storage(e.to_string()))?;
        
        self.engine.persist()?;
        
        Ok(metadata)
    }
    
    /// Get object metadata
    pub fn get_metadata(&self, key: &Key) -> Result<Option<ObjectMetadata>> {
        let metadata_key = self.metadata_key(key);
        
        match self.main_partition.get(&metadata_key) {
            Ok(Some(data)) => {
                let metadata: ObjectMetadata = serde_json::from_slice(&data)
                    .map_err(WflDBError::Serialization)?;
                Ok(Some(metadata))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(WflDBError::Storage(e.to_string())),
        }
    }
    
    /// Get large object chunk by hash
    pub fn get_chunk(&self, hash: &ContentHash) -> Result<Option<Vec<u8>>> {
        let chunk_key = self.chunk_key(hash);
        
        match self.main_partition.get(&chunk_key) {
            Ok(Some(data)) => Ok(Some(data.to_vec())),
            Ok(None) => Ok(None),
            Err(e) => Err(WflDBError::Storage(e.to_string())),
        }
    }
    
    /// Delete object
    pub fn delete(&self, key: &Key) -> Result<()> {
        // Get metadata to check if we need to clean up chunks
        if let Some(metadata) = self.get_metadata(key)? {
            // Remove metadata and data
            let _ = self.main_partition.remove(&self.metadata_key(key));
            let _ = self.main_partition.remove(&self.data_key(key));
            
            // If chunked, decrement reference counts and remove unreferenced chunks
            if let Some(manifest) = metadata.chunk_manifest {
                for chunk_hash in manifest.chunks {
                    let ref_key = self.chunk_ref_key(&chunk_hash);
                    
                    // Get current reference count
                    if let Some(ref_data) = self.main_partition.get(&ref_key)
                        .map_err(|e| WflDBError::Storage(e.to_string()))? {
                        
                        let ref_count = u32::from_le_bytes(ref_data[0..4].try_into().unwrap());
                        
                        if ref_count > 1 {
                            // Decrement reference count
                            let new_ref_count = ref_count - 1;
                            self.main_partition
                                .insert(&ref_key, &new_ref_count.to_le_bytes())
                                .map_err(|e| WflDBError::Storage(e.to_string()))?;
                        } else {
                            // Last reference, remove chunk and reference count
                            let _ = self.main_partition.remove(&self.chunk_key(&chunk_hash));
                            let _ = self.main_partition.remove(&ref_key);
                        }
                    }
                }
            }
        }
        
        self.engine.persist()?;
        Ok(())
    }
    
    /// Scan keys with prefix
    pub fn scan_prefix(&self, prefix: &str, limit: Option<usize>) -> Result<Vec<Key>> {
        let prefix_bytes = format!("meta:{}", prefix).into_bytes();
        let mut keys = Vec::new();
        let max_results = limit.unwrap_or(usize::MAX);
        
        // Use fjall's range iterator for efficient prefix scanning
        let iter = self.main_partition.range(prefix_bytes.clone()..);
        
        for item in iter {
            match item {
                Ok((key_bytes, _value)) => {
                    // Check if key still has our prefix
                    if !key_bytes.starts_with(&prefix_bytes) {
                        break; // We've gone past the prefix range
                    }
                    
                    // Extract the actual key from the metadata key
                    if let Ok(key_str) = std::str::from_utf8(&key_bytes) {
                        if let Some(actual_key) = key_str.strip_prefix("meta:") {
                            if let Ok(key) = Key::new(actual_key) {
                                // Check if the actual key has the requested prefix
                                if key.has_prefix(prefix) {
                                    keys.push(key);
                                    if keys.len() >= max_results {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    return Err(WflDBError::Storage(format!("Scan error: {}", e)));
                }
            }
        }
        
        Ok(keys)
    }
    
    // Helper methods for key formatting
    fn metadata_key(&self, key: &Key) -> Vec<u8> {
        format!("meta:{}", key.as_str()).into_bytes()
    }
    
    fn data_key(&self, key: &Key) -> Vec<u8> {
        format!("data:{}", key.as_str()).into_bytes()
    }
    
    fn chunk_key(&self, hash: &ContentHash) -> Vec<u8> {
        format!("chunk:{}", hash.to_hex()).into_bytes()
    }
    
    fn chunk_ref_key(&self, hash: &ContentHash) -> Vec<u8> {
        format!("chunkref:{}", hash.to_hex()).into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_small_object_roundtrip() {
        let (engine, _temp) = StorageEngine::temp().unwrap();
        let bucket_id = BucketId::new("test-bucket").unwrap();
        let bucket = engine.bucket(&bucket_id).unwrap();
        
        let key = Key::new("test-key").unwrap();
        let data = b"Hello, world!";
        
        // Put small object
        let metadata = bucket.put_small(&key, data).unwrap();
        assert_eq!(metadata.size, data.len() as u64);
        assert!(!metadata.is_chunked());
        
        // Get small object
        let retrieved = bucket.get_small(&key).unwrap().unwrap();
        assert_eq!(retrieved, data);
        
        // Get metadata
        let retrieved_metadata = bucket.get_metadata(&key).unwrap().unwrap();
        assert_eq!(retrieved_metadata.size, metadata.size);
    }
    
    #[tokio::test]
    async fn test_large_object_roundtrip() {
        let (engine, _temp) = StorageEngine::temp().unwrap();
        let bucket_id = BucketId::new("test-bucket").unwrap();
        let bucket = engine.bucket(&bucket_id).unwrap();
        
        let key = Key::new("large-object").unwrap();
        let chunk1 = vec![1u8; 1024];
        let chunk2 = vec![2u8; 1024];
        let chunks = vec![chunk1.clone(), chunk2.clone()];
        
        // Put large object
        let metadata = bucket.put_large(&key, chunks).unwrap();
        assert_eq!(metadata.size, 2048);
        assert!(metadata.is_chunked());
        
        // Get chunks back
        let manifest = metadata.chunk_manifest.unwrap();
        assert_eq!(manifest.chunk_count(), 2);
        
        let retrieved_chunk1 = bucket.get_chunk(&manifest.chunks[0]).unwrap().unwrap();
        let retrieved_chunk2 = bucket.get_chunk(&manifest.chunks[1]).unwrap().unwrap();
        
        assert_eq!(retrieved_chunk1, chunk1);
        assert_eq!(retrieved_chunk2, chunk2);
    }
    
    #[tokio::test]
    async fn test_delete() {
        let (engine, _temp) = StorageEngine::temp().unwrap();
        let bucket_id = BucketId::new("test-bucket").unwrap();
        let bucket = engine.bucket(&bucket_id).unwrap();
        
        let key = Key::new("test-key").unwrap();
        let data = b"test data";
        
        // Put and verify
        bucket.put_small(&key, data).unwrap();
        assert!(bucket.get_small(&key).unwrap().is_some());
        
        // Delete and verify
        bucket.delete(&key).unwrap();
        assert!(bucket.get_small(&key).unwrap().is_none());
        assert!(bucket.get_metadata(&key).unwrap().is_none());
    }
}
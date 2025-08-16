//! Property-based tests for wflDB core

use proptest::prelude::*;
use wfldb_core::*;

proptest! {
    #[test]
    fn props_put_then_get_returns_same_bytes(
        data in prop::collection::vec(any::<u8>(), 0..10000)
    ) {
        // Test that putting data and then getting it returns the exact same bytes
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let (engine, _temp) = wfldb_engine::StorageEngine::temp().unwrap();
            let storage = wfldb_engine::Storage::new(engine);
            
            let bucket_id = BucketId::new("test-bucket").unwrap();
            let key = Key::new("test-key").unwrap();
            
            // Put the data
            let metadata = storage.put_object(&bucket_id, &key, &data).unwrap();
            assert_eq!(metadata.size, data.len() as u64);
            
            // Get the data back
            let retrieved = storage.get_object(&bucket_id, &key).unwrap().unwrap();
            
            // Should be exactly the same
            assert_eq!(retrieved, data);
        });
    }
    
    #[test]
    fn props_manifest_reassembling_is_order_stable(
        chunk_sizes in prop::collection::vec(1usize..1000, 1..20)
    ) {
        // Test that chunk manifests reassemble in the correct order
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            // Create test data from chunk sizes
            let mut data: Vec<u8> = Vec::new();
            let mut chunks = Vec::new();
            
            for (i, size) in chunk_sizes.iter().enumerate() {
                let chunk = vec![i as u8; *size];
                data.extend(&chunk);
                chunks.push(chunk);
            }
            
            // Create manifest
            let chunk_hashes: Vec<ContentHash> = chunks.iter()
                .map(|c| ContentHash::new(c))
                .collect();
            
            let total_size = data.len() as u64;
            let chunk_size = chunks.first().map(|c| c.len() as u32).unwrap_or(0);
            
            let manifest = ChunkManifest::new(chunk_hashes.clone(), chunk_size, total_size);
            
            // Verify manifest properties
            assert_eq!(manifest.chunk_count(), chunks.len());
            assert_eq!(manifest.total_size, total_size);
            
            // Verify chunk ordering is preserved
            for (i, hash) in manifest.chunks.iter().enumerate() {
                assert_eq!(hash, &chunk_hashes[i]);
            }
            
            // Store and retrieve using manifest
            let (engine, _temp) = wfldb_engine::StorageEngine::temp().unwrap();
            let bucket_id = BucketId::new("test-bucket").unwrap();
            let bucket = engine.bucket(&bucket_id).unwrap();
            
            // Store using the engine's API
            let key = Key::new("chunked-object").unwrap();
            let _metadata = bucket.put_large(&key, chunks.clone()).unwrap();
            
            // Retrieve and reassemble
            let storage = wfldb_engine::Storage::new(engine);
            let retrieved = storage.get_object(&bucket_id, &key).unwrap().unwrap();
            
            // Should match original data exactly
            assert_eq!(retrieved, data);
        });
    }
}

#[cfg(test)]
mod chunk_tests {
    use super::*;
    
    #[test]
    fn test_chunk_manifest_creation() {
        let chunks = vec![
            ContentHash::new(b"chunk1"),
            ContentHash::new(b"chunk2"),
            ContentHash::new(b"chunk3"),
        ];
        
        let manifest = ChunkManifest::new(chunks.clone(), 1024, 3072);
        
        assert_eq!(manifest.chunk_count(), 3);
        assert_eq!(manifest.chunk_size, 1024);
        assert_eq!(manifest.total_size, 3072);
        assert_eq!(manifest.chunks.len(), 3);
    }
    
    #[test]
    fn test_version_ordering() {
        let v1 = Version::new();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let v2 = Version::new();
        
        assert!(v1 < v2);
        assert!(v1.timestamp() < v2.timestamp());
    }
}
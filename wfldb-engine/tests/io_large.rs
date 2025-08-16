//! Integration tests for large object I/O

use sha2::{Sha256, Digest};
use wfldb_core::*;
use wfldb_engine::*;

#[tokio::test]
async fn multipart_put_commit_then_get_matches_sha() {
    // Test that multipart upload and retrieval maintains data integrity
    let (engine, _temp) = StorageEngine::temp().unwrap();
    let storage = Storage::new(engine.clone());
    
    let bucket_id = BucketId::new("test-bucket").unwrap();
    let key = Key::new("large-file").unwrap();
    
    // Create test data (5MB in 5 parts)
    const PART_SIZE: usize = 1024 * 1024; // 1MB per part
    let mut parts = Vec::new();
    let mut all_data = Vec::new();
    
    for i in 0..5 {
        let part = vec![i as u8; PART_SIZE];
        parts.push(part.clone());
        all_data.extend(&part);
    }
    
    // Calculate SHA256 of original data
    let mut hasher = Sha256::new();
    hasher.update(&all_data);
    let original_sha = hasher.finalize();
    
    // Simulate multipart upload by chunking
    let metadata = storage.put_object(&bucket_id, &key, &all_data).unwrap();
    assert!(metadata.is_chunked());
    assert_eq!(metadata.size, all_data.len() as u64);
    
    // Retrieve and verify SHA256 matches
    let retrieved = storage.get_object(&bucket_id, &key).unwrap().unwrap();
    let mut hasher = Sha256::new();
    hasher.update(&retrieved);
    let retrieved_sha = hasher.finalize();
    
    assert_eq!(original_sha, retrieved_sha);
    assert_eq!(retrieved.len(), all_data.len());
}

#[tokio::test]
async fn dedup_identical_chunks_written_once() {
    // Test that identical chunks are deduplicated
    let (engine, _temp) = StorageEngine::temp().unwrap();
    let bucket_id = BucketId::new("test-bucket").unwrap();
    let bucket = engine.bucket(&bucket_id).unwrap();
    
    // Create data with duplicate chunks
    let chunk1 = vec![1u8; 1024];
    let chunk2 = vec![2u8; 1024];
    let chunk3 = vec![1u8; 1024]; // Same as chunk1
    
    // Store first object with chunks [1, 2]
    let key1 = Key::new("object1").unwrap();
    let chunks1 = vec![chunk1.clone(), chunk2.clone()];
    let metadata1 = bucket.put_large(&key1, chunks1).unwrap();
    
    // Store second object with chunks [1, 2, 1] (chunk1 repeated)
    let key2 = Key::new("object2").unwrap();
    let chunks2 = vec![chunk1.clone(), chunk2.clone(), chunk3];
    let metadata2 = bucket.put_large(&key2, chunks2).unwrap();
    
    // Verify both objects have manifests
    assert!(metadata1.is_chunked());
    assert!(metadata2.is_chunked());
    
    let manifest1 = metadata1.chunk_manifest.unwrap();
    let manifest2 = metadata2.chunk_manifest.unwrap();
    
    // Object 2 should reference the same chunk hash for chunks at index 0 and 2
    assert_eq!(manifest2.chunks[0], manifest2.chunks[2]);
    
    // The hash for chunk1 should be the same in both manifests
    assert_eq!(manifest1.chunks[0], manifest2.chunks[0]);
    
    // Verify we can retrieve both objects correctly
    let storage = Storage::new(engine);
    
    let retrieved1 = storage.get_object(&bucket_id, &key1).unwrap().unwrap();
    assert_eq!(retrieved1.len(), 2048);
    
    let retrieved2 = storage.get_object(&bucket_id, &key2).unwrap().unwrap();
    assert_eq!(retrieved2.len(), 3072);
    
    // Verify data integrity
    assert_eq!(&retrieved1[0..1024], &chunk1);
    assert_eq!(&retrieved1[1024..2048], &chunk2);
    assert_eq!(&retrieved2[0..1024], &chunk1);
    assert_eq!(&retrieved2[1024..2048], &chunk2);
    assert_eq!(&retrieved2[2048..3072], &chunk1);
}

#[cfg(test)]
mod multipart_helpers {
    use super::*;
    
    #[test]
    fn test_chunk_content_addressing() {
        // Test that identical data produces identical hashes
        let data1 = b"test chunk data";
        let data2 = b"test chunk data";
        let data3 = b"different data";
        
        let hash1 = ContentHash::new(data1);
        let hash2 = ContentHash::new(data2);
        let hash3 = ContentHash::new(data3);
        
        assert_eq!(hash1.as_bytes(), hash2.as_bytes());
        assert_ne!(hash1.as_bytes(), hash3.as_bytes());
    }
    
    #[test]
    fn test_large_object_threshold() {
        // Verify the threshold constant is used consistently
        let (engine, _temp) = StorageEngine::temp().unwrap();
        assert_eq!(engine.value_threshold(), 64 * 1024);
    }
}
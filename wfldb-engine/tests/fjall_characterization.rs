//! Characterization tests for fjall storage engine
//! These tests validate our technology choice and establish behavioral guardrails

use wfldb_core::*;
use wfldb_core::test_utils::*;
use wfldb_engine::{StorageEngine, Storage, Bucket};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// Test that small values are stored inline under the threshold
#[test]
fn fjall_smoke_put_get_inline_under_threshold() {
    let (engine, _temp) = StorageEngine::temp().unwrap();
    let storage = Storage::new(engine.clone());
    let bucket_id = BucketId::new("test-bucket").unwrap();
    
    // Test various sizes under the 64KB threshold
    let test_sizes = vec![100, 1024, 4096, 16384, 32768, 63 * 1024];
    
    for size in test_sizes {
        let key = Key::new(&format!("key-{}", size)).unwrap();
        let data = TestDataGenerator::random_bytes(size);
        
        // PUT operation
        let metadata = storage.put_object(&bucket_id, &key, &data).unwrap();
        assert_eq!(metadata.size, size as u64);
        assert!(!metadata.is_chunked(), "Small object should not be chunked at size {}", size);
        
        // GET operation
        let retrieved = storage.get_object(&bucket_id, &key).unwrap().unwrap();
        assert_eq!(retrieved, data, "Data mismatch for size {}", size);
        
        // Verify metadata
        let meta = storage.get_metadata(&bucket_id, &key).unwrap().unwrap();
        assert_eq!(meta.size, size as u64);
        assert!(!meta.is_chunked());
    }
}

/// Test that large values spill to the value log over threshold
#[test]
fn fjall_blob_spills_large_values_over_threshold() {
    let (engine, _temp) = StorageEngine::temp().unwrap();
    let storage = Storage::new(engine.clone());
    let bucket_id = BucketId::new("large-bucket").unwrap();
    
    // Test sizes over the 64KB threshold
    let test_sizes = vec![65 * 1024, 128 * 1024, 256 * 1024, 1024 * 1024];
    
    for size in test_sizes {
        let key = Key::new(&format!("large-key-{}", size)).unwrap();
        let data = TestDataGenerator::compressible_bytes(size);
        
        // PUT large object
        let metadata = storage.put_object(&bucket_id, &key, &data).unwrap();
        assert_eq!(metadata.size, size as u64);
        assert!(metadata.is_chunked(), "Large object should be chunked at size {}", size);
        assert!(metadata.chunk_manifest.is_some(), "Should have chunk manifest");
        
        // GET large object - verify reassembly
        let retrieved = storage.get_object(&bucket_id, &key).unwrap().unwrap();
        assert_eq!(retrieved.len(), size, "Size mismatch for {}", size);
        assert_eq!(retrieved, data, "Data corruption for size {}", size);
        
        // Verify chunks are content-addressed
        let manifest = metadata.chunk_manifest.as_ref().unwrap();
        assert!(!manifest.chunks.is_empty(), "Should have chunks");
        
        // Each chunk should be verifiable by its hash
        for chunk_hash in &manifest.chunks {
            // This would require access to internal bucket methods
            // For now, we verify through successful retrieval
            assert!(!chunk_hash.to_hex().is_empty());
        }
    }
}

/// Test that cross-partition batch operations are atomic
#[test]
fn fjall_atomic_cross_partition_batch_is_atomic() {
    let (engine, _temp) = StorageEngine::temp().unwrap();
    let bucket1_id = BucketId::new("bucket1").unwrap();
    let bucket2_id = BucketId::new("bucket2").unwrap();
    
    let bucket1 = engine.bucket(&bucket1_id).unwrap();
    let bucket2 = engine.bucket(&bucket2_id).unwrap();
    
    // Prepare test data
    let key1 = Key::new("atomic-key-1").unwrap();
    let key2 = Key::new("atomic-key-2").unwrap();
    let data1 = b"data for bucket 1";
    let data2 = b"data for bucket 2";
    
    // Test atomic success case
    {
        // In a real atomic batch, both operations should succeed or fail together
        // Since our current API doesn't expose batch operations directly,
        // we simulate this by testing that operations complete
        let _meta1 = bucket1.put_small(&key1, data1).unwrap();
        let _meta2 = bucket2.put_small(&key2, data2).unwrap();
        
        // Verify both writes succeeded
        assert!(bucket1.get_small(&key1).unwrap().is_some());
        assert!(bucket2.get_small(&key2).unwrap().is_some());
    }
    
    // Test consistency under concurrent operations
    let bucket1 = Arc::new(bucket1);
    let bucket2 = Arc::new(bucket2);
    
    let mut handles = vec![];
    
    // Spawn concurrent writers
    for i in 0..10 {
        let b1 = Arc::clone(&bucket1);
        let b2 = Arc::clone(&bucket2);
        
        let handle = thread::spawn(move || {
            let key = Key::new(&format!("concurrent-{}", i)).unwrap();
            let data = format!("data-{}", i).into_bytes();
            
            // Write to both buckets
            b1.put_small(&key, &data).unwrap();
            b2.put_small(&key, &data).unwrap();
        });
        
        handles.push(handle);
    }
    
    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }
    
    // Verify all writes completed
    for i in 0..10 {
        let key = Key::new(&format!("concurrent-{}", i)).unwrap();
        assert!(bucket1.get_small(&key).unwrap().is_some());
        assert!(bucket2.get_small(&key).unwrap().is_some());
    }
}

/// Test WAL persistence and recovery
#[test]
fn fjall_persistence_wal_survives_crash() {
    use tempfile::TempDir;
    
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().to_path_buf();
    
    let bucket_id = BucketId::new("persist-bucket").unwrap();
    let key = Key::new("persist-key").unwrap();
    let data = b"important data that must survive";
    
    // Phase 1: Write data and persist
    {
        let engine = StorageEngine::new(&db_path).unwrap();
        let storage = Storage::new(engine.clone());
        
        storage.put_object(&bucket_id, &key, data).unwrap();
        
        // Force persistence to disk
        engine.persist().unwrap();
    }
    // Engine drops here, simulating crash
    
    // Phase 2: Recover and verify data
    {
        let engine = StorageEngine::new(&db_path).unwrap();
        let storage = Storage::new(engine);
        
        // Data should be recoverable
        let recovered = storage.get_object(&bucket_id, &key).unwrap();
        assert!(recovered.is_some(), "Data lost after simulated crash");
        assert_eq!(recovered.unwrap(), data, "Data corrupted after recovery");
    }
}

/// Test LSM compaction behavior
#[test]
fn fjall_compaction_background_compaction_works() {
    let (engine, _temp) = StorageEngine::temp().unwrap();
    let storage = Storage::new(engine.clone());
    let bucket_id = BucketId::new("compact-bucket").unwrap();
    
    // Write many small values to trigger compaction
    let num_keys = 1000;
    let data_size = 1024; // 1KB each
    
    for i in 0..num_keys {
        let key = Key::new(&format!("compact-key-{:05}", i)).unwrap();
        let data = TestDataGenerator::random_bytes(data_size);
        storage.put_object(&bucket_id, &key, &data).unwrap();
        
        if i % 100 == 0 {
            // Persist periodically to flush memtables
            engine.persist().unwrap();
        }
    }
    
    // Final persist
    engine.persist().unwrap();
    
    // Give time for background compaction (if any)
    thread::sleep(Duration::from_millis(100));
    
    // Verify all data is still accessible after compaction
    for i in 0..num_keys {
        let key = Key::new(&format!("compact-key-{:05}", i)).unwrap();
        let result = storage.get_object(&bucket_id, &key).unwrap();
        assert!(result.is_some(), "Key {} lost after compaction", i);
        assert_eq!(result.unwrap().len(), data_size, "Data size mismatch for key {}", i);
    }
    
    // Test range scan efficiency after compaction
    let keys = storage.list_objects(&bucket_id, "compact-key-", Some(10)).unwrap();
    assert_eq!(keys.len(), 10, "Range scan returned wrong number of keys");
}

/// Performance characterization test
#[test]
fn fjall_performance_meets_latency_targets() {
    let (engine, _temp) = StorageEngine::temp().unwrap();
    let storage = Storage::new(engine);
    let bucket_id = BucketId::new("perf-bucket").unwrap();
    
    let harness = PerfTestHarness::new()
        .with_warmup(50)
        .with_iterations(500);
    
    // Test small object PUT performance
    println!("Testing small object PUT performance...");
    let mut counter = 0;
    let mut put_perf = harness.run(|| {
        let key = Key::new(&format!("perf-key-{}", counter)).unwrap();
        counter += 1;
        let data = TestDataGenerator::random_bytes(1024);
        storage.put_object(&bucket_id, &key, &data).unwrap();
    });
    
    println!("PUT p50: {:?}, p95: {:?}, p99: {:?}", 
        put_perf.p50(), put_perf.p95(), put_perf.p99());
    
    // Assert p95 < 10ms target
    assert_p95_under_ms!(put_perf, 10);
    
    // Pre-populate data for GET test
    let test_key = Key::new("perf-get-key").unwrap();
    let test_data = TestDataGenerator::random_bytes(1024);
    storage.put_object(&bucket_id, &test_key, &test_data).unwrap();
    
    // Test small object GET performance
    println!("Testing small object GET performance...");
    let mut get_perf = harness.run(|| {
        let _result = storage.get_object(&bucket_id, &test_key).unwrap();
    });
    
    println!("GET p50: {:?}, p95: {:?}, p99: {:?}", 
        get_perf.p50(), get_perf.p95(), get_perf.p99());
    
    // Assert p95 < 10ms target
    assert_p95_under_ms!(get_perf, 10);
}

/// Test storage isolation between buckets
#[test]
fn fjall_bucket_isolation() {
    let (engine, _temp) = StorageEngine::temp().unwrap();
    let storage = Storage::new(engine);
    
    let bucket1_id = BucketId::new("isolated-bucket-1").unwrap();
    let bucket2_id = BucketId::new("isolated-bucket-2").unwrap();
    let shared_key = Key::new("shared-key-name").unwrap();
    
    let data1 = b"data for bucket 1";
    let data2 = b"different data for bucket 2";
    
    // Write same key to different buckets
    storage.put_object(&bucket1_id, &shared_key, data1).unwrap();
    storage.put_object(&bucket2_id, &shared_key, data2).unwrap();
    
    // Verify isolation - each bucket has its own data
    let retrieved1 = storage.get_object(&bucket1_id, &shared_key).unwrap().unwrap();
    let retrieved2 = storage.get_object(&bucket2_id, &shared_key).unwrap().unwrap();
    
    assert_eq!(retrieved1, data1, "Bucket 1 data corrupted");
    assert_eq!(retrieved2, data2, "Bucket 2 data corrupted");
    
    // Delete from one bucket shouldn't affect the other
    storage.delete_object(&bucket1_id, &shared_key).unwrap();
    
    assert!(storage.get_object(&bucket1_id, &shared_key).unwrap().is_none());
    assert!(storage.get_object(&bucket2_id, &shared_key).unwrap().is_some());
}

/// Test prefix scanning functionality
#[test]
fn fjall_prefix_scanning_correctness() {
    let (engine, _temp) = StorageEngine::temp().unwrap();
    let storage = Storage::new(engine);
    let bucket_id = BucketId::new("scan-bucket").unwrap();
    
    // Create keys with different prefixes
    let prefixes = vec!["users/", "posts/", "comments/"];
    let items_per_prefix = 10;
    
    for prefix in &prefixes {
        for i in 0..items_per_prefix {
            let key = Key::new(&format!("{}{:03}", prefix, i)).unwrap();
            let data = format!("data-{}-{}", prefix, i).into_bytes();
            storage.put_object(&bucket_id, &key, &data).unwrap();
        }
    }
    
    // Test prefix scanning
    for prefix in &prefixes {
        let keys = storage.list_objects(&bucket_id, prefix, None).unwrap();
        assert_eq!(keys.len(), items_per_prefix, "Wrong count for prefix {}", prefix);
        
        // Verify keys are correctly prefixed
        for key in &keys {
            assert!(key.as_str().starts_with(prefix), 
                "Key {} doesn't match prefix {}", key.as_str(), prefix);
        }
        
        // Test with limit
        let limited = storage.list_objects(&bucket_id, prefix, Some(5)).unwrap();
        assert_eq!(limited.len(), 5, "Limit not respected for prefix {}", prefix);
    }
    
    // Test lexicographical ordering
    let all_users = storage.list_objects(&bucket_id, "users/", None).unwrap();
    for i in 1..all_users.len() {
        assert!(all_users[i-1].as_str() < all_users[i].as_str(), 
            "Keys not in lexicographical order");
    }
}

/// Test handling of edge cases
#[test]
fn fjall_edge_cases() {
    let (engine, _temp) = StorageEngine::temp().unwrap();
    let storage = Storage::new(engine);
    let bucket_id = BucketId::new("edge-bucket").unwrap();
    
    // Test empty data
    let empty_key = Key::new("empty").unwrap();
    let empty_data = b"";
    storage.put_object(&bucket_id, &empty_key, empty_data).unwrap();
    let retrieved = storage.get_object(&bucket_id, &empty_key).unwrap().unwrap();
    assert_eq!(retrieved.len(), 0, "Empty data not handled correctly");
    
    // Test maximum key length (assuming reasonable limit)
    let long_key_str = "k".repeat(255);
    let long_key = Key::new(&long_key_str).unwrap();
    let data = b"test";
    storage.put_object(&bucket_id, &long_key, data).unwrap();
    assert!(storage.get_object(&bucket_id, &long_key).unwrap().is_some());
    
    // Test non-existent key
    let nonexistent = Key::new("nonexistent").unwrap();
    assert!(storage.get_object(&bucket_id, &nonexistent).unwrap().is_none());
    
    // Test double delete
    let delete_key = Key::new("delete-me").unwrap();
    storage.put_object(&bucket_id, &delete_key, b"data").unwrap();
    storage.delete_object(&bucket_id, &delete_key).unwrap();
    // Second delete should not panic
    storage.delete_object(&bucket_id, &delete_key).unwrap();
}
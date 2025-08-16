//! Integration tests for batch operations

use wfldb_core::*;
use wfldb_engine::*;

#[tokio::test]
async fn batch_operations_are_atomic() {
    // Test that batch operations succeed or fail atomically
    let (engine, _temp) = StorageEngine::temp().unwrap();
    let storage = Storage::new(engine);
    
    let bucket_id = BucketId::new("test-bucket").unwrap();
    
    // Create batch with multiple operations
    let operations = vec![
        BatchOperation::Put {
            key: Key::new("key1").unwrap(),
            data: b"value1".to_vec(),
        },
        BatchOperation::Put {
            key: Key::new("key2").unwrap(),
            data: b"value2".to_vec(),
        },
        BatchOperation::Delete {
            key: Key::new("key3").unwrap(),
        },
        BatchOperation::Put {
            key: Key::new("key4").unwrap(),
            data: b"value4".to_vec(),
        },
    ];
    
    // Execute batch
    let response = storage.batch(&bucket_id, operations).unwrap();
    
    // All operations should succeed
    assert_eq!(response.results.len(), 4);
    for result in &response.results {
        match result {
            BatchResult::Success => {}
            BatchResult::Error(e) => panic!("Batch operation failed: {}", e),
        }
    }
    
    // Verify all puts were applied
    let val1 = storage.get_object(&bucket_id, &Key::new("key1").unwrap()).unwrap();
    assert_eq!(val1, Some(b"value1".to_vec()));
    
    let val2 = storage.get_object(&bucket_id, &Key::new("key2").unwrap()).unwrap();
    assert_eq!(val2, Some(b"value2".to_vec()));
    
    let val4 = storage.get_object(&bucket_id, &Key::new("key4").unwrap()).unwrap();
    assert_eq!(val4, Some(b"value4".to_vec()));
    
    // Verify delete was applied (key3 shouldn't exist)
    let val3 = storage.get_object(&bucket_id, &Key::new("key3").unwrap()).unwrap();
    assert_eq!(val3, None);
}

#[tokio::test]
async fn batch_with_large_objects_partially_fails() {
    // Test that large objects in batch operations are properly handled
    let (engine, _temp) = StorageEngine::temp().unwrap();
    let storage = Storage::new(engine);
    
    let bucket_id = BucketId::new("test-bucket").unwrap();
    
    // Create batch with mix of small and large objects
    let operations = vec![
        BatchOperation::Put {
            key: Key::new("small").unwrap(),
            data: vec![1u8; 1024], // 1KB - small
        },
        BatchOperation::Put {
            key: Key::new("large").unwrap(),
            data: vec![2u8; 128 * 1024], // 128KB - large
        },
    ];
    
    // Execute batch
    let response = storage.batch(&bucket_id, operations).unwrap();
    
    // Check results
    assert_eq!(response.results.len(), 2);
    
    // First should succeed (small object)
    match &response.results[0] {
        BatchResult::Success => {}
        BatchResult::Error(e) => panic!("Small object should succeed: {}", e),
    }
    
    // Second should fail (large object not supported in batch yet)
    match &response.results[1] {
        BatchResult::Error(e) => {
            assert!(e.contains("Large objects not supported"));
        }
        BatchResult::Success => panic!("Large object should have failed in batch"),
    }
    
    // Verify small object was stored
    let small_val = storage.get_object(&bucket_id, &Key::new("small").unwrap()).unwrap();
    assert!(small_val.is_some());
    assert_eq!(small_val.unwrap().len(), 1024);
    
    // Verify large object was not stored
    let large_val = storage.get_object(&bucket_id, &Key::new("large").unwrap()).unwrap();
    assert!(large_val.is_none());
}
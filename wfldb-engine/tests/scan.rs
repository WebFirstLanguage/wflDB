//! Integration tests for prefix scanning

use wfldb_core::*;
use wfldb_engine::*;

#[tokio::test]
async fn prefix_iter_is_lexicographic_and_bounded() {
    // Test that prefix scanning returns keys in lexicographic order and respects bounds
    let (engine, _temp) = StorageEngine::temp().unwrap();
    let storage = Storage::new(engine);
    
    let bucket_id = BucketId::new("test-bucket").unwrap();
    
    // Insert keys in non-alphabetical order
    let test_keys = vec![
        ("users/alice", b"alice data".as_ref()),
        ("users/charlie", b"charlie data".as_ref()),
        ("users/bob", b"bob data".as_ref()),
        ("products/apple", b"apple data".as_ref()),
        ("users/david", b"david data".as_ref()),
        ("products/banana", b"banana data".as_ref()),
        ("users/eve", b"eve data".as_ref()),
        ("settings/config", b"config data".as_ref()),
    ];
    
    for (key_str, data) in &test_keys {
        let key = Key::new(key_str).unwrap();
        storage.put_object(&bucket_id, &key, data).unwrap();
    }
    
    // Test 1: Scan with "users/" prefix - should return in lexicographic order
    let users = storage.list_objects(&bucket_id, "users/", None).unwrap();
    assert_eq!(users.len(), 5);
    
    // Verify lexicographic ordering
    let expected_order = vec!["users/alice", "users/bob", "users/charlie", "users/david", "users/eve"];
    for (i, key) in users.iter().enumerate() {
        assert_eq!(key.as_str(), expected_order[i]);
    }
    
    // Test 2: Scan with limit
    let limited = storage.list_objects(&bucket_id, "users/", Some(3)).unwrap();
    assert_eq!(limited.len(), 3);
    assert_eq!(limited[0].as_str(), "users/alice");
    assert_eq!(limited[1].as_str(), "users/bob");
    assert_eq!(limited[2].as_str(), "users/charlie");
    
    // Test 3: Scan with "products/" prefix
    let products = storage.list_objects(&bucket_id, "products/", None).unwrap();
    assert_eq!(products.len(), 2);
    assert_eq!(products[0].as_str(), "products/apple");
    assert_eq!(products[1].as_str(), "products/banana");
    
    // Test 4: Scan with empty prefix (all keys)
    let all_keys = storage.list_objects(&bucket_id, "", None).unwrap();
    assert!(all_keys.len() >= 8); // At least our test keys
    
    // Test 5: Scan with non-existent prefix
    let empty = storage.list_objects(&bucket_id, "nonexistent/", None).unwrap();
    assert_eq!(empty.len(), 0);
}

#[tokio::test]
async fn scan_pagination_consistency() {
    // Test that paginated scans maintain consistency
    let (engine, _temp) = StorageEngine::temp().unwrap();
    let storage = Storage::new(engine);
    
    let bucket_id = BucketId::new("test-bucket").unwrap();
    
    // Insert 100 keys
    for i in 0..100 {
        let key = Key::new(&format!("item/{:03}", i)).unwrap();
        let data = format!("data{}", i);
        storage.put_object(&bucket_id, &key, data.as_bytes()).unwrap();
    }
    
    // Get all items at once
    let all_items = storage.list_objects(&bucket_id, "item/", None).unwrap();
    
    // Get items in pages of 10
    let mut paginated_items = Vec::new();
    for _page in 0..10 {
        let page_items = storage.list_objects(&bucket_id, "item/", Some(10)).unwrap();
        // Note: This is a simplified test - real pagination would need a cursor/token
        // For now we're just testing that limit works
        paginated_items.extend(page_items.into_iter().take(10));
        if paginated_items.len() >= all_items.len() {
            break;
        }
    }
    
    // Verify all keys are present and in order
    for i in 0..all_items.len().min(paginated_items.len()) {
        assert_eq!(all_items[i].as_str(), paginated_items[i].as_str());
    }
}

#[cfg(test)]
mod scan_helpers {
    use super::*;
    
    #[test]
    fn test_key_ordering() {
        // Test that Key type maintains proper ordering
        let key1 = Key::new("a/1").unwrap();
        let key2 = Key::new("a/2").unwrap();
        let key3 = Key::new("b/1").unwrap();
        
        assert!(key1 < key2);
        assert!(key2 < key3);
        assert!(key1 < key3);
    }
    
    #[test]
    fn test_prefix_matching() {
        let key = Key::new("users/alice/profile").unwrap();
        
        assert!(key.has_prefix("users/"));
        assert!(key.has_prefix("users/alice"));
        assert!(key.has_prefix("users/alice/"));
        assert!(!key.has_prefix("products/"));
        assert!(!key.has_prefix("users/bob"));
    }
}
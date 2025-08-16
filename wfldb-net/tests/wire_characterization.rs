//! Characterization tests for wire protocol (FlatBuffers)
//! These tests validate zero-copy parsing, schema evolution, and framing efficiency

use wfldb_core::*;
use wfldb_core::test_utils::*;
use wfldb_net::{WireFrame, RequestMessage, ResponseMessage, RequestType};
use std::time::Instant;

/// Test that zero-copy access to wire frames is cheap
#[test]
fn wire_headers_zero_copy_access_is_cheap() {
    let sizes = vec![100, 1024, 4096, 16384, 65536];
    let mut perf = PerfAssert::new();
    
    for &size in &sizes {
        // Create a wire frame with header and body
        let header_data = format!(
            r#"{{"request_id":"test-123","bucket":"photos","key":"image.jpg","size":{}}}"#,
            size
        );
        let body_data = TestDataGenerator::random_bytes(size);
        
        // Serialize frame
        let frame = WireFrame::new(header_data.as_bytes().to_vec(), body_data.clone());
        let frame_bytes = frame.to_bytes();
        
        // Measure deserialization time
        let start = Instant::now();
        let parsed_frame = WireFrame::from_bytes(&frame_bytes).unwrap();
        perf.record_sample(start.elapsed());
        
        // Verify zero-copy semantics - body should not be cloned
        assert_eq!(parsed_frame.body.len(), size);
        assert_eq!(parsed_frame.body, body_data);
        
        // Access to body should be immediate (no parsing)
        let body_access_start = Instant::now();
        let _body_slice: &[u8] = &parsed_frame.body;
        let body_access_time = body_access_start.elapsed();
        
        // Body access should be near-instant (< 1 microsecond)
        assert!(
            body_access_time.as_nanos() < 1000,
            "Body access took {:?}, expected < 1µs",
            body_access_time
        );
    }
    
    // Wire frame parsing should be very fast
    println!("Wire frame parsing - p50: {:?}, p95: {:?}, p99: {:?}",
        perf.p50(), perf.p95(), perf.p99());
    
    // Should be well under 1ms even at p99
    assert_p99_under_ms!(perf, 1);
}

/// Test backward compatibility - older schema fields are ignored
#[test]
fn wire_headers_compat_older_schema_fields_are_ignored() {
    // Simulate an older client sending extra fields
    let header_with_extra_fields = r#"{
        "request_id": "compat-test",
        "bucket": "test-bucket",
        "key": "test-key",
        "size": 1024,
        "deprecated_field": "old_value",
        "legacy_option": true,
        "unknown_array": [1, 2, 3]
    }"#;
    
    let body = b"test data";
    let frame = WireFrame::new(header_with_extra_fields.as_bytes().to_vec(), body.to_vec());
    let frame_bytes = frame.to_bytes();
    
    // Should parse successfully, ignoring unknown fields
    let parsed = WireFrame::from_bytes(&frame_bytes);
    assert!(parsed.is_ok(), "Failed to parse frame with extra fields");
    
    let parsed_frame = parsed.unwrap();
    assert_eq!(parsed_frame.body, body);
    
    // Verify core fields are still accessible
    // Note: This assumes the WireFrame can parse JSON headers
    // In a real FlatBuffers implementation, this would use the schema
    let header_str = String::from_utf8_lossy(&parsed_frame.header);
    assert!(header_str.contains("test-bucket"));
    assert!(header_str.contains("test-key"));
}

/// Test efficient streaming of large bodies
#[test]
fn wire_frame_large_body_streaming_efficient() {
    let large_sizes = vec![
        128 * 1024,      // 128 KB
        512 * 1024,      // 512 KB
        1024 * 1024,     // 1 MB
        4 * 1024 * 1024, // 4 MB
    ];
    
    for &size in &large_sizes {
        let header = format!(r#"{{"request_id":"large","size":{}}}"#, size);
        let body = TestDataGenerator::compressible_bytes(size);
        
        // Create and serialize frame
        let frame = WireFrame::new(header.as_bytes().to_vec(), body.clone());
        let frame_bytes = frame.to_bytes();
        
        // Measure memory efficiency during parsing
        let tracker = MemoryTracker::new();
        tracker.track_allocation(frame_bytes.len());
        
        let start = Instant::now();
        let parsed = WireFrame::from_bytes(&frame_bytes).unwrap();
        let parse_time = start.elapsed();
        
        // Verify data integrity
        assert_eq!(parsed.body.len(), size);
        assert_eq!(parsed.body, body);
        
        // Parsing should scale linearly with size
        let ms_per_mb = parse_time.as_millis() as f64 / (size as f64 / (1024.0 * 1024.0));
        println!("Size: {} KB, Parse time: {:?}, ms/MB: {:.2}", 
            size / 1024, parse_time, ms_per_mb);
        
        // Should maintain reasonable performance even for large frames
        assert!(
            ms_per_mb < 10.0,
            "Parsing too slow: {:.2} ms/MB for {} bytes",
            ms_per_mb,
            size
        );
        
        tracker.track_deallocation(frame_bytes.len());
    }
}

/// Test request canonicalization for authentication
#[test]
fn wire_canonical_request_canonicalization_stable() {
    // Create identical requests
    let request1 = RequestMessage::new_put(
        "canon-123".to_string(),
        "my-bucket".to_string(),
        "path/to/file.txt".to_string(),
        1024,
        blake3::hash(b"test data").as_bytes().to_vec(),
    );
    
    let request2 = RequestMessage::new_put(
        "canon-123".to_string(),
        "my-bucket".to_string(),
        "path/to/file.txt".to_string(),
        1024,
        blake3::hash(b"test data").as_bytes().to_vec(),
    );
    
    // Canonicalization should be deterministic
    let canonical1 = request1.canonical_string();
    let canonical2 = request2.canonical_string();
    
    assert_eq!(canonical1, canonical2, "Canonicalization not deterministic");
    
    // Different requests should have different canonical forms
    let request3 = RequestMessage::new_put(
        "different-123".to_string(),
        "my-bucket".to_string(),
        "path/to/file.txt".to_string(),
        1024,
        blake3::hash(b"test data").as_bytes().to_vec(),
    );
    
    let canonical3 = request3.canonical_string();
    assert_ne!(canonical1, canonical3, "Different requests have same canonical form");
    
    // Canonical string should include all relevant fields
    assert!(canonical1.contains("canon-123"), "Missing request ID");
    assert!(canonical1.contains("my-bucket"), "Missing bucket");
    assert!(canonical1.contains("path/to/file.txt"), "Missing key");
    assert!(canonical1.contains("PUT"), "Missing request type");
}

/// Test request/response message serialization
#[test]
fn wire_message_serialization_roundtrip() {
    // Test PUT request
    let put_request = RequestMessage::new_put(
        "put-test".to_string(),
        "test-bucket".to_string(),
        "test-key".to_string(),
        2048,
        vec![1, 2, 3, 4],
    );
    
    let put_bytes = put_request.to_bytes();
    let parsed_put = RequestMessage::from_bytes(&put_bytes).unwrap();
    
    assert_eq!(parsed_put.request_id, "put-test");
    assert_eq!(parsed_put.bucket, "test-bucket");
    assert_eq!(parsed_put.key, "test-key");
    assert_eq!(parsed_put.request_type, RequestType::Put);
    
    // Test GET request
    let get_request = RequestMessage::new_get(
        "get-test".to_string(),
        "test-bucket".to_string(),
        "test-key".to_string(),
    );
    
    let get_bytes = get_request.to_bytes();
    let parsed_get = RequestMessage::from_bytes(&get_bytes).unwrap();
    
    assert_eq!(parsed_get.request_type, RequestType::Get);
    
    // Test DELETE request
    let delete_request = RequestMessage::new_delete(
        "delete-test".to_string(),
        "test-bucket".to_string(),
        "test-key".to_string(),
    );
    
    let delete_bytes = delete_request.to_bytes();
    let parsed_delete = RequestMessage::from_bytes(&delete_bytes).unwrap();
    
    assert_eq!(parsed_delete.request_type, RequestType::Delete);
}

/// Test response message handling
#[test]
fn wire_response_messages() {
    // Success response
    let success = ResponseMessage::success(
        "resp-123".to_string(),
        Some("Operation completed".to_string()),
    );
    
    let success_bytes = success.to_bytes();
    let parsed_success = ResponseMessage::from_bytes(&success_bytes).unwrap();
    
    assert!(parsed_success.success);
    assert_eq!(parsed_success.request_id, "resp-123");
    assert!(parsed_success.message.is_some());
    
    // Error response
    let error = ResponseMessage::error(
        "err-456".to_string(),
        "Not found".to_string(),
        404,
    );
    
    let error_bytes = error.to_bytes();
    let parsed_error = ResponseMessage::from_bytes(&error_bytes).unwrap();
    
    assert!(!parsed_error.success);
    assert_eq!(parsed_error.error_code.unwrap(), 404);
}

/// Benchmark wire protocol operations
#[test]
fn wire_protocol_performance_characteristics() {
    let harness = PerfTestHarness::new()
        .with_warmup(100)
        .with_iterations(1000);
    
    // Benchmark small frame serialization
    let small_header = b"{'bucket':'test','key':'small'}";
    let small_body = TestDataGenerator::random_bytes(1024);
    
    let mut serialize_perf = harness.run(|| {
        let frame = WireFrame::new(small_header.to_vec(), small_body.clone());
        let _bytes = frame.to_bytes();
    });
    
    println!("Small frame serialization - p50: {:?}, p95: {:?}",
        serialize_perf.p50(), serialize_perf.p95());
    
    // Benchmark small frame deserialization
    let frame = WireFrame::new(small_header.to_vec(), small_body.clone());
    let frame_bytes = frame.to_bytes();
    
    let mut deserialize_perf = harness.run(|| {
        let _parsed = WireFrame::from_bytes(&frame_bytes).unwrap();
    });
    
    println!("Small frame deserialization - p50: {:?}, p95: {:?}",
        deserialize_perf.p50(), deserialize_perf.p95());
    
    // Both should be very fast (< 1ms at p95)
    assert_p95_under_ms!(serialize_perf, 1);
    assert_p95_under_ms!(deserialize_perf, 1);
}

/// Test frame size limits and validation
#[test]
fn wire_frame_size_validation() {
    // Test maximum header size (assume 64KB limit)
    let large_header = vec![b'x'; 64 * 1024];
    let body = b"test";
    
    let frame = WireFrame::new(large_header.clone(), body.to_vec());
    let bytes = frame.to_bytes();
    let parsed = WireFrame::from_bytes(&bytes);
    
    assert!(parsed.is_ok(), "Should handle maximum header size");
    
    // Test empty header and body
    let empty_frame = WireFrame::new(vec![], vec![]);
    let empty_bytes = empty_frame.to_bytes();
    let parsed_empty = WireFrame::from_bytes(&empty_bytes);
    
    assert!(parsed_empty.is_ok(), "Should handle empty frame");
    assert_eq!(parsed_empty.unwrap().body.len(), 0);
}

/// Test concurrent wire protocol operations
#[test]
fn wire_protocol_thread_safety() {
    use std::sync::Arc;
    use std::thread;
    
    let num_threads = 10;
    let ops_per_thread = 100;
    
    let mut handles = vec![];
    
    for thread_id in 0..num_threads {
        let handle = thread::spawn(move || {
            for op_id in 0..ops_per_thread {
                let request = RequestMessage::new_put(
                    format!("thread-{}-op-{}", thread_id, op_id),
                    "concurrent-bucket".to_string(),
                    format!("key-{}-{}", thread_id, op_id),
                    1024,
                    vec![thread_id as u8, op_id as u8],
                );
                
                let bytes = request.to_bytes();
                let parsed = RequestMessage::from_bytes(&bytes).unwrap();
                
                assert_eq!(parsed.request_id, format!("thread-{}-op-{}", thread_id, op_id));
            }
        });
        
        handles.push(handle);
    }
    
    for handle in handles {
        handle.join().unwrap();
    }
}
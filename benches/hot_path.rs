//! Performance benchmarks for wflDB hot paths
//! 
//! These benchmarks validate that we can achieve p95 < 10ms for small operations
//! as specified in the R&D document.

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use std::time::Duration;
use wfldb_core::*;
use wfldb_engine::{StorageEngine, Storage};
use wfldb_net::{WireFrame, RequestMessage, RequestType};

const SMALL_DATA_SIZES: &[usize] = &[100, 1024, 4096, 16384, 32768]; // Up to 32KB
const LARGE_DATA_SIZE: usize = 128 * 1024; // 128KB for chunked test

fn setup_storage() -> (Storage, tempfile::TempDir) {
    let (engine, temp_dir) = StorageEngine::temp().unwrap();
    let storage = Storage::new(engine);
    (storage, temp_dir)
}

/// Benchmark pure storage operations (without network layer)
fn bench_storage_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("storage_operations");
    
    for &size in SMALL_DATA_SIZES {
        let (storage, _temp) = setup_storage();
        let bucket_id = BucketId::new("bench-bucket").unwrap();
        let data = vec![42u8; size];
        
        group.bench_with_input(
            BenchmarkId::new("put_small", size),
            &size,
            |b, &_size| {
                let mut counter = 0;
                b.iter(|| {
                    let key = Key::new(&format!("key-{}", counter)).unwrap();
                    counter += 1;
                    black_box(storage.put_object(&bucket_id, &key, &data).unwrap());
                });
            }
        );
    }
    
    // Benchmark get operations
    for &size in SMALL_DATA_SIZES {
        let (storage, _temp) = setup_storage();
        let bucket_id = BucketId::new("bench-bucket").unwrap();
        let key = Key::new("bench-key").unwrap();
        let data = vec![42u8; size];
        
        // Pre-populate data
        storage.put_object(&bucket_id, &key, &data).unwrap();
        
        group.bench_with_input(
            BenchmarkId::new("get_small", size),
            &size,
            |b, &_size| {
                b.iter(|| {
                    let result = storage.get_object(&bucket_id, &key).unwrap();
                    black_box(result);
                });
            }
        );
    }
    
    group.finish();
}

/// Benchmark wire protocol operations
fn bench_wire_protocol(c: &mut Criterion) {
    let mut group = c.benchmark_group("wire_protocol");
    
    for &size in SMALL_DATA_SIZES {
        let header_data = format!(r#"{{"request_id":"bench","bucket":"photos","key":"test.jpg","size":{}}}"#, size);
        let body_data = vec![42u8; size];
        
        group.bench_with_input(
            BenchmarkId::new("frame_serialize", size),
            &size,
            |b, &_size| {
                b.iter(|| {
                    let frame = WireFrame::new(header_data.as_bytes().to_vec(), body_data.clone());
                    let bytes = black_box(frame.to_bytes());
                    black_box(bytes);
                });
            }
        );
        
        group.bench_with_input(
            BenchmarkId::new("frame_deserialize", size),
            &size,
            |b, &_size| {
                let frame = WireFrame::new(header_data.as_bytes().to_vec(), body_data.clone());
                let bytes = frame.to_bytes();
                
                b.iter(|| {
                    let parsed = black_box(WireFrame::from_bytes(&bytes).unwrap());
                    black_box(parsed);
                });
            }
        );
    }
    
    // Benchmark message parsing
    let request = RequestMessage::new_put(
        "bench-123".to_string(),
        "photos".to_string(),
        "test.jpg".to_string(),
        1024,
        vec![1, 2, 3, 4]
    );
    let request_bytes = request.to_bytes();
    
    group.bench_function("request_parse", |b| {
        b.iter(|| {
            let parsed = black_box(RequestMessage::from_bytes(&request_bytes).unwrap());
            black_box(parsed);
        });
    });
    
    group.finish();
}

/// Benchmark end-to-end synthetic hot path
/// This combines wire parsing + storage operation to simulate real request handling
fn bench_hot_path_synthetic(c: &mut Criterion) {
    let mut group = c.benchmark_group("hot_path_synthetic");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(1000);
    
    for &size in SMALL_DATA_SIZES {
        let (storage, _temp) = setup_storage();
        let bucket_id = BucketId::new("bench-bucket").unwrap();
        let data = vec![42u8; size];
        
        // Create request message
        let request = RequestMessage::new_put(
            "bench-123".to_string(),
            bucket_id.as_str().to_string(),
            "bench-key".to_string(),
            size as u64,
            blake3::hash(&data).as_bytes().to_vec()
        );
        
        group.bench_with_input(
            BenchmarkId::new("put_hot_path", size),
            &size,
            |b, &_size| {
                let mut counter = 0;
                b.iter(|| {
                    // Simulate parsing wire frame (header only, body is separate)
                    let header_bytes = request.to_bytes();
                    let parsed_request = RequestMessage::from_bytes(&header_bytes).unwrap();
                    
                    // Extract bucket and key
                    let bucket = BucketId::new(&parsed_request.bucket).unwrap();
                    let key = Key::new(&format!("{}-{}", parsed_request.key, counter)).unwrap();
                    counter += 1;
                    
                    // Store object
                    let result = storage.put_object(&bucket, &key, &data);
                    black_box(result.unwrap());
                });
            }
        );
        
        // Pre-populate for GET benchmark
        let key = Key::new("get-bench-key").unwrap();
        storage.put_object(&bucket_id, &key, &data).unwrap();
        
        let get_request = RequestMessage::new_get(
            "get-bench-123".to_string(),
            bucket_id.as_str().to_string(),
            key.as_str().to_string(),
        );
        
        group.bench_with_input(
            BenchmarkId::new("get_hot_path", size),
            &size,
            |b, &_size| {
                b.iter(|| {
                    // Parse request
                    let header_bytes = get_request.to_bytes();
                    let parsed_request = RequestMessage::from_bytes(&header_bytes).unwrap();
                    
                    // Extract bucket and key
                    let bucket = BucketId::new(&parsed_request.bucket).unwrap();
                    let key = Key::new(&parsed_request.key).unwrap();
                    
                    // Get object
                    let result = storage.get_object(&bucket, &key);
                    black_box(result.unwrap());
                });
            }
        );
    }
    
    group.finish();
}

/// Benchmark large object handling (chunked storage)
fn bench_large_objects(c: &mut Criterion) {
    let mut group = c.benchmark_group("large_objects");
    group.measurement_time(Duration::from_secs(15));
    group.sample_size(100);
    
    let (storage, _temp) = setup_storage();
    let bucket_id = BucketId::new("large-bucket").unwrap();
    let large_data = vec![42u8; LARGE_DATA_SIZE];
    
    group.bench_function("put_large", |b| {
        let mut counter = 0;
        b.iter(|| {
            let key = Key::new(&format!("large-key-{}", counter)).unwrap();
            counter += 1;
            let result = storage.put_object(&bucket_id, &key, &large_data);
            black_box(result.unwrap());
        });
    });
    
    // Pre-populate for GET test
    let large_key = Key::new("large-test-key").unwrap();
    storage.put_object(&bucket_id, &large_key, &large_data).unwrap();
    
    group.bench_function("get_large", |b| {
        b.iter(|| {
            let result = storage.get_object(&bucket_id, &large_key);
            black_box(result.unwrap());
        });
    });
    
    group.finish();
}

/// Concurrent operations benchmark
fn bench_concurrent_operations(c: &mut Criterion) {
    use std::sync::Arc;
    use std::thread;
    
    let mut group = c.benchmark_group("concurrent_operations");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(50);
    
    let (storage, _temp) = setup_storage();
    let storage = Arc::new(storage);
    let bucket_id = BucketId::new("concurrent-bucket").unwrap();
    let data = vec![42u8; 1024];
    
    group.bench_function("concurrent_puts", |b| {
        b.iter(|| {
            let handles: Vec<_> = (0..4).map(|thread_id| {
                let storage = Arc::clone(&storage);
                let bucket_id = bucket_id.clone();
                let data = data.clone();
                
                thread::spawn(move || {
                    for i in 0..10 {
                        let key = Key::new(&format!("thread-{}-key-{}", thread_id, i)).unwrap();
                        storage.put_object(&bucket_id, &key, &data).unwrap();
                    }
                })
            }).collect();
            
            for handle in handles {
                handle.join().unwrap();
            }
        });
    });
    
    group.finish();
}

/// Memory usage and allocation benchmarks
fn bench_memory_efficiency(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_efficiency");
    
    // Test zero-copy parsing simulation
    group.bench_function("zero_copy_simulation", |b| {
        let header_data = b"{'bucket':'photos','key':'cat.jpg','size':1024}";
        let body_data = vec![42u8; 1024];
        let frame_bytes = {
            let frame = WireFrame::new(header_data.to_vec(), body_data);
            frame.to_bytes()
        };
        
        b.iter(|| {
            let frame = WireFrame::from_bytes(&frame_bytes).unwrap();
            
            // Simulate zero-copy access to body without cloning
            let body_slice: &[u8] = &frame.body;
            black_box(body_slice.len());
            
            // In real implementation, header parsing would also be zero-copy
            // using FlatBuffers get_root_as_*() functions
            black_box(&frame.header[0..10]); // Simulate accessing header fields
        });
    });
    
    group.finish();
}

criterion_group!(
    benches,
    bench_storage_operations,
    bench_wire_protocol, 
    bench_hot_path_synthetic,
    bench_large_objects,
    bench_concurrent_operations,
    bench_memory_efficiency
);

criterion_main!(benches);
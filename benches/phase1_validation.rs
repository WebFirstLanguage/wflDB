//! Phase 1 Performance Validation Benchmarks

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use wfldb_core::*;
use wfldb_engine::*;
use std::time::Duration;

fn benchmark_small_object_put(c: &mut Criterion) {
    let (engine, _temp) = StorageEngine::temp().unwrap();
    let storage = Storage::new(engine);
    let bucket_id = BucketId::new("bench-bucket").unwrap();
    
    let mut group = c.benchmark_group("small_object_put");
    group.measurement_time(Duration::from_secs(10));
    
    for size in &[100, 1024, 4096, 16384, 32768] {
        let data = vec![42u8; *size];
        let key = Key::new(&format!("key-{}", size)).unwrap();
        
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| {
                storage.put_object(&bucket_id, &key, black_box(&data)).unwrap();
            });
        });
    }
    group.finish();
}

fn benchmark_small_object_get(c: &mut Criterion) {
    let (engine, _temp) = StorageEngine::temp().unwrap();
    let storage = Storage::new(engine);
    let bucket_id = BucketId::new("bench-bucket").unwrap();
    
    // Pre-populate data
    for size in &[100, 1024, 4096, 16384, 32768] {
        let data = vec![42u8; *size];
        let key = Key::new(&format!("key-{}", size)).unwrap();
        storage.put_object(&bucket_id, &key, &data).unwrap();
    }
    
    let mut group = c.benchmark_group("small_object_get");
    group.measurement_time(Duration::from_secs(10));
    
    for size in &[100, 1024, 4096, 16384, 32768] {
        let key = Key::new(&format!("key-{}", size)).unwrap();
        
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| {
                let _ = black_box(storage.get_object(&bucket_id, &key).unwrap());
            });
        });
    }
    group.finish();
}

fn benchmark_large_object_chunking(c: &mut Criterion) {
    let (engine, _temp) = StorageEngine::temp().unwrap();
    let storage = Storage::new(engine);
    let bucket_id = BucketId::new("bench-bucket").unwrap();
    
    let mut group = c.benchmark_group("large_object_chunking");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(20);
    
    // Test 1MB, 5MB, 10MB objects
    for size_mb in &[1, 5, 10] {
        let size = size_mb * 1024 * 1024;
        let data = vec![42u8; size];
        let key = Key::new(&format!("large-{}", size_mb)).unwrap();
        
        group.bench_with_input(BenchmarkId::from_parameter(format!("{}MB", size_mb)), size_mb, |b, _| {
            b.iter(|| {
                storage.put_object(&bucket_id, &key, black_box(&data)).unwrap();
            });
        });
    }
    group.finish();
}

fn benchmark_batch_operations(c: &mut Criterion) {
    let (engine, _temp) = StorageEngine::temp().unwrap();
    let storage = Storage::new(engine);
    let bucket_id = BucketId::new("bench-bucket").unwrap();
    
    let mut group = c.benchmark_group("batch_operations");
    group.measurement_time(Duration::from_secs(10));
    
    for batch_size in &[10, 50, 100] {
        let operations: Vec<BatchOperation> = (0..*batch_size)
            .map(|i| BatchOperation::Put {
                key: Key::new(&format!("batch-key-{}", i)).unwrap(),
                data: vec![i as u8; 1024],
            })
            .collect();
        
        group.bench_with_input(BenchmarkId::from_parameter(batch_size), batch_size, |b, _| {
            b.iter(|| {
                storage.batch(&bucket_id, black_box(operations.clone())).unwrap();
            });
        });
    }
    group.finish();
}

fn benchmark_prefix_scan(c: &mut Criterion) {
    let (engine, _temp) = StorageEngine::temp().unwrap();
    let storage = Storage::new(engine);
    let bucket_id = BucketId::new("bench-bucket").unwrap();
    
    // Pre-populate with hierarchical keys
    for i in 0..1000 {
        let key = Key::new(&format!("users/user{:04}", i)).unwrap();
        let data = format!("user data {}", i);
        storage.put_object(&bucket_id, &key, data.as_bytes()).unwrap();
    }
    
    let mut group = c.benchmark_group("prefix_scan");
    group.measurement_time(Duration::from_secs(5));
    
    for limit in &[10, 100, 500] {
        group.bench_with_input(BenchmarkId::from_parameter(limit), limit, |b, &limit| {
            b.iter(|| {
                let _ = black_box(storage.list_objects(&bucket_id, "users/", Some(limit)).unwrap());
            });
        });
    }
    group.finish();
}

criterion_group!(
    phase1_benches,
    benchmark_small_object_put,
    benchmark_small_object_get,
    benchmark_large_object_chunking,
    benchmark_batch_operations,
    benchmark_prefix_scan
);
criterion_main!(phase1_benches);
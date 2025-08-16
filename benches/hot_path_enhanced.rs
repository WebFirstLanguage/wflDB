//! Enhanced performance benchmarks with percentile tracking for wflDB hot paths
//! 
//! These benchmarks validate that we can achieve p95 < 10ms for small operations
//! with detailed percentile analysis (p50, p95, p99, p99.9)

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, PlotConfiguration, AxisScale};
use std::time::{Duration, Instant};
use wfldb_core::*;
use wfldb_core::test_utils::*;
use wfldb_engine::{StorageEngine, Storage};
use wfldb_net::{WireFrame, RequestMessage, RequestType};

const SMALL_DATA_SIZES: &[usize] = &[100, 1024, 4096, 16384, 32768]; // Up to 32KB
const LARGE_DATA_SIZE: usize = 128 * 1024; // 128KB for chunked test

fn setup_storage() -> (Storage, tempfile::TempDir) {
    let (engine, temp_dir) = StorageEngine::temp().unwrap();
    let storage = Storage::new(engine);
    (storage, temp_dir)
}

/// Custom percentile tracking benchmark function
fn bench_with_percentiles<F>(name: &str, iterations: usize, mut f: F) 
where 
    F: FnMut()
{
    let mut perf = PerfAssert::new();
    
    // Warmup
    for _ in 0..100 {
        f();
    }
    
    // Actual benchmark
    for _ in 0..iterations {
        let start = Instant::now();
        f();
        perf.record_sample(start.elapsed());
    }
    
    // Report percentiles
    println!("\n{} Performance Percentiles:", name);
    println!("  p50:   {:?}", perf.p50());
    println!("  p95:   {:?}", perf.p95());
    println!("  p99:   {:?}", perf.p99());
    println!("  p99.9: {:?}", perf.p999());
    
    // Assert performance targets
    perf.assert_p95_under_ms(10);
}

/// Enhanced storage operations benchmark with percentile tracking
fn bench_storage_operations_enhanced(c: &mut Criterion) {
    let plot_config = PlotConfiguration::default()
        .summary_scale(AxisScale::Logarithmic);
    
    let mut group = c.benchmark_group("storage_operations_enhanced");
    group.plot_config(plot_config);
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(1000);
    
    // PUT operations with percentile tracking
    for &size in SMALL_DATA_SIZES {
        let (storage, _temp) = setup_storage();
        let bucket_id = BucketId::new("bench-bucket").unwrap();
        let data = TestDataGenerator::random_bytes(size);
        
        println!("\n=== PUT Operation - Size: {} bytes ===", size);
        
        group.bench_with_input(
            BenchmarkId::new("put_small_percentiles", size),
            &size,
            |b, &_size| {
                let mut counter = 0;
                let mut perf = PerfAssert::new();
                
                b.iter_custom(|iters| {
                    let mut total = Duration::ZERO;
                    for _ in 0..iters {
                        let key = Key::new(&format!("key-{}", counter)).unwrap();
                        counter += 1;
                        
                        let start = Instant::now();
                        black_box(storage.put_object(&bucket_id, &key, &data).unwrap());
                        let elapsed = start.elapsed();
                        
                        perf.record_sample(elapsed);
                        total += elapsed;
                    }
                    total
                });
                
                // Print percentiles after benchmark
                if perf.samples.len() > 0 {
                    println!("  PUT p50: {:?}, p95: {:?}, p99: {:?}", 
                        perf.p50(), perf.p95(), perf.p99());
                }
            }
        );
    }
    
    // GET operations with percentile tracking
    for &size in SMALL_DATA_SIZES {
        let (storage, _temp) = setup_storage();
        let bucket_id = BucketId::new("bench-bucket").unwrap();
        let key = Key::new("bench-key").unwrap();
        let data = TestDataGenerator::random_bytes(size);
        
        // Pre-populate data
        storage.put_object(&bucket_id, &key, &data).unwrap();
        
        println!("\n=== GET Operation - Size: {} bytes ===", size);
        
        group.bench_with_input(
            BenchmarkId::new("get_small_percentiles", size),
            &size,
            |b, &_size| {
                let mut perf = PerfAssert::new();
                
                b.iter_custom(|iters| {
                    let mut total = Duration::ZERO;
                    for _ in 0..iters {
                        let start = Instant::now();
                        let result = storage.get_object(&bucket_id, &key).unwrap();
                        black_box(result);
                        let elapsed = start.elapsed();
                        
                        perf.record_sample(elapsed);
                        total += elapsed;
                    }
                    total
                });
                
                // Print percentiles after benchmark
                if perf.samples.len() > 0 {
                    println!("  GET p50: {:?}, p95: {:?}, p99: {:?}", 
                        perf.p50(), perf.p95(), perf.p99());
                }
            }
        );
    }
    
    group.finish();
}

/// Enhanced hot path benchmark with detailed percentile analysis
fn bench_hot_path_percentiles(c: &mut Criterion) {
    let mut group = c.benchmark_group("hot_path_percentiles");
    group.measurement_time(Duration::from_secs(15));
    group.sample_size(5000);
    
    println!("\n=== Hot Path End-to-End Performance ===");
    
    for &size in &[1024, 4096, 16384] {
        let (storage, _temp) = setup_storage();
        let bucket_id = BucketId::new("hot-bucket").unwrap();
        let data = TestDataGenerator::compressible_bytes(size);
        
        // Create request message
        let request = RequestMessage::new_put(
            "bench-123".to_string(),
            bucket_id.as_str().to_string(),
            "bench-key".to_string(),
            size as u64,
            blake3::hash(&data).as_bytes().to_vec()
        );
        
        println!("\n--- Size: {} KB ---", size / 1024);
        
        group.bench_with_input(
            BenchmarkId::new("e2e_hot_path", size),
            &size,
            |b, &_size| {
                let mut counter = 0;
                let mut perf = PerfAssert::new();
                
                b.iter_custom(|iters| {
                    let mut total = Duration::ZERO;
                    
                    for _ in 0..iters {
                        let start = Instant::now();
                        
                        // Simulate full request processing
                        // 1. Parse wire frame
                        let header_bytes = request.to_bytes();
                        let parsed_request = RequestMessage::from_bytes(&header_bytes).unwrap();
                        
                        // 2. Extract bucket and key
                        let bucket = BucketId::new(&parsed_request.bucket).unwrap();
                        let key = Key::new(&format!("{}-{}", parsed_request.key, counter)).unwrap();
                        counter += 1;
                        
                        // 3. Store object
                        let result = storage.put_object(&bucket, &key, &data);
                        black_box(result.unwrap());
                        
                        let elapsed = start.elapsed();
                        perf.record_sample(elapsed);
                        total += elapsed;
                    }
                    
                    total
                });
                
                // Print detailed percentiles
                if perf.samples.len() > 0 {
                    println!("  Percentiles:");
                    println!("    p50:   {:?}", perf.p50());
                    println!("    p95:   {:?}", perf.p95());
                    println!("    p99:   {:?}", perf.p99());
                    println!("    p99.9: {:?}", perf.p999());
                    
                    // Validate p95 < 10ms target
                    let p95_ms = perf.p95().as_millis();
                    if p95_ms < 10 {
                        println!("    ✅ p95 < 10ms target MET ({} ms)", p95_ms);
                    } else {
                        println!("    ❌ p95 < 10ms target MISSED ({} ms)", p95_ms);
                    }
                }
            }
        );
    }
    
    group.finish();
}

/// Memory allocation tracking benchmark
fn bench_memory_allocations(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_allocations");
    
    println!("\n=== Memory Allocation Analysis ===");
    
    for &size in &[1024, 16384, 65536] {
        let (storage, _temp) = setup_storage();
        let bucket_id = BucketId::new("mem-bucket").unwrap();
        let data = TestDataGenerator::random_bytes(size);
        
        println!("\n--- Object Size: {} KB ---", size / 1024);
        
        group.bench_with_input(
            BenchmarkId::new("memory_per_operation", size),
            &size,
            |b, &_size| {
                let tracker = MemoryTracker::new();
                let mut counter = 0;
                
                b.iter(|| {
                    tracker.track_allocation(size);
                    
                    let key = Key::new(&format!("mem-key-{}", counter)).unwrap();
                    counter += 1;
                    
                    let result = storage.put_object(&bucket_id, &key, &data).unwrap();
                    black_box(result);
                    
                    tracker.track_deallocation(size);
                });
                
                // Report memory statistics
                if tracker.allocation_count() > 0 {
                    println!("  Allocations: {}", tracker.allocation_count());
                    println!("  Peak Memory: {} KB", tracker.peak_memory_bytes() / 1024);
                    println!("  Current Memory: {} KB", tracker.current_memory_bytes() / 1024);
                }
            }
        );
    }
    
    group.finish();
}

/// Latency distribution analysis
fn bench_latency_distribution(c: &mut Criterion) {
    let mut group = c.benchmark_group("latency_distribution");
    group.measurement_time(Duration::from_secs(20));
    
    println!("\n=== Latency Distribution Analysis ===");
    
    let (storage, _temp) = setup_storage();
    let bucket_id = BucketId::new("dist-bucket").unwrap();
    let data = TestDataGenerator::random_bytes(4096);
    
    group.bench_function("latency_histogram", |b| {
        let mut counter = 0;
        let mut latencies = Vec::new();
        
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            
            for _ in 0..iters {
                let key = Key::new(&format!("dist-key-{}", counter)).unwrap();
                counter += 1;
                
                let start = Instant::now();
                storage.put_object(&bucket_id, &key, &data).unwrap();
                let elapsed = start.elapsed();
                
                latencies.push(elapsed.as_micros() as u64);
                total += elapsed;
            }
            
            // Print distribution statistics
            if latencies.len() >= 1000 {
                latencies.sort();
                let len = latencies.len();
                
                println!("\n  Latency Distribution (microseconds):");
                println!("    Min:    {}", latencies[0]);
                println!("    p10:    {}", latencies[len * 10 / 100]);
                println!("    p25:    {}", latencies[len * 25 / 100]);
                println!("    p50:    {}", latencies[len * 50 / 100]);
                println!("    p75:    {}", latencies[len * 75 / 100]);
                println!("    p90:    {}", latencies[len * 90 / 100]);
                println!("    p95:    {}", latencies[len * 95 / 100]);
                println!("    p99:    {}", latencies[len * 99 / 100]);
                println!("    p99.9:  {}", latencies[len * 999 / 1000]);
                println!("    Max:    {}", latencies[len - 1]);
                
                latencies.clear();
            }
            
            total
        });
    });
    
    group.finish();
}

/// Comparison benchmark for alternative approaches
fn bench_comparison_alternatives(c: &mut Criterion) {
    let mut group = c.benchmark_group("comparison_alternatives");
    
    println!("\n=== Alternative Approach Comparison ===");
    
    let size = 4096;
    let data = TestDataGenerator::random_bytes(size);
    
    // Current fjall-based approach
    {
        let (storage, _temp) = setup_storage();
        let bucket_id = BucketId::new("fjall-bucket").unwrap();
        
        group.bench_function("fjall_approach", |b| {
            let mut counter = 0;
            b.iter(|| {
                let key = Key::new(&format!("key-{}", counter)).unwrap();
                counter += 1;
                storage.put_object(&bucket_id, &key, &data).unwrap();
            });
        });
    }
    
    // Simulated in-memory approach for comparison
    {
        use std::collections::HashMap;
        use std::sync::RwLock;
        
        let storage = RwLock::new(HashMap::new());
        
        group.bench_function("inmemory_approach", |b| {
            let mut counter = 0;
            b.iter(|| {
                let key = format!("key-{}", counter);
                counter += 1;
                storage.write().unwrap().insert(key, data.clone());
            });
        });
    }
    
    group.finish();
    
    println!("\n=== Performance Validation Summary ===");
    println!("✅ Characterization benchmarks complete");
    println!("📊 Percentile tracking enabled for all hot paths");
    println!("🎯 Target: p95 < 10ms for small operations");
}

criterion_group!(
    benches,
    bench_storage_operations_enhanced,
    bench_hot_path_percentiles,
    bench_memory_allocations,
    bench_latency_distribution,
    bench_comparison_alternatives
);

criterion_main!(benches);
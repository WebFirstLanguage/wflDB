//! Test utilities and infrastructure for wflDB testing

#![cfg(test)]

use std::time::{Duration, Instant};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Performance assertion helpers
pub struct PerfAssert {
    samples: Vec<Duration>,
}

impl PerfAssert {
    pub fn new() -> Self {
        PerfAssert {
            samples: Vec::new(),
        }
    }
    
    pub fn record_sample(&mut self, duration: Duration) {
        self.samples.push(duration);
    }
    
    pub fn record_operation<F, R>(&mut self, f: F) -> R 
    where 
        F: FnOnce() -> R
    {
        let start = Instant::now();
        let result = f();
        self.samples.push(start.elapsed());
        result
    }
    
    pub fn percentile(&mut self, p: f64) -> Duration {
        assert!(p >= 0.0 && p <= 100.0, "Percentile must be between 0 and 100");
        assert!(!self.samples.is_empty(), "No samples recorded");
        
        self.samples.sort();
        let index = ((p / 100.0) * (self.samples.len() - 1) as f64).round() as usize;
        self.samples[index]
    }
    
    pub fn p50(&mut self) -> Duration {
        self.percentile(50.0)
    }
    
    pub fn p95(&mut self) -> Duration {
        self.percentile(95.0)
    }
    
    pub fn p99(&mut self) -> Duration {
        self.percentile(99.0)
    }
    
    pub fn p999(&mut self) -> Duration {
        self.percentile(99.9)
    }
    
    pub fn assert_p95_under_ms(&mut self, max_ms: u64) {
        let p95 = self.p95();
        assert!(
            p95 <= Duration::from_millis(max_ms),
            "p95 latency {} ms exceeds maximum {} ms",
            p95.as_millis(),
            max_ms
        );
    }
    
    pub fn assert_p99_under_ms(&mut self, max_ms: u64) {
        let p99 = self.p99();
        assert!(
            p99 <= Duration::from_millis(max_ms),
            "p99 latency {} ms exceeds maximum {} ms",
            p99.as_millis(),
            max_ms
        );
    }
}

/// Memory tracking utilities
pub struct MemoryTracker {
    allocations: Arc<AtomicUsize>,
    deallocations: Arc<AtomicUsize>,
    peak_bytes: Arc<AtomicUsize>,
    current_bytes: Arc<AtomicUsize>,
}

impl MemoryTracker {
    pub fn new() -> Self {
        MemoryTracker {
            allocations: Arc::new(AtomicUsize::new(0)),
            deallocations: Arc::new(AtomicUsize::new(0)),
            peak_bytes: Arc::new(AtomicUsize::new(0)),
            current_bytes: Arc::new(AtomicUsize::new(0)),
        }
    }
    
    pub fn track_allocation(&self, bytes: usize) {
        self.allocations.fetch_add(1, Ordering::SeqCst);
        let current = self.current_bytes.fetch_add(bytes, Ordering::SeqCst) + bytes;
        
        // Update peak if necessary
        let mut peak = self.peak_bytes.load(Ordering::SeqCst);
        while current > peak {
            match self.peak_bytes.compare_exchange_weak(
                peak,
                current,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => break,
                Err(p) => peak = p,
            }
        }
    }
    
    pub fn track_deallocation(&self, bytes: usize) {
        self.deallocations.fetch_add(1, Ordering::SeqCst);
        self.current_bytes.fetch_sub(bytes, Ordering::SeqCst);
    }
    
    pub fn allocation_count(&self) -> usize {
        self.allocations.load(Ordering::SeqCst)
    }
    
    pub fn deallocation_count(&self) -> usize {
        self.deallocations.load(Ordering::SeqCst)
    }
    
    pub fn peak_memory_bytes(&self) -> usize {
        self.peak_bytes.load(Ordering::SeqCst)
    }
    
    pub fn current_memory_bytes(&self) -> usize {
        self.current_bytes.load(Ordering::SeqCst)
    }
    
    pub fn assert_no_leaks(&self) {
        let current = self.current_memory_bytes();
        assert_eq!(
            current, 0,
            "Memory leak detected: {} bytes still allocated",
            current
        );
    }
    
    pub fn assert_peak_under_mb(&self, max_mb: usize) {
        let peak_mb = self.peak_memory_bytes() / (1024 * 1024);
        assert!(
            peak_mb <= max_mb,
            "Peak memory {} MB exceeds maximum {} MB",
            peak_mb,
            max_mb
        );
    }
}

/// Storage crash simulation utilities
pub struct CrashSimulator {
    crash_points: BTreeMap<String, bool>,
}

impl CrashSimulator {
    pub fn new() -> Self {
        CrashSimulator {
            crash_points: BTreeMap::new(),
        }
    }
    
    pub fn set_crash_point(&mut self, name: &str) {
        self.crash_points.insert(name.to_string(), true);
    }
    
    pub fn should_crash(&mut self, name: &str) -> bool {
        self.crash_points.get(name).copied().unwrap_or(false)
    }
    
    pub fn maybe_crash(&mut self, name: &str) -> Result<(), String> {
        if self.should_crash(name) {
            self.crash_points.insert(name.to_string(), false);
            Err(format!("Simulated crash at: {}", name))
        } else {
            Ok(())
        }
    }
}

/// Network fault injection utilities
pub struct NetworkFaultInjector {
    latency_ms: Option<u64>,
    packet_loss_rate: f64,
    bandwidth_limit_bps: Option<usize>,
}

impl NetworkFaultInjector {
    pub fn new() -> Self {
        NetworkFaultInjector {
            latency_ms: None,
            packet_loss_rate: 0.0,
            bandwidth_limit_bps: None,
        }
    }
    
    pub fn with_latency(mut self, ms: u64) -> Self {
        self.latency_ms = Some(ms);
        self
    }
    
    pub fn with_packet_loss(mut self, rate: f64) -> Self {
        assert!(rate >= 0.0 && rate <= 1.0);
        self.packet_loss_rate = rate;
        self
    }
    
    pub fn with_bandwidth_limit(mut self, bps: usize) -> Self {
        self.bandwidth_limit_bps = Some(bps);
        self
    }
    
    pub async fn inject_delay(&self) {
        if let Some(ms) = self.latency_ms {
            tokio::time::sleep(Duration::from_millis(ms)).await;
        }
    }
    
    pub fn should_drop_packet(&self) -> bool {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        rng.gen::<f64>() < self.packet_loss_rate
    }
    
    pub async fn throttle_bandwidth(&self, bytes: usize) {
        if let Some(bps) = self.bandwidth_limit_bps {
            let delay_ms = (bytes as f64 / bps as f64 * 1000.0) as u64;
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }
    }
}

/// Test data generators
pub struct TestDataGenerator;

impl TestDataGenerator {
    pub fn random_bytes(size: usize) -> Vec<u8> {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        (0..size).map(|_| rng.gen()).collect()
    }
    
    pub fn compressible_bytes(size: usize) -> Vec<u8> {
        // Generate compressible data with patterns
        let pattern = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
        let mut result = Vec::with_capacity(size);
        
        while result.len() < size {
            let chunk_size = std::cmp::min(pattern.len(), size - result.len());
            result.extend_from_slice(&pattern[..chunk_size]);
        }
        
        result
    }
    
    pub fn incompressible_bytes(size: usize) -> Vec<u8> {
        // Generate high-entropy data that won't compress well
        use rand::Rng;
        let mut rng = rand::thread_rng();
        (0..size).map(|_| rng.gen::<u8>()).collect()
    }
    
    pub fn sequential_keys(prefix: &str, count: usize) -> Vec<String> {
        (0..count).map(|i| format!("{}-{:08}", prefix, i)).collect()
    }
    
    pub fn random_keys(prefix: &str, count: usize) -> Vec<String> {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        (0..count).map(|_| {
            format!("{}-{:016x}", prefix, rng.gen::<u64>())
        }).collect()
    }
}

/// Performance test harness
pub struct PerfTestHarness {
    warmup_iterations: usize,
    test_iterations: usize,
}

impl PerfTestHarness {
    pub fn new() -> Self {
        PerfTestHarness {
            warmup_iterations: 100,
            test_iterations: 1000,
        }
    }
    
    pub fn with_warmup(mut self, iterations: usize) -> Self {
        self.warmup_iterations = iterations;
        self
    }
    
    pub fn with_iterations(mut self, iterations: usize) -> Self {
        self.test_iterations = iterations;
        self
    }
    
    pub fn run<F>(&self, mut f: F) -> PerfAssert
    where
        F: FnMut(),
    {
        // Warmup phase
        for _ in 0..self.warmup_iterations {
            f();
        }
        
        // Test phase
        let mut perf = PerfAssert::new();
        for _ in 0..self.test_iterations {
            let start = Instant::now();
            f();
            perf.record_sample(start.elapsed());
        }
        
        perf
    }
    
    pub async fn run_async<F, Fut>(&self, mut f: F) -> PerfAssert
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        // Warmup phase
        for _ in 0..self.warmup_iterations {
            f().await;
        }
        
        // Test phase
        let mut perf = PerfAssert::new();
        for _ in 0..self.test_iterations {
            let start = Instant::now();
            f().await;
            perf.record_sample(start.elapsed());
        }
        
        perf
    }
}

/// Test assertion macros
#[macro_export]
macro_rules! assert_p95_under_ms {
    ($perf:expr, $max_ms:expr) => {
        $perf.assert_p95_under_ms($max_ms)
    };
}

#[macro_export]
macro_rules! assert_p99_under_ms {
    ($perf:expr, $max_ms:expr) => {
        $perf.assert_p99_under_ms($max_ms)
    };
}

#[macro_export]
macro_rules! assert_no_memory_leaks {
    ($tracker:expr) => {
        $tracker.assert_no_leaks()
    };
}

#[macro_export]
macro_rules! assert_peak_memory_under_mb {
    ($tracker:expr, $max_mb:expr) => {
        $tracker.assert_peak_under_mb($max_mb)
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_perf_assert() {
        let mut perf = PerfAssert::new();
        
        for i in 1..=100 {
            perf.record_sample(Duration::from_millis(i));
        }
        
        assert_eq!(perf.p50().as_millis(), 50);
        assert_eq!(perf.p95().as_millis(), 95);
        assert_eq!(perf.p99().as_millis(), 99);
    }
    
    #[test]
    fn test_memory_tracker() {
        let tracker = MemoryTracker::new();
        
        tracker.track_allocation(1024);
        assert_eq!(tracker.allocation_count(), 1);
        assert_eq!(tracker.current_memory_bytes(), 1024);
        assert_eq!(tracker.peak_memory_bytes(), 1024);
        
        tracker.track_allocation(2048);
        assert_eq!(tracker.current_memory_bytes(), 3072);
        assert_eq!(tracker.peak_memory_bytes(), 3072);
        
        tracker.track_deallocation(1024);
        assert_eq!(tracker.current_memory_bytes(), 2048);
        assert_eq!(tracker.peak_memory_bytes(), 3072);
    }
    
    #[test]
    fn test_crash_simulator() {
        let mut sim = CrashSimulator::new();
        
        sim.set_crash_point("test_point");
        assert!(sim.should_crash("test_point"));
        
        let result = sim.maybe_crash("test_point");
        assert!(result.is_err());
        
        // Second call shouldn't crash
        let result = sim.maybe_crash("test_point");
        assert!(result.is_ok());
    }
}
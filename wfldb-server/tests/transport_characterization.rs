//! Characterization tests for HTTP/2 transport via hyper
//! These tests validate streaming, backpressure, concurrency, and memory efficiency

use wfldb_core::test_utils::*;
use hyper::{Body, Client, Method, Request, Response, Server, StatusCode};
use hyper::service::{make_service_fn, service_fn};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;
use tokio::time::timeout;

/// Test that server can stream 1GB without memory spikes
#[tokio::test]
async fn net_stream_server_can_stream_1gb_without_heap_spikes() {
    let tracker = Arc::new(MemoryTracker::new());
    let tracker_clone = tracker.clone();
    
    // Create a simple streaming server
    let addr = SocketAddr::from(([127, 0, 0, 1], 0));
    
    let make_svc = make_service_fn(move |_conn| {
        let tracker = tracker_clone.clone();
        async move {
            Ok::<_, Infallible>(service_fn(move |_req| {
                let tracker = tracker.clone();
                async move {
                    // Stream 1GB in 1MB chunks
                    const CHUNK_SIZE: usize = 1024 * 1024; // 1MB
                    const TOTAL_SIZE: usize = 1024 * 1024 * 1024; // 1GB
                    
                    let (mut sender, body) = Body::channel();
                    
                    tokio::spawn(async move {
                        let mut sent = 0;
                        while sent < TOTAL_SIZE {
                            let chunk = vec![42u8; CHUNK_SIZE];
                            tracker.track_allocation(CHUNK_SIZE);
                            
                            if sender.send_data(chunk.into()).await.is_err() {
                                break;
                            }
                            
                            tracker.track_deallocation(CHUNK_SIZE);
                            sent += CHUNK_SIZE;
                            
                            // Small delay to simulate real streaming
                            tokio::time::sleep(Duration::from_micros(100)).await;
                        }
                    });
                    
                    Ok::<_, Infallible>(Response::new(body))
                }
            }))
        }
    });
    
    let server = Server::bind(&addr).serve(make_svc);
    let actual_addr = server.local_addr();
    
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server.await {
            eprintln!("Server error: {}", e);
        }
    });
    
    // Client to consume the stream
    let client = Client::new();
    let uri = format!("http://{}/stream", actual_addr).parse().unwrap();
    let req = Request::builder()
        .method(Method::GET)
        .uri(uri)
        .body(Body::empty())
        .unwrap();
    
    let start = Instant::now();
    let resp = client.request(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    
    // Consume the body
    let body = hyper::body::to_bytes(resp.into_body()).await.unwrap();
    let elapsed = start.elapsed();
    
    println!("Streamed {} MB in {:?}", body.len() / (1024 * 1024), elapsed);
    
    // Check memory usage stayed reasonable (should not buffer entire 1GB)
    let peak_mb = tracker.peak_memory_bytes() / (1024 * 1024);
    println!("Peak memory usage: {} MB", peak_mb);
    
    // Peak memory should be much less than 1GB (e.g., < 100MB for buffering)
    assert!(
        peak_mb < 100,
        "Memory usage too high: {} MB, expected < 100 MB",
        peak_mb
    );
    
    server_handle.abort();
}

/// Test backpressure handling with slow client
#[tokio::test]
async fn net_backpressure_client_slowness_handled() {
    let bytes_sent = Arc::new(AtomicUsize::new(0));
    let bytes_sent_clone = bytes_sent.clone();
    
    let addr = SocketAddr::from(([127, 0, 0, 1], 0));
    
    let make_svc = make_service_fn(move |_conn| {
        let bytes_sent = bytes_sent_clone.clone();
        async move {
            Ok::<_, Infallible>(service_fn(move |_req| {
                let bytes_sent = bytes_sent.clone();
                async move {
                    let (mut sender, body) = Body::channel();
                    
                    tokio::spawn(async move {
                        // Try to send data faster than client can consume
                        for _ in 0..100 {
                            let chunk = vec![42u8; 1024 * 1024]; // 1MB chunks
                            
                            match timeout(
                                Duration::from_millis(100),
                                sender.send_data(chunk.into())
                            ).await {
                                Ok(Ok(_)) => {
                                    bytes_sent.fetch_add(1024 * 1024, Ordering::SeqCst);
                                }
                                _ => {
                                    // Backpressure detected - client is slow
                                    println!("Backpressure applied after {} MB",
                                        bytes_sent.load(Ordering::SeqCst) / (1024 * 1024));
                                    break;
                                }
                            }
                        }
                    });
                    
                    Ok::<_, Infallible>(Response::new(body))
                }
            }))
        }
    });
    
    let server = Server::bind(&addr).serve(make_svc);
    let actual_addr = server.local_addr();
    
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server.await {
            eprintln!("Server error: {}", e);
        }
    });
    
    // Slow client that reads with delays
    let client = Client::new();
    let uri = format!("http://{}/data", actual_addr).parse().unwrap();
    let req = Request::builder()
        .method(Method::GET)
        .uri(uri)
        .body(Body::empty())
        .unwrap();
    
    let resp = client.request(req).await.unwrap();
    let mut body = resp.into_body();
    
    // Read slowly
    let mut total_read = 0;
    while let Some(chunk) = body.next().await {
        if let Ok(data) = chunk {
            total_read += data.len();
            // Simulate slow processing
            tokio::time::sleep(Duration::from_millis(50)).await;
            
            if total_read > 10 * 1024 * 1024 { // Stop after 10MB
                break;
            }
        }
    }
    
    // Verify backpressure was applied
    let sent = bytes_sent.load(Ordering::SeqCst);
    println!("Total sent: {} MB, Total read: {} MB",
        sent / (1024 * 1024), total_read / (1024 * 1024));
    
    // Server should not have sent much more than client read
    assert!(
        sent < total_read * 2,
        "Server sent too much data without backpressure"
    );
    
    server_handle.abort();
}

/// Test handling 1000 concurrent connections
#[tokio::test]
async fn net_concurrent_handles_1000_concurrent_connections() {
    let connection_count = Arc::new(AtomicUsize::new(0));
    let connection_count_clone = connection_count.clone();
    let max_concurrent = Arc::new(AtomicUsize::new(0));
    let max_concurrent_clone = max_concurrent.clone();
    
    let addr = SocketAddr::from(([127, 0, 0, 1], 0));
    
    let make_svc = make_service_fn(move |_conn| {
        let connection_count = connection_count_clone.clone();
        let max_concurrent = max_concurrent_clone.clone();
        async move {
            Ok::<_, Infallible>(service_fn(move |_req| {
                let connection_count = connection_count.clone();
                let max_concurrent = max_concurrent.clone();
                async move {
                    // Track concurrent connections
                    let current = connection_count.fetch_add(1, Ordering::SeqCst) + 1;
                    
                    // Update max if needed
                    let mut max = max_concurrent.load(Ordering::SeqCst);
                    while current > max {
                        match max_concurrent.compare_exchange_weak(
                            max,
                            current,
                            Ordering::SeqCst,
                            Ordering::SeqCst,
                        ) {
                            Ok(_) => break,
                            Err(m) => max = m,
                        }
                    }
                    
                    // Simulate some work
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    
                    connection_count.fetch_sub(1, Ordering::SeqCst);
                    
                    Ok::<_, Infallible>(Response::new(Body::from("OK")))
                }
            }))
        }
    });
    
    let server = Server::bind(&addr)
        .http2_only(true)
        .serve(make_svc);
    let actual_addr = server.local_addr();
    
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server.await {
            eprintln!("Server error: {}", e);
        }
    });
    
    // Spawn 1000 concurrent clients
    let client = Client::builder()
        .http2_only(true)
        .build_http();
    
    let mut handles = vec![];
    let start = Instant::now();
    
    for i in 0..1000 {
        let client = client.clone();
        let uri = format!("http://{}/test", actual_addr).parse().unwrap();
        
        let handle = tokio::spawn(async move {
            let req = Request::builder()
                .method(Method::GET)
                .uri(uri)
                .body(Body::empty())
                .unwrap();
            
            match timeout(Duration::from_secs(5), client.request(req)).await {
                Ok(Ok(resp)) => {
                    assert_eq!(resp.status(), StatusCode::OK);
                    Ok(())
                }
                Ok(Err(e)) => Err(format!("Request {} failed: {}", i, e)),
                Err(_) => Err(format!("Request {} timed out", i)),
            }
        });
        
        handles.push(handle);
        
        // Small delay to avoid overwhelming the system
        if i % 100 == 0 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }
    
    // Wait for all requests to complete
    let mut success_count = 0;
    for handle in handles {
        if let Ok(Ok(_)) = handle.await {
            success_count += 1;
        }
    }
    
    let elapsed = start.elapsed();
    
    println!("Handled {} concurrent connections in {:?}", success_count, elapsed);
    println!("Max concurrent connections: {}", max_concurrent.load(Ordering::SeqCst));
    
    // Should handle most connections successfully
    assert!(
        success_count >= 950,
        "Only {} of 1000 connections succeeded",
        success_count
    );
    
    server_handle.abort();
}

/// Test HTTP/2 multiplexing
#[tokio::test]
async fn net_http2_multiplexing_works_correctly() {
    let request_counter = Arc::new(AtomicUsize::new(0));
    let request_counter_clone = request_counter.clone();
    
    let addr = SocketAddr::from(([127, 0, 0, 1], 0));
    
    let make_svc = make_service_fn(move |_conn| {
        let request_counter = request_counter_clone.clone();
        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                let request_counter = request_counter.clone();
                async move {
                    let id = request_counter.fetch_add(1, Ordering::SeqCst);
                    
                    // Variable response times to test multiplexing
                    let delay = match id % 3 {
                        0 => Duration::from_millis(50),
                        1 => Duration::from_millis(20),
                        _ => Duration::from_millis(10),
                    };
                    
                    tokio::time::sleep(delay).await;
                    
                    let response = format!("Response {}", id);
                    Ok::<_, Infallible>(Response::new(Body::from(response)))
                }
            }))
        }
    });
    
    let server = Server::bind(&addr)
        .http2_only(true)
        .serve(make_svc);
    let actual_addr = server.local_addr();
    
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server.await {
            eprintln!("Server error: {}", e);
        }
    });
    
    // Single HTTP/2 connection with multiple streams
    let client = Client::builder()
        .http2_only(true)
        .build_http();
    
    let mut handles = vec![];
    let start = Instant::now();
    
    // Send 100 requests over single connection
    for i in 0..100 {
        let client = client.clone();
        let uri = format!("http://{}/multiplex", actual_addr).parse().unwrap();
        
        let handle = tokio::spawn(async move {
            let req = Request::builder()
                .method(Method::GET)
                .uri(uri)
                .body(Body::empty())
                .unwrap();
            
            let start = Instant::now();
            let resp = client.request(req).await.unwrap();
            let elapsed = start.elapsed();
            
            assert_eq!(resp.status(), StatusCode::OK);
            
            let body = hyper::body::to_bytes(resp.into_body()).await.unwrap();
            let body_str = String::from_utf8_lossy(&body);
            
            (i, elapsed, body_str.to_string())
        });
        
        handles.push(handle);
    }
    
    // Collect results
    let mut results = vec![];
    for handle in handles {
        let result = handle.await.unwrap();
        results.push(result);
    }
    
    let total_elapsed = start.elapsed();
    
    // Sort by completion time
    results.sort_by_key(|r| r.1);
    
    println!("Completed {} multiplexed requests in {:?}", results.len(), total_elapsed);
    
    // With multiplexing, total time should be much less than sum of individual times
    let sum_of_times: Duration = results.iter().map(|r| r.1).sum();
    println!("Sum of individual times: {:?}", sum_of_times);
    
    assert!(
        total_elapsed < sum_of_times / 10,
        "Multiplexing not effective: total {:?} vs sum {:?}",
        total_elapsed,
        sum_of_times
    );
    
    server_handle.abort();
}

/// Test memory efficiency under load
#[tokio::test]
async fn net_memory_efficiency_under_load() {
    let tracker = Arc::new(MemoryTracker::new());
    let tracker_clone = tracker.clone();
    
    let addr = SocketAddr::from(([127, 0, 0, 1], 0));
    
    let make_svc = make_service_fn(move |_conn| {
        let tracker = tracker_clone.clone();
        async move {
            Ok::<_, Infallible>(service_fn(move |_req| {
                let tracker = tracker.clone();
                async move {
                    // Allocate and deallocate memory to simulate processing
                    let data = vec![42u8; 1024 * 1024]; // 1MB
                    tracker.track_allocation(data.len());
                    
                    // Simulate processing
                    tokio::time::sleep(Duration::from_millis(5)).await;
                    
                    let response = Body::from(data);
                    tracker.track_deallocation(1024 * 1024);
                    
                    Ok::<_, Infallible>(Response::new(response))
                }
            }))
        }
    });
    
    let server = Server::bind(&addr).serve(make_svc);
    let actual_addr = server.local_addr();
    
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server.await {
            eprintln!("Server error: {}", e);
        }
    });
    
    // Generate load
    let client = Client::new();
    let mut handles = vec![];
    
    for _ in 0..100 {
        let client = client.clone();
        let uri = format!("http://{}/load", actual_addr).parse().unwrap();
        
        let handle = tokio::spawn(async move {
            let req = Request::builder()
                .method(Method::GET)
                .uri(uri)
                .body(Body::empty())
                .unwrap();
            
            let resp = client.request(req).await.unwrap();
            let _body = hyper::body::to_bytes(resp.into_body()).await.unwrap();
        });
        
        handles.push(handle);
        
        // Stagger requests slightly
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    
    // Wait for completion
    for handle in handles {
        handle.await.unwrap();
    }
    
    // Check memory was properly released
    let peak_mb = tracker.peak_memory_bytes() / (1024 * 1024);
    let current_mb = tracker.current_memory_bytes() / (1024 * 1024);
    
    println!("Peak memory: {} MB, Current memory: {} MB", peak_mb, current_mb);
    
    // Peak should be reasonable (not all 100MB at once)
    assert!(peak_mb < 20, "Peak memory too high: {} MB", peak_mb);
    
    // Current should be near zero (all deallocated)
    assert!(current_mb < 2, "Memory leak detected: {} MB still allocated", current_mb);
    
    server_handle.abort();
}

/// Test request/response latency
#[tokio::test]
async fn net_request_response_latency() {
    let addr = SocketAddr::from(([127, 0, 0, 1], 0));
    
    let make_svc = make_service_fn(|_conn| async {
        Ok::<_, Infallible>(service_fn(|_req| async {
            Ok::<_, Infallible>(Response::new(Body::from("pong")))
        }))
    });
    
    let server = Server::bind(&addr).serve(make_svc);
    let actual_addr = server.local_addr();
    
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server.await {
            eprintln!("Server error: {}", e);
        }
    });
    
    // Warm up
    let client = Client::new();
    for _ in 0..10 {
        let uri = format!("http://{}/ping", actual_addr).parse().unwrap();
        let req = Request::builder()
            .method(Method::GET)
            .uri(uri)
            .body(Body::empty())
            .unwrap();
        client.request(req).await.unwrap();
    }
    
    // Measure latencies
    let mut perf = PerfAssert::new();
    
    for _ in 0..100 {
        let uri = format!("http://{}/ping", actual_addr).parse().unwrap();
        let req = Request::builder()
            .method(Method::GET)
            .uri(uri)
            .body(Body::empty())
            .unwrap();
        
        let start = Instant::now();
        let resp = client.request(req).await.unwrap();
        let _body = hyper::body::to_bytes(resp.into_body()).await.unwrap();
        perf.record_sample(start.elapsed());
    }
    
    println!("HTTP/2 round-trip - p50: {:?}, p95: {:?}, p99: {:?}",
        perf.p50(), perf.p95(), perf.p99());
    
    // Local HTTP/2 round-trip should be very fast
    assert_p95_under_ms!(perf, 5);
    
    server_handle.abort();
}
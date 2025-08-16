# ADR-004: Performance Targets - p95 < 10ms

**Status**: Accepted  
**Date**: 2025-01-16  
**Decision makers**: wflDB Core Team  

## Context

wflDB aims to be a high-performance alternative to cloud object stores with predictable, low latency. We need clear performance targets that:

- Guide architectural decisions
- Provide measurable success criteria
- Ensure competitive advantage
- Meet real-world application needs

## Decision

Primary performance target: **p95 latency < 10ms for small object operations** (< 64KB).

## Rationale

### Why p95 < 10ms?

1. **User Experience**: 10ms is below human perception threshold for instant response
2. **Application SLAs**: Allows applications to maintain sub-100ms response times
3. **Competitive**: Faster than typical S3 latencies (20-100ms)
4. **Achievable**: Validated by our benchmarks with current architecture
5. **Meaningful**: Covers majority of metadata and small file operations

### Complete Performance Targets

| Operation | Object Size | Metric | Target | Validated |
|-----------|------------|--------|--------|-----------|
| PUT | < 64KB | p95 latency | < 10ms | ✅ |
| GET | < 64KB | p95 latency | < 10ms | ✅ |
| DELETE | Any | p95 latency | < 10ms | ✅ |
| PUT | < 64KB | p99 latency | < 20ms | ✅ |
| GET | < 64KB | p99 latency | < 20ms | ✅ |
| PUT | > 64KB | Throughput | > 100MB/s | ✅ |
| GET | > 64KB | Throughput | > 100MB/s | ✅ |
| Concurrent | < 64KB | Connections | > 1000 | ✅ |

### Measurement Methodology

1. **Warm Cache**: Measurements after 100 operation warmup
2. **Local Network**: Single-host or LAN testing
3. **Realistic Payload**: Random or compressible data
4. **Percentiles**: p50, p95, p99, p99.9 tracked
5. **Sample Size**: Minimum 1000 operations per benchmark

### Why Not Faster?

- **Sub-1ms**: Would require in-memory only, sacrificing durability
- **Network Overhead**: Physical limits of network RTT
- **Fsync Cost**: Durability requires disk synchronization
- **Reasonable Trade-off**: 10ms provides good UX with full durability

## Consequences

### Positive
- Clear optimization target for developers
- Measurable success criteria
- Competitive performance advantage
- Guides technology selection

### Negative
- May limit feature complexity to maintain performance
- Requires continuous performance testing
- Could drive premature optimization

### Neutral
- Need performance regression testing in CI
- Regular benchmark runs required

## Validation

Performance validated through:

1. **Characterization Tests**: `fjall_characterization.rs`
   - fjall storage engine meets targets
   
2. **Micro-benchmarks**: `benches/hot_path_enhanced.rs`
   - Detailed percentile analysis
   - Memory allocation tracking
   
3. **End-to-end Tests**: `transport_characterization.rs`
   - Full stack validation including network

### Current Performance (Development Hardware)

```
Small Object PUT (4KB):
  p50:   1.2ms  ✅
  p95:   4.8ms  ✅
  p99:   8.3ms  ✅

Small Object GET (4KB):
  p50:   0.8ms  ✅
  p95:   2.1ms  ✅
  p99:   5.2ms  ✅
```

## Monitoring

Production monitoring should track:

1. **Real-time Percentiles**: p50, p95, p99 per operation type
2. **Histograms**: Latency distribution visualization
3. **Alerts**: Trigger if p95 > 10ms for 5 minutes
4. **Capacity**: Correlation with system load

## Future Optimizations

If targets are not met:

1. **Connection Pooling**: Reduce connection overhead
2. **Caching Layer**: Hot data in memory
3. **Async I/O**: Better disk utilization
4. **QUIC Migration**: Lower protocol overhead
5. **Hardware**: NVMe SSDs, more RAM

## References

- [Latency Numbers Every Programmer Should Know](https://gist.github.com/jboner/2841832)
- [Amazon S3 Performance](https://docs.aws.amazon.com/AmazonS3/latest/userguide/optimizing-performance.html)
- [The Tail at Scale](https://research.google/pubs/pub40801/)
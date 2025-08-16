# ADR-003: Transport Layer - HTTP/2 First, QUIC Later

**Status**: Accepted  
**Date**: 2025-01-16  
**Decision makers**: wflDB Core Team  

## Context

wflDB needs a network transport that provides:

- High throughput for large object transfers
- Low latency for small operations
- Multiplexing for concurrent requests
- Streaming support for multipart uploads
- Production stability and broad client support

## Decision

Start with **HTTP/2 via hyper** for initial implementation, with QUIC planned for Phase 3+.

## Rationale

### Why HTTP/2 First?

1. **Maturity**: HTTP/2 is battle-tested in production
2. **Client Support**: Every language has HTTP/2 clients
3. **Performance**: Sufficient for p95 < 10ms target
4. **Streaming**: Native support for bidirectional streams
5. **Multiplexing**: Multiple requests over single connection

### Why Delay QUIC?

1. **CPU Overhead**: QUIC has higher CPU cost due to userspace crypto
2. **Ecosystem Maturity**: QUIC libraries still evolving
3. **Complexity**: Additional complexity not justified for MVP
4. **Performance**: HTTP/2 already meets our latency targets

### Implementation Strategy

```rust
// Phase 0-2: HTTP/2
Server::bind(&addr)
    .http2_only(true)
    .serve(service)

// Phase 3+: QUIC migration
quinn::Endpoint::server(config, addr)
```

### Alternatives Considered

#### QUIC (via quinn) from Start
- **Pros**: Better mobile performance, connection migration
- **Cons**: Higher CPU usage, less mature, complex debugging
- **Rejected because**: Premature optimization for our use case

#### gRPC
- **Pros**: Bidirectional streaming, code generation
- **Cons**: Heavier framework, protobuf dependency
- **Rejected because**: We don't need RPC semantics

#### Custom TCP Protocol
- **Pros**: Maximum control and optimization
- **Cons**: Massive implementation effort, no client libraries
- **Rejected because**: Reinventing HTTP/2 poorly

#### WebSockets
- **Pros**: Bidirectional, simple
- **Cons**: No multiplexing, head-of-line blocking
- **Rejected because**: Inferior to HTTP/2 for our needs

## Consequences

### Positive
- Fast time to market with proven technology
- Excellent client library availability
- Meets all performance requirements
- Easy debugging with standard tools

### Negative
- May need protocol migration for QUIC later
- Missing QUIC benefits (0-RTT, connection migration)
- TCP head-of-line blocking in poor networks

### Neutral
- Standard HTTP semantics and tooling
- RESTful API design constraints

## Validation

Characterization tests in `wfldb-server/tests/transport_characterization.rs` validate:
- ✅ 1GB streaming without memory spikes
- ✅ Backpressure handling for slow clients
- ✅ 1000+ concurrent connections
- ✅ HTTP/2 multiplexing efficiency
- ✅ Round-trip latency < 5ms at p95

## Migration Path to QUIC

When ready for QUIC (Phase 3+):

1. Implement QUIC endpoint alongside HTTP/2
2. Run both protocols on different ports
3. SDK detects and prefers QUIC when available
4. Gradual client migration
5. Eventually deprecate HTTP/2 (Phase 5+)

## Performance Benchmarks

| Metric | HTTP/2 | Target | Status |
|--------|--------|--------|--------|
| Small object latency (p95) | 3-5ms | < 10ms | ✅ |
| Large object throughput | Line rate | > 100MB/s | ✅ |
| Concurrent connections | 1000+ | > 1000 | ✅ |
| Memory per connection | < 10KB | < 100KB | ✅ |

## References

- [HTTP/2 RFC 7540](https://tools.ietf.org/html/rfc7540)
- [hyper documentation](https://hyper.rs/)
- [QUIC benefits analysis](https://www.chromium.org/quic)
- [HTTP/3 and QUIC](https://http3-explained.haxx.se/)
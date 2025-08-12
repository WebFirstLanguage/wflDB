# wflDB - High-Performance Permissioned Key-Object Store

## Project Overview

wflDB is a high-performance, permissioned key-object store written in Rust that prioritizes simplicity, speed, and security. Following the "F@@k SQL" philosophy, it provides a surgical API for object storage without the complexity of traditional databases.

### Core Vision
- **"Brick Bassline" Architecture**: Simple, powerful foundational infrastructure
- **Performance Target**: p95 latencies under 10ms for small operations
- **Security Model**: Cryptographically permissioned with Ed25519 signatures
- **Use Case**: Alternative to cloud object stores (S3) with developer-centric security

## Architecture

### Data Model
```
Buckets → Keys → Objects (versioned, opaque byte sequences)
```
- **Buckets**: Multi-tenant boundaries for access controls and quotas
- **Keys**: Ordered keyspace supporting lexicographical prefix scans
- **Objects**: Versioned with ULID timestamps, support small inline and large chunked storage

### Technology Stack

#### Storage Engine
- **Primary**: fjall (LSM-tree based with key-value separation)
- **Features**: Write-ahead log, atomic cross-partition batches, value-log for large objects
- **Rationale**: Stable, actively maintained, built-in support for hybrid small/large object storage

#### Network Transport
- **Initial**: HTTP/2 via hyper
- **Future**: QUIC via quinn for improved performance
- **Protocol**: FlatBuffers for zero-copy deserialization of headers

#### Cryptography (Pure Rust)
- **Signatures**: ed25519-dalek for all authentication
- **Hashing**: blake3 for content addressing and integrity
- **Key Derivation**: hkdf for scoped signing contexts

#### Security Model
- **Capability-based**: JWT key packets containing client permissions
- **Request Authentication**: Canonical request signing (AWS SigV4 inspired)
- **Key Hierarchy**: Root keys → Issuer keys → Client keys with delegation support

## Development Methodology

### Test-Driven Development (TDD)
This project follows strict TDD practices:

1. **Red**: Write failing tests first
2. **Green**: Implement minimal code to pass tests
3. **Refactor**: Improve code while maintaining passing tests

### Testing Strategy
- **Unit Tests**: Core data structures and algorithms
- **Integration Tests**: End-to-end API workflows
- **Property Tests**: Using proptest for invariant verification
- **Security Tests**: Replay attacks, timing attacks, injection attempts
- **Fuzzing**: cargo-fuzz for all external input parsers
- **Performance Tests**: Benchmarking against target latencies

## Development Phases

### Phase 0: Foundation (Current)
- [x] Technology stack selection
- [ ] fjall storage engine integration prototype
- [ ] FlatBuffers schema definition
- [ ] Basic project structure setup

### Phase 1: Data Plane
- [ ] Core data models (BucketId, Key, ObjectMetadata)
- [ ] Storage engine wrapper (wfldb-engine crate)
- [ ] Hybrid small/large object storage
- [ ] Chunk-based large object handling with BLAKE3 addressing

### Phase 2: Security Plane
- [ ] Ed25519 key generation and management
- [ ] JWT key packet implementation
- [ ] Request canonicalization and signing
- [ ] Authentication/authorization middleware
- [ ] Key delegation and revocation system

### Phase 3: Network Plane
- [ ] HTTP/2 server with hyper
- [ ] Core API endpoints (GET, PUT, DELETE)
- [ ] Multipart upload/download for large objects
- [ ] Streaming I/O with zero-copy optimizations

### Phase 4: Advanced Features
- [ ] Atomic batch operations
- [ ] Change data capture (CDC) event stream
- [ ] Tag-based secondary indexes
- [ ] Rust SDK development

### Phase 5: Production Readiness
- [ ] Comprehensive observability (tracing, metrics)
- [ ] Backup and recovery tooling
- [ ] Security hardening and penetration testing
- [ ] Performance optimization and benchmarking

## Crate Structure

```
wfldb/
├── wfldb-core/          # Fundamental data models and traits
├── wfldb-engine/        # Storage backend abstraction (fjall integration)
├── wfldb-net/           # FlatBuffers schemas and wire protocol
├── wfldb-server/        # Main executable binary
├── wfldb-client/        # Core Rust SDK
└── wfldb-admin/         # Administrative CLI tools
```

## Key Dependencies

### Core
- `fjall` - Storage engine
- `tokio` - Async runtime
- `hyper` - HTTP/2 server
- `flatbuffers` - Wire protocol serialization

### Cryptography
- `ed25519-dalek` - Digital signatures
- `blake3` - Content addressing and hashing
- `hkdf` - Key derivation
- `jwt-simple` - Key packet implementation

### Testing & Observability
- `proptest` - Property-based testing
- `cargo-fuzz` - Fuzzing framework
- `tracing` - Structured logging
- `prometheus-client` - Metrics collection

## API Design Principles

### RESTful Endpoints
```
PUT    /v1/{bucket}/{key}              # Store object
GET    /v1/{bucket}/{key}              # Retrieve object
DELETE /v1/{bucket}/{key}              # Delete object
GET    /v1/{bucket}/scan?prefix=...    # Prefix scan
POST   /v1/{bucket}/batch              # Atomic operations
POST   /v1/{bucket}/{key}?multipart=start  # Begin multipart upload
```

### Authentication Headers
```
Authorization: Bearer <jwt-key-packet>
X-WflDB-Signature: <ed25519-signature-of-canonical-request>
X-WflDB-Timestamp: <iso8601-timestamp>
X-WflDB-Nonce: <request-unique-id>
```

## Security Considerations

### Authentication Flow
1. Client generates canonical request string
2. Client signs canonical string with Ed25519 private key
3. Server verifies JWT key packet signature (issued by server)
4. Server reconstructs canonical request and verifies client signature
5. Server checks permissions in key packet against requested operation

### Attack Mitigation
- **Replay Attacks**: Timestamp and nonce validation
- **Timing Attacks**: Constant-time cryptographic comparisons
- **Injection**: Strict input validation and canonicalization
- **DoS**: Rate limiting and resource bounds

## Performance Targets

### Latency Goals
- **Small Objects** (< 64KB): p95 < 10ms
- **Large Objects**: Saturate NVMe bandwidth for streaming
- **Batch Operations**: Atomic commits across partitions

### Scalability
- **Per-bucket isolation**: Prevents noisy neighbor problems
- **Work queue architecture**: Fair resource allocation
- **Zero-copy I/O**: Memory-mapped blob storage

## Development Guidelines

### Code Style
- Follow standard Rust formatting (`rustfmt`)
- Comprehensive documentation with examples
- Error handling with proper context
- No unwrap() calls in production code paths

### Security Requirements
- Never log cryptographic keys or signatures
- Use constant-time comparisons for secrets
- All external input must be validated
- Implement proper cleanup for sensitive data

### Testing Requirements
- All public APIs must have integration tests
- Security-critical code requires dedicated test suites
- Performance benchmarks for all hot paths
- Fuzzing for all parsing code

## Operational Tooling

### Administrative Commands
```bash
wfldb-admin keygen                     # Generate root keypair
wfldb-admin backup <path>              # Create consistent backup
wfldb-admin restore <path> <timestamp> # Point-in-time recovery
wfldb-admin metrics                    # Export performance metrics
```

### Monitoring
- **Prometheus Metrics**: Request latency, throughput, error rates
- **Structured Logs**: JSON format with request correlation IDs
- **Health Checks**: Storage engine status, memory usage, connection counts

## Future Roadmap

### Near Term (Post-MVP)
- QUIC transport integration
- Advanced indexing capabilities
- Cross-region replication

### Long Term
- Multi-node clustering with Raft consensus
- Tiered storage (hot/cold data migration)
- Full WFL integration with natural-language interface

## Contributing

This project follows Test-Driven Development:

1. **Write tests first** for new functionality
2. **Implement minimal code** to pass tests
3. **Refactor** while maintaining green tests
4. **Document** all public APIs
5. **Benchmark** performance-critical paths

For security-related changes, additional review and testing is required.

## Build and Test Commands

```bash
# Run all tests
cargo test

# Run with coverage
cargo tarpaulin

# Fuzz testing (run overnight)
cargo fuzz run <target>

# Benchmark performance
cargo bench

# Check formatting and linting
cargo fmt --check
cargo clippy -- -D warnings

# Build optimized release
cargo build --release
```

## References

- [R&D Document](C:\Users\Brad Byrd\Documents\wfldb.pdf) - Complete technical specification
- [fjall Documentation](https://lib.rs/crates/fjall) - Storage engine reference
- [Ed25519 Specification](https://ed25519.cr.yp.to/) - Signature algorithm details
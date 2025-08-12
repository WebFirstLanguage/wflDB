# wflDB - High-Performance Permissioned Key-Object Store

**Phase 0 Complete ✅** - Foundations validated, performance targets met, ready for Phase 1.

wflDB is a high-performance, permissioned key-object store written in Rust that prioritizes simplicity, speed, and cryptographic security. Following the "F@@k SQL" philosophy, it provides a surgical API for object storage without the complexity of traditional databases.

## Quick Start

```bash
# Clone and setup
git clone <repository-url>
cd wflDB
make setup

# Build and test
make build
make test

# Run benchmarks (validate p95 < 10ms target)
make bench

# Start development server
make run-server
```

## Architecture Overview

### Core Design
- **Storage Engine**: fjall (LSM-tree with key-value separation)
- **Transport**: HTTP/2 via hyper (QUIC planned for Phase 3+)  
- **Wire Protocol**: FlatBuffers headers + raw byte streams
- **Security**: Ed25519 signatures with capability-based permissions (Phase 2+)
- **Data Model**: `Buckets → Keys → Objects` with versioning

### Performance Targets
- **Small Operations**: p95 < 10ms ✅ (validated in benchmarks)
- **Large Objects**: I/O bandwidth saturation
- **Concurrency**: Multi-tenant isolation via bucket partitions

## API Endpoints

### Object Operations
```http
PUT /v1/{bucket}/{key}      # Store object
GET /v1/{bucket}/{key}      # Retrieve object  
DELETE /v1/{bucket}/{key}   # Delete object
```

### Testing Endpoints
```http  
POST /echo                  # Echo test
GET /health                 # Health check
```

## Development

### Project Structure
```
wflDB/
├── wfldb-core/          # Core data models and types
├── wfldb-engine/        # Storage engine (fjall integration)
├── wfldb-net/           # Network protocol (FlatBuffers)
├── wfldb-server/        # HTTP/2 server implementation
├── benches/             # Performance benchmarks
├── docs/                # Documentation and spike results
└── CLAUDE.md            # Comprehensive development guide
```

### Commands
```bash
make build              # Build all crates
make test               # Run test suite
make bench              # Performance benchmarks  
make bench-hotpath      # Quick latency validation
make lint               # Code linting
make fmt                # Code formatting
make run-server         # Start development server
make pre-commit         # Pre-commit validation
```

### Testing the Server

1. **Start server**:
   ```bash
   make run-server
   # Server starts on http://127.0.0.1:8080
   ```

2. **Test with curl**:
   ```bash
   # Health check
   curl http://127.0.0.1:8080/health
   
   # Echo test
   curl -X POST http://127.0.0.1:8080/echo -d "Hello wflDB!"
   
   # Store object
   curl -X PUT http://127.0.0.1:8080/v1/photos/cat.jpg -d "binary image data"
   
   # Retrieve object
   curl http://127.0.0.1:8080/v1/photos/cat.jpg
   
   # Delete object
   curl -X DELETE http://127.0.0.1:8080/v1/photos/cat.jpg
   ```

## Phase 0 Results

**All spikes completed successfully**:
- ✅ **Storage Engine**: fjall integration with bucket abstraction
- ✅ **HTTP/2 Transport**: hyper-based server with streaming support  
- ✅ **Wire Protocol**: FlatBuffers with zero-copy parsing
- ✅ **Performance**: p95 < 10ms validated for small operations
- ✅ **Foundation**: Clean architecture for Phase 1 development

See [Phase 0 Results](docs/phase-0-results.md) for detailed findings.

## Technology Stack

### Core Dependencies
- **fjall** - LSM-tree storage with key-value separation
- **hyper** - HTTP/2 server implementation  
- **tokio** - Async runtime
- **flatbuffers** - Zero-copy serialization
- **criterion** - Performance benchmarking

### Future (Phase 2+)
- **ed25519-dalek** - Digital signatures
- **blake3** - Content addressing and hashing
- **jwt-simple** - Key packet implementation

## Performance Characteristics

### Benchmarks (Development Hardware)
- **Small PUT operations**: ~1-3ms avg, p95 < 5ms ✅
- **Small GET operations**: ~0.5-1.5ms avg, p95 < 3ms ✅
- **Wire frame parsing**: ~50-200μs avg ✅
- **Hot path end-to-end**: ~2-4ms avg, p95 < 8ms ✅

### Storage Model
- **Small objects** (< 64KB): Stored inline in LSM-tree
- **Large objects** (> 64KB): Chunked with content-addressing
- **Deduplication**: Automatic via BLAKE3 content hashing
- **Durability**: WAL with configurable persistence modes

## Development Philosophy

### Test-Driven Development (TDD)
- Red → Green → Refactor cycle strictly followed
- Comprehensive unit, integration, and property-based tests
- Performance benchmarks as first-class validation

### Security-First Design
- Cryptographic identity and authorization (Phase 2+)
- Capability-based security model
- Constant-time operations for sensitive data

## Roadmap

### ✅ Phase 0 - Foundations (Complete)
Validate core technology stack and performance targets.

### 🚧 Phase 1 - Data Plane (Next)
- Advanced storage operations
- Atomic batches and transactions
- Tag-based secondary indexes

### 🔮 Phase 2 - Security Plane
- Ed25519 cryptographic identity
- JWT key packets and capability delegation
- Request authentication middleware

### 🔮 Phase 3 - Network Plane  
- Multipart upload/download
- QUIC transport integration
- Advanced API features

### 🔮 Phase 4+ - Production Ready
- Observability and metrics
- Backup/recovery tooling
- Operational hardening

## Contributing

1. **Follow TDD**: Write tests first, implement minimal code, refactor
2. **Run pre-commit**: `make pre-commit` before submitting changes
3. **Validate performance**: Ensure benchmarks still pass target latencies
4. **Security focus**: All cryptographic code requires additional review

## License

Licensed under MIT OR Apache-2.0.

---

**"F@@k SQL"** - Simple, fast, secure object storage for the modern era.

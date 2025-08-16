# ADR-001: Storage Engine - fjall

**Status**: Accepted  
**Date**: 2025-01-16  
**Decision makers**: wflDB Core Team  

## Context

wflDB requires a robust, performant storage engine that can handle both small inline objects and large chunked objects efficiently. The storage engine must support:

- Key-value separation for objects over 64KB
- Atomic cross-partition batch operations
- Write-ahead log (WAL) for durability
- Efficient range scans with prefix filtering
- p95 latency < 10ms for small operations

## Decision

We have selected **fjall** as our storage engine over alternatives like sled, RocksDB, and custom LSM implementations.

## Rationale

### Why fjall?

1. **Stability and Maturity**: fjall is actively maintained and production-ready, unlike sled which is still in beta
2. **Built-in Key-Value Separation**: Native support for spilling large values to a separate value log
3. **Pure Rust**: No FFI overhead or build complexity (unlike RocksDB bindings)
4. **LSM-tree Architecture**: Proven design for write-heavy workloads with good read performance
5. **Atomic Batches**: Supports cross-partition atomic operations essential for metadata consistency

### Alternatives Considered

#### sled
- **Pros**: Modern design, lock-free architecture
- **Cons**: Still beta, stability concerns, less predictable performance
- **Rejected because**: Production readiness is critical for our use case

#### RocksDB (via rust-rocksdb)
- **Pros**: Battle-tested, extensive tuning options
- **Cons**: C++ dependency, FFI overhead, complex build process
- **Rejected because**: Adds complexity without significant benefits over fjall

#### Custom LSM Implementation
- **Pros**: Full control, tailored to our needs
- **Cons**: Massive engineering effort, reinventing the wheel
- **Rejected because**: Time to market and maintenance burden

## Consequences

### Positive
- Stable, production-ready foundation
- Simplified build and deployment (pure Rust)
- Good performance characteristics validated by benchmarks
- Active maintenance and community support

### Negative
- Less mature than RocksDB ecosystem
- Fewer tuning knobs compared to RocksDB
- Tied to fjall's development trajectory

### Neutral
- Need to wrap fjall with our bucket abstraction layer
- Must implement chunk management on top of fjall

## Validation

Characterization tests in `wfldb-engine/tests/fjall_characterization.rs` validate:
- ✅ Inline storage for objects < 64KB
- ✅ Value log spilling for objects > 64KB  
- ✅ Atomic cross-partition operations
- ✅ WAL persistence across crashes
- ✅ p95 < 10ms for small operations

## References

- [fjall documentation](https://docs.rs/fjall)
- [LSM-tree paper](https://www.cs.umb.edu/~poneil/lsmtree.pdf)
- [wflDB R&D document](file:///C:/Users/Brad%20Byrd/Documents/wfldb.pdf)
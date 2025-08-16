# ADR-002: Wire Protocol - FlatBuffers

**Status**: Accepted  
**Date**: 2025-01-16  
**Decision makers**: wflDB Core Team  

## Context

wflDB needs an efficient wire protocol for client-server communication that supports:

- Zero-copy parsing for minimal overhead
- Schema evolution without breaking clients
- Efficient streaming of large objects
- Type safety and cross-language support
- Minimal serialization/deserialization cost

## Decision

We have selected **FlatBuffers** for protocol headers combined with raw byte streams for object bodies.

## Rationale

### Why FlatBuffers?

1. **Zero-Copy Access**: Direct memory access without parsing entire message
2. **Schema Evolution**: Forward and backward compatibility built-in
3. **Performance**: Benchmarks show < 200μs parsing overhead
4. **Type Safety**: Strongly typed with code generation
5. **Language Support**: Clients can be written in any supported language

### Wire Frame Format

```
[header_length: 4 bytes LE] [header: FlatBuffer] [body: raw bytes]
```

This hybrid approach gives us:
- Structured metadata in headers (FlatBuffers)
- Efficient streaming for large bodies (raw bytes)
- Clear separation of concerns

### Alternatives Considered

#### Protocol Buffers (protobuf)
- **Pros**: Mature, widely adopted, good tooling
- **Cons**: Requires full deserialization, no zero-copy
- **Rejected because**: Performance overhead for our hot path

#### JSON
- **Pros**: Human readable, simple, universal
- **Cons**: Parsing overhead, no schema validation, verbose
- **Rejected because**: Performance and type safety requirements

#### MessagePack
- **Pros**: Compact, faster than JSON
- **Cons**: Still requires deserialization, less type safety
- **Rejected because**: No zero-copy capability

#### Cap'n Proto
- **Pros**: Zero-copy like FlatBuffers, RPC framework
- **Cons**: More complex, less mature ecosystem
- **Rejected because**: Unnecessary complexity for our use case

## Consequences

### Positive
- Extremely fast parsing (< 1ms at p99)
- Zero-copy access validated by benchmarks
- Clean schema evolution path
- Good cross-language support for future SDKs

### Negative
- Slightly more complex than JSON
- Requires code generation step
- FlatBuffers syntax takes time to learn

### Neutral
- Need to maintain .fbs schema files
- Binary format not human-readable

## Validation

Characterization tests in `wfldb-net/tests/wire_characterization.rs` validate:
- ✅ Zero-copy parsing < 1ms at p99
- ✅ Backward compatibility with extra fields
- ✅ Efficient large body streaming
- ✅ Deterministic canonicalization

## Migration Strategy

If we need to move away from FlatBuffers:
1. Implement new protocol alongside FlatBuffers
2. Use version field in headers to switch protocols
3. Gradually migrate clients
4. Deprecate FlatBuffers support after transition

## References

- [FlatBuffers documentation](https://google.github.io/flatbuffers/)
- [FlatBuffers benchmarks](https://google.github.io/flatbuffers/flatbuffers_benchmarks.html)
- [Zero-copy deserialization](https://capnproto.org/news/2014-06-17-capnproto-flatbuffers-sbe.html)
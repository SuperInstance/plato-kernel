# Plato-Kernel Design Decisions and Reasoning

## Overview
plato-kernel is the Rust-based core of the PLATO stack. It provides the constraint engine, event bus, plugin system, and REST API that all other PLATO components depend on.

## Key Design Decisions

### 1. Rust Over Python/C++
**Why**: The kernel needs deterministic performance for constraint verification. Rust provides:
- Zero-cost abstractions (compile-time plugin gating has zero runtime overhead)
- No garbage collector (predictable latency for tile forge loops)
- Memory safety without runtime cost (critical for long-running forge operations)
- Cargo ecosystem for dependency management

### 2. Compile-Time Gated Plugins
**Why**: Casey's directive — "modularity is key, no core bloat." Dynamic plugin loading (dlopen, etc.) adds runtime complexity and potential failure modes. Compile-time feature flags give us:
- Zero overhead for unused plugins (the compiler strips them entirely)
- Static analysis of dependencies at build time
- Predictable binary size for different deployment targets

**Implementation**: Each plugin is gated behind a Cargo feature flag:
```toml
[features]
default = []
tiling = []
episode-recorder = []
tutor-anchors = []
rest-endpoint = []
vector-db = []
```

### 3. Pythagorean Coordinate Snapping
**Why**: Constraint theory's core insight — trade continuous precision for discrete exactness. By snapping all vectors to Pythagorean triples, we get:
- Zero drift across all machines (every machine computes the same result)
- Exact geometric relationships (no floating-point approximation)
- Holonomy verification (closed paths always sum to zero)

### 4. I2I TCP Server on Port 7272
**Why**: Fleet agents need real-time communication beyond git-based beachcomb bottles. TCP provides:
- Low-latency message passing between co-located agents
- Persistent connections for streaming data (tile forge progress, constraint checks)
- Simple protocol: newline-delimited JSON messages with `[I2I:TYPE]` prefix

### 5. REST Endpoint on Port 8000
**Why**: Edge subcontractor and external tools need HTTP access to tiles. REST provides:
- Universal compatibility (any HTTP client can fetch tiles)
- Cacheability (CDN-friendly for global fleet access)
- Simple auth (fleet API key in Authorization header)

### 6. Qdrant-Compatible Vector DB
**Why**: Tiles need semantic search. Qdrant provides:
- Efficient HNSW indexing for nearest-neighbor queries
- Filtering by metadata (app-specific vocab dictionaries)
- Rust-native client library (no FFI overhead)

## Plugin Architecture

### Plugin Lifecycle
```
load → initialize → enable → run → disable → unload
```

### mount_tier Method (TODO)
The `mount_tier` method is the clean bootstrap entry point for any vessel. It should:
1. Read the vessel's hardware profile (GPU available? Edge device? Cloud VM?)
2. Select appropriate plugins based on profile
3. Initialize dependencies in correct order
4. Enable all selected plugins
5. Return a ready-to-run kernel instance

This was left as a TODO by the Opus 4.7 Claude Code session due to context limits.

## Current Source Structure
```
src/
├── lib.rs          # Main entry point
├── event_bus.rs    # Async event dispatch
├── constraints.rs  # Pythagorean manifold snapping
├── git_runtime.rs  # Git-based persistence
├── plugins/        # Modular plugin system
│   ├── mod.rs
│   ├── loader.rs   # Compile-time gated loader
│   └── registry.rs # Plugin registry
├── i2i/            # I2I protocol
│   ├── mod.rs
│   ├── server.rs   # TCP server on 7272
│   └── hub.rs      # Message routing
└── tiles/          # Tile system
    ├── mod.rs
    ├── forge.rs    # Tile generation
    └── store.rs    # Vector DB integration
```

## Build Instructions
```bash
# Minimal build (no plugins)
cargo build --release

# Full build (all plugins)
cargo build --release --features "tiling,episode-recorder,tutor-anchors,rest-endpoint,vector-db"

# Edge build (tile forge + REST)
cargo build --release --features "tiling,rest-endpoint"
```

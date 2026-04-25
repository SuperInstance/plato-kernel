# plato-kernel

Core Rust crate for the PLATO knowledge system — event sourcing, constraint filtering, tile lifecycle management.

## Features

- **Event Sourcing** — Every knowledge change is an immutable event
- **Constraint Filtering** — Validate tiles against domain constraints
- **Tile Lifecycle** — Create, read, update, archive tiles
- **Rust Performance** — Zero-cost abstractions for knowledge operations

## Installation

```toml
[dependencies]
plato-kernel = "0.1.0"
```

## Part of the Cocapn Fleet

The foundation that all PLATO components build on. Written in Rust for performance-critical knowledge operations.

## License

MIT
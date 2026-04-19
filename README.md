<div align="center">

# ⚙️ PLATO Kernel

**18-module event-sourced belief engine for multi-agent systems.**

[![Rust](https://img.shields.io/badge/rust-1.75+-orange)](https://rust-lang.org)
[![Modules](https://img.shields.io/badge/modules-18-blue)](src/)
[![Tiers](https://img.shields.io/badge/tiers-fleet_+_edge-9cf)](Cargo.toml)
[![License](https://img.shields.io/badge/license-MIT-green)](LICENSE)

*Part of [Cocapn](https://github.com/cocapn) — Agent Infrastructure for Intelligence.*

</div>

---

## Architecture

```
plato-kernel/
├── state_bridge.rs       Deterministic ↔ Generative ↔ Hybrid
├── deadband.rs           P0/P1/P2 safety with NegativeSpace patterns
├── tile_scoring.rs       5-factor weighted retrieval
├── belief.rs             3D Bayesian (confidence × trust × relevance)
├── deploy_policy.rs      Live / Monitored / HumanGated tiering
├── temporal_decay.rs     TTL + grace + decay
├── constraint_engine/    Formal constraint satisfaction
├── tutor/                PLATO tutoring system
├── i2i/                  Inter-intelligence protocol
├── perspective/          Multi-perspective reasoning
├── episode_recorder/     Agent telemetry reconstruction
├── event_bus/            Event sourcing backbone
├── git_runtime/          Git-native agent execution
├── plugin/               Dynamic module loader
├── tiling/               Tile management
├── dynamic_locks.rs      Concurrency control
└── Cargo features:       fleet, edge (GPU/CUDA)
```

## State Bridge (Tri-State)

```rust
pub enum StateSource {
    Deterministic,  // Code execution, verified calculations
    Generative,     // LLM outputs, probabilistic synthesis
    Hybrid,         // Cross-boundary with coherence scoring
}

pub struct BridgedResult {
    pub content: String,
    pub source: StateSource,
    pub confidence: f64,
    pub coherence: Option<f64>,  // Set when Hybrid
}
```

## 3D Bayesian Belief

```rust
pub struct BeliefScore {
    pub confidence: f32,  // Evidence strength
    pub trust: f32,       // Source reliability
    pub relevance: f32,   // Contextual fit
}
// Composite = ∛(conf × trust × rel)
// Tracks positive/negative evidence with temporal decay
```

## Tile Scoring (5 Factors)

| Factor | Weight | Measures |
|--------|--------|----------|
| Keyword | 30% | Direct term overlap |
| Ghost | 15% | Inverse ghost score |
| Belief | 25% | Confidence × trust × relevance |
| Domain | 20% | Domain-query alignment |
| Frequency | 10% | Usage-weighted recency |

## Deploy Policy

```
Composite > 0.8  →  Live (auto-deploy)
0.5 - 0.8        →  Monitored (5% → +10% incremental)
< 0.5            →  HumanGated (manual approval required)
Absolute floor: confidence ≥ 0.3, trust ≥ 0.3
```

## Deadband Engine

NegativeSpace pattern matching + Channel-based safe routing:

```rust
pub struct DeadbandCheck {
    pub passed: bool,
    pub p0_clear: bool,      // No dangerous patterns
    pub p1_clear: bool,      // Safe channel identified
    pub violations: Vec<String>,
    pub recommended_channel: Option<String>,
}
```

Default dangerous patterns: `rm -rf`, `DROP TABLE`, `DELETE FROM`, `chmod 777`, `eval(`, `sudo rm`, `> /dev/sda`

Default safe channels: `math`(0.9), `search`(0.85), `navigate`(0.8), `analysis`(0.85), `safety`(0.95)

## Cargo Feature Tiers

```toml
[features]
fleet = []           # I2I swarm, Kimi swarm router
edge = ["fleet"]     # GPU/CUDA, LoRA, CUDA MUD arena (RTX 4050+)
```

## For Agents

```yaml
plato_kernel_v1:
  type: event_sourced_belief_engine
  modules: 18
  state_model: tri_state (deterministic/generative/hybrid)
  belief: 3D Bayesian (confidence × trust × relevance)
  deploy: live/monitored/human_gated
  scoring: 5-factor weighted retrieval
  deadband: P0→P1→P2 pattern engine
  tiers: [base, fleet, edge]
```

## License

MIT

//! Compile-time gated plugin loader.
//!
//! This module is the **only** place where higher-tier plugin structs are
//! instantiated. Every higher-tier block is wrapped in `#[cfg(feature = "...")]`
//! so that:
//!
//! - `cargo build` (no features) → Tier 1 only. No Fleet or Edge code in binary.
//! - `cargo build --features fleet` → Tier 1 + Tier 2.
//! - `cargo build --features edge`  → Tier 1 + Tier 2 + Tier 3 (edge implies fleet).
//!
//! Adding a new plugin: implement [`PlatoPlugin`], pick the right tier block,
//! declare `requires`/`provides` in the manifest, and add `registry.register(...)`.
//! The resolver validates everything at mount time — no manual wiring needed.

use super::{PlatoPlugin, PluginManifest, PluginRegistry, PluginTier};

// ─── Tier 1: Core plugins (always compiled) ───────────────────────────────────
//
// These are always present regardless of feature flags. They form the minimum
// viable kernel: event routing, constraint checking, and room checkout.

struct CoreEventBusPlugin(PluginManifest);
impl CoreEventBusPlugin {
    fn new() -> Self {
        Self(PluginManifest {
            id:       "core-event-bus".into(),
            name:     "Core Event Bus".into(),
            version:  "1.0.0".into(),
            tier:     PluginTier::Core,
            requires: vec![],
            provides: vec!["event-bus".into()],
        })
    }
}
impl PlatoPlugin for CoreEventBusPlugin {
    fn manifest(&self) -> &PluginManifest { &self.0 }
}

struct CoreConstraintPlugin(PluginManifest);
impl CoreConstraintPlugin {
    fn new() -> Self {
        Self(PluginManifest {
            id:       "core-constraint".into(),
            name:     "Core Constraint Engine".into(),
            version:  "1.0.0".into(),
            tier:     PluginTier::Core,
            requires: vec!["core-event-bus".into()],
            provides: vec!["constraint-engine".into()],
        })
    }
}
impl PlatoPlugin for CoreConstraintPlugin {
    fn manifest(&self) -> &PluginManifest { &self.0 }
}

struct CoreGitRuntimePlugin(PluginManifest);
impl CoreGitRuntimePlugin {
    fn new() -> Self {
        Self(PluginManifest {
            id:       "core-git-runtime".into(),
            name:     "Core Git Runtime (repo-as-room, cocapn)".into(),
            version:  "1.0.0".into(),
            tier:     PluginTier::Core,
            requires: vec!["core-event-bus".into()],
            provides: vec!["git-runtime".into()],
        })
    }
}
impl PlatoPlugin for CoreGitRuntimePlugin {
    fn manifest(&self) -> &PluginManifest { &self.0 }
}

struct CoreTilingPlugin(PluginManifest);
impl CoreTilingPlugin {
    fn new() -> Self {
        Self(PluginManifest {
            id:       "core-tiling".into(),
            name:     "Core Tiling Knowledge Substrate".into(),
            version:  "1.0.0".into(),
            tier:     PluginTier::Core,
            requires: vec!["core-event-bus".into()],
            provides: vec!["tiling".into(), "tutor-anchors".into()],
        })
    }
}
impl PlatoPlugin for CoreTilingPlugin {
    fn manifest(&self) -> &PluginManifest { &self.0 }
}

// ─── Tier 2: Fleet plugins (compiled with `fleet` feature) ───────────────────
//
// These types do not exist in the binary unless `--features fleet` is passed.
// `edge` implies `fleet` via Cargo.toml, so Edge instances get these too.

#[cfg(feature = "fleet")]
struct FleetSwarmPlugin(PluginManifest);

#[cfg(feature = "fleet")]
impl FleetSwarmPlugin {
    fn new() -> Self {
        Self(PluginManifest {
            id:       "fleet-swarm".into(),
            name:     "Fleet I2I Swarm Coordinator".into(),
            version:  "1.0.0".into(),
            tier:     PluginTier::Fleet,
            requires: vec!["core-event-bus".into(), "core-git-runtime".into()],
            provides: vec!["fleet-coordination".into(), "i2i-routing".into()],
        })
    }
}

#[cfg(feature = "fleet")]
impl PlatoPlugin for FleetSwarmPlugin {
    fn manifest(&self) -> &PluginManifest { &self.0 }
}

/// Kimi swarm router — implements Oracle1's multi-ship topology routing.
/// Ships announce via I2I `ANNOUNCE`; this plugin maintains the live topology
/// graph and routes constraint checks to the nearest capable kernel.
#[cfg(feature = "fleet")]
struct KimiSwarmRouterPlugin(PluginManifest);

#[cfg(feature = "fleet")]
impl KimiSwarmRouterPlugin {
    fn new() -> Self {
        Self(PluginManifest {
            id:       "kimi-swarm-router".into(),
            name:     "Kimi Swarm Router (Oracle1 roadmap)".into(),
            version:  "1.0.0".into(),
            tier:     PluginTier::Fleet,
            // Requires fleet-swarm for the I2I transport layer.
            requires: vec!["fleet-swarm".into()],
            provides: vec!["kimi-routing".into(), "swarm-topology".into()],
        })
    }
}

#[cfg(feature = "fleet")]
impl PlatoPlugin for KimiSwarmRouterPlugin {
    fn manifest(&self) -> &PluginManifest { &self.0 }
}

/// Episode sync — broadcasts KNOWLEDGE.md entries across fleet instances so
/// that muscle-memory is fleet-wide, not per-ship.
#[cfg(feature = "fleet")]
struct FleetEpisodeSyncPlugin(PluginManifest);

#[cfg(feature = "fleet")]
impl FleetEpisodeSyncPlugin {
    fn new() -> Self {
        Self(PluginManifest {
            id:       "fleet-episode-sync".into(),
            name:     "Fleet Episode Sync (cross-ship muscle memory)".into(),
            version:  "1.0.0".into(),
            tier:     PluginTier::Fleet,
            requires: vec!["fleet-swarm".into()],
            provides: vec!["episode-sync".into()],
        })
    }
}

#[cfg(feature = "fleet")]
impl PlatoPlugin for FleetEpisodeSyncPlugin {
    fn manifest(&self) -> &PluginManifest { &self.0 }
}

// ─── Tier 3: Edge / GPU plugins (compiled with `edge` feature) ───────────────
//
// Forgemaster-class nodes only. `edge` implies `fleet` (see Cargo.toml).
// Actual CUDA kernel invocations would live inside these plugin impls.

/// GPU constraint validation via Monte Carlo (1A from Oracle1's parallel tracks).
/// Runs rigidity percolation at k=8..16 neighbors; targets Laman threshold k=12.
#[cfg(feature = "edge")]
struct GpuSimulationPlugin(PluginManifest);

#[cfg(feature = "edge")]
impl GpuSimulationPlugin {
    fn new() -> Self {
        Self(PluginManifest {
            id:       "gpu-simulation".into(),
            name:     "GPU Constraint Simulation (Monte Carlo, Ricci Flow)".into(),
            version:  "1.0.0".into(),
            tier:     PluginTier::Edge,
            requires: vec!["fleet-swarm".into(), "core-constraint".into()],
            provides: vec!["gpu-simulation".into(), "monte-carlo".into(), "ricci-flow".into()],
        })
    }
}

#[cfg(feature = "edge")]
impl PlatoPlugin for GpuSimulationPlugin {
    fn manifest(&self) -> &PluginManifest { &self.0 }
}

/// LoRA fine-tuning on captain decision data (1D from Oracle1's parallel tracks).
/// Produces small models exported for JC1 Jetson deployment.
#[cfg(feature = "edge")]
struct LoraFinetuningPlugin(PluginManifest);

#[cfg(feature = "edge")]
impl LoraFinetuningPlugin {
    fn new() -> Self {
        Self(PluginManifest {
            id:       "lora-finetuning".into(),
            name:     "LoRA Fine-tuning Engine (JC1 edge model export)".into(),
            version:  "1.0.0".into(),
            tier:     PluginTier::Edge,
            // Needs GPU simulation results as training data.
            requires: vec!["gpu-simulation".into()],
            provides: vec!["lora-training".into(), "model-export".into()],
        })
    }
}

#[cfg(feature = "edge")]
impl PlatoPlugin for LoraFinetuningPlugin {
    fn manifest(&self) -> &PluginManifest { &self.0 }
}

/// CUDA MUD Arena — 1 GPU thread = 1 agent, 1000+ parallel scenarios (1C).
/// Backtests agent scripts; evolves via genetic algorithm for Day 47 drill.
#[cfg(feature = "edge")]
struct CudaMudArenaPlugin(PluginManifest);

#[cfg(feature = "edge")]
impl CudaMudArenaPlugin {
    fn new() -> Self {
        Self(PluginManifest {
            id:       "cuda-mud-arena".into(),
            name:     "CUDA MUD Arena (parallel agent backtest, Day 47 drill)".into(),
            version:  "1.0.0".into(),
            tier:     PluginTier::Edge,
            requires: vec!["gpu-simulation".into(), "fleet-swarm".into()],
            provides: vec!["cuda-arena".into(), "agent-backtest".into()],
        })
    }
}

#[cfg(feature = "edge")]
impl PlatoPlugin for CudaMudArenaPlugin {
    fn manifest(&self) -> &PluginManifest { &self.0 }
}

// ─── Loader entry-point ───────────────────────────────────────────────────────

/// Populate `registry` with all builtins available under the current feature set.
///
/// This is the **sole** place where feature-gated plugin types are referenced.
/// All other code (registry, resolver, kernel) is feature-agnostic.
///
/// After calling this, the caller decides which plugins to mount by calling
/// [`PluginRegistry::mount`] or [`PluginRegistry::mount_tier`].
pub fn load_builtins(registry: &mut PluginRegistry) {
    // ── Tier 1 (always) ──────────────────────────────────────────────────────
    registry.register(Box::new(CoreEventBusPlugin::new())).expect("core-event-bus");
    registry.register(Box::new(CoreConstraintPlugin::new())).expect("core-constraint");
    registry.register(Box::new(CoreGitRuntimePlugin::new())).expect("core-git-runtime");
    registry.register(Box::new(CoreTilingPlugin::new())).expect("core-tiling");

    // ── Tier 2 (fleet feature) ───────────────────────────────────────────────
    #[cfg(feature = "fleet")]
    {
        registry.register(Box::new(FleetSwarmPlugin::new())).expect("fleet-swarm");
        registry.register(Box::new(KimiSwarmRouterPlugin::new())).expect("kimi-swarm-router");
        registry.register(Box::new(FleetEpisodeSyncPlugin::new())).expect("fleet-episode-sync");
    }

    // ── Tier 3 (edge feature, implies fleet) ─────────────────────────────────
    #[cfg(feature = "edge")]
    {
        registry.register(Box::new(GpuSimulationPlugin::new())).expect("gpu-simulation");
        registry.register(Box::new(LoraFinetuningPlugin::new())).expect("lora-finetuning");
        registry.register(Box::new(CudaMudArenaPlugin::new())).expect("cuda-mud-arena");
    }

    tracing::info!(
        "plugin::loader    {} plugins registered [core{}{}]",
        registry.registered_count(),
        if cfg!(feature = "fleet") { "+fleet" } else { "" },
        if cfg!(feature = "edge")  { "+edge"  } else { "" },
    );
}

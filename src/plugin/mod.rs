//! Modular Plugin Architecture for Plato Kernel
//!
//! Implements the JC1 three-tier RFC and Oracle1 Kimi swarm roadmap.
//!
//! ## Three-Tier Architecture
//!
//! | Tier  | Cargo feature | Purpose                                           |
//! |-------|---------------|---------------------------------------------------|
//! | Core  | (always)      | Event bus, constraints, git runtime — always on   |
//! | Fleet | `fleet`       | I2I swarm, Kimi routing, fleet coordination       |
//! | Edge  | `edge`        | GPU simulation, LoRA fine-tuning, CUDA MUD arena  |
//!
//! **Zero-overhead guarantee**: higher-tier plugin *types* are only compiled
//! when the corresponding Cargo feature is active. A base fleet instance
//! compiled without `fleet` or `edge` carries literally no code from those
//! tiers — not even a vtable entry.
//!
//! **Dependency resolution**: before any plugin may be mounted, the
//! [`PluginRegistry`] runs a full topological sort (Kahn's algorithm) over
//! the dependency sub-graph, rejecting cycles, missing deps, and tier
//! violations in one pass.

pub mod loader;

use std::collections::{HashMap, HashSet, VecDeque};
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ─── Tier definitions (JC1 three-tier RFC) ───────────────────────────────────

/// The deployment tier of a plugin, matching JC1's three-tier RFC.
///
/// Tiers are totally ordered (`Core < Fleet < Edge`). A plugin at tier N may
/// only declare dependencies on plugins of tier ≤ N — enforced by
/// [`PluginRegistry::resolve_mount`] before any code runs.
///
/// This ordering is the structural guarantee that lets a base fleet kernel
/// binary be compiled without a single byte of GPU/edge code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PluginTier {
    /// Tier 1 — always compiled. Event bus, constraint engine, git runtime.
    Core = 0,
    /// Tier 2 — `fleet` Cargo feature. I2I swarm coordination, Kimi routing.
    Fleet = 1,
    /// Tier 3 — `edge` Cargo feature. GPU simulation, LoRA, CUDA MUD arena.
    Edge = 2,
}

impl std::fmt::Display for PluginTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginTier::Core  => write!(f, "Core(T1)"),
            PluginTier::Fleet => write!(f, "Fleet(T2)"),
            PluginTier::Edge  => write!(f, "Edge(T3)"),
        }
    }
}

// ─── Plugin manifest ─────────────────────────────────────────────────────────

/// Declarative, serialisable description of a plugin's identity and
/// requirements. Checked by [`PluginRegistry::resolve_mount`] *before* any
/// plugin initialisation runs, so problems are caught statically.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Stable, unique identifier (e.g. `"gpu-simulation"`). Fleet-wide unique.
    pub id: String,
    /// Human-readable display name for logs and diagnostics.
    pub name: String,
    /// Semver string — display only; structural validation is out of scope.
    pub version: String,
    /// Which compile-time tier covers this plugin (enforces the feature gate).
    pub tier: PluginTier,
    /// IDs of plugins that must be mounted before this one.
    ///
    /// Invariant: every entry must have `tier ≤ self.tier`. A Core plugin
    /// may not require a Fleet plugin, or the zero-overhead guarantee breaks.
    pub requires: Vec<String>,
    /// Capability strings exported by this plugin (e.g. `"cuda-arena"`).
    /// Other plugins and kernel subsystems query these via [`PluginRegistry::provides`].
    pub provides: Vec<String>,
}

// ─── Plugin trait ─────────────────────────────────────────────────────────────

/// A Plato plugin. Implement this and hand a `Box<dyn PlatoPlugin>` to
/// [`PluginRegistry::register`].
///
/// The trait is intentionally minimal: the manifest carries all metadata; a
/// richer lifecycle (`init`, `shutdown`) can be layered on top without
/// changing the registry contract.
pub trait PlatoPlugin: Send + Sync + 'static {
    fn manifest(&self) -> &PluginManifest;
}

// ─── Mount plan ───────────────────────────────────────────────────────────────

/// The fully-validated, dependency-first mount sequence produced by
/// [`PluginRegistry::resolve_mount`].
///
/// Apply with [`PluginRegistry::apply_plan`] — idempotent: already-mounted
/// plugins are silently skipped, so re-entrant bootstrap calls are safe.
#[derive(Debug, Clone)]
pub struct MountPlan {
    /// Plugin IDs in safe mount order (all deps appear before their dependents).
    pub order: Vec<String>,
}

// ─── Errors ───────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum DependencyError {
    #[error("Plugin '{plugin}' requires '{requires}', which is not registered")]
    MissingDependency { plugin: String, requires: String },

    #[error("Circular dependency detected among: {cycle:?}")]
    CircularDependency { cycle: Vec<String> },

    /// A lower-tier plugin attempting to pull in a higher-tier dependency
    /// would break the zero-overhead compile-time guarantee.
    #[error(
        "Tier violation: '{plugin}' ({plugin_tier}) requires '{dep}' ({dep_tier}) \
         — a plugin may only depend on equal-or-lower tiers"
    )]
    TierViolation {
        plugin: String,
        plugin_tier: PluginTier,
        dep: String,
        dep_tier: PluginTier,
    },

    #[error("Plugin '{id}' is already registered")]
    AlreadyRegistered { id: String },

    #[error("Plugin '{id}' is not registered")]
    NotFound { id: String },
}

// ─── Registry ─────────────────────────────────────────────────────────────────

/// The central plugin registry and mount tracker.
///
/// **Thread safety**: the registry is not `Sync` by default; wrap in
/// `Arc<Mutex<PluginRegistry>>` for shared async access.
///
/// **Zero-overhead note**: the registry itself is always compiled (it is a
/// Core-tier component). The compile-time feature gates live entirely in
/// [`loader::load_builtins`], which never *constructs* higher-tier plugin
/// values in base fleet builds. The registry sees only what the loader hands
/// it — no dead branches, no unused vtables.
pub struct PluginRegistry {
    plugins: HashMap<String, Box<dyn PlatoPlugin>>,
    mounted: HashSet<String>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
            mounted: HashSet::new(),
        }
    }

    /// Register a plugin. Fails if the ID is already taken.
    ///
    /// Registration is unconditional — no dependency checks here. Validation
    /// happens at mount time so that the registry can be populated
    /// incrementally (e.g. plugins loaded from dynamic manifests before their
    /// deps have been registered).
    pub fn register(&mut self, plugin: Box<dyn PlatoPlugin>) -> Result<(), DependencyError> {
        let id = plugin.manifest().id.clone();
        if self.plugins.contains_key(&id) {
            return Err(DependencyError::AlreadyRegistered { id });
        }
        tracing::debug!("plugin::register  {} ({})", id, plugin.manifest().tier);
        self.plugins.insert(id, plugin);
        Ok(())
    }

    /// Resolve the full, validated mount plan for `plugin_id` and all its
    /// transitive dependencies.
    ///
    /// **Algorithm** (Kahn's topological sort):
    /// 1. Walk the dependency graph from `plugin_id` to collect the transitive
    ///    closure, failing immediately on missing deps.
    /// 2. Validate tier constraints across the entire subgraph.
    /// 3. Run Kahn's BFS: start from zero-in-degree nodes, process dependents,
    ///    detect cycles by checking whether all nodes were emitted.
    ///
    /// Already-mounted plugins appear in the plan; [`apply_plan`] skips them.
    pub fn resolve_mount(&self, plugin_id: &str) -> Result<MountPlan, DependencyError> {
        // 1. Collect transitive closure.
        let subgraph = self.transitive_deps(plugin_id)?;

        // 2. Tier enforcement: no plugin may depend on a higher tier.
        for id in &subgraph {
            let m = self.plugins[id].manifest();
            for req_id in &m.requires {
                if let Some(req_plugin) = self.plugins.get(req_id) {
                    let req_tier = req_plugin.manifest().tier;
                    if req_tier > m.tier {
                        return Err(DependencyError::TierViolation {
                            plugin: id.clone(),
                            plugin_tier: m.tier,
                            dep: req_id.clone(),
                            dep_tier: req_tier,
                        });
                    }
                }
            }
        }

        // 3. Kahn's topological sort over the subgraph.
        //    in_degree[x] = number of x's deps that are in the subgraph.
        let mut in_degree: HashMap<String, usize> =
            subgraph.iter().map(|id| (id.clone(), 0)).collect();
        // dependents[dep] = list of plugins in subgraph that require `dep`.
        let mut dependents: HashMap<String, Vec<String>> =
            subgraph.iter().map(|id| (id.clone(), vec![])).collect();

        for id in &subgraph {
            let m = self.plugins[id].manifest();
            for req_id in &m.requires {
                if subgraph.contains(req_id) {
                    *in_degree.entry(id.clone()).or_insert(0) += 1;
                    dependents.entry(req_id.clone()).or_default().push(id.clone());
                }
            }
        }

        // Seed: all nodes with no remaining dependencies.
        // Sort for deterministic output across different HashMap orderings.
        let mut queue: VecDeque<String> = {
            let mut seeds: Vec<String> = in_degree
                .iter()
                .filter(|(_, &deg)| deg == 0)
                .map(|(id, _)| id.clone())
                .collect();
            seeds.sort();
            seeds.into()
        };

        let mut order: Vec<String> = Vec::with_capacity(subgraph.len());
        while let Some(id) = queue.pop_front() {
            order.push(id.clone());
            if let Some(deps) = dependents.get(&id) {
                let mut ready: Vec<String> = deps
                    .iter()
                    .filter_map(|dep| {
                        let deg = in_degree.get_mut(dep)?;
                        *deg -= 1;
                        if *deg == 0 { Some(dep.clone()) } else { None }
                    })
                    .collect();
                ready.sort(); // determinism
                for r in ready {
                    queue.push_back(r);
                }
            }
        }

        // Any node left with in_degree > 0 is part of a cycle.
        if order.len() != subgraph.len() {
            let mut cycle: Vec<String> = in_degree
                .into_iter()
                .filter(|(_, deg)| *deg > 0)
                .map(|(id, _)| id)
                .collect();
            cycle.sort();
            return Err(DependencyError::CircularDependency { cycle });
        }

        Ok(MountPlan { order })
    }

    /// Mark all plugins in `plan.order` as mounted, skipping already-mounted ones.
    ///
    /// Returns the IDs that were *newly* mounted this call.
    pub fn apply_plan(&mut self, plan: &MountPlan) -> Vec<String> {
        let mut newly_mounted = Vec::new();
        for id in &plan.order {
            if !self.mounted.contains(id.as_str()) {
                tracing::info!("plugin::mount     {}", id);
                self.mounted.insert(id.clone());
                newly_mounted.push(id.clone());
            }
        }
        newly_mounted
    }

    /// Convenience: resolve and apply in one call.
    pub fn mount(&mut self, plugin_id: &str) -> Result<MountPlan, DependencyError> {
        let plan = self.resolve_mount(plugin_id)?;
        self.apply_plan(&plan);
        Ok(plan)
    }

    /// Mount all registered plugins of the given `tier` (and their deps).
    ///
    /// This is the primary bootstrap entry-point for the kernel. Call it once
    /// per tier level you want active (e.g. `mount_tier(PluginTier::Core)`
    /// for base instances, then optionally `mount_tier(PluginTier::Fleet)`).
    ///
    /// **Your turn**: implement this — the strategy you choose shapes the
    /// kernel startup contract:
    ///
    /// - **Fail-fast**: return on the first `Err` so the operator sees a clean
    ///   error rather than a partially-mounted system.
    /// - **Best-effort**: collect all errors and continue; useful for dev
    ///   builds where some plugins may be intentionally absent.
    /// - **Ordered**: sort by dependency-count first to minimise retry
    ///   iterations when deps arrive out of registration order.
    ///
    /// Location: `src/plugin/mod.rs`, method `mount_tier` — ~8-12 lines.
    pub fn mount_tier(&mut self, tier: PluginTier) -> Result<Vec<MountPlan>, DependencyError> {
        // Collect all registered plugins at exactly this tier.
        let mut targets: Vec<String> = self.plugins
            .iter()
            .filter(|(_, p)| p.manifest().tier == tier)
            .map(|(id, _)| id.clone())
            .collect();

        // Sort for deterministic mount order across HashMap iterations.
        targets.sort();

        let mut plans = Vec::new();
        for id in targets {
            if self.mounted.contains(&id) {
                continue; // idempotent — skip already-mounted plugins
            }
            // resolve_mount performs full Kahn topo-sort over transitive deps,
            // so lower-tier deps are mounted before this plugin. Fail-fast: any
            // missing dep, cycle, or tier violation bubbles up immediately.
            let plan = self.resolve_mount(&id)?;
            self.apply_plan(&plan);
            plans.push(plan);
        }
        Ok(plans)
    }

    /// Returns `true` if a mounted plugin declares `capability` in its `provides` list.
    pub fn provides(&self, capability: &str) -> bool {
        self.mounted.iter().any(|id| {
            self.plugins
                .get(id)
                .map(|p| p.manifest().provides.iter().any(|c| c == capability))
                .unwrap_or(false)
        })
    }

    /// Whether plugin `id` is currently mounted.
    pub fn is_mounted(&self, id: &str) -> bool {
        self.mounted.contains(id)
    }

    /// Number of registered plugins.
    pub fn registered_count(&self) -> usize {
        self.plugins.len()
    }

    /// Iterate registered plugin IDs.
    pub fn registered_ids(&self) -> impl Iterator<Item = &str> {
        self.plugins.keys().map(String::as_str)
    }

    /// Iterate mounted plugin IDs.
    pub fn mounted_ids(&self) -> impl Iterator<Item = &str> {
        self.mounted.iter().map(String::as_str)
    }

    // ── Internals ────────────────────────────────────────────────────────────

    /// DFS from `root` to collect the transitive closure of its dependencies.
    /// Fails immediately if a required plugin is not registered.
    fn transitive_deps(&self, root: &str) -> Result<HashSet<String>, DependencyError> {
        let mut visited: HashSet<String> = HashSet::new();
        let mut stack = vec![root.to_string()];

        while let Some(id) = stack.pop() {
            if visited.contains(&id) {
                continue;
            }
            let plugin = self.plugins.get(&id).ok_or_else(|| DependencyError::NotFound {
                id: id.clone(),
            })?;
            for req in &plugin.manifest().requires {
                if !self.plugins.contains_key(req) {
                    return Err(DependencyError::MissingDependency {
                        plugin: id.clone(),
                        requires: req.clone(),
                    });
                }
                if !visited.contains(req) {
                    stack.push(req.clone());
                }
            }
            visited.insert(id);
        }

        Ok(visited)
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_plugin(id: &str, tier: PluginTier, requires: Vec<&str>, provides: Vec<&str>) -> Box<dyn PlatoPlugin> {
        struct Stub(PluginManifest);
        impl PlatoPlugin for Stub {
            fn manifest(&self) -> &PluginManifest { &self.0 }
        }
        Box::new(Stub(PluginManifest {
            id: id.to_string(),
            name: id.to_string(),
            version: "0.0.1".to_string(),
            tier,
            requires: requires.iter().map(|s| s.to_string()).collect(),
            provides: provides.iter().map(|s| s.to_string()).collect(),
        }))
    }

    #[test]
    fn test_basic_mount_order() {
        let mut reg = PluginRegistry::new();
        reg.register(make_plugin("a", PluginTier::Core, vec![], vec!["cap-a"])).unwrap();
        reg.register(make_plugin("b", PluginTier::Core, vec!["a"], vec!["cap-b"])).unwrap();
        reg.register(make_plugin("c", PluginTier::Core, vec!["b"], vec!["cap-c"])).unwrap();

        let plan = reg.resolve_mount("c").unwrap();
        // a must precede b, b must precede c
        let pos = |id: &str| plan.order.iter().position(|x| x == id).unwrap();
        assert!(pos("a") < pos("b"));
        assert!(pos("b") < pos("c"));
    }

    #[test]
    fn test_missing_dependency_rejected() {
        let mut reg = PluginRegistry::new();
        reg.register(make_plugin("b", PluginTier::Core, vec!["a"], vec![])).unwrap();

        let err = reg.resolve_mount("b").unwrap_err();
        assert!(matches!(err, DependencyError::MissingDependency { .. }));
    }

    #[test]
    fn test_circular_dependency_detected() {
        let mut reg = PluginRegistry::new();
        reg.register(make_plugin("x", PluginTier::Core, vec!["y"], vec![])).unwrap();
        reg.register(make_plugin("y", PluginTier::Core, vec!["x"], vec![])).unwrap();

        let err = reg.resolve_mount("x").unwrap_err();
        assert!(matches!(err, DependencyError::CircularDependency { .. }));
    }

    #[test]
    fn test_tier_violation_rejected() {
        let mut reg = PluginRegistry::new();
        // A Core plugin trying to depend on a Fleet plugin — forbidden.
        reg.register(make_plugin("fleet-thing", PluginTier::Fleet, vec![], vec![])).unwrap();
        reg.register(make_plugin("core-bad",   PluginTier::Core, vec!["fleet-thing"], vec![])).unwrap();

        let err = reg.resolve_mount("core-bad").unwrap_err();
        assert!(matches!(err, DependencyError::TierViolation { .. }));
    }

    #[test]
    fn test_provides_capability() {
        let mut reg = PluginRegistry::new();
        reg.register(make_plugin("a", PluginTier::Core, vec![], vec!["event-bus"])).unwrap();
        reg.mount("a").unwrap();

        assert!(reg.provides("event-bus"));
        assert!(!reg.provides("gpu-simulation"));
    }

    #[test]
    fn test_duplicate_registration_rejected() {
        let mut reg = PluginRegistry::new();
        reg.register(make_plugin("a", PluginTier::Core, vec![], vec![])).unwrap();
        let err = reg.register(make_plugin("a", PluginTier::Core, vec![], vec![])).unwrap_err();
        assert!(matches!(err, DependencyError::AlreadyRegistered { .. }));
    }

    #[test]
    fn test_apply_plan_idempotent() {
        let mut reg = PluginRegistry::new();
        reg.register(make_plugin("a", PluginTier::Core, vec![], vec![])).unwrap();
        let plan = reg.resolve_mount("a").unwrap();

        let first  = reg.apply_plan(&plan);
        let second = reg.apply_plan(&plan);

        assert_eq!(first,  vec!["a"]);
        assert!(second.is_empty()); // idempotent — already mounted
    }

    #[test]
    fn test_fleet_can_depend_on_core() {
        let mut reg = PluginRegistry::new();
        reg.register(make_plugin("core-bus", PluginTier::Core,  vec![], vec!["event-bus"])).unwrap();
        reg.register(make_plugin("fleet-sw", PluginTier::Fleet, vec!["core-bus"], vec![])).unwrap();

        // This should succeed — Fleet depending on Core is legal.
        let plan = reg.resolve_mount("fleet-sw").unwrap();
        let pos = |id: &str| plan.order.iter().position(|x| x == id).unwrap();
        assert!(pos("core-bus") < pos("fleet-sw"));
    }
}

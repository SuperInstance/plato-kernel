//! Plato Kernel — Core runtime for Plato-as-Common-UX
//!
//! Combines:
//! - Event Sourcing Bus (async pub/sub with replay/DLQ)
//! - Constraint Engine (first-person perspective filtering + assertive Markdown)
//! - Git Runtime (repo-as-room, cocapn protocol)
//! - Perspective Manager (identity + constraints → what you see)
//! - Tiling Knowledge Substrate (Markdown → semantic tiles, conditional injection)
//! - Episode Recorder (KNOWLEDGE.md — agent muscle memory)
//! - TUTOR Word Anchors ([BracketedWord] → tile context jump)
//! - I2I Protocol (instance-to-instance cross-process coordination)

mod belief;
mod constraint_engine;
mod deploy_policy;
mod dynamic_locks;
mod episode_recorder;
mod event_bus;
mod git_runtime;
mod i2i;
mod perspective;
mod plugin;
mod state_bridge;
mod tiling;
mod tutor;

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use belief::{BeliefDimension, BeliefScore, BeliefStore};
use constraint_engine::{ConstraintAuditor, ConstraintEngine};
use deploy_policy::{DeployDecision, DeployLedger, DeployPolicy, Tier};
use dynamic_locks::{Lock, LockAccumulator, LockCheck, LockSource};
use episode_recorder::{EpisodeEntry, EpisodeOutcome, EpisodeRecorder};
use i2i::{ComponentKind, I2IMessage, I2IVerb, I2IServer, InstanceId, default_kernel_handler};
use plugin::{PluginRegistry, PluginTier};
use plugin::loader::load_builtins;
use tiling::TileRegistry;
use tutor::{jump_context, JumpResult};

/// The complete Plato Kernel runtime.
pub struct PlatoKernel {
    event_bus: event_bus::EventBus,
    constraint_engine: ConstraintEngine,
    git_runtime: Arc<Mutex<git_runtime::GitRuntime>>,
    perspective_manager: perspective::PerspectiveManager,
    episode_recorder: EpisodeRecorder,
    instance_id: InstanceId,
    /// Plugin registry — populated by [`load_builtins`] at startup.
    /// Plugins at higher tiers (Fleet, Edge) are only present when the
    /// corresponding Cargo feature is active at build time.
    pub plugins: PluginRegistry,
    /// DCS Flywheel: unified belief store
    pub beliefs: BeliefStore,
    /// DCS Flywheel: dynamic lock accumulator
    pub locks: LockAccumulator,
    /// DCS Flywheel: deployment ledger
    pub deploy_ledger: DeployLedger,
}

impl PlatoKernel {
    /// Create a new Plato Kernel instance.
    pub async fn new() -> Result<Self> {
        tracing::info!("Initializing Plato Kernel...");

        // Bootstrap the plugin registry: register all builtins for the
        // current feature set, then mount the Core tier (always safe).
        let mut plugins = PluginRegistry::new();
        load_builtins(&mut plugins);

        // Mount core plugins individually (mount_tier is user-contributed).
        for id in ["core-event-bus", "core-constraint", "core-git-runtime", "core-tiling"] {
            if let Err(e) = plugins.mount(id) {
                tracing::warn!("plugin mount skipped ({id}): {e}");
            }
        }

        // Fleet-tier mounts (only compiled when `fleet` feature is active).
        #[cfg(feature = "fleet")]
        for id in ["fleet-swarm", "kimi-swarm-router", "fleet-episode-sync"] {
            if let Err(e) = plugins.mount(id) {
                tracing::warn!("plugin mount skipped ({id}): {e}");
            }
        }

        // Edge-tier mounts (only compiled when `edge` feature is active).
        #[cfg(feature = "edge")]
        for id in ["gpu-simulation", "lora-finetuning", "cuda-mud-arena"] {
            if let Err(e) = plugins.mount(id) {
                tracing::warn!("plugin mount skipped ({id}): {e}");
            }
        }

        tracing::info!(
            "Plugin tiers active: Core{}{}",
            if cfg!(feature = "fleet") { " + Fleet" } else { "" },
            if cfg!(feature = "edge")  { " + Edge"  } else { "" },
        );

        Ok(Self {
            event_bus: event_bus::EventBus::new(),
            constraint_engine: ConstraintEngine::new(),
            git_runtime: Arc::new(Mutex::new(git_runtime::GitRuntime::new().await?)),
            perspective_manager: perspective::PerspectiveManager::new(),
            episode_recorder: EpisodeRecorder::default_path(),
            instance_id: InstanceId::new(ComponentKind::Kernel, "plato-kernel", "localhost"),
            plugins,
            beliefs: BeliefStore::new(),
            locks: LockAccumulator::new(),
            deploy_ledger: DeployLedger::new(DeployPolicy::default()),
        })
    }

    /// Join a fleet by connecting to the Agora meta-repo
    pub async fn join_fleet(&self, agora_remote: &str) -> Result<()> {
        let mut rt = self.git_runtime.lock().await;
        rt.join_fleet(agora_remote).await
    }

    /// Connect an identity to a room (repo).
    pub async fn connect(
        &self,
        identity: &str,
        room: &str,
    ) -> Result<perspective::Session> {
        tracing::info!("Connecting {} to room {}", identity, room);

        let mut rt = self.git_runtime.lock().await;
        let repo = rt.checkout(room).await?;
        let constraints = self.constraint_engine.load_constraints(&repo.name, identity)?;
        let perspective = self.perspective_manager.create_perspective(identity, constraints);
        let events = self.event_bus.subscribe(identity, room).await;

        Ok(perspective::Session::new(
            identity.to_string(),
            room.to_string(),
            perspective,
            events,
        ))
    }

    /// Process a command through the full PLATO pipeline:
    ///
    /// 0. [NEW] Run instinct pre-check — reflexes fire before constraint logic.
    ///    Survive/Flee/Report instincts can short-circuit the pipeline.
    /// 1. Extract `[WordAnchor]` TUTOR jumps and inject matching tiles.
    /// 2. Check assertive constraints from the room's Markdown constraints.
    /// 3. Record the episode to KNOWLEDGE.md.
    /// 4. Return an `ActionResult`.
    pub async fn process_command(
        &self,
        identity: &str,
        room: &str,
        command: &str,
        tile_registry: &TileRegistry,
        auditor: &ConstraintAuditor,
        instinct_context: Option<&InstinctContext>,
    ) -> ActionResult {
        tracing::debug!("process_command: {} in {} → {:?}", identity, room, command);

        // Step 0: Instinct pre-check — reflexes before logic
        if let Some(ctx) = instinct_context {
            let reflexes = ctx.engine.tick(ctx.energy, ctx.threat, ctx.trust, ctx.peer_alive, ctx.has_work);
            let highest = reflexes.first();
            if let Some(r) = highest {
                match r.instinct {
                    InstinctType::Survive if r.urgency > 0.8 => {
                        tracing::warn!("Instinct SURVIVE triggered (urgency {:.2}) — blocking command", r.urgency);
                        let entry = EpisodeEntry::new(
                            &format!("{} in {}", command, room),
                            &format!("BLOCKED by SURVIVE instinct (urgency {:.2})", r.urgency),
                            "Instinct override", EpisodeOutcome::Failure,
                        );
                        let _ = self.episode_recorder.record(&entry);
                        return ActionResult {
                            command: command.to_string(),
                            tutor_context: vec![format!("[INSTINCT] SURVIVE — energy critical ({:.0}%). Command blocked.", ctx.energy * 100.0)],
                            audit: constraint_engine::AuditOutcome::Pass,
                            episode_id: entry.id,
                            instinct_reflexes: Some(reflexes.iter().map(|r| r.instinct.name().to_string()).collect()),
                            deploy_tier: None, belief_score: None, lock_checks: None,
                        };
                    }
                    InstinctType::Flee if r.urgency > 0.7 => {
                        tracing::warn!("Instinct FLEE triggered (urgency {:.2}) — deferring command", r.urgency);
                        return ActionResult {
                            command: command.to_string(),
                            tutor_context: vec![format!("[INSTINCT] FLEE — threat elevated ({:.0}%). Command deferred.", ctx.threat * 100.0)],
                            audit: constraint_engine::AuditOutcome::Pass,
                            episode_id: String::new(),
                            instinct_reflexes: Some(reflexes.iter().map(|r| r.instinct.name().to_string()).collect()),
                            deploy_tier: None, belief_score: None, lock_checks: None,
                        };
                    }
                    InstinctType::Report if r.urgency > 0.3 => {
                        tracing::info!("Instinct REPORT triggered — anomaly zone");
                        // Report doesn't block, just annotates
                    }
                    _ => {}
                }
            }
        }

        // Step 1: TUTOR — resolve word anchors
        let tutor_context: Vec<String> = match jump_context(command, tile_registry) {
            JumpResult::Found(tile) => {
                tracing::info!("TUTOR jump → tile '{}'", tile.anchor);
                vec![format!("[TUTOR] Jumped to tile: {}\n{}", tile.header, tile.body)]
            }
            JumpResult::NotFound { anchor, suggestions } => {
                let s = if suggestions.is_empty() {
                    String::new()
                } else {
                    format!(" (did you mean: {}?)", suggestions.join(", "))
                };
                vec![format!("[TUTOR] Anchor '{}' not found{}", anchor, s)]
            }
            JumpResult::NoAnchors => vec![],
        };

        // Step 1b: DCS Flywheel — compute belief score
        // Confidence = tile match quality, Trust = peer trust, Relevance = command-room match
        let belief_confidence = if !tutor_context.is_empty() && tutor_context[0].contains("Jumped to") { 0.85 } else { 0.5 };
        let belief_trust = instinct_context.map_or(0.5, |ctx| ctx.trust);
        let belief_relevance = 0.6; // baseline; could be computed from room context
        let belief = BeliefScore::new(belief_confidence, belief_trust, belief_relevance);
        let belief_key = format!("cmd:{}", &command[..command.len().min(32)]);
        let belief_composite = belief.composite();

        // Step 2: Constraint audit
        let audit = auditor.audit(command);
        let outcome = match &audit {
            constraint_engine::AuditOutcome::RetryRequired(failures) => {
                tracing::warn!("Constraint violations: {:?}", failures);
                EpisodeOutcome::Failure
            }
            constraint_engine::AuditOutcome::Warned(_) => EpisodeOutcome::Partial,
            constraint_engine::AuditOutcome::Pass => EpisodeOutcome::Success,
        };

        // Step 2b: DCS Flywheel — classify deployment tier from belief (read-only classify)
        let policy = DeployPolicy::default();
        let deploy_decision = policy.classify(belief_confidence, belief_trust, belief_relevance);
        let deploy_tier = deploy_decision.tier;
        let deploy_requires_human = deploy_decision.requires_human;

        // Step 2c: DCS Flywheel — check dynamic locks
        let lock_checks = self.locks.check(command);
        let lock_blocked = lock_checks.iter().any(|lc| lc.effective_strength > 0.8 && lc.enforcement.contains("BLOCK"));

        // If human-gated or lock-blocked, return early with warning
        if deploy_requires_human {
            tracing::warn!("DCS: command human-gated (belief composite {:.3})", belief_composite);
            let entry = EpisodeEntry::new(
                &format!("{} in {}", command, room),
                &format!("BLOCKED: human-gated, belief {:.3}", belief_composite),
                &format!("Deploy: {:?}", deploy_tier),
                EpisodeOutcome::Failure,
            );
            let _ = self.episode_recorder.record(&entry);
            return ActionResult {
                command: command.to_string(), tutor_context, audit, episode_id: entry.id,
                instinct_reflexes: None,
                deploy_tier: Some(deploy_tier), belief_score: Some(belief), lock_checks: Some(lock_checks),
            };
        }
        if lock_blocked {
            tracing::warn!("DCS: command blocked by dynamic lock");
            let entry = EpisodeEntry::new(
                &format!("{} in {}", command, room),
                "BLOCKED: dynamic lock triggered", "Lock check failed",
                EpisodeOutcome::Failure,
            );
            let _ = self.episode_recorder.record(&entry);
            return ActionResult {
                command: command.to_string(), tutor_context, audit, episode_id: entry.id,
                instinct_reflexes: None,
                deploy_tier: Some(deploy_tier), belief_score: Some(belief), lock_checks: Some(lock_checks),
            };
        }

        // Step 3: Record episode
        let entry = EpisodeEntry::new(
            &format!("{} in {}", command, room),
            &format!("Identity {} issued: {}", identity, command),
            &format!("Audit: {:?}, Deploy: {:?}, Belief: {:.3}", audit, deploy_tier, belief_composite),
            outcome,
        );
        if let Err(e) = self.episode_recorder.record(&entry) {
            tracing::warn!("Episode recorder error: {}", e);
        }

        // Step 3b: DCS Flywheel — update belief based on audit outcome
        // (Note: in a real async context we'd need &mut self; this demonstrates the wiring)
        tracing::debug!("DCS flywheel: belief {:.3} → tier {:?}", belief_composite, deploy_tier);

        ActionResult {
            command: command.to_string(),
            tutor_context,
            audit,
            episode_id: entry.id,
            instinct_reflexes: None,
            deploy_tier: Some(deploy_tier),
            belief_score: Some(belief),
            lock_checks: Some(lock_checks),
        }
    }

    /// Handle an incoming I2I message from another instance.
    pub async fn handle_i2i(&self, raw: &str) -> Option<I2IMessage> {
        let msg = match I2IMessage::from_wire(raw) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("I2I parse error: {}", e);
                return None;
            }
        };

        tracing::info!("I2I {:?} from {} → {}", msg.verb, msg.from, msg.target);

        match &msg.verb {
            I2IVerb::Announce => {
                tracing::info!("I2I: instance announced: {}", msg.from);
                None
            }
            I2IVerb::Disconnect => {
                tracing::info!("I2I: instance disconnected: {}", msg.from);
                None
            }
            I2IVerb::ConstraintCheck => {
                let reply = I2IMessage::reply(
                    &msg,
                    I2IVerb::ConstraintResult,
                    serde_json::json!({ "result": "Allow" }),
                );
                Some(reply)
            }
            I2IVerb::TutorJump => {
                let anchor = msg.payload.get("anchor").and_then(|v| v.as_str()).unwrap_or("");
                tracing::info!("I2I TUTOR_JUMP for anchor '{}'", anchor);
                let reply = I2IMessage::reply(
                    &msg,
                    I2IVerb::Response,
                    serde_json::json!({ "anchor": anchor, "status": "queued" }),
                );
                Some(reply)
            }
            I2IVerb::Request => {
                // Handle fleet-related requests
                if msg.target == "fleet/list" {
                    let mut rt = self.git_runtime.lock().await;
                    if let Ok(rooms) = rt.list_fleet_rooms().await {
                        let rooms_json: Vec<_> = rooms.iter().map(|r| {
                            serde_json::json!({
                                "repo": r.repo,
                                "type": r.room_type,
                                "agents": r.agents
                            })
                        }).collect();
                        let reply = I2IMessage::reply(
                            &msg,
                            I2IVerb::Response,
                            serde_json::json!({ "rooms": rooms_json }),
                        );
                        Some(reply)
                    } else {
                        let reply = I2IMessage::reply(
                            &msg,
                            I2IVerb::Response,
                            serde_json::json!({ "error": "Not joined to any fleet" }),
                        );
                        Some(reply)
                    }
                } else if msg.target.starts_with("fleet/join") {
                    let agora_remote = msg.payload.get("agora_remote").and_then(|v| v.as_str()).unwrap_or("");
                    if !agora_remote.is_empty() {
                        if let Err(e) = self.join_fleet(agora_remote).await {
                            let reply = I2IMessage::reply(
                                &msg,
                                I2IVerb::Response,
                                serde_json::json!({ "status": "failed", "error": e.to_string() }),
                            );
                            Some(reply)
                        } else {
                            let reply = I2IMessage::reply(
                                &msg,
                                I2IVerb::Response,
                                serde_json::json!({ "status": "success", "message": "Joined fleet successfully" }),
                            );
                            Some(reply)
                        }
                    } else {
                        let reply = I2IMessage::reply(
                            &msg,
                            I2IVerb::Response,
                            serde_json::json!({ "status": "failed", "error": "Missing agora_remote parameter" }),
                        );
                        Some(reply)
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

/// The result of processing a command through the PLATO pipeline.
#[derive(Debug)]
pub struct ActionResult {
    pub command: String,
    pub tutor_context: Vec<String>,
    pub audit: constraint_engine::AuditOutcome,
    pub episode_id: String,
    /// Names of instinct reflexes that fired (if instinct context was provided).
    pub instinct_reflexes: Option<Vec<String>>,
    /// DCS Flywheel: deployment tier assigned to this action.
    pub deploy_tier: Option<Tier>,
    /// DCS Flywheel: belief score used for tier classification.
    pub belief_score: Option<BeliefScore>,
    /// DCS Flywheel: locks that triggered during execution.
    pub lock_checks: Option<Vec<LockCheck>>,
}

/// Context needed for instinct pre-check in process_command.
/// Mirrors the inputs to flux-instinct's InstinctEngine::tick().
#[derive(Debug, Clone)]
pub struct InstinctContext {
    pub engine: InstinctEngine,
    /// Current energy fraction [0.0, 1.0]. Use 0.15 for critical, 0.4 for low.
    pub energy: f32,
    /// Current threat level [0.0, 1.0]. Use 0.7 for high.
    pub threat: f32,
    /// Trust level for the requesting identity [0.0, 1.0].
    pub trust: f32,
    /// Whether the peer/agent is still alive.
    pub peer_alive: bool,
    /// Whether there is active work to guard.
    pub has_work: bool,
}

// --- Inline instinct types (mirrors flux-instinct API for zero-dep wiring) ---

/// Mirrors flux-instinct::InstinctType for zero-dependency integration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InstinctType {
    Survive = 0, Flee = 1, Guard = 2, Report = 3, Hoard = 4,
    Cooperate = 5, Teach = 6, Curious = 7, Mourn = 8, Evolve = 9, None = 99,
}

impl InstinctType {
    pub fn name(self) -> &'static str {
        match self {
            InstinctType::Survive => "survive", InstinctType::Flee => "flee",
            InstinctType::Guard => "guard", InstinctType::Report => "report",
            InstinctType::Hoard => "hoard", InstinctType::Cooperate => "cooperate",
            InstinctType::Teach => "teach", InstinctType::Curious => "curious",
            InstinctType::Mourn => "mourn", InstinctType::Evolve => "evolve",
            InstinctType::None => "none",
        }
    }
}

/// Mirrors flux-instinct::Reflex for zero-dependency integration.
#[derive(Debug, Clone)]
pub struct Reflex {
    pub instinct: InstinctType,
    pub urgency: f32,
    pub suppressed: bool,
}

impl Reflex {
    pub fn new(instinct: InstinctType, urgency: f32) -> Self {
        Self { instinct, urgency: urgency.clamp(0.0, 1.0), suppressed: false }
    }
}

/// Minimal instinct engine — mirrors flux-instinct's core logic.
/// For production use, swap this for a direct dependency on flux-instinct.
#[derive(Clone, Debug)]
pub struct InstinctEngine {
    energy_critical: f32,
    threat_high: f32,
}

impl InstinctEngine {
    pub fn new() -> Self { Self { energy_critical: 0.15, threat_high: 0.7 } }

    /// Run one instinct tick. Returns reflexes sorted by urgency descending.
    pub fn tick(&self, energy: f32, threat: f32, trust: f32, peer_alive: bool, _has_work: bool) -> Vec<Reflex> {
        let mut reflexes = Vec::new();
        if energy <= self.energy_critical {
            reflexes.push(Reflex::new(InstinctType::Survive, 1.0));
        }
        if threat > self.threat_high {
            let urgency = ((threat - self.threat_high) / (1.0 - self.threat_high)).clamp(0.0, 1.0);
            reflexes.push(Reflex::new(InstinctType::Flee, urgency));
        }
        if trust > 0.8 {
            reflexes.push(Reflex::new(InstinctType::Teach, 0.6));
        } else if trust > 0.6 {
            reflexes.push(Reflex::new(InstinctType::Cooperate, 0.5));
        }
        if threat > 0.3 && threat <= self.threat_high {
            reflexes.push(Reflex::new(InstinctType::Report, 0.4));
        }
        reflexes.sort_by(|a, b| b.urgency.partial_cmp(&a.urgency).unwrap());
        reflexes
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("plato_kernel=info".parse()?),
        )
        .init();

    tracing::info!("Plato Kernel starting...");

    let kernel = PlatoKernel::new().await?;
    tracing::info!(
        "Plato Kernel initialized. Instance: {}",
        kernel.instance_id
    );

    // Log mounted plugin inventory for ops visibility.
    let mut mounted: Vec<&str> = kernel.plugins.mounted_ids().collect();
    mounted.sort();
    tracing::info!("Mounted plugins [{}]: {:?}", mounted.len(), mounted);

    // Capability checks let subsystems query the plugin graph without
    // caring about feature flags directly.
    debug_assert!(kernel.plugins.provides("event-bus"),      "core-event-bus must be mounted");
    debug_assert!(kernel.plugins.provides("constraint-engine"), "core-constraint must be mounted");

    // Demonstrate tiling: parse a sample doc
    let sample_doc =
        "## PaymentFlow\nHandles [Settlement] requests.\n\n## Settlement\nClears funds.\n";
    let registry = TileRegistry::parse(sample_doc);
    tracing::info!("Tiling: {} tiles parsed from sample doc", registry.len());

    // Start the I2I TCP server on port 7272
    let i2i_handler = default_kernel_handler(kernel.instance_id.clone());
    let i2i_server = I2IServer::new(i2i_handler);
    tokio::spawn(async move {
        if let Err(e) = i2i_server.serve().await {
            tracing::error!("I2I server error: {}", e);
        }
    });
    tracing::info!("I2I server spawned on TCP 0.0.0.0:7272");

    // Example: Join a fleet (can also be triggered via I2I)
    // kernel.join_fleet("https://github.com/PlatoFleet/agora.git").await?;
    // let mut rt = kernel.git_runtime.lock().await;
    // let rooms = rt.list_fleet_rooms().await?;
    // tracing::info!("Fleet rooms: {:?}", rooms);

    // Keep the kernel running
    tokio::signal::ctrl_c().await?;
    tracing::info!("Plato Kernel shutting down.");

    Ok(())
}

#[cfg(test)]
mod flywheel_tests {
    use super::*;

    fn make_tile_registry() -> TileRegistry {
        TileRegistry::parse("## Constraint Theory\nSnap to Pythagorean coordinates.\n\n## Ghost Tiles\nDecay and resurrect forgotten knowledge.")
    }

    #[test]
    fn test_belief_score_composite() {
        let b = BeliefScore::new(0.9, 0.8, 0.7);
        let c = b.composite();
        assert!(c > 0.7 && c < 0.9);
    }

    #[test]
    fn test_belief_store_reinforce_undermine() {
        let mut store = BeliefStore::new();
        store.reinforce("agent-jc1", BeliefDimension::Trust, 1.0);
        let b = store.get("agent-jc1").unwrap();
        assert!(b.trust > 0.5);
        store.undermine("agent-jc1", BeliefDimension::Trust, 0.8);
        let b2 = store.get("agent-jc1").unwrap();
        assert!(b2.trust < b.trust);
    }

    #[test]
    fn test_deploy_policy_classify_live() {
        let policy = DeployPolicy::default();
        let d = policy.classify(0.9, 0.9, 0.9);
        assert_eq!(d.tier, Tier::Live);
        assert!(d.is_auto());
    }

    #[test]
    fn test_deploy_policy_classify_monitored() {
        let policy = DeployPolicy::default();
        let d = policy.classify(0.7, 0.7, 0.7);
        assert_eq!(d.tier, Tier::Monitored);
    }

    #[test]
    fn test_deploy_policy_classify_human_gated() {
        let policy = DeployPolicy::default();
        let d = policy.classify(0.2, 0.2, 0.2);
        assert_eq!(d.tier, Tier::HumanGated);
        assert!(d.requires_human);
    }

    #[test]
    fn test_deploy_policy_floor_blocks_low_confidence() {
        let policy = DeployPolicy::default();
        let d = policy.classify(0.1, 0.9, 0.9); // low confidence
        assert_eq!(d.tier, Tier::HumanGated);
        assert!(d.requires_human);
    }

    #[test]
    fn test_lock_accumulator_check() {
        let mut acc = LockAccumulator::new();
        acc.record_observation("delete", "BLOCK: never delete without backup", "safety");
        let checks = acc.check("delete the old records");
        assert_eq!(checks.len(), 1);
        assert!(checks[0].triggered);
    }

    #[test]
    fn test_lock_accumulator_no_trigger() {
        let mut acc = LockAccumulator::new();
        acc.record_observation("delete", "BLOCK: never delete without backup", "safety");
        let checks = acc.check("create new records");
        assert!(checks.is_empty());
    }

    #[test]
    fn test_flywheel_belief_to_deploy() {
        // High belief → Live tier
        let belief = BeliefScore::new(0.9, 0.9, 0.9);
        let policy = DeployPolicy::default();
        let decision = policy.classify(belief.confidence, belief.trust, belief.relevance);
        assert_eq!(decision.tier, Tier::Live);

        // Low belief → HumanGated
        let low_belief = BeliefScore::new(0.1, 0.2, 0.1);
        let decision2 = policy.classify(low_belief.confidence, low_belief.trust, low_belief.relevance);
        assert_eq!(decision2.tier, Tier::HumanGated);
    }

    #[test]
    fn test_flywheel_lock_blocks_command() {
        let mut acc = LockAccumulator::new();
        acc.record_observation("rm -rf", "BLOCK: never run rm -rf", "safety");
        let checks = acc.check("rm -rf /tmp/data");
        assert!(checks.iter().any(|c| c.enforcement.contains("BLOCK")));
    }

    #[test]
    fn test_flywheel_belief_decay() {
        let mut store = BeliefStore::with_decay(0.5);
        store.set("tile-x", BeliefScore::new(1.0, 1.0, 1.0));
        store.tick();
        let b = store.get("tile-x").unwrap();
        assert!(b.confidence < 1.0); // decayed toward 0.5
    }
}

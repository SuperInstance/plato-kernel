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

mod constraint_engine;
mod episode_recorder;
mod event_bus;
mod git_runtime;
mod i2i;
mod perspective;
mod plugin;
mod tiling;
mod tutor;

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use constraint_engine::{ConstraintAuditor, ConstraintEngine};
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
    ) -> ActionResult {
        tracing::debug!("process_command: {} in {} → {:?}", identity, room, command);

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

        // Step 3: Record episode
        let entry = EpisodeEntry::new(
            &format!("{} in {}", command, room),
            &format!("Identity {} issued: {}", identity, command),
            &format!("Audit: {:?}", audit),
            outcome,
        );
        if let Err(e) = self.episode_recorder.record(&entry) {
            tracing::warn!("Episode recorder error: {}", e);
        }

        ActionResult {
            command: command.to_string(),
            tutor_context,
            audit,
            episode_id: entry.id,
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

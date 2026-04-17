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
mod tiling;
mod tutor;

use anyhow::Result;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use constraint_engine::{ConstraintAuditor, ConstraintEngine};
use episode_recorder::{EpisodeEntry, EpisodeOutcome, EpisodeRecorder};
use i2i::{ComponentKind, I2IMessage, I2IVerb, InstanceId};
use tiling::TileRegistry;
use tutor::{jump_context, JumpResult};

/// The complete Plato Kernel runtime.
#[derive(Debug)]
pub struct PlatoKernel {
    event_bus: event_bus::EventBus,
    constraint_engine: ConstraintEngine,
    git_runtime: git_runtime::GitRuntime,
    perspective_manager: perspective::PerspectiveManager,
    episode_recorder: EpisodeRecorder,
    instance_id: InstanceId,
}

impl PlatoKernel {
    /// Create a new Plato Kernel instance.
    pub async fn new() -> Result<Self> {
        tracing::info!("Initializing Plato Kernel...");

        Ok(Self {
            event_bus: event_bus::EventBus::new(),
            constraint_engine: ConstraintEngine::new(),
            git_runtime: git_runtime::GitRuntime::new().await?,
            perspective_manager: perspective::PerspectiveManager::new(),
            episode_recorder: EpisodeRecorder::default_path(),
            instance_id: InstanceId::new(ComponentKind::Kernel, "plato-kernel", "localhost"),
        })
    }

    /// Connect an identity to a room (repo).
    pub async fn connect(
        &self,
        identity: &str,
        room: &str,
    ) -> Result<perspective::Session> {
        tracing::info!("Connecting {} to room {}", identity, room);

        let repo = self.git_runtime.checkout(room).await?;
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

    // Demonstrate tiling: parse a sample doc
    let sample_doc =
        "## PaymentFlow\nHandles [Settlement] requests.\n\n## Settlement\nClears funds.\n";
    let registry = TileRegistry::parse(sample_doc);
    tracing::info!("Tiling: {} tiles parsed from sample doc", registry.len());

    // Keep the kernel running
    tokio::signal::ctrl_c().await?;
    tracing::info!("Plato Kernel shutting down.");

    Ok(())
}

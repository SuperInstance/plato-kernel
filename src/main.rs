//! Plato Kernel - Core runtime for Plato-as-Common-UX
//! 
//! Combines:
//! - Event Sourcing Bus (async pub/sub with replay/DLQ)
//! - Constraint Engine (first-person perspective filtering)
//! - Git Runtime (repo-as-room, cocapn protocol)
//! - Perspective Manager (identity + constraints → what you see)

mod event_bus;
mod constraint_engine;
mod git_runtime;
mod perspective;

use anyhow::Result;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Debug, Clone)]
pub struct PlatoKernel {
    event_bus: event_bus::EventBus,
    constraint_engine: constraint_engine::ConstraintEngine,
    git_runtime: git_runtime::GitRuntime,
    perspective_manager: perspective::PerspectiveManager,
}

impl PlatoKernel {
    /// Create a new Plato Kernel instance
    pub async fn new() -> Result<Self> {
        tracing::info!("Initializing Plato Kernel...");
        
        Ok(Self {
            event_bus: event_bus::EventBus::new(),
            constraint_engine: constraint_engine::ConstraintEngine::new(),
            git_runtime: git_runtime::GitRuntime::new().await?,
            perspective_manager: perspective::PerspectiveManager::new(),
        })
    }

    /// Connect an identity to a room (repo)
    pub async fn connect(&self, identity: &str, room: &str) -> Result<perspective::Session> {
        tracing::info!("Connecting {} to room {}", identity, room);
        
        // 1. Checkout/fetch the room-repo
        let repo = self.git_runtime.checkout(room).await?;
        
        // 2. Load constraints for this identity
        let constraints = self.constraint_engine.load_constraints(&room, identity)?;
        
        // 3. Create first-person perspective
        let perspective = self.perspective_manager.create_perspective(identity, constraints);
        
        // 4. Subscribe to events for this identity/room
        let events = self.event_bus.subscribe(identity, room);
        
        Ok(perspective::Session::new(identity.to_string(), room.to_string(), perspective, events))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env()
            .add_directive("plato_kernel=info".parse()?))
        .init();
    
    tracing::info!("Plato Kernel starting...");
    
    let kernel = PlatoKernel::new().await?;
    tracing::info!("Plato Kernel initialized successfully");
    
    // Keep the kernel running
    tokio::signal::ctrl_c().await?;
    
    Ok(())
}

//! Event Sourcing Bus module
//! 
//! Handles async pub/sub communication with replay and DLQ support.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

/// Channel type for events
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelType {
    Tell,       // Direct message
    Channel,    // Room/channel broadcast
    Mission,    // Mission-related
    Constraint, // Constraint-related events
}

/// Event payload types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum EventPayload {
    Tell { message: String, urgency: Urgency },
    StateChange { field: String, old: String, new: String },
    SkillInvocation { skill: String, params: serde_json::Value },
    ConstraintViolation { constraint: String, attempted_action: String },
}

/// Urgency level for tells
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Urgency {
    Low,
    Normal,
    High,
    Critical,
}

/// Core Plato event structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatoEvent {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub sender: String,      // @casey or @agent-name
    pub recipient: String,   // @casey or @agent-name or "broadcast"
    pub channel: ChannelType,
    pub payload: EventPayload,
    pub context: EventContext,
}

/// Event context (what room/file/etc)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventContext {
    pub repo: String,
    pub branch: Option<String>,
    pub room: Option<String>,
}

/// Dead Letter Queue for failed/unhandled events
pub struct DeadLetterQueue {
    events: Arc<RwLock<Vec<PlatoEvent>>>,
}

impl DeadLetterQueue {
    pub fn new() -> Self {
        Self {
            events: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Add an event to the DLQ (when it can't be delivered)
    pub async fn push(&self, event: PlatoEvent) {
        let mut events = self.events.write().await;
        events.push(event);
    }

    /// Get all undelivered events for a recipient
    pub async fn get_for(&self, recipient: &str) -> Vec<PlatoEvent> {
        let events = self.events.read().await;
        events.iter()
            .filter(|e| e.recipient == recipient)
            .cloned()
            .collect()
    }

    /// Clear delivered events for a recipient
    pub async fn clear_for(&self, recipient: &str) {
        let mut events = self.events.write().await;
        events.retain(|e| e.recipient != recipient);
    }
}

/// Event Bus - pub/sub with DLQ support
pub struct EventBus {
    subscribers: Arc<RwLock<HashMap<String, broadcast::Sender<PlatoEvent>>>>,
    dlq: Arc<DeadLetterQueue>,
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            subscribers: Arc::new(RwLock::new(HashMap::new())),
            dlq: Arc::new(DeadLetterQueue::new()),
        }
    }

    /// Subscribe to events for an identity
    pub async fn subscribe(&self, identity: &str, _room: &str) -> broadcast::Receiver<PlatoEvent> {
        let mut subscribers = self.subscribers.write().await;
        
        // Create a new channel for this identity
        let (sender, receiver) = broadcast::channel(100);
        subscribers.insert(identity.to_string(), sender);
        
        receiver
    }

    /// Publish an event
    pub async fn publish(&self, event: PlatoEvent) {
        let subscribers = self.subscribers.read().await;
        
        // Send to recipient if subscribed
        if let Some(sender) = subscribers.get(&event.recipient) {
            if sender.send(event.clone()).is_err() {
                // Recipient not receiving, add to DLQ
                self.dlq.push(event).await;
            }
            return;
        }
        
        // Also send to broadcast subscribers
        for (identity, sender) in subscribers.iter() {
            if identity.starts_with("@") || identity == "broadcast" {
                let _ = sender.send(event.clone());
            }
        }
    }

    /// Get DLQ events for a recipient
    pub async fn get_dlq_for(&self, recipient: &str) -> Vec<PlatoEvent> {
        self.dlq.get_for(recipient).await
    }

    /// Clear DLQ for a recipient (they've handled them)
    pub async fn clear_dlq_for(&self, recipient: &str) {
        self.dlq.clear_for(recipient).await;
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for DeadLetterQueue {
    fn default() -> Self {
        Self::new()
    }
}

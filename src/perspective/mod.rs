//! Perspective Manager module
//! 
//! Creates first-person perspectives based on identity + constraints.
//! No omniscience - entities see only what permissions allow.

use crate::constraint_engine::ConstraintMatrix;
use tokio::sync::broadcast;

/// A first-person perspective in Plato
#[derive(Debug, Clone)]
pub struct Perspective {
    pub identity: String,
    pub room: String,
    pub visible_entities: Vec<String>,
    pub visible_exits: Vec<String>,
    pub constraints: Vec<String>,
    pub can_execute: Vec<String>,
}

/// A session connecting an identity to a room
#[derive(Debug)]
pub struct Session {
    pub identity: String,
    pub room: String,
    pub perspective: Perspective,
    pub event_receiver: broadcast::Receiver<crate::event_bus::PlatoEvent>,
}

impl Session {
    pub fn new(
        identity: String,
        room: String,
        perspective: Perspective,
        event_receiver: broadcast::Receiver<crate::event_bus::PlatoEvent>,
    ) -> Self {
        Self {
            identity,
            room,
            perspective,
            event_receiver,
        }
    }
}

/// Perspective Manager - creates and manages first-person views
pub struct PerspectiveManager {
    // In a real implementation, this would cache perspectives
}

impl PerspectiveManager {
    pub fn new() -> Self {
        Self {}
    }

    /// Create a first-person perspective from identity + constraints
    pub fn create_perspective(&self, identity: &str, constraints: ConstraintMatrix) -> Perspective {
        // Filter visible entities based on constraints
        let visible_entities = self.filter_by_constraint(&constraints, "see_entities");
        let visible_exits = self.filter_by_constraint(&constraints, "see_exits");
        let constraints_list: Vec<String> = constraints.constraints
            .iter()
            .filter(|c| c.enabled)
            .map(|c| c.id.clone())
            .collect();
        let can_execute = self.filter_executable(&constraints);

        Perspective {
            identity: identity.to_string(),
            room: constraints.room.clone(),
            visible_entities,
            visible_exits,
            constraints: constraints_list,
            can_execute,
        }
    }

    fn filter_by_constraint(&self, constraints: &ConstraintMatrix, _filter_type: &str) -> Vec<String> {
        // For now, return all entities/exits
        // Real implementation would check specific constraint flags
        vec![]
    }

    fn filter_executable(&self, constraints: &ConstraintMatrix) -> Vec<String> {
        constraints.constraints
            .iter()
            .filter(|c| c.enabled)
            .map(|c| c.id.clone())
            .collect()
    }
}

impl Default for PerspectiveManager {
    fn default() -> Self {
        Self::new()
    }
}

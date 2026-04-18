//! Constraint Theory Core
//! 
//! Provides the foundational types for first-person perspective filtering in
//! the PLATO stack.  The plato-kernel constraint engine builds on these
//! primitives; higher-level policy parsing lives in the kernel itself.
//! 
//! Includes fleet-wide constraint propagation for cross-room policy enforcement
//! across the entire Plato network.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The fundamental axiom of Constraint Theory: every entity perceives the
/// world through a lens shaped by its constraints.  No omniscience.
pub trait FirstPersonPerspective {
    /// Return the identity tag (e.g. `@casey`) for this perspective.
    fn identity(&self) -> &str;

    /// Return true iff this perspective is permitted to observe `resource`.
    fn can_observe(&self, resource: &str) -> bool;

    /// Return true iff this perspective is permitted to act on `resource`.
    fn can_act(&self, resource: &str) -> bool;
}

/// The polarity of a constraint rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConstraintPolarity {
    /// Explicitly permits the governed action.
    Allow,
    /// Explicitly forbids the governed action.
    Deny,
}

/// A primitive constraint triple: `(subject, polarity, resource)`.
///
/// Read as: *subject* **may/may-not** access *resource*.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintTriple {
    pub subject: String,
    pub polarity: ConstraintPolarity,
    pub resource: String,
}

impl ConstraintTriple {
    pub fn allow(subject: impl Into<String>, resource: impl Into<String>) -> Self {
        Self {
            subject: subject.into(),
            polarity: ConstraintPolarity::Allow,
            resource: resource.into(),
        }
    }

    pub fn deny(subject: impl Into<String>, resource: impl Into<String>) -> Self {
        Self {
            subject: subject.into(),
            polarity: ConstraintPolarity::Deny,
            resource: resource.into(),
        }
    }

    /// Evaluate this triple against an (`actor`, `resource`) pair.
    ///
    /// Returns `Some(polarity)` when the triple applies, `None` when it
    /// does not match the given actor/resource combination.
    pub fn evaluate(&self, actor: &str, resource: &str) -> Option<ConstraintPolarity> {
        if self.subject == actor && self.resource == resource {
            Some(self.polarity)
        } else {
            None
        }
    }
}

/// Fleet-level constraint policy that propagates across all rooms in the fleet
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetConstraintPolicy {
    /// Unique policy identifier
    pub id: String,
    /// Human-readable description
    pub description: String,
    /// Whether this policy is enforced globally across all rooms
    pub global: bool,
    /// Specific rooms this policy applies to (if not global)
    pub room_scope: Vec<String>,
    /// The constraint triples that make up this policy
    pub constraints: Vec<ConstraintTriple>,
    /// Priority (higher priority policies override lower ones)
    pub priority: u8,
}

/// A distributed constraint matrix that works across fleet rooms
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetConstraintMatrix {
    /// Local room constraints
    pub local_constraints: Vec<ConstraintTriple>,
    /// Fleet-wide policies that apply to this room
    pub fleet_policies: HashMap<String, FleetConstraintPolicy>,
    /// Cached evaluation of all applicable constraints
    cached_constraints: Vec<ConstraintTriple>,
}

impl FleetConstraintMatrix {
    /// Create a new fleet-aware constraint matrix
    pub fn new() -> Self {
        Self {
            local_constraints: Vec::new(),
            fleet_policies: HashMap::new(),
            cached_constraints: Vec::new(),
        }
    }

    /// Add a fleet policy to this matrix
    pub fn add_fleet_policy(&mut self, policy: FleetConstraintPolicy) {
        self.cached_constraints.extend(policy.constraints.clone());
        self.fleet_policies.insert(policy.id.clone(), policy);
    }

    /// Add a local constraint to this matrix
    pub fn add_local_constraint(&mut self, constraint: ConstraintTriple) {
        self.local_constraints.push(constraint.clone());
        self.cached_constraints.push(constraint);
    }

    /// Evaluate all constraints (local + fleet) for an actor/resource pair
    pub fn evaluate_all(&self, actor: &str, resource: &str) -> Vec<ConstraintPolarity> {
        self.cached_constraints
            .iter()
            .filter_map(|c| c.evaluate(actor, resource))
            .collect()
    }

    /// Check if an action is allowed across all applicable constraints
    pub fn is_allowed(&self, actor: &str, resource: &str) -> bool {
        let results = self.evaluate_all(actor, resource);
        // If any policy denies, the action is denied
        !results.contains(&ConstraintPolarity::Deny)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fleet_constraint_policy() {
        // Create a global fleet policy that denies guests from accessing production resources
        let policy = FleetConstraintPolicy {
            id: "guest-production-deny".to_string(),
            description: "Prevent guest accounts from accessing production systems".to_string(),
            global: true,
            room_scope: vec![],
            constraints: vec![
                ConstraintTriple::deny("@guest", "production/deploy")
            ],
            priority: 100,
        };

        let mut matrix = FleetConstraintMatrix::new();
        matrix.add_fleet_policy(policy);

        // Test that guest can't deploy to production
        assert!(!matrix.is_allowed("@guest", "production/deploy"));
        // Test that admin can deploy
        assert!(matrix.is_allowed("@admin", "production/deploy"));
    }

    #[test]
    fn test_local_plus_fleet_constraints() {
        let mut matrix = FleetConstraintMatrix::new();
        
        // Add fleet policy
        let fleet_policy = FleetConstraintPolicy {
            id: "global-secrets-deny".to_string(),
            description: "No regular users can access secrets".to_string(),
            global: true,
            room_scope: vec![],
            constraints: vec![ConstraintTriple::deny("@user", "core/secrets")],
            priority: 90,
        };
        matrix.add_fleet_policy(fleet_policy);

        // Add local exception for devops users
        matrix.add_local_constraint(ConstraintTriple::allow("@devops", "core/secrets"));

        // Regular user can't access
        assert!(!matrix.is_allowed("@user", "core/secrets"));
        // Devops can access
        assert!(matrix.is_allowed("@devops", "core/secrets"));
    }
}

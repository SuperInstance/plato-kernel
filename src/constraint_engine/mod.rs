//! Constraint Engine module
//! 
//! Implements Constraint-Theory's first-person perspective filtering.
//! No omniscience - entities see only what permissions allow.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A constraint that governs what an entity can see/do
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Constraint {
    pub id: String,
    pub description: String,
    pub enabled: bool,
    pub filter_type: FilterType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterType {
    /// This constraint allows the action
    Allow,
    /// This constraint denies the action
    Deny,
    /// This constraint requires approval from another entity
    RequestApproval,
}

/// Constraint matrix for an entity in a room
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintMatrix {
    pub identity: String,
    pub room: String,
    pub constraints: Vec<Constraint>,
}

/// Result of checking constraints
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstraintResult {
    /// Action is allowed
    Allow,
    /// Action is denied (violation is computed, not an error)
    Deny(ConstraintViolation),
    /// Action requires approval from another entity
    RequestApproval(ApprovalRequest),
}

/// Details of a constraint violation
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstraintViolation {
    pub constraint: String,
    pub attempted_action: String,
    pub reason: String,
}

/// Details of an approval request
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalRequest {
    pub constraint: String,
    pub attempted_action: String,
    pub approvers: Vec<String>,
}

/// Command to check against constraints
#[derive(Debug, Clone)]
pub struct Command {
    pub verb: String,
    pub target: String,
    pub args: Vec<String>,
}

impl Command {
    pub fn new(verb: &str, target: &str, args: Vec<&str>) -> Self {
        Self {
            verb: verb.to_string(),
            target: target.to_string(),
            args: args.iter().map(|s| s.to_string()).collect(),
        }
    }

    pub fn from_string(input: &str) -> Self {
        let parts: Vec<&str> = input.split_whitespace().collect();
        if parts.is_empty() {
            return Self {
                verb: String::new(),
                target: String::new(),
                args: vec![],
            };
        }
        
        let verb = parts[0].to_string();
        let target = parts.get(1).map(|s| s.to_string()).unwrap_or_default();
        let args = parts[2..].iter().map(|s| s.to_string()).collect();
        
        Self { verb, target, args }
    }
}

/// Constraint Engine - checks commands against first-person permissions
pub struct ConstraintEngine {
    matrices: HashMap<(String, String), ConstraintMatrix>, // (identity, room) -> matrix
}

impl ConstraintEngine {
    pub fn new() -> Self {
        Self {
            matrices: HashMap::new(),
        }
    }

    /// Load constraints for an identity in a room (from repo's .plato/CONSTRAINTS.yaml)
    pub fn load_constraints(&self, room_name: &str, identity: &str) -> Result<ConstraintMatrix, anyhow::Error> {
        // For now, return a default matrix
        // Real implementation would read from repo's .plato/CONSTRAINTS.yaml
        Ok(ConstraintMatrix {
            identity: identity.to_string(),
            room: room_name.to_string(),
            constraints: vec![
                Constraint {
                    id: "view_room".to_string(),
                    description: "Can view room description".to_string(),
                    enabled: true,
                    filter_type: FilterType::Allow,
                },
                Constraint {
                    id: "send_tell".to_string(),
                    description: "Can send tells to other entities".to_string(),
                    enabled: true,
                    filter_type: FilterType::Allow,
                },
                Constraint {
                    id: "admin_commands".to_string(),
                    description: "Can execute admin commands".to_string(),
                    enabled: false,
                    filter_type: FilterType::Deny,
                },
            ],
        })
    }

    /// Check a command against the constraint matrix
    pub fn check(&self, matrix: &ConstraintMatrix, command: &Command) -> ConstraintResult {
        // Find relevant constraints for this command
        for constraint in &matrix.constraints {
            if !constraint.enabled {
                continue;
            }

            // Match constraint to command type
            let matches = match constraint.id.as_str() {
                "view_room" => command.verb == "look" || command.verb == "examine",
                "send_tell" => command.verb == "tell" || command.verb == "page",
                "admin_commands" => command.verb.starts_with("@") || command.verb == "delete" || command.verb == "create",
                _ => false,
            };

            if matches {
                match constraint.filter_type {
                    FilterType::Allow => return ConstraintResult::Allow,
                    FilterType::Deny => {
                        return ConstraintResult::Deny(ConstraintViolation {
                            constraint: constraint.id.clone(),
                            attempted_action: format!("{} {}", command.verb, command.target),
                            reason: constraint.description.clone(),
                        });
                    }
                    FilterType::RequestApproval => {
                        return ConstraintResult::RequestApproval(ApprovalRequest {
                            constraint: constraint.id.clone(),
                            attempted_action: format!("{} {}", command.verb, command.target),
                            approvers: vec!["@admin".to_string()],
                        });
                    }
                }
            }
        }

        // Default: allow
        ConstraintResult::Allow
    }

    /// Add a constraint to a matrix
    pub fn add_constraint(&mut self, mut matrix: ConstraintMatrix, constraint: Constraint) -> ConstraintMatrix {
        matrix.constraints.push(constraint);
        matrix
    }
}

impl Default for ConstraintEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_constraint_check() {
        let engine = ConstraintEngine::new();
        
        let matrix = ConstraintMatrix {
            identity: "@test".to_string(),
            room: "test-room".to_string(),
            constraints: vec![
                Constraint {
                    id: "send_tell".to_string(),
                    description: "Can send tells".to_string(),
                    enabled: true,
                    filter_type: FilterType::Allow,
                },
            ],
        };
        
        let cmd = Command::new("tell", "@other", vec!["Hello"]);
        let result = engine.check(&matrix, &cmd);
        
        assert_eq!(result, ConstraintResult::Allow);
    }
}

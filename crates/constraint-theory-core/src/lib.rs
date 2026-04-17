//! Constraint Theory Core
//!
//! Provides the foundational types for first-person perspective filtering in
//! the PLATO stack.  The plato-kernel constraint engine builds on these
//! primitives; higher-level policy parsing lives in the kernel itself.

use serde::{Deserialize, Serialize};

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

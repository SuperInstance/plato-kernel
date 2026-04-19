//! plato-kernel inline dynamic-locks module — extracted from plato-dynamic-locks
//! Runtime lock accumulation from experience. Extends static gates with dynamic locks.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LockSource {
    Inconsistency,
    Observation,
    CrossModel,
    Expert,
    Inferred,
}

impl LockSource {
    pub fn base_trust(&self) -> f32 {
        match self { LockSource::Expert => 1.0, LockSource::Inconsistency => 0.8, LockSource::Observation => 0.7, LockSource::CrossModel => 0.6, LockSource::Inferred => 0.4 }
    }
}

#[derive(Debug, Clone)]
pub struct Lock {
    pub id: u64,
    pub description: String,
    pub trigger_pattern: String,
    pub enforcement: String,
    pub source: LockSource,
    pub strength: f32,
    pub verifications: u32,
    pub violations: u32,
    pub category: String,
}

impl Lock {
    pub fn new(description: &str, trigger: &str, enforcement: &str, source: LockSource) -> Self {
        Self { id: nanos_now(), description: description.to_string(), trigger_pattern: trigger.to_string(),
            enforcement: enforcement.to_string(), source, strength: source.base_trust(),
            verifications: 0, violations: 0, category: String::new() }
    }
    pub fn with_category(mut self, cat: &str) -> Self { self.category = cat.to_string(); self }
    pub fn verify(&mut self) { self.verifications += 1; self.strength = (self.strength + 0.05).min(1.0); }
    pub fn violate(&mut self) { self.violations += 1; self.strength = (self.strength - 0.15).max(0.0); }
    pub fn is_active(&self, min_strength: f32) -> bool { self.strength >= min_strength }
    pub fn confidence(&self) -> f32 {
        if self.verifications == 0 && self.violations == 0 { return self.source.base_trust(); }
        let ratio = self.verifications as f32 / (self.verifications + self.violations) as f32;
        let evidence = (self.verifications + self.violations) as f32;
        let blend = evidence / (evidence + 5.0);
        ratio * blend + self.source.base_trust() * (1.0 - blend)
    }
    pub fn effective_strength(&self) -> f32 { self.strength * self.confidence() }
}

#[derive(Debug, Clone)]
pub struct LockCheck {
    pub lock_id: u64,
    pub triggered: bool,
    pub description: String,
    pub enforcement: String,
    pub effective_strength: f32,
}

pub struct LockAccumulator {
    locks: HashMap<u64, Lock>,
    min_strength: f32,
}

impl LockAccumulator {
    pub fn new() -> Self { Self { locks: HashMap::new(), min_strength: 0.1 } }
    pub fn with_min_strength(min: f32) -> Self { Self { locks: HashMap::new(), min_strength: min } }
    pub fn add(&mut self, lock: Lock) -> u64 { let id = lock.id; self.locks.insert(id, lock); id }
    pub fn check(&self, input: &str) -> Vec<LockCheck> {
        let mut checks = Vec::new();
        for lock in self.locks.values() {
            if !lock.is_active(self.min_strength) { continue; }
            let triggered = input.contains(&lock.trigger_pattern) || lock.trigger_pattern.contains(input);
            if triggered {
                checks.push(LockCheck { lock_id: lock.id, triggered: true, description: lock.description.clone(),
                    enforcement: lock.enforcement.clone(), effective_strength: lock.effective_strength() });
            }
        }
        checks.sort_by(|a, b| b.effective_strength.partial_cmp(&a.effective_strength).unwrap_or(std::cmp::Ordering::Equal));
        checks
    }
    pub fn record_observation(&mut self, trigger: &str, enforcement: &str, category: &str) -> u64 {
        let desc = format!("Observation: {}", if enforcement.len() > 80 { &enforcement[..80] } else { enforcement });
        self.add(Lock::new(&desc, trigger, enforcement, LockSource::Observation).with_category(category))
    }
    pub fn active_locks(&self) -> Vec<&Lock> { self.locks.values().filter(|l| l.is_active(self.min_strength)).collect() }
    pub fn len(&self) -> usize { self.locks.len() }
    pub fn is_empty(&self) -> bool { self.locks.is_empty() }
}

impl Default for LockAccumulator {
    fn default() -> Self { Self::new() }
}

fn nanos_now() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0)
}

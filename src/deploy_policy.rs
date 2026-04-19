//! plato-kernel inline deploy-policy module — extracted from plato-deploy-policy
//! Three-tier deployment: Live, Monitored, HumanGated based on belief scores.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tier {
    Live,
    Monitored,
    HumanGated,
}

impl Tier {
    pub fn name(&self) -> &'static str {
        match self { Tier::Live => "live", Tier::Monitored => "monitored", Tier::HumanGated => "human-gated" }
    }
    pub fn risk_level(&self) -> u8 {
        match self { Tier::Live => 1, Tier::Monitored => 2, Tier::HumanGated => 3 }
    }
}

#[derive(Debug, Clone)]
pub struct DeployDecision {
    pub tier: Tier,
    pub confidence: f32,
    pub trust: f32,
    pub relevance: f32,
    pub reason: String,
    pub requires_human: bool,
    pub rollout_pct: u8,
}

impl DeployDecision {
    pub fn is_auto(&self) -> bool { !self.requires_human }
}

#[derive(Debug, Clone)]
pub struct DeployPolicy {
    pub live_threshold: f32,
    pub human_threshold: f32,
    pub monitored_start_pct: u8,
    pub monitored_increment: u8,
    pub absolute_min_confidence: f32,
    pub absolute_min_trust: f32,
}

impl Default for DeployPolicy {
    fn default() -> Self {
        Self { live_threshold: 0.8, human_threshold: 0.5, monitored_start_pct: 5, monitored_increment: 10,
            absolute_min_confidence: 0.3, absolute_min_trust: 0.3 }
    }
}

impl DeployPolicy {
    pub fn new(live: f32, human: f32) -> Self {
        Self { live_threshold: live, human_threshold: human, ..Default::default() }
    }
    pub fn classify(&self, confidence: f32, trust: f32, relevance: f32) -> DeployDecision {
        let composite = (confidence * trust * relevance).powf(1.0 / 3.0);
        if confidence < self.absolute_min_confidence {
            return DeployDecision { tier: Tier::HumanGated, confidence, trust, relevance,
                reason: format!("confidence {:.2} below floor", confidence), requires_human: true, rollout_pct: 0 };
        }
        if trust < self.absolute_min_trust {
            return DeployDecision { tier: Tier::HumanGated, confidence, trust, relevance,
                reason: format!("trust {:.2} below floor", trust), requires_human: true, rollout_pct: 0 };
        }
        if composite >= self.live_threshold {
            DeployDecision { tier: Tier::Live, confidence, trust, relevance,
                reason: format!("composite {:.3} >= live {:.1}", composite, self.live_threshold),
                requires_human: false, rollout_pct: 100 }
        } else if composite >= self.human_threshold {
            DeployDecision { tier: Tier::Monitored, confidence, trust, relevance,
                reason: format!("composite {:.3} monitored [{:.1}, {:.1})", composite, self.human_threshold, self.live_threshold),
                requires_human: false, rollout_pct: self.monitored_start_pct }
        } else {
            DeployDecision { tier: Tier::HumanGated, confidence, trust, relevance,
                reason: format!("composite {:.3} < human {:.1}", composite, self.human_threshold),
                requires_human: true, rollout_pct: 0 }
        }
    }
}

pub struct DeployLedger {
    records: HashMap<u64, DeployRecord>,
    next_id: u64,
    policy: DeployPolicy,
}

#[derive(Debug, Clone)]
pub struct DeployRecord {
    pub id: u64,
    pub tier: Tier,
    pub rollout_pct: u8,
    pub ticks_observed: u32,
    pub rolled_back: bool,
}

impl DeployLedger {
    pub fn new(policy: DeployPolicy) -> Self {
        Self { records: HashMap::new(), next_id: 1, policy }
    }
    pub fn submit(&mut self, confidence: f32, trust: f32, relevance: f32) -> (u64, DeployDecision) {
        let decision = self.policy.classify(confidence, trust, relevance);
        let id = self.next_id;
        self.next_id += 1;
        self.records.insert(id, DeployRecord { id, tier: decision.tier, rollout_pct: decision.rollout_pct,
            ticks_observed: 0, rolled_back: false });
        (id, decision)
    }
    pub fn rollback(&mut self, id: u64) -> bool {
        if let Some(r) = self.records.get_mut(&id) { r.rolled_back = true; true } else { false }
    }
    pub fn active_count(&self) -> usize { self.records.values().filter(|r| !r.rolled_back).count() }
}

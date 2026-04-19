//! plato-kernel inline belief module — extracted from plato-unified-belief
//! Three Bayesian dimensions unified: Confidence, Trust, Relevance.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BeliefDimension {
    Confidence,
    Trust,
    Relevance,
}

impl BeliefDimension {
    pub fn all() -> &'static [BeliefDimension] {
        &[BeliefDimension::Confidence, BeliefDimension::Trust, BeliefDimension::Relevance]
    }
    pub fn name(&self) -> &'static str {
        match self {
            BeliefDimension::Confidence => "confidence",
            BeliefDimension::Trust => "trust",
            BeliefDimension::Relevance => "relevance",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BeliefScore {
    pub confidence: f32,
    pub trust: f32,
    pub relevance: f32,
}

impl Default for BeliefScore {
    fn default() -> Self { Self { confidence: 0.5, trust: 0.5, relevance: 0.5 } }
}

impl BeliefScore {
    pub fn new(confidence: f32, trust: f32, relevance: f32) -> Self {
        Self { confidence: confidence.max(0.0).min(1.0), trust: trust.max(0.0).min(1.0), relevance: relevance.max(0.0).min(1.0) }
    }
    pub fn get(&self, dim: BeliefDimension) -> f32 {
        match dim { BeliefDimension::Confidence => self.confidence, BeliefDimension::Trust => self.trust, BeliefDimension::Relevance => self.relevance }
    }
    pub fn set(&mut self, dim: BeliefDimension, value: f32) {
        let v = value.max(0.0).min(1.0);
        match dim { BeliefDimension::Confidence => self.confidence = v, BeliefDimension::Trust => self.trust = v, BeliefDimension::Relevance => self.relevance = v }
    }
    pub fn positive_evidence(&mut self, dim: BeliefDimension, strength: f32) {
        let current = self.get(dim);
        self.set(dim, (current * 4.0 + strength) / 5.0);
    }
    pub fn negative_evidence(&mut self, dim: BeliefDimension, strength: f32) {
        let current = self.get(dim);
        self.set(dim, (current * 4.0 - strength) / 5.0);
    }
    pub fn decay(&mut self, decay_rate: f32) {
        let d = |v: f32| v + (0.5 - v) * decay_rate;
        self.confidence = d(self.confidence);
        self.trust = d(self.trust);
        self.relevance = d(self.relevance);
    }
    pub fn composite(&self) -> f32 {
        (self.confidence * self.trust * self.relevance).powf(1.0 / 3.0)
    }
    pub fn actionable(&self, min_c: f32, min_t: f32, min_r: f32) -> bool {
        self.confidence >= min_c && self.trust >= min_t && self.relevance >= min_r
    }
}

pub struct BeliefStore {
    beliefs: HashMap<String, BeliefScore>,
    decay_per_tick: f32,
}

impl BeliefStore {
    pub fn new() -> Self { Self { beliefs: HashMap::new(), decay_per_tick: 0.02 } }
    pub fn with_decay(decay_per_tick: f32) -> Self { Self { beliefs: HashMap::new(), decay_per_tick } }
    pub fn set(&mut self, key: &str, score: BeliefScore) { self.beliefs.insert(key.to_string(), score); }
    pub fn get(&self, key: &str) -> Option<BeliefScore> { self.beliefs.get(key).copied() }
    pub fn reinforce(&mut self, key: &str, dim: BeliefDimension, strength: f32) {
        self.beliefs.entry(key.to_string()).or_insert_with(BeliefScore::default).positive_evidence(dim, strength);
    }
    pub fn undermine(&mut self, key: &str, dim: BeliefDimension, strength: f32) {
        self.beliefs.entry(key.to_string()).or_insert_with(BeliefScore::default).negative_evidence(dim, strength);
    }
    pub fn tick(&mut self) {
        for score in self.beliefs.values_mut() { score.decay(self.decay_per_tick); }
    }
    pub fn len(&self) -> usize { self.beliefs.len() }
    pub fn is_empty(&self) -> bool { self.beliefs.is_empty() }
}

impl Default for BeliefStore {
    fn default() -> Self { Self::new() }
}

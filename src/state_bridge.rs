//! State Bridge — Deterministic ↔ Generative dual-state engine
//!
//! From ct-lab's Plato-First Runtime research: the core PLATO innovation is
//! running two states in parallel — a deterministic FSM (TUTOR, constraints,
//! episode recorder) and a generative LLM (probabilistic, contextual synthesis).
//! The StateBridge trait provides bidirectional translation maintaining coherence.
//!
//! Why: JC1's ct-lab research document `deep-plato-first-runtime.md` describes
//! this as the fundamental PLATO architecture. plato-kernel already implements
//! it implicitly in `process_command()`. This trait makes the pattern explicit,
//! swappable, and testable — a hull bolt in the fleet architecture.

/// Which state produced this output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateSource {
    /// Deterministic FSM — TUTOR jumps, constraint checks, instinct reflexes.
    Deterministic,
    /// Generative LLM — probabilistic synthesis, emergent behavior.
    Generative,
    /// Both states contributed (coherence required).
    Hybrid,
}

/// A bridged result from either state, annotated with provenance.
#[derive(Debug, Clone)]
pub struct BridgedResult {
    /// The actual content.
    pub content: String,
    /// Which state produced this.
    pub source: StateSource,
    /// Confidence from the producing state (0.0 – 1.0).
    pub confidence: f64,
    /// Coherence score when Hybrid — how well deterministic and generative agree.
    /// None when source is Deterministic or Generative alone.
    pub coherence: Option<f64>,
}

impl BridgedResult {
    pub fn deterministic(content: impl Into<String>, confidence: f64) -> Self {
        Self { content: content.into(), source: StateSource::Deterministic, confidence, coherence: None }
    }

    pub fn generative(content: impl Into<String>, confidence: f64) -> Self {
        Self { content: content.into(), source: StateSource::Generative, confidence, coherence: None }
    }

    pub fn hybrid(content: impl Into<String>, confidence: f64, coherence: f64) -> Self {
        Self { content: content.into(), source: StateSource::Hybrid, confidence, coherence: Some(coherence) }
    }

    /// Whether the result is actionable (confidence above threshold).
    pub fn is_actionable(&self, threshold: f64) -> bool {
        self.confidence >= threshold
    }
}

/// The StateBridge trait — bidirectional translation between deterministic
/// and generative states.
///
/// Implementors translate between the two states:
/// - `to_generative_prompt`: deterministic result → prompt for LLM
/// - `from_generative_output`: LLM output → deterministic result
/// - `check_coherence`: measure agreement between states
pub trait StateBridge {
    /// Translate a deterministic result into a prompt for the generative state.
    /// This is how TUTOR anchors, constraint checks, and instinct reflexes
    /// feed into the LLM context.
    fn to_generative_prompt(&self, deterministic: &BridgedResult) -> String;

    /// Translate a generative (LLM) output back into a deterministic result.
    /// This is how LLM responses get scored, tagged, and stored as tiles.
    fn from_generative_output(&self, raw_output: &str, context: &str) -> BridgedResult;

    /// Check coherence between deterministic and generative results.
    /// Returns 0.0 (contradictory) to 1.0 (perfectly aligned).
    /// Used to detect hallucination, drift, or constraint violations.
    fn check_coherence(&self, deterministic: &BridgedResult, generative: &BridgedResult) -> f64;

    /// Run deadband safety check on an action string.
    /// Default impl always passes (override in DefaultStateBridge).
    fn check_deadband(&self, _action: &str) -> crate::deadband::DeadbandCheck {
        crate::deadband::DeadbandCheck {
            passed: true,
            p0_clear: true,
            p1_clear: true,
            violations: vec![],
            recommended_channel: None,
        }
    }

    /// Score a slice of tiles against a query.
    /// tiles: (question, answer, domain, confidence, ghost_score, use_count)
    fn score_tiles(&self, tiles: &[(&str, &str, &str, f64, f64, u32)], query: &str) -> Vec<crate::tile_scoring::TileScore> {
        tiles.iter().enumerate().map(|(i, (q, a, d, conf, ghost, uses))| {
            crate::tile_scoring::score_tile(i, query, q, a, &[], d, *conf, *ghost, *uses)
        }).collect()
    }
}

/// Default StateBridge implementation — keyword overlap + constraint match.
///
/// Coherence is measured as:
/// 1. Keyword overlap between deterministic and generative content
/// 2. Whether the generative output mentions key concepts from the deterministic state
///
/// This is a baseline — smarter bridges (embedding-based, constraint-aware) can
/// be swapped in via the trait.
pub struct DefaultStateBridge {
    /// Minimum coherence threshold for Hybrid results.
    coherence_threshold: f64,
    /// Deadband engine for safety pre-checks.
    pub deadband: crate::deadband::DeadbandEngine,
}

impl DefaultStateBridge {
    pub fn new() -> Self {
        Self {
            coherence_threshold: 0.3,
            deadband: crate::deadband::DeadbandEngine::new(),
        }
    }

    pub fn with_threshold(mut self, threshold: f64) -> Self {
        self.coherence_threshold = threshold;
        self
    }

    /// Extract significant words from text (3+ chars, lowercase).
    fn significant_words(&self, text: &str) -> Vec<String> {
        text.split_whitespace()
            .map(|w| w.to_lowercase())
            .filter(|w| w.len() >= 3)
            .collect()
    }
}

impl Default for DefaultStateBridge {
    fn default() -> Self { Self::new() }
}

impl StateBridge for DefaultStateBridge {
    fn to_generative_prompt(&self, deterministic: &BridgedResult) -> String {
        match deterministic.source {
            StateSource::Deterministic => {
                format!(
                    "[DETERMINISTIC CONTEXT]\n{}\n[CONFIDENCE: {:.2}]\n\nGenerate a response incorporating the above constraints.",
                    deterministic.content, deterministic.confidence
                )
            }
            StateSource::Generative => deterministic.content.clone(),
            StateSource::Hybrid => deterministic.content.clone(),
        }
    }

    fn from_generative_output(&self, raw_output: &str, context: &str) -> BridgedResult {
        // Default: use keyword overlap with context as confidence proxy
        let gen_words = self.significant_words(raw_output);
        let ctx_words = self.significant_words(context);
        if gen_words.is_empty() || ctx_words.is_empty() {
            return BridgedResult::generative(raw_output, 0.3);
        }
        let overlap = gen_words.iter()
            .filter(|w| ctx_words.contains(w))
            .count();
        let confidence = (overlap as f64 / gen_words.len() as f64).min(1.0).max(0.1);
        BridgedResult::generative(raw_output, confidence)
    }

    fn check_coherence(&self, deterministic: &BridgedResult, generative: &BridgedResult) -> f64 {
        let det_words = self.significant_words(&deterministic.content);
        let gen_words = self.significant_words(&generative.content);
        if det_words.is_empty() || gen_words.is_empty() {
            return 0.0;
        }
        let overlap = det_words.iter()
            .filter(|w| gen_words.contains(w))
            .count();
        // Jaccard-like: intersection / union
        let union: std::collections::HashSet<String> =
            det_words.iter().chain(gen_words.iter()).cloned().collect();
        overlap as f64 / union.len() as f64
    }

    fn check_deadband(&self, action: &str) -> crate::deadband::DeadbandCheck {
        self.deadband.check(action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bridged_result_deterministic() {
        let r = BridgedResult::deterministic("constraint check passed", 0.9);
        assert_eq!(r.source, StateSource::Deterministic);
        assert!(r.is_actionable(0.5));
        assert!(r.coherence.is_none());
    }

    #[test]
    fn test_bridged_result_generative() {
        let r = BridgedResult::generative("LLM response", 0.7);
        assert_eq!(r.source, StateSource::Generative);
        assert!(r.is_actionable(0.5));
        assert!(!r.is_actionable(0.8));
    }

    #[test]
    fn test_bridged_result_hybrid() {
        let r = BridgedResult::hybrid("combined", 0.85, 0.9);
        assert_eq!(r.source, StateSource::Hybrid);
        assert_eq!(r.coherence, Some(0.9));
    }

    #[test]
    fn test_default_bridge_to_prompt() {
        let bridge = DefaultStateBridge::new();
        let det = BridgedResult::deterministic("TUTOR: use constraint snapping", 0.95);
        let prompt = bridge.to_generative_prompt(&det);
        assert!(prompt.contains("DETERMINISTIC CONTEXT"));
        assert!(prompt.contains("constraint snapping"));
        assert!(prompt.contains("0.95"));
    }

    #[test]
    fn test_default_bridge_from_output() {
        let bridge = DefaultStateBridge::new();
        let result = bridge.from_generative_output(
            "Use Pythagorean snapping for exact coordinates",
            "constraint theory Pythagorean manifold"
        );
        assert!(result.confidence >= 0.1); // keyword overlap measured
        assert_eq!(result.source, StateSource::Generative);
    }

    #[test]
    fn test_default_bridge_coherence_high() {
        let bridge = DefaultStateBridge::new();
        let det = BridgedResult::deterministic("Use constraint snapping for coordinates", 0.9);
        let gen = BridgedResult::generative("Apply constraint snapping to achieve exact coordinates", 0.8);
        let coherence = bridge.check_coherence(&det, &gen);
        assert!(coherence > 0.3); // high word overlap
    }

    #[test]
    fn test_default_bridge_coherence_low() {
        let bridge = DefaultStateBridge::new();
        let det = BridgedResult::deterministic("Use constraint snapping for coordinates", 0.9);
        let gen = BridgedResult::generative("The weather is nice today and birds are singing", 0.7);
        let coherence = bridge.check_coherence(&det, &gen);
        assert!(coherence < 0.3); // minimal overlap
    }

    #[test]
    fn test_default_bridge_coherence_empty() {
        let bridge = DefaultStateBridge::new();
        let det = BridgedResult::deterministic("", 0.5);
        let gen = BridgedResult::generative("content", 0.5);
        assert_eq!(bridge.check_coherence(&det, &gen), 0.0);
    }

    #[test]
    fn test_state_source_equality() {
        assert_eq!(StateSource::Deterministic, StateSource::Deterministic);
        assert_ne!(StateSource::Deterministic, StateSource::Generative);
    }
}

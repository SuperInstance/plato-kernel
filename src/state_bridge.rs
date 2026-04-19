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

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::deadband::{DeadbandCheck, DeadbandEngine};
use crate::tile_scoring::{rank_tiles, TileScore};

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Validity window for tiles before they enter grace period (1 day).
const DEFAULT_VALID_SECS: u64 = 86_400;

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

/// Lightweight tile descriptor for StateBridge scoring calls.
pub struct TileInput {
    pub tile_id: usize,
    pub question: String,
    pub answer: String,
    pub tags: Vec<String>,
    pub domain: String,
    pub confidence: f64,
    pub ghost_score: f64,
    pub use_count: u32,
}

/// Richer tile descriptor for score_tiles_v2 — adds temporal validity, ghost
/// resurrection, and controversy survival signals over the v1 TileInput.
pub struct TileInputV2 {
    pub tile_id: usize,
    pub question: String,
    pub answer: String,
    pub tags: Vec<String>,
    pub domain: String,
    pub confidence: f64,
    pub ghost_score: f64,
    pub use_count: u32,
    /// Unix timestamp (seconds) when the tile was first created.
    pub created_at_secs: u64,
    /// Unix timestamp (seconds) of the last refresh — resets the validity window.
    pub refreshed_at_secs: u64,
    /// Extra seconds of grace after the 1-day validity window before Expired.
    pub grace_period_secs: u64,
    /// How many times this tile has been challenged by other agents.
    pub challenge_count: u32,
    /// 0.0 = uncontested, 1.0 = highly contested. High score + high confidence
    /// indicates the tile survived scrutiny → gets a survival bonus.
    pub controversy_score: f64,
}

/// Temporal lifecycle status of a tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemporalStatus {
    /// Within the 1-day validity window.
    Valid,
    /// Past validity window but within grace_period_secs.
    Grace,
    /// Past both validity window and grace period.
    Expired,
}

/// 7-signal tile score returned by score_tiles_v2.
#[derive(Debug, Clone)]
pub struct TileScoreV2 {
    pub tile_id: usize,
    /// Composite weighted score (0.0 – 1.0).
    pub score: f64,
    pub keyword: f64,
    pub ghost: f64,
    pub belief: f64,
    pub domain: f64,
    pub frequency: f64,
    /// Temporal freshness signal (1.0 = just refreshed, 0.05 = expired).
    pub temporal: f64,
    /// Controversy survival bonus (high controversy + high confidence).
    pub controversy: f64,
}

/// Result of the tile validation pipeline.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// True iff all mandatory gates passed.
    pub passed: bool,
    /// Names of gates that failed (empty when passed).
    pub gates_failed: Vec<String>,
    /// True when stored in degraded mode (validation failed but tile is indexed).
    pub degraded: bool,
}

/// Result of a DCS consensus check on a high-impact action.
#[derive(Debug, Clone)]
pub struct ConsensusResult {
    /// True iff consensus score meets the threshold.
    pub consensus_reached: bool,
    /// The confidence score used as consensus proxy.
    pub score: f64,
    /// True iff the action was escalated to P1 (requires human review).
    pub escalated_to_p1: bool,
}

/// The StateBridge trait — bidirectional translation between deterministic
/// and generative states.
///
/// Implementors translate between the two states:
/// - `to_generative_prompt`: deterministic result → prompt for LLM
/// - `from_generative_output`: LLM output → deterministic result
/// - `check_coherence`: measure agreement between states
/// - `check_deadband`: safety pre-flight gate (default: always pass)
/// - `score_tiles`: score and rank tiles against a query
pub trait StateBridge {
    /// Translate a deterministic result into a prompt for the generative state.
    fn to_generative_prompt(&self, deterministic: &BridgedResult) -> String;

    /// Translate a generative (LLM) output back into a deterministic result.
    fn from_generative_output(&self, raw_output: &str, context: &str) -> BridgedResult;

    /// Check coherence between deterministic and generative results.
    /// Returns 0.0 (contradictory) to 1.0 (perfectly aligned).
    fn check_coherence(&self, deterministic: &BridgedResult, generative: &BridgedResult) -> f64;

    /// Run the deadband safety pre-flight check on an action string.
    ///
    /// Default implementation always passes — `DefaultStateBridge` overrides this
    /// with real P0/P1 pattern matching.
    fn check_deadband(&self, _action: &str) -> DeadbandCheck {
        DeadbandCheck {
            passed: true,
            p0_clear: true,
            p1_clear: true,
            violations: vec![],
            recommended_channel: None,
        }
    }

    /// Score and rank a slice of tile inputs against a query string.
    ///
    /// Returns tiles sorted by descending relevance, limited to `limit` results.
    fn score_tiles(&self, tiles: &[TileInput], query: &str, limit: usize) -> Vec<TileScore> {
        let scores: Vec<TileScore> = tiles.iter()
            .map(|t| crate::tile_scoring::score_tile(
                t.tile_id, query, &t.question, &t.answer,
                &t.tags, &t.domain, t.confidence, t.ghost_score, t.use_count,
            ))
            .collect();
        rank_tiles(scores, limit)
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
    /// Deadband safety engine — seeded with default P0/P1 patterns.
    pub deadband: DeadbandEngine,
    /// Dependency graph: upstream tile_id → list of downstream tile_ids.
    tile_deps: HashMap<usize, Vec<usize>>,
    /// Tiles marked as needing refresh after propagate_invalidation().
    invalidated: HashSet<usize>,
    /// Minimum confidence score for DCS consensus to be reached.
    consensus_threshold: f64,
    /// Injectable wall-clock override for temporal scoring (0 = use system time).
    current_time_secs: u64,
}

impl DefaultStateBridge {
    pub fn new() -> Self {
        Self {
            coherence_threshold: 0.3,
            deadband: DeadbandEngine::new(),
            tile_deps: HashMap::new(),
            invalidated: HashSet::new(),
            consensus_threshold: 0.7,
            current_time_secs: 0,
        }
    }

    pub fn with_threshold(mut self, threshold: f64) -> Self {
        self.coherence_threshold = threshold;
        self
    }

    /// Override the DCS consensus threshold (default 0.7).
    pub fn with_consensus_threshold(mut self, threshold: f64) -> Self {
        self.consensus_threshold = threshold;
        self
    }

    /// Inject a fixed unix timestamp for deterministic temporal tests.
    pub fn with_time(mut self, secs: u64) -> Self {
        self.current_time_secs = secs;
        self
    }

    fn now(&self) -> u64 {
        if self.current_time_secs > 0 { self.current_time_secs } else { unix_now() }
    }

    /// Extract significant words from text (3+ chars, lowercase).
    fn significant_words(&self, text: &str) -> Vec<String> {
        text.split_whitespace()
            .map(|w| w.to_lowercase())
            .filter(|w| w.len() >= 3)
            .collect()
    }

    /// Compute temporal signal and lifecycle status for a tile.
    ///
    /// - Valid  (age ≤ 1 day):   signal linearly decays 1.0 → 0.5
    /// - Grace  (inside window):  signal = 0.3
    /// - Expired (past grace):    signal = 0.05 (penalty)
    fn temporal_signal(&self, refreshed_at: u64, grace_period: u64) -> (f64, TemporalStatus) {
        let age = self.now().saturating_sub(refreshed_at);
        if age <= DEFAULT_VALID_SECS {
            let decay = age as f64 / DEFAULT_VALID_SECS as f64;
            (1.0 - decay * 0.5, TemporalStatus::Valid)
        } else if age <= DEFAULT_VALID_SECS + grace_period {
            (0.3, TemporalStatus::Grace)
        } else {
            (0.05, TemporalStatus::Expired)
        }
    }

    /// Score and rank tiles using the 7-signal v2 model.
    ///
    /// Signal weights (sum = 1.0):
    ///   keyword 0.25 · ghost 0.10 · belief 0.20 · domain 0.15
    ///   frequency 0.08 · temporal 0.15 · controversy 0.07
    pub fn score_tiles_v2(&self, tiles: &[TileInputV2], query: &str, limit: usize) -> Vec<TileScoreV2> {
        let mut scores: Vec<TileScoreV2> = tiles.iter()
            .map(|t| self.score_tile_v2(t, query))
            .collect();
        scores.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(limit);
        scores
    }

    fn score_tile_v2(&self, tile: &TileInputV2, query: &str) -> TileScoreV2 {
        let zero = TileScoreV2 {
            tile_id: tile.tile_id, score: 0.0, keyword: 0.0, ghost: 0.0,
            belief: 0.0, domain: 0.0, frequency: 0.0, temporal: 0.0, controversy: 0.0,
        };

        let query_words: Vec<String> = query.split_whitespace().map(|w| w.to_lowercase()).collect();
        if query_words.is_empty() { return zero; }

        let text = format!("{} {}", tile.question, tile.answer);
        let text_words: Vec<String> = text.split_whitespace().map(|w| w.to_lowercase()).collect();

        let keyword_hits = query_words.iter().filter(|w| text_words.contains(w)).count();
        let keyword = keyword_hits as f64 / query_words.len() as f64;
        if keyword < 0.01 { return zero; }

        let ghost = 1.0 - tile.ghost_score;
        let belief = tile.confidence;

        let domain_lower = tile.domain.to_lowercase();
        let domain_hits = query_words.iter().filter(|w| domain_lower.contains(w.as_str())).count();
        let domain = domain_hits as f64 / query_words.len() as f64;

        let frequency = (tile.use_count as f64 / 100.0).min(1.0);

        let (temporal, _status) = self.temporal_signal(tile.refreshed_at_secs, tile.grace_period_secs);

        // Tiles that survived challenges (high controversy + high confidence) get a bonus.
        let controversy = (tile.controversy_score * tile.confidence).min(1.0);

        let score = keyword    * 0.25
            + ghost       * 0.10
            + belief      * 0.20
            + domain      * 0.15
            + frequency   * 0.08
            + temporal    * 0.15
            + controversy * 0.07;

        TileScoreV2 { tile_id: tile.tile_id, score, keyword, ghost, belief, domain, frequency, temporal, controversy }
    }

    /// Validate a tile against mandatory gates before it enters the tile store.
    ///
    /// Gates: confidence ≥ 0.3, content length 10–100k chars, domain non-empty.
    /// Failures are logged via the returned ValidationResult; the tile is still
    /// stored in degraded mode so the caller retains control over indexing.
    pub fn validate_tile(&self, tile: &TileInputV2) -> ValidationResult {
        let mut gates_failed: Vec<String> = Vec::new();

        if tile.confidence < 0.3 {
            gates_failed.push("confidence".to_string());
        }

        let content_len = tile.question.len() + tile.answer.len();
        if content_len < 10 {
            gates_failed.push("content_length_min".to_string());
        }
        if content_len > 100_000 {
            gates_failed.push("content_length_max".to_string());
        }

        if tile.domain.trim().is_empty() {
            gates_failed.push("domain_empty".to_string());
        }

        let passed = gates_failed.is_empty();
        ValidationResult { passed, gates_failed, degraded: !passed }
    }

    /// Register a dependency: changes to `upstream` affect `downstream`.
    pub fn add_dependency(&mut self, upstream: usize, downstream: usize) {
        self.tile_deps.entry(upstream).or_default().push(downstream);
    }

    /// BFS over the dependency graph to find all tiles transitively affected
    /// by a change to `tile_id`. Returns the affected tile IDs (not including
    /// `tile_id` itself).
    pub fn check_impact(&self, tile_id: usize) -> Vec<usize> {
        let mut affected: Vec<usize> = Vec::new();
        let mut queue: VecDeque<usize> = VecDeque::new();
        let mut visited: HashSet<usize> = HashSet::new();

        if let Some(deps) = self.tile_deps.get(&tile_id) {
            queue.extend(deps.iter().copied());
        }

        while let Some(id) = queue.pop_front() {
            if visited.insert(id) {
                affected.push(id);
                if let Some(deps) = self.tile_deps.get(&id) {
                    for &dep in deps {
                        if !visited.contains(&dep) {
                            queue.push_back(dep);
                        }
                    }
                }
            }
        }

        affected
    }

    /// Mark `tile_id` and all transitively-downstream tiles as needing refresh.
    pub fn propagate_invalidation(&mut self, tile_id: usize) {
        let affected = self.check_impact(tile_id);
        self.invalidated.extend(affected);
        self.invalidated.insert(tile_id);
    }

    /// True iff `tile_id` has been marked as invalidated.
    pub fn is_invalidated(&self, tile_id: usize) -> bool {
        self.invalidated.contains(&tile_id)
    }

    /// DCS consensus gate — proxy for multi-agent agreement.
    ///
    /// Uses `confidence` as the consensus score. If score < `consensus_threshold`
    /// the action is escalated to P1 (requires human review).
    pub fn consensus_check(&self, confidence: f64) -> ConsensusResult {
        let consensus_reached = confidence >= self.consensus_threshold;
        ConsensusResult {
            consensus_reached,
            score: confidence,
            escalated_to_p1: !consensus_reached,
        }
    }
}

impl Default for DefaultStateBridge {
    fn default() -> Self { Self::new() }
}

impl StateBridge for DefaultStateBridge {
    fn check_deadband(&self, action: &str) -> DeadbandCheck {
        self.deadband.check(action)
    }

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

    #[test]
    fn test_check_deadband_blocks_p0() {
        let bridge = DefaultStateBridge::new();
        let check = bridge.check_deadband("rm -rf /tmp/data");
        assert!(!check.passed);
        assert!(!check.p0_clear);
    }

    #[test]
    fn test_check_deadband_passes_safe_action() {
        let bridge = DefaultStateBridge::new();
        let check = bridge.check_deadband("calculate the sum of values");
        assert!(check.passed);
    }

    // ── V2 tests (RED phase) ──────────────────────────────────────────────

    fn make_tile_v2(id: usize, question: &str, answer: &str, domain: &str,
                    confidence: f64, ghost_score: f64, use_count: u32,
                    refreshed_at_secs: u64, grace_period_secs: u64,
                    challenge_count: u32, controversy_score: f64) -> TileInputV2 {
        TileInputV2 {
            tile_id: id,
            question: question.to_string(),
            answer: answer.to_string(),
            tags: vec![],
            domain: domain.to_string(),
            confidence,
            ghost_score,
            use_count,
            created_at_secs: refreshed_at_secs,
            refreshed_at_secs,
            grace_period_secs,
            challenge_count,
            controversy_score,
        }
    }

    #[test]
    fn test_score_tiles_v2_valid_tile_scores_higher_than_expired() {
        let now = 1_000_000u64;
        let bridge = DefaultStateBridge::new().with_time(now);
        let fresh = make_tile_v2(0, "constraint theory", "snap to grid", "constraint",
                                 0.9, 0.0, 10, now - 3600, 43200, 0, 0.0);
        let expired = make_tile_v2(1, "constraint theory", "snap to grid", "constraint",
                                   0.9, 0.0, 10, now - 200_000, 43200, 0, 0.0);
        let results = bridge.score_tiles_v2(&[fresh, expired], "constraint theory", 10);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].tile_id, 0, "fresh tile should rank first");
        assert!(results[0].temporal > results[1].temporal, "fresh tile must have higher temporal signal");
    }

    #[test]
    fn test_score_tiles_v2_controversy_survival_boosts_score() {
        let now = 1_000_000u64;
        let bridge = DefaultStateBridge::new().with_time(now);
        let contested = make_tile_v2(0, "rust memory safety", "ownership rules", "rust",
                                     0.9, 0.0, 10, now, 43200, 5, 0.8);
        let quiet = make_tile_v2(1, "rust memory safety", "ownership rules", "rust",
                                 0.9, 0.0, 10, now, 43200, 0, 0.0);
        let results = bridge.score_tiles_v2(&[contested, quiet], "rust memory safety", 10);
        assert!(results[0].controversy > results[1].controversy);
        assert!(results[0].score > results[1].score);
    }

    #[test]
    fn test_score_tiles_v2_low_ghost_score_gives_max_ghost_component() {
        let now = 1_000_000u64;
        let bridge = DefaultStateBridge::new().with_time(now);
        let live = make_tile_v2(0, "plato tiles", "knowledge graph", "plato",
                                0.8, 0.0, 5, now, 43200, 0, 0.0);
        let ghost = make_tile_v2(1, "plato tiles", "knowledge graph", "plato",
                                 0.8, 1.0, 5, now, 43200, 0, 0.0);
        let results = bridge.score_tiles_v2(&[live, ghost], "plato tiles", 10);
        assert_eq!(results[0].tile_id, 0, "live tile (ghost_score=0) should rank first");
        assert!(results[0].ghost > results[1].ghost);
    }

    #[test]
    fn test_score_tiles_v2_empty_query_returns_zero_scores() {
        let now = 1_000_000u64;
        let bridge = DefaultStateBridge::new().with_time(now);
        let tile = make_tile_v2(0, "anything", "anything", "domain", 0.9, 0.0, 5, now, 43200, 0, 0.0);
        let results = bridge.score_tiles_v2(&[tile], "", 10);
        assert_eq!(results[0].score, 0.0);
    }

    #[test]
    fn test_score_tiles_v2_grace_period_gives_lower_temporal_than_valid() {
        let now = 1_000_000u64;
        let grace_secs = 43200u64;
        let bridge = DefaultStateBridge::new().with_time(now);
        // refreshed 90001 seconds ago → just past 1-day valid window, but inside grace
        let in_grace = make_tile_v2(0, "test tile", "answer", "domain",
                                    0.9, 0.0, 0, now - 90_001, grace_secs, 0, 0.0);
        // refreshed 1 hour ago → fully valid
        let valid = make_tile_v2(1, "test tile", "answer", "domain",
                                 0.9, 0.0, 0, now - 3600, grace_secs, 0, 0.0);
        let results = bridge.score_tiles_v2(&[in_grace, valid], "test tile", 10);
        assert_eq!(results[0].tile_id, 1, "valid tile should rank first");
        assert!(results[0].temporal > results[1].temporal);
    }

    // ── validate_tile tests ───────────────────────────────────────────────

    #[test]
    fn test_validate_tile_passes_valid_tile() {
        let bridge = DefaultStateBridge::new();
        let tile = make_tile_v2(0, "what is rust", "a systems language", "programming",
                                0.9, 0.0, 5, 0, 43200, 0, 0.0);
        let result = bridge.validate_tile(&tile);
        assert!(result.passed);
        assert!(result.gates_failed.is_empty());
        assert!(!result.degraded);
    }

    #[test]
    fn test_validate_tile_fails_low_confidence() {
        let bridge = DefaultStateBridge::new();
        let tile = make_tile_v2(0, "what is rust", "a systems language", "programming",
                                0.1, 0.0, 5, 0, 43200, 0, 0.0);
        let result = bridge.validate_tile(&tile);
        assert!(!result.passed);
        assert!(result.gates_failed.contains(&"confidence".to_string()));
        assert!(result.degraded);
    }

    #[test]
    fn test_validate_tile_fails_empty_domain() {
        let bridge = DefaultStateBridge::new();
        let tile = make_tile_v2(0, "what is rust", "a systems language", "",
                                0.9, 0.0, 5, 0, 43200, 0, 0.0);
        let result = bridge.validate_tile(&tile);
        assert!(!result.passed);
        assert!(result.gates_failed.contains(&"domain_empty".to_string()));
    }

    #[test]
    fn test_validate_tile_fails_content_too_short() {
        let bridge = DefaultStateBridge::new();
        let tile = make_tile_v2(0, "hi", "ok", "domain",
                                0.9, 0.0, 5, 0, 43200, 0, 0.0);
        let result = bridge.validate_tile(&tile);
        assert!(!result.passed);
        assert!(result.gates_failed.contains(&"content_length_min".to_string()));
    }

    #[test]
    fn test_validate_tile_fails_content_too_long() {
        let bridge = DefaultStateBridge::new();
        let long_content = "x".repeat(100_001);
        let tile = make_tile_v2(0, &long_content, "answer", "domain",
                                0.9, 0.0, 5, 0, 43200, 0, 0.0);
        let result = bridge.validate_tile(&tile);
        assert!(!result.passed);
        assert!(result.gates_failed.contains(&"content_length_max".to_string()));
    }

    #[test]
    fn test_validate_tile_reports_multiple_failures() {
        let bridge = DefaultStateBridge::new();
        let tile = make_tile_v2(0, "hi", "ok", "",
                                0.1, 0.0, 5, 0, 43200, 0, 0.0);
        let result = bridge.validate_tile(&tile);
        assert!(!result.passed);
        assert!(result.gates_failed.len() >= 2); // confidence + domain_empty + content_length_min
    }

    // ── check_impact / propagate_invalidation tests ──────────────────────

    #[test]
    fn test_check_impact_no_deps_returns_empty() {
        let bridge = DefaultStateBridge::new();
        assert!(bridge.check_impact(0).is_empty());
    }

    #[test]
    fn test_check_impact_direct_dep_returned() {
        let mut bridge = DefaultStateBridge::new();
        bridge.add_dependency(0, 1);
        let affected = bridge.check_impact(0);
        assert_eq!(affected, vec![1]);
    }

    #[test]
    fn test_check_impact_transitive_deps_returned() {
        let mut bridge = DefaultStateBridge::new();
        bridge.add_dependency(0, 1);
        bridge.add_dependency(1, 2);
        bridge.add_dependency(2, 3);
        let affected = bridge.check_impact(0);
        assert!(affected.contains(&1));
        assert!(affected.contains(&2));
        assert!(affected.contains(&3));
    }

    #[test]
    fn test_propagate_invalidation_marks_downstream_and_self() {
        let mut bridge = DefaultStateBridge::new();
        bridge.add_dependency(10, 11);
        bridge.add_dependency(11, 12);
        bridge.propagate_invalidation(10);
        assert!(bridge.is_invalidated(10));
        assert!(bridge.is_invalidated(11));
        assert!(bridge.is_invalidated(12));
    }

    #[test]
    fn test_is_invalidated_false_for_unknown_tile() {
        let bridge = DefaultStateBridge::new();
        assert!(!bridge.is_invalidated(999));
    }

    // ── consensus_check tests ─────────────────────────────────────────────

    #[test]
    fn test_consensus_check_high_confidence_reaches_consensus() {
        let bridge = DefaultStateBridge::new().with_consensus_threshold(0.7);
        let result = bridge.consensus_check(0.9);
        assert!(result.consensus_reached);
        assert!(!result.escalated_to_p1);
        assert_eq!(result.score, 0.9);
    }

    #[test]
    fn test_consensus_check_low_confidence_escalates_to_p1() {
        let bridge = DefaultStateBridge::new().with_consensus_threshold(0.7);
        let result = bridge.consensus_check(0.5);
        assert!(!result.consensus_reached);
        assert!(result.escalated_to_p1);
    }

    #[test]
    fn test_consensus_check_boundary_exact_threshold() {
        let bridge = DefaultStateBridge::new().with_consensus_threshold(0.7);
        let result = bridge.consensus_check(0.7);
        assert!(result.consensus_reached, "exact threshold should reach consensus");
    }

    // ── existing test kept intact ─────────────────────────────────────────

    #[test]
    fn test_score_tiles_returns_ranked_results() {
        let bridge = DefaultStateBridge::new();
        let tiles = vec![
            TileInput {
                tile_id: 0,
                question: "constraint theory overview".to_string(),
                answer: "Pythagorean snap coordinates".to_string(),
                tags: vec![],
                domain: "constraint".to_string(),
                confidence: 0.9,
                ghost_score: 0.0,
                use_count: 10,
            },
            TileInput {
                tile_id: 1,
                question: "completely unrelated topic".to_string(),
                answer: "gardening and horticulture tips".to_string(),
                tags: vec![],
                domain: "gardening".to_string(),
                confidence: 0.5,
                ghost_score: 0.5,
                use_count: 0,
            },
        ];
        let results = bridge.score_tiles(&tiles, "constraint theory", 10);
        assert!(!results.is_empty());
        assert_eq!(results[0].tile_id, 0);
    }
}

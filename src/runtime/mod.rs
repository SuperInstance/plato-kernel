//! Pillar 5 — Unified PLATO Runtime
//!
//! Sequences Pillars 1–4 into a single `process_query` pipeline:
//!
//! ```text
//! Query
//!   ──▶ [P1 Tiling]      inject knowledge tiles for word anchors
//!   ──▶ [P3 Recall]      surface similar past episodes
//!   ──▶ [Mock LLM]       assemble context + query (replace with real call)
//!   ──▶ [P2 Constraints] audit output against assertive Markdown rules
//!   ──▶ [P4 Anchors]     expand [BracketedWord] tokens inline
//!   ──▶ [P3 Record]      persist episode to KNOWLEDGE.md
//!   ──▶ Response
//! ```

use crate::constraint_engine::{AuditOutcome, ConstraintAuditor, parse_markdown_constraints};
use crate::episode_recorder::{EpisodeEntry, EpisodeOutcome, EpisodeRecorder};
use crate::tiling::TileRegistry;
use crate::tutor::{expand_anchors, extract_anchors, jump_all_contexts};

// ─── Response ────────────────────────────────────────────────────────────────

/// The result of running a query through the 5-pillar PLATO pipeline.
#[derive(Debug)]
pub struct Response {
    /// Final content: query + injected tile context, with anchors expanded.
    pub content: String,
    /// Anchor slugs of knowledge tiles that were injected (Pillar 1).
    pub tiles_used: Vec<String>,
    /// Number of assertive constraints evaluated against the response (Pillar 2).
    pub constraints_checked: usize,
    /// Word anchors that were expanded inside the response (Pillar 4).
    pub anchors_expanded: Vec<String>,
    /// Pipeline confidence: 1.0 = Pass, 0.8 = Warned, 0.5 = RetryRequired.
    pub confidence: f32,
}

// ─── PlatoRuntime ─────────────────────────────────────────────────────────────

/// The unified PLATO runtime — Pillar 5.
///
/// Owns all four pillar subsystems and sequences them per query.
/// Construct via [`PlatoRuntime::new`], then call [`process_query`] for each
/// incoming query.
pub struct PlatoRuntime {
    /// Pillar 1: Tiling knowledge substrate.
    pub tile_registry: TileRegistry,
    /// Pillar 2: Assertive constraint auditor.
    auditor: ConstraintAuditor,
    /// Pillar 3: Semantic muscle memory recorder.
    episode_recorder: EpisodeRecorder,
    /// Cached count of loaded assertive constraints (for `Response::constraints_checked`).
    constraint_count: usize,
}

impl PlatoRuntime {
    /// Build a runtime from raw Markdown documents.
    ///
    /// - `knowledge_doc`   — `##`-delimited tiles for Pillar 1 (tiling substrate).
    /// - `constraints_doc` — Bullet-point assertions for Pillar 2 (constraint engine).
    /// - `episode_path`    — File path for Pillar 3 KNOWLEDGE.md episodes.
    pub fn new(knowledge_doc: &str, constraints_doc: &str, episode_path: &str) -> Self {
        let constraints = parse_markdown_constraints(constraints_doc);
        let constraint_count = constraints.len();
        Self {
            tile_registry: TileRegistry::parse(knowledge_doc),
            auditor: ConstraintAuditor::new(constraints),
            episode_recorder: EpisodeRecorder::new(episode_path),
            constraint_count,
        }
    }

    /// Process a query through the full 5-pillar pipeline.
    ///
    /// The LLM step is **mocked**: it assembles tile context + recall + query
    /// into a string and returns it verbatim. Replace the "Mock LLM" block
    /// with a real inference call once you have a backend wired in.
    pub fn process_query(&self, query: &str, agent: &str) -> Response {
        // ── P1: Tiling — inject tiles for every word anchor in the query ─────
        let matched_tiles = jump_all_contexts(query, &self.tile_registry);
        let tiles_used: Vec<String> = matched_tiles.iter().map(|t| t.anchor.clone()).collect();
        let tile_context: String = matched_tiles
            .iter()
            .map(|t| format!("[TILE:{}]\n{}\n", t.anchor, t.body.trim()))
            .collect::<Vec<_>>()
            .join("\n");

        // ── P3 (recall): surface relevant past episodes ──────────────────────
        let past_episodes = self.episode_recorder.recall_relevant(query).unwrap_or_default();
        let recall_context: String = past_episodes
            .iter()
            .take(2)
            .map(|e| format!("[RECALL:{}]\n{}\n", e.header.trim(), e.body.trim()))
            .collect::<Vec<_>>()
            .join("\n");

        // ── Mock LLM — assemble context + query ─────────────────────────────
        // Production: pass (recall_context + tile_context + query) to your LLM.
        // The mock just concatenates so the pipeline is fully testable without an API key.
        let raw_content = format!("{}{}{}", recall_context, tile_context, query);

        // ── P2: Constraints — audit the raw response ─────────────────────────
        let audit = self.auditor.audit(&raw_content);
        let confidence: f32 = match &audit {
            AuditOutcome::Pass             => 1.0,
            AuditOutcome::Warned(_)        => 0.8,
            AuditOutcome::RetryRequired(_) => 0.5,
        };

        // ── P4: Anchors — expand [BracketedWord] tokens in the response ──────
        let content = expand_anchors(&raw_content, &self.tile_registry);
        let anchors_expanded = extract_anchors(&raw_content);

        // ── P3 (record): persist this episode to KNOWLEDGE.md ────────────────
        let outcome = match &audit {
            AuditOutcome::Pass             => EpisodeOutcome::Success,
            AuditOutcome::Warned(_)        => EpisodeOutcome::Partial,
            AuditOutcome::RetryRequired(_) => EpisodeOutcome::Failure,
        };
        let entry = EpisodeEntry::new(
            query,
            &format!("{} via PLATO runtime", agent),
            &format!("{:?}", audit),
            outcome,
        );
        if let Err(e) = self.episode_recorder.record(&entry) {
            tracing::warn!("runtime: episode record failed: {}", e);
        }

        Response {
            content,
            tiles_used,
            constraints_checked: self.constraint_count,
            anchors_expanded,
            confidence,
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const KNOWLEDGE: &str = "\
## PaymentFlow
Handles payment initiation via [Settlement].

## Settlement
Clears funds after [PaymentFlow] confirms.

## RefundPolicy
Refunds reference the original [PaymentFlow].
";

    // Empty constraints doc → auditor always returns Pass.
    fn unconstrained_runtime(episode_path: &str) -> PlatoRuntime {
        PlatoRuntime::new(KNOWLEDGE, "", episode_path)
    }

    #[test]
    fn test_pipeline_no_anchors() {
        let rt = unconstrained_runtime("/tmp/plato_rt_no_anchors.md");
        let resp = rt.process_query("hello world no anchors here", "test-agent");
        assert!(resp.tiles_used.is_empty(), "no anchors → no tiles injected");
        assert!(resp.anchors_expanded.is_empty());
        assert_eq!(resp.confidence, 1.0, "no constraints → always Pass");
        assert_eq!(resp.constraints_checked, 0);
    }

    #[test]
    fn test_pipeline_tile_injection() {
        let rt = unconstrained_runtime("/tmp/plato_rt_tile.md");
        let resp = rt.process_query("explain [PaymentFlow] to me", "test-agent");
        assert!(
            resp.tiles_used.contains(&"PaymentFlow".to_string()),
            "PaymentFlow anchor must inject its tile"
        );
        assert!(resp.content.contains("PaymentFlow"));
        assert_eq!(resp.confidence, 1.0);
    }

    #[test]
    fn test_pipeline_multi_anchor() {
        let rt = unconstrained_runtime("/tmp/plato_rt_multi.md");
        let resp = rt.process_query("[PaymentFlow] and [Settlement] overview", "agent");
        assert!(resp.tiles_used.contains(&"PaymentFlow".to_string()));
        assert!(resp.tiles_used.contains(&"Settlement".to_string()));
        assert_eq!(resp.tiles_used.len(), 2);
    }

    #[test]
    fn test_anchor_expansion_in_content() {
        let rt = unconstrained_runtime("/tmp/plato_rt_expand.md");
        let resp = rt.process_query("[Settlement] details", "agent");
        // expand_anchors rewrites [Settlement] with the tile body inline.
        assert!(resp.content.contains("Expanded tile: Settlement"));
    }

    #[test]
    fn test_constraints_checked_count() {
        let constraints_doc = "- Output must include summary.\n- Output should be concise.\n";
        let rt = PlatoRuntime::new(KNOWLEDGE, constraints_doc, "/tmp/plato_rt_constraints.md");
        let resp = rt.process_query("any query", "agent");
        assert_eq!(resp.constraints_checked, 2, "two bullets parsed from constraints_doc");
    }

    #[test]
    fn test_confidence_on_pass() {
        let rt = unconstrained_runtime("/tmp/plato_rt_confidence.md");
        let resp = rt.process_query("test", "agent");
        assert_eq!(resp.confidence, 1.0);
    }

    #[test]
    fn test_episode_recorded() {
        let path = "/tmp/plato_rt_episode_test.md";
        let _ = std::fs::remove_file(path);
        let rt = unconstrained_runtime(path);
        rt.process_query("payment flow question", "agent");
        // KNOWLEDGE.md should now exist.
        assert!(std::path::Path::new(path).exists(), "episode must be persisted");
        let _ = std::fs::remove_file(path);
    }
}

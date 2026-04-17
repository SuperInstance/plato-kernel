//! TUTOR — Command Word Anchors
//!
//! In the original PLATO language (TUTOR), you could jump to any lesson label
//! instantly with a single command. This module implements the modern equivalent:
//! **Word Anchors**.
//!
//! If an agent (or user) sees a bracketed word like `[PaymentFlow]`, the runtime
//! "jumps" its entire context to that specific `.md` tile, injecting it into the
//! agent's working context as if it were the current lesson page.
//!
//! This makes words hyperlinked neural pathways — editing a single bracket in
//! prose re-routes the agent's entire knowledge focus.

use crate::tiling::{KnowledgeTile, TileRegistry};

/// The result of resolving a TUTOR jump.
#[derive(Debug, Clone)]
pub enum JumpResult<'a> {
    /// Found the target tile — inject this into agent context.
    Found(&'a KnowledgeTile),
    /// No tile matched this anchor; return suggestions.
    NotFound { anchor: String, suggestions: Vec<String> },
    /// The input contained no word anchors.
    NoAnchors,
}

/// Parse all `[BracketedWord]` anchors from an input string.
///
/// Only alphanumeric tokens (with hyphens/underscores) are treated as anchors.
/// Regular `[markdown links](url)` are ignored by checking for a following `(`.
pub fn extract_anchors(input: &str) -> Vec<String> {
    let mut anchors = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '[' {
            let start = i + 1;
            let mut end = start;
            while end < chars.len() && chars[end] != ']' {
                end += 1;
            }
            if end < chars.len() {
                let word: String = chars[start..end].iter().collect();
                // Skip markdown links: [text](url)
                let after = end + 1;
                let is_md_link = after < chars.len() && chars[after] == '(';
                if !is_md_link
                    && !word.is_empty()
                    && word.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_')
                {
                    anchors.push(word);
                }
                i = end + 1;
                continue;
            }
        }
        i += 1;
    }
    anchors
}

/// Resolve the first word anchor found in `input` against the tile registry.
///
/// If multiple anchors are present, the first one found in the input is resolved.
/// Callers that need to resolve all should iterate `extract_anchors()` themselves.
pub fn jump_context<'a>(input: &str, registry: &'a TileRegistry) -> JumpResult<'a> {
    let anchors = extract_anchors(input);

    if anchors.is_empty() {
        return JumpResult::NoAnchors;
    }

    let anchor = &anchors[0];

    if let Some(tile) = registry.get_by_anchor(anchor) {
        return JumpResult::Found(tile);
    }

    // Build fuzzy suggestions: anchors whose slug shares a prefix
    let lower = anchor.to_lowercase();
    let suggestions: Vec<String> = registry
        .all()
        .iter()
        .filter(|t| {
            let ta = t.anchor.to_lowercase();
            ta.starts_with(&lower[..lower.len().min(4)]) || lower.starts_with(&ta[..ta.len().min(4)])
        })
        .map(|t| t.anchor.clone())
        .take(5)
        .collect();

    JumpResult::NotFound {
        anchor: anchor.clone(),
        suggestions,
    }
}

/// Resolve ALL word anchors in `input`, returning tiles in order of appearance.
///
/// Deduplicates: if the same anchor appears twice, the tile is returned once.
pub fn jump_all_contexts<'a>(input: &str, registry: &'a TileRegistry) -> Vec<&'a KnowledgeTile> {
    let anchors = extract_anchors(input);
    let mut seen = std::collections::HashSet::new();
    let mut tiles = Vec::new();

    for anchor in &anchors {
        if seen.insert(anchor.clone()) {
            if let Some(tile) = registry.get_by_anchor(anchor) {
                tiles.push(tile);
            }
        }
    }
    tiles
}

/// Rewrite an input string, expanding `[Anchor]` tokens with the tile body.
///
/// This is the "homoiconic Markdown" operation: the agent can call this to
/// produce a version of its input where word anchors are inline-expanded —
/// effectively rewriting its own spec with the referenced knowledge.
pub fn expand_anchors(input: &str, registry: &TileRegistry) -> String {
    let anchors = extract_anchors(input);
    let mut result = input.to_string();

    for anchor in anchors {
        if let Some(tile) = registry.get_by_anchor(&anchor) {
            let placeholder = format!("[{}]", anchor);
            let expansion = format!(
                "[{}]\n\n> **Expanded tile: {}**\n> {}\n",
                anchor,
                tile.header.trim_start_matches('#').trim(),
                tile.body.lines().next().unwrap_or("").trim(),
            );
            result = result.replacen(&placeholder, &expansion, 1);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tiling::TileRegistry;

    const DOC: &str = "## PaymentFlow\nHandles payment initiation.\n\n## Settlement\nClears funds.\n\n## RefundPolicy\nRefunds the original charge.\n";

    #[test]
    fn test_extract_anchors() {
        let anchors = extract_anchors("please review [PaymentFlow] and [Settlement]");
        assert_eq!(anchors, vec!["PaymentFlow", "Settlement"]);
    }

    #[test]
    fn test_extract_anchors_skips_md_links() {
        let anchors = extract_anchors("[click here](https://example.com) vs [PaymentFlow]");
        assert_eq!(anchors, vec!["PaymentFlow"]);
    }

    #[test]
    fn test_jump_context_found() {
        let reg = TileRegistry::parse(DOC);
        let result = jump_context("review [PaymentFlow] now", &reg);
        assert!(matches!(result, JumpResult::Found(t) if t.anchor == "PaymentFlow"));
    }

    #[test]
    fn test_jump_context_not_found_with_suggestions() {
        let reg = TileRegistry::parse(DOC);
        let result = jump_context("review [Payment]", &reg);
        match result {
            JumpResult::NotFound { anchor, suggestions } => {
                assert_eq!(anchor, "Payment");
                assert!(!suggestions.is_empty());
            }
            _ => panic!("expected NotFound"),
        }
    }

    #[test]
    fn test_jump_all() {
        let reg = TileRegistry::parse(DOC);
        let tiles = jump_all_contexts("[PaymentFlow] and [Settlement]", &reg);
        assert_eq!(tiles.len(), 2);
    }

    #[test]
    fn test_expand_anchors() {
        let reg = TileRegistry::parse(DOC);
        let expanded = expand_anchors("Apply [PaymentFlow] rules.", &reg);
        assert!(expanded.contains("Expanded tile: PaymentFlow"));
    }
}

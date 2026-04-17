//! Tiling Knowledge Substrate
//!
//! Implements the PLATO "tiling" concept: a Markdown document is split by `##` headers
//! into independent semantic nodes ("tiles"). The runtime conditionally injects only the
//! tiles relevant to the agent's current prose position, reducing token cost while
//! preserving a coherent "local detail + global overview" view.
//!
//! Analogous to the original PLATO conditional branching: a student (agent) only sees
//! the lesson tile appropriate to their current position in the curriculum.

use std::collections::HashMap;

/// A single semantic tile — one `##`-delimited section of a Markdown document.
#[derive(Debug, Clone)]
pub struct KnowledgeTile {
    /// Unique anchor derived from the header text (e.g. `## Payment Flow` → `PaymentFlow`)
    pub anchor: String,
    /// Original header text
    pub header: String,
    /// Body content (everything between this header and the next)
    pub body: String,
    /// Zero-based position index in the original document
    pub position: usize,
    /// Tags extracted from the tile body (words inside `[brackets]`)
    pub word_anchors: Vec<String>,
}

impl KnowledgeTile {
    /// Build an anchor slug from a header string.
    fn slugify(header: &str) -> String {
        header
            .trim_start_matches('#')
            .trim()
            .split_whitespace()
            .map(|w| {
                let mut c = w.chars();
                match c.next() {
                    Some(f) => f.to_uppercase().to_string() + c.as_str(),
                    None => String::new(),
                }
            })
            .collect::<String>()
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-')
            .collect()
    }

    /// Extract `[BracketedWord]` anchors from body text.
    fn extract_word_anchors(body: &str) -> Vec<String> {
        let mut anchors = Vec::new();
        let mut chars = body.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '[' {
                let word: String = chars.by_ref().take_while(|&c| c != ']').collect();
                if !word.is_empty() && word.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
                    anchors.push(word);
                }
            }
        }
        anchors
    }
}

/// The full tile registry for a parsed document.
#[derive(Debug, Default)]
pub struct TileRegistry {
    tiles: Vec<KnowledgeTile>,
    anchor_index: HashMap<String, usize>, // anchor → tile index
}

impl TileRegistry {
    /// Parse a Markdown string into tiles split on `##` headers.
    pub fn parse(content: &str) -> Self {
        let mut tiles = Vec::new();
        let mut current_header = String::from("Preamble");
        let mut current_body = String::new();
        let mut position = 0usize;

        for line in content.lines() {
            if line.starts_with("## ") {
                // Flush the current tile
                if !current_body.trim().is_empty() || !tiles.is_empty() || current_header != "Preamble" {
                    let anchor = KnowledgeTile::slugify(&current_header);
                    let word_anchors = KnowledgeTile::extract_word_anchors(&current_body);
                    tiles.push(KnowledgeTile {
                        anchor,
                        header: current_header.clone(),
                        body: current_body.clone(),
                        position,
                        word_anchors,
                    });
                    position += 1;
                }
                current_header = line.to_string();
                current_body = String::new();
            } else {
                current_body.push_str(line);
                current_body.push('\n');
            }
        }

        // Flush final tile
        if !current_body.trim().is_empty() {
            let anchor = KnowledgeTile::slugify(&current_header);
            let word_anchors = KnowledgeTile::extract_word_anchors(&current_body);
            tiles.push(KnowledgeTile {
                anchor,
                header: current_header,
                body: current_body,
                position,
                word_anchors,
            });
        }

        let anchor_index = tiles
            .iter()
            .enumerate()
            .map(|(i, t)| (t.anchor.clone(), i))
            .collect();

        Self { tiles, anchor_index }
    }

    /// Return the tile at a given prose position.
    pub fn get_at(&self, position: usize) -> Option<&KnowledgeTile> {
        self.tiles.get(position)
    }

    /// Look up a tile by its anchor slug (case-insensitive).
    pub fn get_by_anchor(&self, anchor: &str) -> Option<&KnowledgeTile> {
        // Try exact match first
        if let Some(&idx) = self.anchor_index.get(anchor) {
            return self.tiles.get(idx);
        }
        // Case-insensitive fallback
        let lower = anchor.to_lowercase();
        self.tiles.iter().find(|t| t.anchor.to_lowercase() == lower)
    }

    /// Inject context: return tiles relevant to `current_position` within a window.
    ///
    /// The window includes the current tile plus `lookahead` tiles ahead and
    /// `lookbehind` tiles behind, giving the agent local detail + global orientation.
    pub fn inject_context(
        &self,
        current_position: usize,
        lookbehind: usize,
        lookahead: usize,
    ) -> Vec<&KnowledgeTile> {
        let start = current_position.saturating_sub(lookbehind);
        let end = (current_position + lookahead + 1).min(self.tiles.len());
        self.tiles[start..end].iter().collect()
    }

    /// Return all tiles (for full-document injection when needed).
    pub fn all(&self) -> &[KnowledgeTile] {
        &self.tiles
    }

    /// Total number of tiles.
    pub fn len(&self) -> usize {
        self.tiles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
## Payment Flow
The user initiates a [PaymentFlow] request.
Funds move through [Settlement].

## Settlement
Settlement occurs after [PaymentFlow] clears.

## Refund Policy
Refunds reference the original [PaymentFlow].
"#;

    #[test]
    fn test_parse_tiles() {
        let reg = TileRegistry::parse(SAMPLE);
        assert_eq!(reg.len(), 3);
        assert_eq!(reg.get_at(0).unwrap().anchor, "PaymentFlow");
        assert_eq!(reg.get_at(1).unwrap().anchor, "Settlement");
    }

    #[test]
    fn test_anchor_lookup() {
        let reg = TileRegistry::parse(SAMPLE);
        assert!(reg.get_by_anchor("PaymentFlow").is_some());
        assert!(reg.get_by_anchor("paymentflow").is_some()); // case-insensitive
    }

    #[test]
    fn test_word_anchors_extracted() {
        let reg = TileRegistry::parse(SAMPLE);
        let tile = reg.get_at(0).unwrap();
        assert!(tile.word_anchors.contains(&"PaymentFlow".to_string()));
        assert!(tile.word_anchors.contains(&"Settlement".to_string()));
    }

    #[test]
    fn test_inject_context_window() {
        let reg = TileRegistry::parse(SAMPLE);
        // Position 1 (Settlement), window 1/1 → tiles 0,1,2
        let ctx = reg.inject_context(1, 1, 1);
        assert_eq!(ctx.len(), 3);
    }
}

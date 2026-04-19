//! Tile Scoring — relevance scoring for knowledge tiles.

/// Score breakdown for a single tile against a query.
#[derive(Debug, Clone)]
pub struct TileScore {
    pub tile_id: usize,
    pub score: f64,
    pub keyword: f64,
    pub ghost: f64,
    pub belief: f64,
    pub domain: f64,
    pub frequency: f64,
}

pub fn score_tile(
    tile_id: usize,
    query: &str,
    question: &str,
    answer: &str,
    tags: &[String],
    domain: &str,
    confidence: f64,
    ghost_score: f64,
    use_count: u32,
) -> TileScore {
    let query_words: Vec<String> = query
        .split_whitespace()
        .map(|w| w.to_lowercase())
        .collect();

    if query_words.is_empty() {
        return TileScore {
            tile_id,
            score: 0.0,
            keyword: 0.0,
            ghost: 0.0,
            belief: 0.0,
            domain: 0.0,
            frequency: 0.0,
        };
    }

    let text_combined = format!("{} {}", question, answer);
    let text_words: Vec<String> = text_combined
        .split_whitespace()
        .map(|w| w.to_lowercase())
        .collect();

    let keyword_hits = query_words.iter().filter(|w| text_words.contains(w)).count();
    let keyword_component = keyword_hits as f64 / query_words.len() as f64;

    // Early exit if keyword component is essentially zero
    if keyword_component < 0.01 {
        return TileScore {
            tile_id,
            score: 0.0,
            keyword: 0.0,
            ghost: 0.0,
            belief: 0.0,
            domain: 0.0,
            frequency: 0.0,
        };
    }

    let ghost_component = 1.0 - ghost_score;
    let belief_component = confidence;

    let domain_lower = domain.to_lowercase();
    let domain_hits = query_words.iter().filter(|w| domain_lower.contains(w.as_str())).count();
    let domain_component = domain_hits as f64 / query_words.len() as f64;

    let frequency_component = (use_count as f64 / 100.0).min(1.0);

    let score = keyword_component * 0.30
        + ghost_component * 0.15
        + belief_component * 0.25
        + domain_component * 0.20
        + frequency_component * 0.10;

    TileScore {
        tile_id,
        score,
        keyword: keyword_component,
        ghost: ghost_component,
        belief: belief_component,
        domain: domain_component,
        frequency: frequency_component,
    }
}

pub fn rank_tiles(mut scores: Vec<TileScore>, limit: usize) -> Vec<TileScore> {
    scores.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    scores.truncate(limit);
    scores
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_perfect_keyword_match_gives_high_score() {
        let score = score_tile(
            0,
            "what is rust programming",
            "what is rust programming",
            "rust is a systems language",
            &[],
            "programming",
            0.9,
            0.0,
            50,
        );
        // All query words match → keyword_component = 1.0
        assert!(score.keyword > 0.9);
        assert!(score.score > 0.5);
    }

    #[test]
    fn test_zero_keyword_match_returns_score_zero() {
        let score = score_tile(
            1,
            "quantum physics photon",
            "cooking recipes pasta",
            "how to boil water",
            &[],
            "culinary",
            0.9,
            0.0,
            50,
        );
        assert_eq!(score.score, 0.0);
        assert_eq!(score.keyword, 0.0);
    }

    #[test]
    fn test_ghost_score_one_reduces_ghost_component_to_zero() {
        let score = score_tile(
            2,
            "rust programming",
            "rust programming guide",
            "learn rust today",
            &[],
            "programming",
            0.5,
            1.0, // ghost_score = 1.0 → ghost_component = 0.0
            0,
        );
        assert_eq!(score.ghost, 0.0);
    }

    #[test]
    fn test_high_confidence_boosts_belief_component() {
        let high = score_tile(
            3,
            "data analysis",
            "data analysis techniques",
            "statistical methods for data",
            &[],
            "data",
            0.95, // high confidence
            0.5,
            10,
        );
        let low = score_tile(
            4,
            "data analysis",
            "data analysis techniques",
            "statistical methods for data",
            &[],
            "data",
            0.1, // low confidence
            0.5,
            10,
        );
        assert!(high.belief > low.belief);
        assert!(high.score > low.score);
    }

    #[test]
    fn test_domain_match_boosts_domain_component() {
        let with_domain = score_tile(
            5,
            "rust programming",
            "rust programming tutorial",
            "guide to rust",
            &[],
            "rust programming language", // domain matches query words
            0.7,
            0.5,
            10,
        );
        let without_domain = score_tile(
            6,
            "rust programming",
            "rust programming tutorial",
            "guide to rust",
            &[],
            "cooking", // domain doesn't match
            0.7,
            0.5,
            10,
        );
        assert!(with_domain.domain > without_domain.domain);
        assert!(with_domain.score > without_domain.score);
    }

    #[test]
    fn test_rank_tiles_returns_correct_order_and_limit() {
        let s1 = score_tile(0, "hello world", "hello world example", "world is round", &[], "general", 0.9, 0.0, 50);
        let s2 = score_tile(1, "hello world", "hello world advanced", "learn more about the world", &[], "advanced", 0.5, 0.5, 10);
        let s3 = score_tile(2, "hello world", "hello world beginner", "start with hello in the world", &[], "beginner", 0.7, 0.2, 20);

        let scores = vec![s1.clone(), s2.clone(), s3.clone()];
        let ranked = rank_tiles(scores, 2);

        assert_eq!(ranked.len(), 2);
        assert!(ranked[0].score >= ranked[1].score);
    }

    #[test]
    fn test_frequency_capped_at_one() {
        let score_100 = score_tile(
            7,
            "test query",
            "test query result",
            "this is a test",
            &[],
            "testing",
            0.7,
            0.5,
            100,
        );
        let score_200 = score_tile(
            8,
            "test query",
            "test query result",
            "this is a test",
            &[],
            "testing",
            0.7,
            0.5,
            200,
        );
        assert_eq!(score_100.frequency, score_200.frequency);
        assert_eq!(score_100.score, score_200.score);
    }

    #[test]
    fn test_partial_keyword_match_scores_correctly() {
        let score = score_tile(
            9,
            "rust async programming",
            "rust language basics",
            "introduction to rust",
            &[],
            "software",
            0.8,
            0.3,
            30,
        );
        // 1 of 3 query words match ("rust") → keyword_component ≈ 0.333
        assert!(score.keyword > 0.0 && score.keyword < 0.5);
        assert!(score.score > 0.0);
    }

    #[test]
    fn test_empty_query_returns_zero_score() {
        let score = score_tile(
            10,
            "",
            "some question here",
            "some answer here",
            &[],
            "domain",
            0.9,
            0.0,
            100,
        );
        assert_eq!(score.score, 0.0);
    }

    #[test]
    fn test_rank_tiles_limit_larger_than_input() {
        let s1 = score_tile(0, "hello world", "hello world", "world", &[], "general", 0.9, 0.0, 50);
        let scores = vec![s1];
        let ranked = rank_tiles(scores, 10);
        assert_eq!(ranked.len(), 1);
    }
}

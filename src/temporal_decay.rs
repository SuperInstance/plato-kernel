//! Temporal Decay — ghost score aging and resurrection logic.

pub struct TileLike {
    pub ghost_score: f64,
}

pub fn apply_decay(tiles: &mut [TileLike]) {
    for tile in tiles.iter_mut() {
        tile.ghost_score = (tile.ghost_score + 0.04).min(1.0);
    }
}

pub fn should_resurrect(ghost_score: f64, relevance: f64) -> bool {
    relevance > 0.5
}

pub fn temporal_score(ghost_score: f64) -> f64 {
    1.0 - ghost_score
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_decay_increments_by_0_04() {
        let mut tiles = vec![TileLike { ghost_score: 0.5 }];
        apply_decay(&mut tiles);
        assert!((tiles[0].ghost_score - 0.54).abs() < 1e-9);
    }

    #[test]
    fn test_apply_decay_caps_at_1_0() {
        let mut tiles = vec![TileLike { ghost_score: 0.98 }];
        apply_decay(&mut tiles);
        assert_eq!(tiles[0].ghost_score, 1.0);
    }

    #[test]
    fn test_apply_decay_exactly_at_cap() {
        let mut tiles = vec![TileLike { ghost_score: 1.0 }];
        apply_decay(&mut tiles);
        assert_eq!(tiles[0].ghost_score, 1.0);
    }

    #[test]
    fn test_apply_decay_works_on_multiple_tiles() {
        let mut tiles = vec![
            TileLike { ghost_score: 0.0 },
            TileLike { ghost_score: 0.5 },
            TileLike { ghost_score: 0.97 },
        ];
        apply_decay(&mut tiles);
        assert!((tiles[0].ghost_score - 0.04).abs() < 1e-9);
        assert!((tiles[1].ghost_score - 0.54).abs() < 1e-9);
        assert_eq!(tiles[2].ghost_score, 1.0);
    }

    #[test]
    fn test_should_resurrect_true_when_relevance_above_0_5() {
        assert!(should_resurrect(0.9, 0.51));
        assert!(should_resurrect(0.0, 1.0));
        assert!(should_resurrect(0.5, 0.6));
    }

    #[test]
    fn test_should_resurrect_false_when_relevance_at_or_below_0_5() {
        assert!(!should_resurrect(0.5, 0.5));
        assert!(!should_resurrect(0.3, 0.0));
        assert!(!should_resurrect(0.9, 0.5));
    }

    #[test]
    fn test_temporal_score_returns_complement() {
        assert!((temporal_score(0.0) - 1.0).abs() < 1e-9);
        assert!((temporal_score(0.5) - 0.5).abs() < 1e-9);
        assert!((temporal_score(1.0) - 0.0).abs() < 1e-9);
        assert!((temporal_score(0.3) - 0.7).abs() < 1e-9);
    }
}

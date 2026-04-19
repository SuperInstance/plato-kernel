//! Deadband Engine — safety layer blocking prohibited actions
//! before they reach the LLM pipeline.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority { P0, P1, P2 }

#[derive(Debug, Clone)]
pub struct NegativeSpace {
    pub pattern: String,
    pub reason: String,
    pub severity: f64,
    pub confirmed: u32,
}

#[derive(Debug, Clone)]
pub struct Channel {
    pub id: String,
    pub description: String,
    pub confidence: f64,
}

#[derive(Debug, Clone)]
pub struct DeadbandCheck {
    pub passed: bool,
    pub p0_clear: bool,
    pub p1_clear: bool,
    pub violations: Vec<String>,
    pub recommended_channel: Option<String>,
}

pub struct DeadbandEngine {
    negatives: Vec<NegativeSpace>,
    channels: HashMap<String, Channel>,
}

impl DeadbandEngine {
    pub fn new() -> Self {
        let mut engine = Self {
            negatives: Vec::new(),
            channels: HashMap::new(),
        };
        engine.seed_defaults();
        engine
    }

    pub fn seed_defaults(&mut self) {
        let negatives = [
            ("rm -rf",      "Destructive filesystem wipe",     1.0),
            ("DROP TABLE",  "Destructive SQL operation",        1.0),
            ("DELETE FROM", "Mass data deletion",               0.9),
            ("chmod 777",   "Insecure permission change",       0.8),
            ("eval(",       "Dynamic code execution",           0.85),
            ("sudo rm",     "Privileged destructive removal",   0.9),
            ("> /dev/sda",  "Direct disk write",                1.0),
        ];
        for (pattern, reason, severity) in negatives {
            self.negatives.push(NegativeSpace {
                pattern: pattern.to_string(),
                reason: reason.to_string(),
                severity,
                confirmed: 0,
            });
        }

        let channels = [
            ("math",     "Mathematical computation",    0.9),
            ("search",   "Information retrieval",       0.85),
            ("navigate", "Navigation and routing",      0.8),
            ("analysis", "Data analysis",               0.85),
            ("safety",   "Safety and compliance checks",0.95),
        ];
        for (id, description, confidence) in channels {
            self.channels.insert(id.to_string(), Channel {
                id: id.to_string(),
                description: description.to_string(),
                confidence,
            });
        }
    }

    pub fn learn_negative(&mut self, pattern: &str, reason: &str, severity: f64) {
        self.negatives.push(NegativeSpace {
            pattern: pattern.to_string(),
            reason: reason.to_string(),
            severity,
            confirmed: 0,
        });
    }

    pub fn check_p0(&self, action: &str) -> Vec<String> {
        let action_lower = action.to_lowercase();
        self.negatives
            .iter()
            .filter(|n| n.severity >= 0.9 && action_lower.contains(&n.pattern.to_lowercase()))
            .map(|n| n.pattern.clone())
            .collect()
    }

    pub fn mark_channel(&mut self, id: &str, description: &str, confidence: f64) {
        self.channels.insert(id.to_string(), Channel {
            id: id.to_string(),
            description: description.to_string(),
            confidence,
        });
    }

    pub fn find_channels(&self, query: &str) -> Vec<&Channel> {
        let query_words: Vec<String> = query
            .split_whitespace()
            .map(|w| w.to_lowercase())
            .collect();

        let mut matched: Vec<&Channel> = self.channels.values().filter(|ch| {
            let ch_id = ch.id.to_lowercase();
            query_words.iter().any(|word| {
                ch_id.contains(word.as_str()) || word.contains(ch_id.as_str())
            })
        }).collect();

        matched.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));
        matched
    }

    pub fn check(&self, action: &str) -> DeadbandCheck {
        let action_lower = action.to_lowercase();

        // P0: severity >= 0.9
        let p0_violations: Vec<String> = self.negatives
            .iter()
            .filter(|n| n.severity >= 0.9 && action_lower.contains(&n.pattern.to_lowercase()))
            .map(|n| n.pattern.clone())
            .collect();

        // P1: severity >= 0.7 (but not already in P0)
        let p1_violations: Vec<String> = self.negatives
            .iter()
            .filter(|n| n.severity >= 0.7 && n.severity < 0.9 && action_lower.contains(&n.pattern.to_lowercase()))
            .map(|n| n.pattern.clone())
            .collect();

        let recommended_channel = self.find_channels(action).first().map(|c| c.id.clone());

        if !p0_violations.is_empty() {
            DeadbandCheck {
                passed: false,
                p0_clear: false,
                p1_clear: false,
                violations: p0_violations,
                recommended_channel,
            }
        } else if !p1_violations.is_empty() {
            DeadbandCheck {
                passed: false,
                p0_clear: true,
                p1_clear: false,
                violations: p1_violations,
                recommended_channel,
            }
        } else {
            DeadbandCheck {
                passed: true,
                p0_clear: true,
                p1_clear: true,
                violations: vec![],
                recommended_channel,
            }
        }
    }

    pub fn execute<F: FnOnce() -> String>(&self, action: &str, fallback: F) -> String {
        if self.check(action).passed {
            format!("EXECUTED: {}", action)
        } else {
            fallback()
        }
    }
}

impl Default for DeadbandEngine {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_p0_blocks_rm_rf() {
        let engine = DeadbandEngine::new();
        let check = engine.check("rm -rf /home/user");
        assert!(!check.passed);
        assert!(!check.p0_clear);
        assert!(check.violations.iter().any(|v| v.contains("rm -rf")));
    }

    #[test]
    fn test_p0_blocks_drop_table() {
        let engine = DeadbandEngine::new();
        let check = engine.check("DROP TABLE users");
        assert!(!check.passed);
        assert!(!check.p0_clear);
        assert!(check.violations.iter().any(|v| v.to_lowercase().contains("drop table")));
    }

    #[test]
    fn test_p0_blocks_delete_from() {
        // DELETE FROM has severity 0.9 so it's P0
        let engine = DeadbandEngine::new();
        let check = engine.check("DELETE FROM accounts WHERE id > 0");
        assert!(!check.passed);
        assert!(!check.p0_clear);
    }

    #[test]
    fn test_clean_action_passes() {
        let engine = DeadbandEngine::new();
        let check = engine.check("list all users in the database");
        assert!(check.passed);
        assert!(check.p0_clear);
        assert!(check.p1_clear);
        assert!(check.violations.is_empty());
    }

    #[test]
    fn test_learn_negative_adds_custom_pattern() {
        let mut engine = DeadbandEngine::new();
        engine.learn_negative("TRUNCATE", "Truncate table operation", 0.95);
        let check = engine.check("TRUNCATE users");
        assert!(!check.passed);
        assert!(!check.p0_clear);
    }

    #[test]
    fn test_find_channels_finds_math() {
        let engine = DeadbandEngine::new();
        let channels = engine.find_channels("what is 2+2");
        // "math" should not match "what", "is", "2+2" directly;
        // but the query word "what" doesn't contain "math" and "math" doesn't contain "what"
        // Actually let's use a query with "math" in it
        let channels2 = engine.find_channels("math problem: what is 2+2");
        assert!(!channels2.is_empty());
        assert!(channels2[0].id == "math");
    }

    #[test]
    fn test_execute_returns_fallback_for_blocked_action() {
        let engine = DeadbandEngine::new();
        let result = engine.execute("rm -rf /tmp/data", || "BLOCKED: unsafe operation".to_string());
        assert_eq!(result, "BLOCKED: unsafe operation");
    }

    #[test]
    fn test_execute_succeeds_for_clean_action() {
        let engine = DeadbandEngine::new();
        let result = engine.execute("list all files in /tmp", || "fallback".to_string());
        assert_eq!(result, "EXECUTED: list all files in /tmp");
    }

    #[test]
    fn test_deadband_check_fields_populated() {
        let engine = DeadbandEngine::new();
        let check = engine.check("rm -rf /critical");
        assert!(!check.passed);
        assert!(!check.p0_clear);
        assert!(!check.violations.is_empty());
    }

    #[test]
    fn test_p1_block_chmod_777() {
        // chmod 777 has severity 0.8, which is >= 0.7 but < 0.9 → P1 only
        let engine = DeadbandEngine::new();
        let check = engine.check("chmod 777 /etc/passwd");
        assert!(!check.passed);
        assert!(check.p0_clear);   // P0 is clear
        assert!(!check.p1_clear);  // P1 triggered
        assert!(check.violations.iter().any(|v| v.contains("chmod 777")));
    }

    #[test]
    fn test_check_p0_returns_p0_patterns_only() {
        let engine = DeadbandEngine::new();
        // chmod 777 is severity 0.8, should NOT appear in check_p0
        let p0 = engine.check_p0("chmod 777 /etc");
        assert!(p0.is_empty());
        // rm -rf is severity 1.0, should appear
        let p0b = engine.check_p0("rm -rf /tmp");
        assert!(!p0b.is_empty());
    }

    #[test]
    fn test_mark_channel_insert_update() {
        let mut engine = DeadbandEngine::new();
        engine.mark_channel("code", "Code execution channel", 0.88);
        let channels = engine.find_channels("code review");
        assert!(channels.iter().any(|c| c.id == "code"));
    }

    #[test]
    fn test_find_channels_sorted_by_confidence() {
        let engine = DeadbandEngine::new();
        // "safety" (0.95) and "search" (0.85) both contain 's'
        // use a query that matches multiple channels
        let channels = engine.find_channels("safety analysis");
        // safety (0.95) should come before analysis (0.85)
        if channels.len() >= 2 {
            assert!(channels[0].confidence >= channels[1].confidence);
        }
    }

    #[test]
    fn test_case_insensitive_check() {
        let engine = DeadbandEngine::new();
        // Test uppercase version
        let check = engine.check("RM -RF /home");
        assert!(!check.passed);
        assert!(!check.p0_clear);
    }

    #[test]
    fn test_dev_sda_blocked() {
        let engine = DeadbandEngine::new();
        let check = engine.check("echo 'data' > /dev/sda");
        assert!(!check.passed);
        assert!(!check.p0_clear);
    }
}

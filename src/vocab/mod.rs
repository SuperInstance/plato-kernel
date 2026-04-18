//! HAV (Higher Abstraction Vocabularies) — Rust-native vocabulary index.
//!
//! Seeded from the Python HAV engine at research/higher-abstraction-vocabularies.
//! Pure Rust: no Python dependency.  All term data is `&'static str`.

/// A single vocabulary term extracted from the HAV corpus.
#[derive(Debug, Clone)]
pub struct VocabTerm {
    pub id: &'static str,
    pub term: &'static str,
    pub domain: &'static str,
    /// Abstraction level: 0=concrete 1=pattern 2=behavior 3=domain 4=meta
    pub level: u8,
    pub definition: &'static str,
    pub related_terms: &'static [&'static str],
}

/// In-memory vocabulary index: search, domain filter, cross-domain bridge, suggest.
pub struct VocabIndex {
    terms: Vec<VocabTerm>,
}

impl VocabIndex {
    /// Build an index seeded with the default PLATO-relevant term set.
    pub fn new() -> Self { VocabIndex { terms: seed_terms() } }

    pub fn len(&self) -> usize { self.terms.len() }

    /// Fuzzy search: case-insensitive substring match on term name and definition.
    /// Exact term-name matches sort first.
    pub fn search(&self, query: &str) -> Vec<&VocabTerm> {
        if query.is_empty() { return vec![]; }
        let q = query.to_lowercase();
        let mut results: Vec<&VocabTerm> = self.terms.iter()
            .filter(|t| t.term.to_lowercase().contains(&q)
                     || t.definition.to_lowercase().contains(&q))
            .collect();
        results.sort_by_key(|t| {
            let tl = t.term.to_lowercase();
            if tl == q { 0u8 } else if tl.contains(&q) { 1 } else { 2 }
        });
        results
    }

    /// Return all terms for a domain (exact, case-insensitive).
    pub fn terms_for_domain(&self, domain: &str) -> Vec<&VocabTerm> {
        let d = domain.to_lowercase();
        self.terms.iter().filter(|t| t.domain.to_lowercase() == d).collect()
    }

    /// Find terms in `to_domain` whose `related_terms` references `term`.
    pub fn bridge(&self, term: &str, _from_domain: &str, to_domain: &str)
        -> Option<Vec<&VocabTerm>>
    {
        let t = term.to_lowercase();
        let d = to_domain.to_lowercase();
        let results: Vec<&VocabTerm> = self.terms.iter()
            .filter(|v| {
                let domain_ok = d.is_empty() || v.domain.to_lowercase() == d;
                let related = v.related_terms.iter().any(|r| r.to_lowercase() == t);
                domain_ok && related
            })
            .collect();
        if results.is_empty() { None } else { Some(results) }
    }

    /// Suggest terms by scoring significant-word overlap against term + definition.
    pub fn suggest(&self, intent: &str) -> Vec<&VocabTerm> {
        let words: Vec<String> = intent.split_whitespace()
            .map(|w| w.to_lowercase().trim_matches(|c: char| !c.is_alphanumeric()).to_string())
            .filter(|w| w.len() > 3)
            .collect();
        if words.is_empty() { return self.search(intent); }
        let mut scored: Vec<(usize, &VocabTerm)> = self.terms.iter()
            .map(|t| {
                let hay = format!("{} {}", t.term, t.definition).to_lowercase();
                let score = words.iter().filter(|w| hay.contains(w.as_str())).count();
                (score, t)
            })
            .filter(|(s, _)| *s > 0)
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0));
        scored.into_iter().map(|(_, t)| t).take(10).collect()
    }

    /// Return all terms whose name appears (as a substring) in `body`.
    /// Used to auto-tag tiles.
    pub fn tag_tile(&self, body: &str) -> Vec<&VocabTerm> {
        let lower = body.to_lowercase();
        self.terms.iter().filter(|t| lower.contains(t.term)).collect()
    }
}

impl Default for VocabIndex {
    fn default() -> Self { Self::new() }
}

// ── Seed Data (55 terms across 9 domains) ─────────────────────────────────

fn seed_terms() -> Vec<VocabTerm> { vec![
    // uncertainty
    VocabTerm { id:"u1", term:"confidence", domain:"uncertainty", level:3,
        definition:"A 0-1 value representing certainty about a proposition or observation",
        related_terms:&["trust","belief","probability","calibration"] },
    VocabTerm { id:"u2", term:"harmonic-mean-fusion", domain:"uncertainty", level:1,
        definition:"Combining independent confidence sources via 1/(1/a + 1/b)",
        related_terms:&["bayesian-update","weighted-average","consensus"] },
    VocabTerm { id:"u3", term:"trust", domain:"uncertainty", level:3,
        definition:"Slowly-accumulating confidence in another agent's reliability",
        related_terms:&["confidence","reputation","credit-assignment"] },
    VocabTerm { id:"u4", term:"bayesian-update", domain:"uncertainty", level:1,
        definition:"Adjusting beliefs based on new evidence using prior and likelihood",
        related_terms:&["harmonic-mean-fusion","confidence","learning-rate"] },
    VocabTerm { id:"u5", term:"entropy", domain:"uncertainty", level:3,
        definition:"Measure of uncertainty or surprise in a probability distribution",
        related_terms:&["uncertainty","surprise","information","exploration"] },
    VocabTerm { id:"u6", term:"calibration", domain:"uncertainty", level:2,
        definition:"How well an agent's confidence matches its actual accuracy",
        related_terms:&["confidence","self-model","meta-cognition"] },
    VocabTerm { id:"u7", term:"information", domain:"uncertainty", level:3,
        definition:"Reduction in uncertainty gained from an observation or message",
        related_terms:&["entropy","confidence","attention"] },
    // memory
    VocabTerm { id:"m1", term:"working-memory", domain:"memory", level:0,
        definition:"Fast, limited-capacity buffer for current task context",
        related_terms:&["attention","focus","registers","confidence"] },
    VocabTerm { id:"m2", term:"episodic-memory", domain:"memory", level:3,
        definition:"Specific experiences stored with timestamp and emotional valence",
        related_terms:&["semantic-memory","procedural-memory","narrative","learning"] },
    VocabTerm { id:"m3", term:"semantic-memory", domain:"memory", level:3,
        definition:"General knowledge extracted from many episodes — the wisdom layer",
        related_terms:&["episodic-memory","procedural-memory","world-model"] },
    VocabTerm { id:"m4", term:"procedural-memory", domain:"memory", level:3,
        definition:"How to do things — skills, patterns, automatic behaviors",
        related_terms:&["working-memory","skill","reflex","habit"] },
    VocabTerm { id:"m5", term:"forgetting-curve", domain:"memory", level:1,
        definition:"Exponential decay of memory strength over time without rehearsal",
        related_terms:&["memory","decay","spaced-repetition","episodic-memory"] },
    VocabTerm { id:"m6", term:"consolidation", domain:"memory", level:2,
        definition:"Transfer from short-term to long-term memory during rest",
        related_terms:&["episodic-memory","semantic-memory","circadian-rhythm"] },
    VocabTerm { id:"m7", term:"chunking", domain:"memory", level:1,
        definition:"Grouping individual items into larger meaningful units to expand capacity",
        related_terms:&["working-memory","abstraction","hierarchy","pattern"] },
    VocabTerm { id:"m8", term:"rehearsal", domain:"memory", level:1,
        definition:"Active recall of a memory to strengthen it and reset its decay timer",
        related_terms:&["forgetting-curve","consolidation","spaced-repetition"] },
    // coordination
    VocabTerm { id:"c1", term:"stigmergy", domain:"coordination", level:2,
        definition:"Indirect coordination through environment modification — agents leave traces others react to",
        related_terms:&["gossip","consensus","swarm","tuplespace"] },
    VocabTerm { id:"c2", term:"consensus", domain:"coordination", level:2,
        definition:"Agreement among agents on a shared state or decision",
        related_terms:&["deliberation","voting","agreement","quorum"] },
    VocabTerm { id:"c3", term:"deliberation", domain:"coordination", level:2,
        definition:"Structured consideration of options leading to a decision",
        related_terms:&["consensus","decision-making","convergence","filtration"] },
    VocabTerm { id:"c4", term:"gossip", domain:"coordination", level:1,
        definition:"Agents sharing information with random neighbors, spreading knowledge through the network",
        related_terms:&["stigmergy","broadcast","consensus","trust"] },
    VocabTerm { id:"c5", term:"swarm", domain:"coordination", level:2,
        definition:"Collective behavior emerging from simple local rules without central control",
        related_terms:&["stigmergy","emergence","consensus","decentralized"] },
    VocabTerm { id:"c6", term:"emergence", domain:"coordination", level:4,
        definition:"Complex global behavior arising from simple local interactions",
        related_terms:&["swarm","stigmergy","self-organization","complexity"] },
    VocabTerm { id:"c7", term:"quorum", domain:"coordination", level:1,
        definition:"Minimum number of agents required for a decision to be valid",
        related_terms:&["consensus","voting","byzantine","election"] },
    VocabTerm { id:"c8", term:"leader-election", domain:"coordination", level:1,
        definition:"Process of selecting a coordinator from a group of peers",
        related_terms:&["quorum","consensus","heartbeat","fault-tolerance"] },
    // learning
    VocabTerm { id:"l1", term:"exploration", domain:"learning", level:2,
        definition:"Trying new actions to discover potentially better strategies",
        related_terms:&["exploitation","curiosity","entropy","discovery"] },
    VocabTerm { id:"l2", term:"exploitation", domain:"learning", level:2,
        definition:"Using currently known best actions to maximize reward",
        related_terms:&["exploration","optimization","convergence","habit"] },
    VocabTerm { id:"l3", term:"credit-assignment", domain:"learning", level:4,
        definition:"Determining which action caused an outcome when many actions contribute",
        related_terms:&["learning","causality","attribution","provenance"] },
    VocabTerm { id:"l4", term:"transfer-learning", domain:"learning", level:1,
        definition:"Applying knowledge from one domain to a different but related domain",
        related_terms:&["generalization","abstraction","analogy","genepool"] },
    VocabTerm { id:"l5", term:"curriculum", domain:"learning", level:1,
        definition:"Structured sequence of learning tasks progressing from easy to hard",
        related_terms:&["skill","learning-rate","scaffolding","progression"] },
    VocabTerm { id:"l6", term:"spaced-repetition", domain:"learning", level:1,
        definition:"Reviewing material at increasing intervals to maximize retention",
        related_terms:&["forgetting-curve","rehearsal","consolidation","memory"] },
    VocabTerm { id:"l7", term:"overfitting", domain:"learning", level:2,
        definition:"Learning training examples too well, failing to generalize to new situations",
        related_terms:&["generalization","regularization","robustness"] },
    // biological
    VocabTerm { id:"b1", term:"homeostasis", domain:"biological", level:3,
        definition:"Maintenance of stable internal conditions despite external changes",
        related_terms:&["feedback-loop","adaptation","setpoint","circadian-rhythm"] },
    VocabTerm { id:"b2", term:"apoptosis", domain:"biological", level:3,
        definition:"Programmed self-termination when fitness drops below threshold — graceful shutdown",
        related_terms:&["shutdown","graceful-degradation","fitness","resource-release"] },
    VocabTerm { id:"b3", term:"circadian-rhythm", domain:"biological", level:1,
        definition:"Time-based modulation of behavior and capability following a periodic cycle",
        related_terms:&["energy","instinct","homeostasis","scheduling"] },
    VocabTerm { id:"b4", term:"neurotransmitter", domain:"biological", level:3,
        definition:"Chemical signal modulating neural activity — confidence amplifier for the fleet",
        related_terms:&["confidence","trust","attention","emotion"] },
    VocabTerm { id:"b5", term:"instinct", domain:"biological", level:3,
        definition:"Inherited behavioral program that drives action without conscious reasoning",
        related_terms:&["reflex","energy","opcode","habit"] },
    // architecture
    VocabTerm { id:"a1", term:"tiling", domain:"architecture", level:1,
        definition:"Splitting a document into independent semantic nodes for conditional injection",
        related_terms:&["anchor","context-window","knowledge-substrate","perspective"] },
    VocabTerm { id:"a2", term:"anchor", domain:"architecture", level:0,
        definition:"A named pointer to a tile or knowledge node enabling context jumps",
        related_terms:&["tiling","reference","jump","tutor"] },
    VocabTerm { id:"a3", term:"constraint", domain:"architecture", level:1,
        definition:"A rule that limits or requires behavior in a system; checked at runtime",
        related_terms:&["assertion","invariant","policy","perspective"] },
    VocabTerm { id:"a4", term:"plugin", domain:"architecture", level:1,
        definition:"A modular capability unit that can be loaded or unloaded at runtime",
        related_terms:&["tier","mount","capability","dependency"] },
    VocabTerm { id:"a5", term:"perspective", domain:"architecture", level:1,
        definition:"Filtered view of a system based on identity and active constraints",
        related_terms:&["identity","constraint","projection","tiling"] },
    // control-theory
    VocabTerm { id:"ct1", term:"feedback-loop", domain:"control-theory", level:1,
        definition:"System output influences its own input to regulate or amplify behavior",
        related_terms:&["homeostasis","pid-controller","setpoint","stability"] },
    VocabTerm { id:"ct2", term:"setpoint", domain:"control-theory", level:0,
        definition:"Target value a control system attempts to maintain",
        related_terms:&["feedback-loop","homeostasis","stability","convergence"] },
    VocabTerm { id:"ct3", term:"convergence", domain:"control-theory", level:2,
        definition:"Process of approaching a stable fixed point or consensus state",
        related_terms:&["stability","consensus","exploitation","setpoint"] },
    VocabTerm { id:"ct4", term:"stability", domain:"control-theory", level:2,
        definition:"Property of a system that returns to equilibrium after perturbation",
        related_terms:&["homeostasis","feedback-loop","resilience","convergence"] },
    VocabTerm { id:"ct5", term:"pid-controller", domain:"control-theory", level:0,
        definition:"Proportional-Integral-Derivative controller for smooth convergence to setpoint",
        related_terms:&["feedback-loop","setpoint","convergence","control"] },
    // complexity
    VocabTerm { id:"cx1", term:"phase-transition", domain:"complexity", level:4,
        definition:"Abrupt qualitative change in system behavior at a threshold parameter value",
        related_terms:&["emergence","bifurcation","criticality","attractor"] },
    VocabTerm { id:"cx2", term:"attractor", domain:"complexity", level:4,
        definition:"State or set of states a dynamic system tends toward over time",
        related_terms:&["stability","convergence","equilibrium","phase-transition"] },
    VocabTerm { id:"cx3", term:"bifurcation", domain:"complexity", level:4,
        definition:"Point where a small parameter change causes a qualitative shift in behavior",
        related_terms:&["phase-transition","criticality","chaos","emergence"] },
    VocabTerm { id:"cx4", term:"self-organization", domain:"complexity", level:2,
        definition:"Spontaneous order arising from local interactions without central control",
        related_terms:&["emergence","swarm","stigmergy","decentralized"] },
    VocabTerm { id:"cx5", term:"criticality", domain:"complexity", level:4,
        definition:"Operating at the boundary between order and chaos for maximum adaptability",
        related_terms:&["phase-transition","emergence","complexity","bifurcation"] },
    // epistemology
    VocabTerm { id:"e1", term:"abstraction", domain:"epistemology", level:4,
        definition:"Removing specific details to reveal general patterns applicable across contexts",
        related_terms:&["generalization","compression","hierarchy","chunking"] },
    VocabTerm { id:"e2", term:"analogy", domain:"epistemology", level:4,
        definition:"Mapping structure from one domain to another to enable transfer of insight",
        related_terms:&["transfer-learning","bridge","metaphor","abstraction"] },
    VocabTerm { id:"e3", term:"compression", domain:"epistemology", level:4,
        definition:"Reducing representation size while preserving essential information",
        related_terms:&["chunking","abstraction","encoding","information"] },
    // systems-thinking
    VocabTerm { id:"s1", term:"leverage-point", domain:"systems-thinking", level:4,
        definition:"Place in a system where a small intervention produces large systemic change",
        related_terms:&["bifurcation","feedback-loop","emergence","constraint"] },
    VocabTerm { id:"s2", term:"resilience", domain:"systems-thinking", level:2,
        definition:"Ability to absorb disturbances and maintain core function",
        related_terms:&["homeostasis","stability","redundancy","robustness"] },
    VocabTerm { id:"s3", term:"redundancy", domain:"systems-thinking", level:1,
        definition:"Backup capacity that maintains function when primary components fail",
        related_terms:&["resilience","fault-tolerance","robustness","reliability"] },
]}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_has_at_least_50_terms() {
        let ix = VocabIndex::new();
        assert!(ix.len() >= 50, "got only {} terms", ix.len());
    }

    #[test]
    fn search_finds_term_by_name() {
        let ix = VocabIndex::new();
        let r = ix.search("confidence");
        assert!(!r.is_empty());
        assert_eq!(r[0].term, "confidence");
    }

    #[test]
    fn search_finds_term_by_definition_word() {
        let ix = VocabIndex::new();
        // "decay" appears in forgetting-curve's definition
        let r = ix.search("decay");
        assert!(r.iter().any(|t| t.term == "forgetting-curve"),
            "forgetting-curve not found via 'decay'");
    }

    #[test]
    fn search_is_case_insensitive() {
        let ix = VocabIndex::new();
        assert_eq!(ix.search("entropy").len(), ix.search("ENTROPY").len());
        assert!(!ix.search("entropy").is_empty());
    }

    #[test]
    fn search_returns_empty_for_blank_query() {
        assert!(VocabIndex::new().search("").is_empty());
    }

    #[test]
    fn terms_for_domain_returns_only_that_domain() {
        let ix = VocabIndex::new();
        let terms = ix.terms_for_domain("uncertainty");
        assert!(!terms.is_empty());
        assert!(terms.iter().all(|t| t.domain == "uncertainty"));
    }

    #[test]
    fn terms_for_domain_unknown_returns_empty() {
        assert!(VocabIndex::new().terms_for_domain("xyzzy-nonexistent").is_empty());
    }

    #[test]
    fn bridge_finds_related_terms_in_target_domain() {
        let ix = VocabIndex::new();
        // complexity terms reference "emergence" in related_terms
        let result = ix.bridge("emergence", "coordination", "complexity");
        assert!(result.is_some(), "expected bridge from emergence → complexity");
        let bridged = result.unwrap();
        assert!(!bridged.is_empty());
        assert!(bridged.iter().all(|t| t.domain == "complexity"));
    }

    #[test]
    fn bridge_returns_none_for_unknown_term() {
        assert!(VocabIndex::new().bridge("xyzzy", "", "memory").is_none());
    }

    #[test]
    fn suggest_finds_swarm_from_intent() {
        let ix = VocabIndex::new();
        let r = ix.suggest("agents coordinate without central control");
        assert!(r.iter().any(|t| t.term == "swarm" || t.term == "stigmergy"),
            "got: {:?}", r.iter().map(|t| t.term).collect::<Vec<_>>());
    }

    #[test]
    fn tag_tile_returns_matching_terms_for_body() {
        let ix = VocabIndex::new();
        let tags = ix.tag_tile("The fleet uses consensus and deliberation.");
        assert!(tags.iter().any(|t| t.term == "consensus"));
    }

    #[test]
    fn tag_tile_returns_empty_for_unrelated_body() {
        let ix = VocabIndex::new();
        let tags = ix.tag_tile("The weather today is sunny and warm.");
        assert!(tags.is_empty(),
            "unexpected: {:?}", tags.iter().map(|t| t.term).collect::<Vec<_>>());
    }

    #[test]
    fn index_covers_at_least_5_domains() {
        let ix = VocabIndex::new();
        let mut domains: Vec<&str> = ix.terms.iter().map(|t| t.domain).collect();
        domains.sort(); domains.dedup();
        assert!(domains.len() >= 5, "got: {:?}", domains);
    }

    #[test]
    fn all_terms_have_valid_level() {
        let ix = VocabIndex::new();
        for t in ix.terms.iter() {
            assert!(t.level <= 4, "'{}' has level {}", t.term, t.level);
        }
    }
}

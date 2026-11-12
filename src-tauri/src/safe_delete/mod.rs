// SPDX-License-Identifier: GPL-3.0-or-later
//
// Safe-to-Delete intelligence layer.
// Master architecture Chapter 6.
//
// Phase 1: declarative rule engine (50+ built-in rules).
// Phase 2 (v1.4 spec, code v2): TFLite-tiered ML scorer (Chapter 6.5).
//
// Principles:
//   * Score 0-100, colors: 0-30 red, 31-60 yellow, 61-85 green, 86-100 blue.
//   * Inference is NEVER on the scan critical path (Chapter 6.5.1).
//   * ML cache (Chapter 6.5.2): not recomputed when mtime is unchanged.
//   * Battery + idle throttle (Chapter 6.5.3).
//   * v2 stub trait (Chapter 26.1).

pub mod rules;
pub mod scorer;
pub mod user_rules;

pub use scorer::RulesScorer;

pub use rules::{match_rule, Rule, RULES};
pub use user_rules::{
    add_rule, delete_rule, list_active_snapshots, list_rules, match_user_rule, toggle_rule,
    UserPatternType, UserRule, UserRuleSnapshot,
};

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ScoredFile {
    pub path: String,
    pub score: u8, // 0..=100
    pub matched_rules: Vec<RuleMatch>,
}

#[derive(Debug, Serialize)]
pub struct RuleMatch {
    pub rule_id: String,
    pub explanation: String,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub enum ScoreTier {
    Danger,  // 0-30  red
    Caution, // 31-60 yellow
    Likely,  // 61-85 green
    Cache,   // 86-100 blue
}

impl ScoreTier {
    pub fn from_score(score: u8) -> Self {
        match score {
            0..=30 => Self::Danger,
            31..=60 => Self::Caution,
            61..=85 => Self::Likely,
            _ => Self::Cache,
        }
    }
}

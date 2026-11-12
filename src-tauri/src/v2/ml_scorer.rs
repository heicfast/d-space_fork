// SPDX-License-Identifier: GPL-3.0-or-later
//
// 26.1 ML Safe-Delete Scorer — v2 stub.
//
// The Phase 1 rule engine (53 rules, `crate::safe_delete::rules`) is deterministic.
// Phase 2 adds a TFLite-tiered ML scorer:
//
//   Tier 1: deterministic rule engine (current) — always runs
//   Tier 2: local ML inference (TFLite, small model ~5 MB) — async cache
//   Tier 3: cloud inference (premium, opt-in) — large model
//
// Principle: ML stays OUTSIDE the scan critical path (Chapter 6.5.1). The
// SQLite `ml_scores` table is the cache (Chapter 6.5.2 — schema already
// exists). No recomputation while mtime is unchanged.

use crate::error::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum InferenceTier {
    /// Chapter 6.5.1 — deterministic rule engine.
    Rules,
    /// Chapter 6.5.1 — local TFLite inference.
    LocalMl,
    /// Chapter 6.5.1 — cloud inference (premium).
    CloudMl,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MlScoreRecord {
    pub path_hash_hex: String,
    pub mtime_unix: i64,
    pub score: u8,
    pub tier: InferenceTier,
    pub computed_at_unix: i64,
}

/// Chapter 6.5 — ML scorer trait.
///
/// In v1.x the concrete impl is `safe_delete::RulesScorer`; the tract path
/// comes via the optional `ml-tflite` feature. Caller: NEVER invoked from
/// the scan-time path; UI drilldown or idle background job (Chapter 6.5.3).
///
/// The `Send + Sync` constraint will be added in v0.2 — for now `RulesScorer`
/// holds a SQLite `&Connection` ref (Connection is !Sync), Phase 1 caller
/// is single-threaded. When the background indexing implementation is
/// added, it will be made thread-safe via the `Arc<Mutex<Connection>>` pattern.
pub trait MlSafeDeleteScorer {
    /// Score for a single file. Fast on a cache hit, model inference on a miss.
    fn score_one(&self, path: &str, mtime_unix: i64) -> Result<MlScoreRecord>;

    /// Batch — for idle background indexing (Chapter 6.5.3).
    fn score_batch(&self, paths: &[(&str, i64)]) -> Result<Vec<MlScoreRecord>>;

    /// Read a known score from the cache, None on a miss.
    fn lookup_cache(&self, path_hash: &[u8; 32]) -> Result<Option<MlScoreRecord>>;

    /// The inference tier.
    fn tier(&self) -> InferenceTier;
}

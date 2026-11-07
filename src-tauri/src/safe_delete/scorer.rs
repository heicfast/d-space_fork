// SPDX-License-Identifier: GPL-3.0-or-later
//
// Safe-delete scorer concrete impl — Bölüm 6.5.
//
// Tier 1 (RulesScorer): deterministik kural motoru (mevcut `RULES` +
// user_rules tablosu) + `ml_scores` SQLite cache lookup. **Daima** çalışır,
// derleme zamanı bağımlılığı sıfır.
//
// Tier 2 (TractMlScorer, opsiyonel): `ml-tflite` feature aktif olduğunda
// tract-tflite runtime ile inference. Model `assets/models/safe_delete_v1.tflite`
// (henüz yok — v0.3 sprint'inde train edilecek). Cold start ~50-200 ms,
// idle background indexing için (Bölüm 6.5.3).
//
// Tier 3 (CloudScorer): premium opt-in, v2.0. Trait stub `crate::v2::ml_scorer`.

use crate::error::{Error, Result};
use crate::safe_delete::UserRuleSnapshot;
use crate::v2::{InferenceTier, MlSafeDeleteScorer, MlScoreRecord};
use blake3::Hasher;
use rusqlite::{params, Connection};
use std::time::{SystemTime, UNIX_EPOCH};

/// Tier 1 — kural motoru. `match_user_rule` öncelikli + `match_rule` built-in.
/// SQLite cache lookup (`ml_scores`) Tier 2/3 ile paylaşılan görsel —
/// burada `tier=Rules` ile yazılır, tutarlılık için.
pub struct RulesScorer<'a> {
    pub conn: &'a Connection,
    /// Gemini review B7 — construction anında yüklenen aktif user rules
    /// (Bölüm 6.4 önceliği). Kural seti nadiren değişir; scorer kısa ömürlü
    /// tutulmalı.
    user_rules: Vec<UserRuleSnapshot>,
}

fn path_hash(path: &str) -> [u8; 32] {
    let mut h = Hasher::new();
    h.update(path.as_bytes());
    *h.finalize().as_bytes()
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn score_path_via_rules(path: &str, is_dir: bool, user_rules: &[UserRuleSnapshot]) -> u8 {
    let name = std::path::Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string());
    // Gemini review B7 — Bölüm 6.4 önceliği: user rules önce (build_tree
    // ile aynı sıra), sonra built-in. Eskiden user rules yok sayılıyordu.
    if let Some(ur) = crate::safe_delete::match_user_rule(user_rules, &name, is_dir) {
        return ur.score;
    }
    match crate::safe_delete::match_rule(&name, is_dir) {
        Some(r) => r.score,
        None => 50, // bilinmeyen → İNCELE tier varsayılan
    }
}

impl<'a> RulesScorer<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        let user_rules = crate::safe_delete::list_active_snapshots(conn).unwrap_or_default();
        Self { conn, user_rules }
    }

    /// `ml_scores` cache'inden okur; miss ise rules üzerinden skorlar + cache'le.
    fn score_and_cache(&self, path: &str, mtime_unix: i64, is_dir: bool) -> Result<MlScoreRecord> {
        let hash = path_hash(path);

        // Hit check (mtime eşleşmeli; aksi durumda yeniden hesapla)
        let cached: Option<(u8, i64, i64)> = self
            .conn
            .query_row(
                "SELECT score, mtime, computed_at FROM ml_scores WHERE path_hash = ?1",
                params![&hash[..]],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)? as u8,
                        r.get::<_, i64>(1)?,
                        r.get::<_, i64>(2)?,
                    ))
                },
            )
            .ok();
        if let Some((score, cached_mtime, computed_at)) = cached {
            if cached_mtime == mtime_unix {
                return Ok(MlScoreRecord {
                    path_hash_hex: hex32(&hash),
                    mtime_unix,
                    score,
                    tier: InferenceTier::Rules,
                    computed_at_unix: computed_at,
                });
            }
        }

        let score = score_path_via_rules(path, is_dir, &self.user_rules);
        let computed_at = now_unix();

        self.conn
            .execute(
                // Gemini review B22 — model_version kolonu INTEGER NOT NULL
                // (migration 0001); TEXT 'rules-v1' affinity ile sessizce TEXT
                // kalıyordu. Kural motoru = tier 1.
                "INSERT INTO ml_scores (path_hash, mtime, score, model_version, computed_at)
                 VALUES (?1, ?2, ?3, 1, ?4)
                 ON CONFLICT(path_hash) DO UPDATE SET
                     mtime = excluded.mtime,
                     score = excluded.score,
                     model_version = excluded.model_version,
                     computed_at = excluded.computed_at",
                params![&hash[..], mtime_unix, score as i64, computed_at],
            )
            .map_err(|e| Error::Db(format!("ml_scores upsert: {}", e)))?;

        Ok(MlScoreRecord {
            path_hash_hex: hex32(&hash),
            mtime_unix,
            score,
            tier: InferenceTier::Rules,
            computed_at_unix: computed_at,
        })
    }
}

fn hex32(bytes: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

impl<'a> MlSafeDeleteScorer for RulesScorer<'a> {
    fn score_one(&self, path: &str, mtime_unix: i64) -> Result<MlScoreRecord> {
        // is_dir bilinmiyor (path metadata yapılmıyor — pure-rule scoring),
        // varsayılan false.
        self.score_and_cache(path, mtime_unix, false)
    }

    fn score_batch(&self, paths: &[(&str, i64)]) -> Result<Vec<MlScoreRecord>> {
        let mut out = Vec::with_capacity(paths.len());
        for (path, mtime) in paths {
            out.push(self.score_and_cache(path, *mtime, false)?);
        }
        Ok(out)
    }

    fn lookup_cache(&self, path_hash: &[u8; 32]) -> Result<Option<MlScoreRecord>> {
        let row = self
            .conn
            .query_row(
                "SELECT mtime, score, computed_at FROM ml_scores WHERE path_hash = ?1",
                params![&path_hash[..]],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, i64>(1)? as u8,
                        r.get::<_, i64>(2)?,
                    ))
                },
            )
            .ok();
        Ok(row.map(|(mtime, score, computed_at)| MlScoreRecord {
            path_hash_hex: hex32(path_hash),
            mtime_unix: mtime,
            score,
            tier: InferenceTier::Rules,
            computed_at_unix: computed_at,
        }))
    }

    fn tier(&self) -> InferenceTier {
        InferenceTier::Rules
    }
}

/// Tier 2 — `ml-tflite` feature aktifse tract-tflite ile inference.
/// **Henüz model dosyası yok**; v0.3 sprint'inde train + paketleme.
/// Şu an placeholder olarak `unimplemented!()` döner; cargo feature gate
/// ile derleme zamanında devre dışı.
#[cfg(feature = "ml-tflite")]
pub struct TractMlScorer {
    // `tract` runtime burada tutulacak — placeholder.
}

#[cfg(feature = "ml-tflite")]
impl TractMlScorer {
    pub fn load_from_path(_model_path: &std::path::Path) -> Result<Self> {
        // v0.3: tract::tflite::tflite().model_for_path(model_path)...
        Err(Error::Db(
            "TractMlScorer henüz yok — model dosyası v0.3 sprint'inde".into(),
        ))
    }
}

#[cfg(feature = "ml-tflite")]
impl MlSafeDeleteScorer for TractMlScorer {
    fn score_one(&self, _path: &str, _mtime_unix: i64) -> Result<MlScoreRecord> {
        unimplemented!("TractMlScorer score_one — v0.3 sprint")
    }
    fn score_batch(&self, _paths: &[(&str, i64)]) -> Result<Vec<MlScoreRecord>> {
        unimplemented!("TractMlScorer score_batch — v0.3 sprint")
    }
    fn lookup_cache(&self, _path_hash: &[u8; 32]) -> Result<Option<MlScoreRecord>> {
        unimplemented!("TractMlScorer lookup_cache — v0.3 sprint")
    }
    fn tier(&self) -> InferenceTier {
        InferenceTier::LocalMl
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        let migrations = rusqlite_migration::Migrations::new(vec![
            rusqlite_migration::M::up(include_str!("../db/migrations/0001_initial.sql")),
            rusqlite_migration::M::up(include_str!("../db/migrations/0002_user_rules.sql")),
        ]);
        migrations.to_latest(&mut conn).unwrap();
        conn
    }

    #[test]
    fn rules_scorer_known_file_uses_built_in_score() {
        let conn = fresh_db();
        let scorer = RulesScorer::new(&conn);
        let rec = scorer
            .score_one(r"C:\Projects\node_modules", 0)
            .expect("score ok");
        // node_modules built-in rule → 95
        assert_eq!(rec.score, 95);
        assert!(matches!(rec.tier, InferenceTier::Rules));
    }

    #[test]
    fn rules_scorer_unknown_falls_back_to_50() {
        let conn = fresh_db();
        let scorer = RulesScorer::new(&conn);
        let rec = scorer
            .score_one(r"C:\Some\rArEnAmE12345.x", 0)
            .expect("score ok");
        assert_eq!(rec.score, 50);
    }

    #[test]
    fn rules_scorer_caches_and_retrieves() {
        let conn = fresh_db();
        let scorer = RulesScorer::new(&conn);
        let path = r"C:\app\debug.log";
        let mtime = 1_700_000_000i64;

        let first = scorer.score_one(path, mtime).unwrap();
        assert_eq!(first.score, 88); // ext-log

        // İkinci çağrı cache'ten gelir (aynı mtime).
        let hash = path_hash(path);
        let cached = scorer.lookup_cache(&hash).unwrap();
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().score, 88);
    }

    #[test]
    fn rules_scorer_mtime_change_recomputes() {
        let conn = fresh_db();
        let scorer = RulesScorer::new(&conn);
        let path = r"C:\big\file.dat";

        let r1 = scorer.score_one(path, 100).unwrap();
        let r2 = scorer.score_one(path, 200).unwrap();
        // Skor aynı (kural değişmedi) ama mtime farklı kaydedilmiş olmalı.
        assert_eq!(r1.score, r2.score);
        assert_eq!(r2.mtime_unix, 200);
    }

    #[test]
    fn batch_returns_same_length() {
        let conn = fresh_db();
        let scorer = RulesScorer::new(&conn);
        let recs = scorer
            .score_batch(&[
                (r"C:\a\node_modules", 1),
                (r"C:\b\.cache", 2),
                (r"C:\c\Documents", 3),
            ])
            .unwrap();
        assert_eq!(recs.len(), 3);
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later
//
// User-Defined Rules — Master architecture Chapter 6.4.
//
// Principle: the built-in `RULES` const array (Chapter 6.2, 63 rules) is
// static and fixed at compile time. User rules are read from the DB at
// **runtime**; a snapshot is taken before the build_tree call, and user
// rules are tried first while placing Nodes; no match falls to the built-in engine.
//
// Pattern type: 'name' (full name, case-insensitive) or 'extension'.
// Glob/regex in the v0.2 sprint (Chapter 6.4 "user-defined" detail).

use crate::error::{Error, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UserPatternType {
    Name,
    Extension,
}

impl UserPatternType {
    fn as_str(&self) -> &'static str {
        match self {
            UserPatternType::Name => "name",
            UserPatternType::Extension => "extension",
        }
    }

    fn from_str(s: &str) -> Option<Self> {
        match s {
            "name" => Some(UserPatternType::Name),
            "extension" => Some(UserPatternType::Extension),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct UserRule {
    pub id: i64,
    pub pattern: String,
    pub pattern_type: UserPatternType,
    pub score: u8,
    pub explanation: String,
    pub enabled: bool,
    pub created_at_unix: i64,
    pub updated_at_unix: i64,
}

/// Cloneable snapshot for build-time compatibility. `match_user_rule` runs
/// in a loop inside build_tree; we keep a snapshot instead of querying the DB on every iteration.
#[derive(Debug, Clone)]
pub struct UserRuleSnapshot {
    pub pattern_lower: String,
    pub pattern_type: UserPatternType,
    pub score: u8,
    pub explanation: String,
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn list_rules(conn: &Connection) -> Result<Vec<UserRule>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, pattern, pattern_type, score, explanation, enabled,
                    created_at, updated_at
             FROM user_rules
             ORDER BY created_at DESC",
        )
        .map_err(|e| Error::Db(format!("user_rules prepare: {}", e)))?;

    let rows = stmt
        .query_map([], |r| {
            let ptype_str: String = r.get(2)?;
            let ptype = UserPatternType::from_str(&ptype_str).ok_or_else(|| {
                rusqlite::Error::FromSqlConversionFailure(
                    2,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("invalid pattern_type: {}", ptype_str),
                    )),
                )
            })?;
            Ok(UserRule {
                id: r.get(0)?,
                pattern: r.get(1)?,
                pattern_type: ptype,
                score: r.get::<_, i64>(3)? as u8,
                explanation: r.get(4)?,
                enabled: r.get::<_, i64>(5)? != 0,
                created_at_unix: r.get(6)?,
                updated_at_unix: r.get(7)?,
            })
        })
        .map_err(|e| Error::Db(format!("user_rules query: {}", e)))?;

    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| Error::Db(format!("user_rules row: {}", e)))?);
    }
    Ok(out)
}

pub fn list_active_snapshots(conn: &Connection) -> Result<Vec<UserRuleSnapshot>> {
    let mut stmt = conn
        .prepare(
            "SELECT pattern, pattern_type, score, explanation
             FROM user_rules
             WHERE enabled = 1",
        )
        .map_err(|e| Error::Db(format!("active_rules prepare: {}", e)))?;

    let rows = stmt
        .query_map([], |r| {
            let ptype_str: String = r.get(1)?;
            let ptype = UserPatternType::from_str(&ptype_str).unwrap_or(UserPatternType::Name);
            let pattern: String = r.get(0)?;
            Ok(UserRuleSnapshot {
                pattern_lower: pattern.to_ascii_lowercase(),
                pattern_type: ptype,
                score: r.get::<_, i64>(2)? as u8,
                explanation: r.get(3)?,
            })
        })
        .map_err(|e| Error::Db(format!("active_rules query: {}", e)))?;

    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| Error::Db(format!("active_rules row: {}", e)))?);
    }
    Ok(out)
}

pub fn add_rule(
    conn: &Connection,
    pattern: &str,
    pattern_type: UserPatternType,
    score: u8,
    explanation: &str,
) -> Result<UserRule> {
    let p = pattern.trim();
    if p.is_empty() {
        return Err(Error::Db("Pattern cannot be empty".into()));
    }
    if score > 100 {
        return Err(Error::Db("Score must be between 0 and 100".into()));
    }
    let now = now_unix();
    conn.execute(
        "INSERT INTO user_rules
            (pattern, pattern_type, score, explanation, enabled, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6)",
        params![
            p,
            pattern_type.as_str(),
            score as i64,
            explanation,
            now,
            now
        ],
    )
    .map_err(|e| Error::Db(format!("user_rules insert: {}", e)))?;
    let id = conn.last_insert_rowid();
    Ok(UserRule {
        id,
        pattern: p.to_string(),
        pattern_type,
        score,
        explanation: explanation.to_string(),
        enabled: true,
        created_at_unix: now,
        updated_at_unix: now,
    })
}

pub fn delete_rule(conn: &Connection, id: i64) -> Result<()> {
    let n = conn
        .execute("DELETE FROM user_rules WHERE id = ?1", params![id])
        .map_err(|e| Error::Db(format!("user_rules delete: {}", e)))?;
    if n == 0 {
        return Err(Error::Db(format!("Rule not found: id={}", id)));
    }
    Ok(())
}

pub fn toggle_rule(conn: &Connection, id: i64, enabled: bool) -> Result<()> {
    let now = now_unix();
    let n = conn
        .execute(
            "UPDATE user_rules SET enabled = ?1, updated_at = ?2 WHERE id = ?3",
            params![enabled as i64, now, id],
        )
        .map_err(|e| Error::Db(format!("user_rules toggle: {}", e)))?;
    if n == 0 {
        return Err(Error::Db(format!("Rule not found: id={}", id)));
    }
    Ok(())
}

/// Called for every node in the build_tree loop. User rules are evaluated
/// BEFORE the built-in engine — the user can override.
pub fn match_user_rule<'a>(
    snapshots: &'a [UserRuleSnapshot],
    name: &str,
    is_dir: bool,
) -> Option<&'a UserRuleSnapshot> {
    let name_lower = name.to_ascii_lowercase();
    for r in snapshots {
        match r.pattern_type {
            UserPatternType::Name => {
                if name_lower == r.pattern_lower {
                    return Some(r);
                }
            }
            UserPatternType::Extension => {
                if !is_dir && name_lower.ends_with(&r.pattern_lower) {
                    return Some(r);
                }
            }
        }
    }
    None
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
    fn add_and_list_user_rule() {
        let conn = fresh_db();
        let r = add_rule(
            &conn,
            "personalArchive",
            UserPatternType::Name,
            5,
            "Test kural",
        )
        .unwrap();
        assert!(r.id > 0);
        assert_eq!(r.score, 5);

        let list = list_rules(&conn).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].pattern, "personalArchive");
    }

    #[test]
    fn match_user_rule_overrides_case_insensitive() {
        let snaps = vec![UserRuleSnapshot {
            pattern_lower: "myproject".into(),
            pattern_type: UserPatternType::Name,
            score: 90,
            explanation: "test".into(),
        }];
        assert!(match_user_rule(&snaps, "MyProject", true).is_some());
        assert!(match_user_rule(&snaps, "OtherProject", true).is_none());
    }

    #[test]
    fn match_extension_pattern_ignores_dir() {
        let snaps = vec![UserRuleSnapshot {
            pattern_lower: ".bak".into(),
            pattern_type: UserPatternType::Extension,
            score: 80,
            explanation: "test".into(),
        }];
        assert!(match_user_rule(&snaps, "doc.bak", false).is_some());
        assert!(match_user_rule(&snaps, "doc.bak", true).is_none()); // dir uzantı atlar
    }

    #[test]
    fn delete_and_toggle() {
        let conn = fresh_db();
        let r = add_rule(&conn, "test", UserPatternType::Name, 50, "x").unwrap();

        toggle_rule(&conn, r.id, false).unwrap();
        let snaps = list_active_snapshots(&conn).unwrap();
        assert_eq!(snaps.len(), 0, "disabled rule snapshot'a girmemeli");

        toggle_rule(&conn, r.id, true).unwrap();
        let snaps2 = list_active_snapshots(&conn).unwrap();
        assert_eq!(snaps2.len(), 1);

        delete_rule(&conn, r.id).unwrap();
        let list = list_rules(&conn).unwrap();
        assert_eq!(list.len(), 0);
    }

    #[test]
    fn add_rule_validation() {
        let conn = fresh_db();
        assert!(add_rule(&conn, "  ", UserPatternType::Name, 50, "x").is_err());
        assert!(add_rule(&conn, "ok", UserPatternType::Name, 200, "x").is_err());
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later
//
// ScanTree builder — Chapter 4.3 Step 3 + Chapter 4.4 Single Source of Truth.
//
// From raw MFT entries:
//   1. Node dictionary (`HashMap<NodeId, Node>`)
//   2. Child list (`HashMap<NodeId, Vec<NodeId>>`)
//   3. Walk-up aggregate size computation (each file's size is added to its parent folders)
//
// The result is kept as `Arc<ScanTree>` in Tauri state. Vue never owns
// the full tree — it works only via the `tree_summary` and `top_consumers`
// window queries (Chapter 9.6 lazy viewport).

use crate::safe_delete::UserRuleSnapshot;
use crate::scan::find_first::{scan_find_first, scan_find_first_with_progress};
use crate::scan::privilege::is_elevated;
use crate::scan::walk::{
    collect_mft_entries, collect_mft_entries_with_progress, ProgressCb, RawMftEntry,
};
use crate::scan::{NodeId, ScanStrategy};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tracing::{debug, info};

/// NTFS root directory MFT record number.
/// `KnownNtfsFileRecordNumber::RootDirectory = 5`.
pub const ROOT_RECORD: NodeId = 5;
const MAX_PARENT_HOPS: u32 = 256;

#[derive(Debug, Clone, Serialize)]
pub struct Node {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    pub name: String,
    pub data_size: u64,
    pub aggregate_size: u64,
    pub is_dir: bool,
    /// Chapter 6 safe-to-delete score (0-100). None = no matching rule.
    pub score: Option<u8>,
    /// Matching rule id. Fixed for built-in (e.g. "node_modules"),
    /// "user:<id>" for user-defined (Chapter 6.4).
    pub score_rule: Option<String>,
    pub score_reason: Option<String>,
    /// `$STANDARD_INFORMATION.modification_time` in Unix seconds. For the
    /// Chapter 9.1 mod 4/4 Timeline axis. 0 = unknown (synthetic root or error).
    pub modified_unix: i64,
}

#[derive(Debug, Serialize)]
pub struct ScanTree {
    pub volume_id: String,
    pub root_id: NodeId,
    pub nodes: HashMap<NodeId, Node>,
    pub children: HashMap<NodeId, Vec<NodeId>>,
    pub total_bytes: u64,
    pub file_count: u64,
    pub dir_count: u64,
    pub built_at_unix: i64,
    pub build_elapsed_ms: u64,
}

/// Chapter 9.6.5 + 15.4 — plain node for the live scan view.
/// The `ScanProgress.partial_tree` field carries this vector. The frontend's
/// `LiveSunburst.vue` consumes it directly — a smaller payload than the
/// full `Node` (score fields not needed, modified_unix not needed).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PartialNode {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    pub name: String,
    pub aggregate_size: u64,
    pub depth: usize,
    pub is_dir: bool,
}

/// Chapter 9.6.5 — returns the root + the top_n aggregate consumers of the
/// first N levels for the live sunburst, in descending aggregate size order.
///
/// * `max_depth` — maximum depth to include, counting the root (depth 0).
///   E.g. `max_depth=2` → root + first level + second level.
/// * `top_n` — max element count of the result vector (root + first N-1).
///   The root is always the first element; remaining slots go to aggregate consumers.
pub fn build_partial_view(tree: &ScanTree, max_depth: usize, top_n: usize) -> Vec<PartialNode> {
    if top_n == 0 {
        return Vec::new();
    }
    let Some(root_node) = tree.nodes.get(&tree.root_id) else {
        return Vec::new();
    };

    // BFS: (id, depth). depth=0 is the root. max_depth inclusive.
    let mut candidates: Vec<PartialNode> = Vec::new();
    let mut queue: std::collections::VecDeque<(NodeId, usize)> = std::collections::VecDeque::new();
    queue.push_back((tree.root_id, 0));
    let mut visited: std::collections::HashSet<NodeId> = std::collections::HashSet::new();
    visited.insert(tree.root_id);

    while let Some((id, depth)) = queue.pop_front() {
        if depth > max_depth {
            continue;
        }
        if let Some(node) = tree.nodes.get(&id) {
            // Root is always included; other nodes require aggregate_size > 0.
            if id == tree.root_id || node.aggregate_size > 0 {
                candidates.push(PartialNode {
                    id: node.id,
                    parent: node.parent,
                    name: node.name.clone(),
                    aggregate_size: node.aggregate_size,
                    depth,
                    is_dir: node.is_dir,
                });
            }
        }
        if depth >= max_depth {
            continue;
        }
        if let Some(child_ids) = tree.children.get(&id) {
            for &cid in child_ids {
                if visited.insert(cid) {
                    queue.push_back((cid, depth + 1));
                }
            }
        }
    }

    // Separate the root, sort the rest by descending size, then merge them
    // back as root + top (N-1). If top_n=1, only the root is returned.
    let root_partial = candidates
        .iter()
        .find(|p| p.id == tree.root_id)
        .cloned()
        .unwrap_or_else(|| PartialNode {
            id: tree.root_id,
            parent: None,
            name: root_node.name.clone(),
            aggregate_size: root_node.aggregate_size,
            depth: 0,
            is_dir: true,
        });
    let mut rest: Vec<PartialNode> = candidates
        .into_iter()
        .filter(|p| p.id != tree.root_id)
        .collect();
    rest.sort_by_key(|p| std::cmp::Reverse(p.aggregate_size));
    rest.truncate(top_n.saturating_sub(1));

    let mut out = Vec::with_capacity(rest.len() + 1);
    out.push(root_partial);
    out.extend(rest);
    out
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanSummary {
    pub drive: String,
    pub volume_id: String,
    pub strategy: ScanStrategy,
    pub root_id: NodeId,
    pub node_count: u64,
    pub file_count: u64,
    pub dir_count: u64,
    pub total_bytes: u64,
    pub collect_elapsed_ms: u64,
    pub build_elapsed_ms: u64,
}

/// Backward-compatible wrapper — calls it without user rule snapshots.
pub fn build_tree(volume_id: String, raw: Vec<RawMftEntry>) -> ScanTree {
    build_tree_with_user_rules(volume_id, raw, &[])
}

/// Gemini review (partial-tree perf) — lightweight tree builder for live
/// sunburst snapshots. Differences from `build_tree_with_user_rules`:
///   * works on borrows (NO copies),
///   * NO safe-delete rule scoring (the partial view carries only
///     size/name — Node.score fields stay None),
///   * volume_id empty (used only for the partial_view computation).
pub fn build_tree_light(raw: &[RawMftEntry]) -> ScanTree {
    let start = Instant::now();
    let count = raw.len();

    let mut nodes: HashMap<NodeId, Node> = HashMap::with_capacity(count + 1);
    let mut children: HashMap<NodeId, Vec<NodeId>> = HashMap::with_capacity(count / 8 + 1);

    for e in raw {
        nodes.insert(
            e.record_no,
            Node {
                id: e.record_no,
                parent: Some(e.parent_record_no),
                name: e.name.clone(),
                data_size: e.data_size,
                aggregate_size: e.data_size,
                is_dir: e.is_dir,
                score: None,
                score_rule: None,
                score_reason: None,
                modified_unix: e.modified_unix,
            },
        );
    }

    nodes.entry(ROOT_RECORD).or_insert(Node {
        id: ROOT_RECORD,
        parent: None,
        name: "<volume root>".into(),
        data_size: 0,
        aggregate_size: 0,
        is_dir: true,
        score: None,
        score_rule: None,
        score_reason: None,
        modified_unix: 0,
    });

    let known: std::collections::HashSet<NodeId> = nodes.keys().copied().collect();
    for node in nodes.values_mut() {
        if node.id == ROOT_RECORD {
            node.parent = None;
            continue;
        }
        match node.parent {
            // Gemini review B7 — a self-cycle (corrupt MFT) is attached
            // to the root; the node stays reachable.
            Some(p) if p == node.id => {
                node.parent = Some(ROOT_RECORD);
            }
            Some(p) if !known.contains(&p) => {
                node.parent = Some(ROOT_RECORD);
            }
            _ => {}
        }
    }

    for n in nodes.values() {
        if let Some(p) = n.parent {
            children.entry(p).or_default().push(n.id);
        }
    }

    let mut total_bytes = 0u64;
    let mut file_count = 0u64;
    let mut dir_count = 0u64;
    for e in raw {
        if e.is_dir {
            dir_count += 1;
        } else {
            file_count += 1;
            total_bytes = total_bytes.saturating_add(e.data_size);
        }
    }

    // Walk-up aggregate — cycle-guarded (same logic as below).
    let snapshot: Vec<(NodeId, Option<NodeId>, u64)> = nodes
        .iter()
        .map(|(&id, n)| (id, n.parent, n.data_size))
        .collect();
    for (id, parent, size) in snapshot {
        if size == 0 {
            continue;
        }
        let mut visited: std::collections::HashSet<NodeId> =
            std::collections::HashSet::with_capacity(8);
        visited.insert(id);
        let mut cur = parent;
        let mut hops = 0u32;
        while let Some(p) = cur {
            if hops >= MAX_PARENT_HOPS || !visited.insert(p) {
                break;
            }
            match nodes.get_mut(&p) {
                Some(pn) => {
                    pn.aggregate_size = pn.aggregate_size.saturating_add(size);
                    cur = pn.parent;
                }
                None => break,
            }
            hops += 1;
        }
    }

    let build_elapsed_ms = start.elapsed().as_millis() as u64;
    let built_at_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    ScanTree {
        volume_id: String::new(),
        root_id: ROOT_RECORD,
        nodes,
        children,
        total_bytes,
        file_count,
        dir_count,
        built_at_unix,
        build_elapsed_ms,
    }
}

/// Builds a full ScanTree from raw MFT entries:
/// 1. Places the nodes; ROOT_RECORD guaranteed as synthetic.
/// 2. Builds the child lists (orphans go under the root).
/// 3. Walk-up aggregate: each file's size is added to its parent folders.
///
/// Chapter 6.4 — `user_rules` snapshots are evaluated **with priority**
/// over the built-in engine. If nothing matches, `safe_delete::match_rule` is called.
pub fn build_tree_with_user_rules(
    volume_id: String,
    raw: Vec<RawMftEntry>,
    user_rules: &[UserRuleSnapshot],
) -> ScanTree {
    let start = Instant::now();
    let count = raw.len();

    let mut nodes: HashMap<NodeId, Node> = HashMap::with_capacity(count + 1);
    let mut children: HashMap<NodeId, Vec<NodeId>> = HashMap::with_capacity(count / 8 + 1);

    // 1. Node placement — a self-cycle (record == parent) is attached to
    //    the root (Gemini review B7: we used to set parent=None and leave
    //    the node invisible; now it is treated as an orphan and rewired
    //    under the root). Chapter 6.4 (user rules first) + Chapter 6.2
    //    (built-in) safe-to-delete rules are also applied.
    for e in &raw {
        let parent_opt = Some(e.parent_record_no);

        let (score, score_rule, score_reason) =
            if let Some(ur) = crate::safe_delete::match_user_rule(user_rules, &e.name, e.is_dir) {
                (
                    Some(ur.score),
                    Some("user".to_string()),
                    Some(ur.explanation.clone()),
                )
            } else if let Some(r) = crate::safe_delete::match_rule(&e.name, e.is_dir) {
                (
                    Some(r.score),
                    Some(r.id.to_string()),
                    Some(r.explanation.to_string()),
                )
            } else {
                (None, None, None)
            };
        nodes.insert(
            e.record_no,
            Node {
                id: e.record_no,
                parent: parent_opt,
                name: e.name.clone(),
                data_size: e.data_size,
                aggregate_size: e.data_size,
                is_dir: e.is_dir,
                score,
                score_rule,
                score_reason,
                modified_unix: e.modified_unix,
            },
        );
    }

    // Root synthetic node guarantee.
    nodes.entry(ROOT_RECORD).or_insert(Node {
        id: ROOT_RECORD,
        parent: None,
        name: "<volume root>".into(),
        data_size: 0,
        aggregate_size: 0,
        is_dir: true,
        score: None,
        score_rule: None,
        score_reason: None,
        modified_unix: 0,
    });

    // 2. Orphan parent rewiring: unknown parent → ROOT_RECORD.
    //    Also keeps ROOT_RECORD's own parent as None.
    let known: std::collections::HashSet<NodeId> = nodes.keys().copied().collect();
    for node in nodes.values_mut() {
        if node.id == ROOT_RECORD {
            node.parent = None;
            continue;
        }
        match node.parent {
            // Gemini review B7 — a self-cycle (corrupt MFT) is attached
            // to the root; the node stays reachable.
            Some(p) if p == node.id => {
                node.parent = Some(ROOT_RECORD);
            }
            Some(p) if !known.contains(&p) => {
                node.parent = Some(ROOT_RECORD);
            }
            _ => {}
        }
    }

    // 3. Child list — built from the corrected parent pointers.
    for n in nodes.values() {
        if let Some(p) = n.parent {
            children.entry(p).or_default().push(n.id);
        }
    }

    // 4. Counting — based only on raw input; the synthetic root is not counted.
    let mut total_bytes = 0u64;
    let mut file_count = 0u64;
    let mut dir_count = 0u64;
    for e in &raw {
        if e.is_dir {
            dir_count += 1;
        } else {
            file_count += 1;
            total_bytes = total_bytes.saturating_add(e.data_size);
        }
    }

    // 5. Walk-up aggregate. Two phases (borrow checker):
    //    a) Snapshot (id, parent, data_size)
    //    b) Walk-up + mut update
    // Gemini review B6 — in mutual parent cycles (A↔B) the same size used
    //    to be added ~128 times (only self-cycles were guarded). From now
    //    on, every walk keeps its own visited set — an ancestor is added
    //    at most once.
    let snapshot: Vec<(NodeId, Option<NodeId>, u64)> = nodes
        .iter()
        .map(|(&id, n)| (id, n.parent, n.data_size))
        .collect();

    for (id, parent, size) in snapshot {
        if size == 0 {
            continue;
        }
        let mut visited: std::collections::HashSet<NodeId> =
            std::collections::HashSet::with_capacity(8);
        visited.insert(id);
        let mut cur = parent;
        let mut hops = 0u32;
        while let Some(p) = cur {
            if hops >= MAX_PARENT_HOPS || !visited.insert(p) {
                break;
            }
            match nodes.get_mut(&p) {
                Some(pn) => {
                    pn.aggregate_size = pn.aggregate_size.saturating_add(size);
                    cur = pn.parent;
                }
                None => break,
            }
            hops += 1;
        }
    }

    let build_elapsed_ms = start.elapsed().as_millis() as u64;
    let built_at_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    info!(
        nodes = nodes.len(),
        files = file_count,
        dirs = dir_count,
        total_gb = total_bytes / 1_073_741_824,
        elapsed_ms = build_elapsed_ms,
        "ScanTree built"
    );

    ScanTree {
        volume_id,
        root_id: ROOT_RECORD,
        nodes,
        children,
        total_bytes,
        file_count,
        dir_count,
        built_at_unix,
        build_elapsed_ms,
    }
}

/// Returns N children of a given node in descending order by aggregate size.
pub fn top_consumers(tree: &ScanTree, parent: NodeId, limit: usize) -> Vec<Node> {
    let Some(child_ids) = tree.children.get(&parent) else {
        return Vec::new();
    };
    let mut out: Vec<Node> = child_ids
        .iter()
        .filter_map(|id| tree.nodes.get(id).cloned())
        .collect();
    out.sort_by_key(|n| std::cmp::Reverse(n.aggregate_size));
    out.truncate(limit);
    out
}

/// Chapter 9.6.3 — viewport-aware window query key.
#[derive(Debug, Clone, Copy, serde::Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum SortKey {
    /// Aggregate size, descending.
    #[default]
    SizeDesc,
    /// Name, alphabetical ascending.
    NameAsc,
    /// Direct data size (only meaningful for files), descending.
    DataSizeDesc,
}

/// Chapter 9.6.3 — the window that the `tree_window` Tauri command returns.
#[derive(Debug, Clone, Serialize)]
pub struct WindowResult {
    pub parent_id: NodeId,
    pub parent_name: String,
    pub parent_aggregate_size: u64,
    pub total_children: usize,
    pub returned: usize,
    pub nodes: Vec<Node>,
}

/// Chapter 9.6 — returns a parent's children filtered + sorted + paginated.
/// `min_size_bytes`: viewport pixel threshold (skip if it would render below 1px).
pub fn window_query(
    tree: &ScanTree,
    parent: NodeId,
    sort: SortKey,
    limit: usize,
    offset: usize,
    min_size_bytes: Option<u64>,
) -> WindowResult {
    let parent_node = tree.nodes.get(&parent);
    let parent_name = parent_node
        .map(|n| n.name.clone())
        .unwrap_or_else(|| "<unknown>".to_string());
    let parent_aggregate_size = parent_node.map(|n| n.aggregate_size).unwrap_or(0);

    let mut children: Vec<Node> = tree
        .children
        .get(&parent)
        .map(|ids| {
            ids.iter()
                .filter_map(|id| tree.nodes.get(id).cloned())
                .filter(|n| match min_size_bytes {
                    Some(min) => n.aggregate_size >= min,
                    None => true,
                })
                .collect()
        })
        .unwrap_or_default();

    match sort {
        SortKey::SizeDesc => {
            children.sort_by_key(|n| std::cmp::Reverse(n.aggregate_size));
        }
        SortKey::NameAsc => {
            // Gemini review B15 — decorate-sort-undecorate instead of a
            // String alloc per comparison (lowercased once).
            children.sort_by_cached_key(|a| a.name.to_lowercase());
        }
        SortKey::DataSizeDesc => {
            children.sort_by_key(|n| std::cmp::Reverse(n.data_size));
        }
    }

    let total_children = children.len();
    let window: Vec<Node> = children.into_iter().skip(offset).take(limit).collect();

    WindowResult {
        parent_id: parent,
        parent_name,
        parent_aggregate_size,
        total_children,
        returned: window.len(),
        nodes: window,
    }
}

/// Produces the node's full Windows path: `C:\Projeler\d-space\main.rs`.
/// The synthetic root ("<volume root>") is hidden; `drive_letter` is
/// uppercased. Chapter 7 duplicate detector uses it as hash input.
pub fn node_full_path(tree: &ScanTree, drive_letter: char, id: NodeId) -> Option<String> {
    let chain = node_path(tree, id);
    if chain.is_empty() {
        return None;
    }
    // Skip the first element if it is the synthetic root.
    let parts: Vec<&str> = chain
        .iter()
        .filter(|n| n.id != tree.root_id)
        .map(|n| n.name.as_str())
        .collect();
    let letter = drive_letter.to_ascii_uppercase();
    if parts.is_empty() {
        return Some(format!("{}:\\", letter));
    }
    Some(format!("{}:\\{}", letter, parts.join("\\")))
}

/// Returns the chain from a node up to the root, in root → node order.
/// For breadcrumbs (Chapter 15.1.2 progressive disclosure).
pub fn node_path(tree: &ScanTree, id: NodeId) -> Vec<Node> {
    let mut path = Vec::new();
    let mut cur = Some(id);
    let mut hops = 0u32;
    while let Some(c) = cur {
        if hops >= MAX_PARENT_HOPS {
            break;
        }
        match tree.nodes.get(&c) {
            Some(node) => {
                let parent = node.parent;
                path.push(node.clone());
                if parent == Some(c) {
                    break;
                }
                cur = parent;
            }
            None => break,
        }
        hops += 1;
    }
    path.reverse();
    path
}

/// Backward compatible — MFT scan without user rules.
pub fn scan_to_tree_mft(drive: &str) -> crate::error::Result<(ScanSummary, ScanTree)> {
    scan_to_tree_mft_with_user_rules(drive, &[])
}

pub fn scan_to_tree_mft_with_user_rules(
    drive: &str,
    user_rules: &[UserRuleSnapshot],
) -> crate::error::Result<(ScanSummary, ScanTree)> {
    debug!(drive, "MFT full scan starting");
    let collected = collect_mft_entries(drive)?;
    chain_into_summary(drive, ScanStrategy::DirectRawVolume, collected, user_rules)
}

/// Backward compatible — fallback scan without user rules.
pub fn scan_to_tree_fallback(drive: &str) -> crate::error::Result<(ScanSummary, ScanTree)> {
    scan_to_tree_fallback_with_user_rules(drive, &[])
}

pub fn scan_to_tree_fallback_with_user_rules(
    drive: &str,
    user_rules: &[UserRuleSnapshot],
) -> crate::error::Result<(ScanSummary, ScanTree)> {
    debug!(drive, "FindFirstFile fallback scan starting");
    let collected = scan_find_first(drive)?;
    chain_into_summary(
        drive,
        ScanStrategy::FindFirstFileFallback,
        collected,
        user_rules,
    )
}

/// Chapter 5.2A — automatic strategy selection based on privilege. Backward compatible.
pub fn scan_to_tree(drive: &str) -> crate::error::Result<(ScanSummary, ScanTree)> {
    scan_to_tree_with_user_rules(drive, &[])
}

/// Chapter 5.2A + 6.4 — automatic strategy selection together with user rules.
pub fn scan_to_tree_with_user_rules(
    drive: &str,
    user_rules: &[UserRuleSnapshot],
) -> crate::error::Result<(ScanSummary, ScanTree)> {
    scan_to_tree_with_progress(drive, user_rules, None, None)
}

/// Chapter 5.5 — the MFT path is only meaningful on NTFS drives. On
/// ReFS/exFAT/FAT32 volumes the `ntfs` crate's boot sector parse or MFT
/// record read fails; by checking FS info in pre_flight up front and
/// skipping the MFT attempt, ~3-5 s of wasted wait + log noise is saved.
/// UNC (`\\server\share`) input is rejected here — the `scan_unc_path`
/// command is reserved for UNC scans (Gemini review B2).
pub(crate) fn is_mft_compatible(drive: &str) -> bool {
    // Gemini review B2 — UNC/radical input must never turn into a drive letter.
    if crate::volume::parse_drive_letter(drive).is_none() {
        return false;
    }
    match crate::volume::pre_flight_check(drive) {
        Ok(info) => {
            let fs = info.file_system.to_uppercase();
            // Only NTFS is supported. An empty string (BitLocker locked
            // etc.) is unknown; we conservatively return true, MFT is tried
            // and on a faulty volume the fallback logs appear.
            fs.is_empty() || fs == "NTFS"
        }
        Err(_) => true,
    }
}

/// Chapter 5.2A + 6.4 + 9.6.5 — strategy selection + live progress callback +
/// optional cancel flag. When the periodic check sees `cancel.load() == true`,
/// the walker returns `Error::Scan("scan-cancelled")`.
pub fn scan_to_tree_with_progress(
    drive: &str,
    user_rules: &[UserRuleSnapshot],
    progress_cb: Option<ProgressCb<'_>>,
    cancel: Option<&std::sync::atomic::AtomicBool>,
) -> crate::error::Result<(ScanSummary, ScanTree)> {
    // Gemini review B2 — scan_full IPC boundary: UNC/path input gets a
    // clear error here (risk of scanning the wrong volume's MFT unnoticed).
    // For UNC scans there is the `scan_unc_path` command.
    crate::volume::parse_drive_letter(drive).ok_or_else(|| {
        crate::error::Error::Scan(format!(
            "Invalid drive parameter: '{}' (UNC/path not supported here — use the UNC scan command)",
            drive
        ))
    })?;
    let mft_capable = is_mft_compatible(drive);
    if is_elevated() && mft_capable {
        debug!("elevated process + NTFS — trying the MFT path");
        let collected = collect_mft_entries_with_progress(drive, progress_cb, cancel);
        match collected {
            Ok(c) => {
                return chain_into_summary(drive, ScanStrategy::DirectRawVolume, c, user_rules)
            }
            Err(e) => {
                // Do not demote the cancel error to fallback — pass it through directly.
                if let crate::error::Error::Scan(m) = &e {
                    if m == "scan-cancelled" {
                        return Err(e);
                    }
                }
                tracing::warn!(?e, "MFT path failed → fallback");
            }
        }
    } else if is_elevated() && !mft_capable {
        // Chapter 5.5 — we do not try the MFT path on ReFS/exFAT/FAT32 drives.
        debug!("elevated but non-NTFS — straight to fallback (chapter 5.5)");
    } else {
        debug!("not elevated — fallback path");
    }
    let collected = scan_find_first_with_progress(drive, progress_cb, cancel)?;
    chain_into_summary(
        drive,
        ScanStrategy::FindFirstFileFallback,
        collected,
        user_rules,
    )
}

fn chain_into_summary(
    drive: &str,
    strategy: ScanStrategy,
    collected: crate::scan::walk::MftEntries,
    user_rules: &[UserRuleSnapshot],
) -> crate::error::Result<(ScanSummary, ScanTree)> {
    let collect_elapsed_ms = collected.elapsed_ms;
    let volume_id = collected.volume_path.clone();
    let tree = build_tree_with_user_rules(volume_id.clone(), collected.entries, user_rules);
    let summary = ScanSummary {
        drive: drive.to_string(),
        volume_id,
        strategy,
        root_id: tree.root_id,
        node_count: tree.nodes.len() as u64,
        file_count: tree.file_count,
        dir_count: tree.dir_count,
        total_bytes: tree.total_bytes,
        collect_elapsed_ms,
        build_elapsed_ms: tree.build_elapsed_ms,
    };
    Ok((summary, tree))
}

/// Tauri-managed state — optional Arc<ScanTree>. Single writer, many readers.
pub struct ScanTreeState {
    pub current: RwLock<Option<Arc<ScanTree>>>,
}

impl Default for ScanTreeState {
    fn default() -> Self {
        Self {
            current: RwLock::new(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(record_no: u64, parent: u64, name: &str, size: u64, dir: bool) -> RawMftEntry {
        RawMftEntry {
            record_no,
            parent_record_no: parent,
            name: name.into(),
            data_size: size,
            is_dir: dir,
            modified_unix: 0,
        }
    }

    #[test]
    fn aggregate_two_level() {
        // root(5)
        //   docs(100, dir, aggregate 600)
        //     a.txt(101, 200)
        //     b.txt(102, 400)
        //   c.bin(103, 50, aggregate just 50)
        let raw = vec![
            r(100, 5, "docs", 0, true),
            r(101, 100, "a.txt", 200, false),
            r(102, 100, "b.txt", 400, false),
            r(103, 5, "c.bin", 50, false),
        ];
        let tree = build_tree("vol".into(), raw);
        assert_eq!(tree.file_count, 3);
        assert_eq!(tree.dir_count, 1);
        assert_eq!(tree.total_bytes, 650);
        assert_eq!(tree.nodes.get(&100).unwrap().aggregate_size, 600);
        assert_eq!(tree.nodes.get(&5).unwrap().aggregate_size, 650);
        // top_consumers root → docs first
        let top = top_consumers(&tree, 5, 10);
        assert_eq!(top[0].id, 100);
        assert_eq!(top[1].id, 103);
    }

    #[test]
    fn orphan_attached_to_root() {
        // parent 999 not in the dictionary → under the root
        let raw = vec![r(200, 999, "lonely.dat", 128, false)];
        let tree = build_tree("vol".into(), raw);
        let root_children = tree.children.get(&ROOT_RECORD).unwrap();
        assert!(root_children.contains(&200));
        assert_eq!(tree.nodes.get(&5).unwrap().aggregate_size, 128);
    }

    #[test]
    fn cycle_protection() {
        // self-referencing entry — Gemini review B7: rewired to the root,
        // the node stays reachable (previously invisible with parent=None).
        let raw = vec![r(300, 300, "weird", 64, false)];
        let tree = build_tree("vol".into(), raw);
        assert_eq!(tree.total_bytes, 64);
        assert_eq!(tree.nodes.get(&300).unwrap().parent, Some(ROOT_RECORD));
        let root_children = tree.children.get(&ROOT_RECORD).unwrap();
        assert!(
            root_children.contains(&300),
            "self-cycle must attach to root"
        );
        // Walk-up does not enter a cycle — root aggregate is exactly 64.
        assert_eq!(tree.nodes.get(&ROOT_RECORD).unwrap().aggregate_size, 64);
    }

    #[test]
    fn mutual_parent_cycle_does_not_inflate_aggregates() {
        // Gemini review B6 — A↔B mutual parent cycle: each ancestor must
        // be counted at most once (old code added it ~128 times).
        let raw = vec![
            r(400, 401, "a", 100, false),
            r(401, 400, "b", 0, true),
            r(5, 5, "root", 0, true),
        ];
        let tree = build_tree("vol".into(), raw);
        let a = tree.nodes.get(&400).unwrap().aggregate_size;
        let b = tree.nodes.get(&401).unwrap().aggregate_size;
        assert!(a <= 100, "aggregate a must not inflate: {}", a);
        assert!(b <= 100, "aggregate b must not inflate: {}", b);
    }

    #[test]
    fn window_size_desc_default() {
        let raw = vec![
            r(100, 5, "docs", 0, true),
            r(101, 100, "a.txt", 200, false),
            r(102, 100, "b.txt", 400, false),
            r(103, 5, "c.bin", 50, false),
        ];
        let tree = build_tree("vol".into(), raw);
        let w = window_query(&tree, 5, SortKey::SizeDesc, 10, 0, None);
        // There are 2 children under the root: docs(agg 600) and c.bin(50)
        assert_eq!(w.total_children, 2);
        assert_eq!(w.returned, 2);
        assert_eq!(w.nodes[0].id, 100);
        assert_eq!(w.nodes[1].id, 103);
        assert_eq!(w.parent_aggregate_size, 650);
    }

    #[test]
    fn window_name_asc_and_pagination() {
        let raw = vec![
            r(10, 5, "Zeta", 100, false),
            r(11, 5, "Alpha", 200, false),
            r(12, 5, "Mu", 50, false),
        ];
        let tree = build_tree("vol".into(), raw);
        let w = window_query(&tree, 5, SortKey::NameAsc, 2, 0, None);
        assert_eq!(w.nodes[0].name, "Alpha");
        assert_eq!(w.nodes[1].name, "Mu");
        assert_eq!(w.total_children, 3);
        assert_eq!(w.returned, 2);

        let w2 = window_query(&tree, 5, SortKey::NameAsc, 2, 2, None);
        assert_eq!(w2.returned, 1);
        assert_eq!(w2.nodes[0].name, "Zeta");
    }

    #[test]
    fn window_min_size_filter() {
        let raw = vec![
            r(20, 5, "tiny", 10, false),
            r(21, 5, "huge", 9_000_000, false),
        ];
        let tree = build_tree("vol".into(), raw);
        let w = window_query(&tree, 5, SortKey::SizeDesc, 10, 0, Some(1_000_000));
        assert_eq!(w.returned, 1);
        assert_eq!(w.nodes[0].name, "huge");
    }

    #[test]
    fn node_path_breadcrumb() {
        let raw = vec![
            r(100, 5, "Projeler", 0, true),
            r(101, 100, "d-space", 0, true),
            r(102, 101, "src", 0, true),
            r(103, 102, "main.rs", 4096, false),
        ];
        let tree = build_tree("vol".into(), raw);
        let path = node_path(&tree, 103);
        let names: Vec<&str> = path.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["<volume root>", "Projeler", "d-space", "src", "main.rs"]
        );
    }

    #[test]
    fn partial_view_top_n_sorted_desc() {
        // Sprint 3.7 — 5 nodes under the root, top_n=3 must give the three largest (root + 2).
        let raw = vec![
            r(100, 5, "small", 100, false),
            r(101, 5, "medium", 500, false),
            r(102, 5, "big", 5_000, false),
            r(103, 5, "huge", 50_000, false),
            r(104, 5, "tiny", 10, false),
        ];
        let tree = build_tree("vol".into(), raw);
        let view = build_partial_view(&tree, 2, 3);
        assert_eq!(view.len(), 3);
        // The first element is the root.
        assert_eq!(view[0].id, ROOT_RECORD);
        assert_eq!(view[0].depth, 0);
        // Then descending size order: huge(50000), big(5000).
        assert_eq!(view[1].name, "huge");
        assert_eq!(view[1].aggregate_size, 50_000);
        assert_eq!(view[2].name, "big");
        assert_eq!(view[2].aggregate_size, 5_000);
        // medium / small / tiny are left out.
    }

    #[test]
    fn partial_view_respects_max_depth() {
        // root → docs/ → a.txt
        // root → bin/  → sub/  → deep.bin
        // max_depth=1 → root + docs + bin. deep.bin (depth 3) not included.
        // max_depth=2 → root + docs + bin + a.txt + sub. deep.bin (depth 3) not included.
        let raw = vec![
            r(100, 5, "docs", 0, true),
            r(101, 100, "a.txt", 200, false),
            r(200, 5, "bin", 0, true),
            r(201, 200, "sub", 0, true),
            r(202, 201, "deep.bin", 4096, false),
        ];
        let tree = build_tree("vol".into(), raw);

        let v1 = build_partial_view(&tree, 1, 100);
        let names_v1: Vec<&str> = v1.iter().map(|p| p.name.as_str()).collect();
        assert!(names_v1.contains(&"<volume root>"));
        assert!(names_v1.contains(&"docs"));
        assert!(names_v1.contains(&"bin"));
        assert!(!names_v1.contains(&"a.txt"));
        assert!(!names_v1.contains(&"sub"));
        assert!(!names_v1.contains(&"deep.bin"));

        let v2 = build_partial_view(&tree, 2, 100);
        let names_v2: Vec<&str> = v2.iter().map(|p| p.name.as_str()).collect();
        assert!(names_v2.contains(&"a.txt"));
        assert!(names_v2.contains(&"sub"));
        assert!(
            !names_v2.contains(&"deep.bin"),
            "depth 3 must stay excluded"
        );
    }

    #[test]
    fn partial_view_handles_empty_tree() {
        // No raw entries at all — only the synthetic root is guaranteed.
        let tree = build_tree("vol".into(), vec![]);
        let view = build_partial_view(&tree, 2, 50);
        // The root must return alone (aggregate_size=0 but always included).
        assert_eq!(view.len(), 1);
        assert_eq!(view[0].id, ROOT_RECORD);
        assert_eq!(view[0].depth, 0);
        assert_eq!(view[0].aggregate_size, 0);

        // top_n=0 → empty vector.
        let view0 = build_partial_view(&tree, 2, 0);
        assert!(view0.is_empty());
    }

    #[test]
    fn partial_view_top_n_one_returns_only_root() {
        let raw = vec![r(100, 5, "docs", 500, false), r(101, 5, "bin", 1000, false)];
        let tree = build_tree("vol".into(), raw);
        let view = build_partial_view(&tree, 2, 1);
        assert_eq!(view.len(), 1);
        assert_eq!(view[0].id, ROOT_RECORD);
    }

    #[test]
    fn full_path_strips_synthetic_root_and_uses_drive() {
        let raw = vec![
            r(100, 5, "Projeler", 0, true),
            r(101, 100, "d-space", 0, true),
            r(102, 101, "main.rs", 4096, false),
        ];
        let tree = build_tree("vol".into(), raw);
        assert_eq!(
            node_full_path(&tree, 'c', 102).unwrap(),
            r"C:\Projeler\d-space\main.rs"
        );
        // the root node itself → "C:\"
        assert_eq!(node_full_path(&tree, 'C', tree.root_id).unwrap(), r"C:\");
        // unknown id → None
        assert!(node_full_path(&tree, 'D', 9999).is_none());
    }
}

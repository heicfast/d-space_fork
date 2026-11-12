// SPDX-License-Identifier: GPL-3.0-or-later
//
// Result query engine — DiskWizard recovery engine, phase 3.
//
// Server-side filtering / sorting / pagination over the unified scan
// result set (existing + deleted + lost-path), plus the path and type
// trees. Mirrors the connector contract's Results domain: the UI never
// filters client-side beyond what it asked for here.

use crate::error::{Error, Result};
use crate::recovery::deleted::Confidence;
use crate::recovery::{RecoveryScan, LOST_PATH_ROOT};
use crate::scan::tree::{Node, ScanTree};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

/// Cap for per-query candidate materialization (defensive; matches the
/// duplicates engine's guard philosophy).
const MAX_CANDIDATES: usize = 2_000_000;
/// Maximum children returned for one tree page.
const TREE_PAGE_LIMIT: usize = 500;

// ---------------------------------------------------------------------------
// Category mapping
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileCategory {
    Graphics,
    Video,
    Audio,
    Document,
    Email,
    Archive,
    Bookmarks,
    Other,
}

impl FileCategory {
    pub const ALL: [FileCategory; 8] = [
        FileCategory::Graphics,
        FileCategory::Video,
        FileCategory::Audio,
        FileCategory::Document,
        FileCategory::Email,
        FileCategory::Archive,
        FileCategory::Bookmarks,
        FileCategory::Other,
    ];

    pub fn wire_name(self) -> &'static str {
        match self {
            FileCategory::Graphics => "graphics",
            FileCategory::Video => "video",
            FileCategory::Audio => "audio",
            FileCategory::Document => "document",
            FileCategory::Email => "email",
            FileCategory::Archive => "archive",
            FileCategory::Bookmarks => "bookmarks",
            FileCategory::Other => "other",
        }
    }
}

/// Extension → category (without the leading dot, lowercase).
pub fn category_of(extension: &str) -> FileCategory {
    match extension {
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "tif" | "tiff" | "webp" | "heic" | "heif"
        | "svg" | "ico" | "raw" | "cr2" | "cr3" | "nef" | "arw" | "dng" | "orf" | "rw2" | "psd"
        | "ai" | "eps" => FileCategory::Graphics,
        "mp4" | "mov" | "avi" | "mkv" | "wmv" | "flv" | "webm" | "m4v" | "mpg" | "mpeg" | "3gp"
        | "mts" | "m2ts" | "ts" => FileCategory::Video,
        "mp3" | "wav" | "flac" | "aac" | "ogg" | "wma" | "m4a" | "aiff" | "opus" | "mid"
        | "mka" => FileCategory::Audio,
        "doc" | "docx" | "pdf" | "txt" | "rtf" | "odt" | "xls" | "xlsx" | "ppt" | "pptx"
        | "csv" | "md" | "xlsx~" | "xlam" | "xlsb" | "dotx" | "json" | "xml" | "log" | "one"
        | "onetoc2" => FileCategory::Document,
        "pst" | "ost" | "eml" | "msg" | "vcf" | "ics" => FileCategory::Email,
        "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "zst" | "iso" | "cab" | "msi"
        | "arj" | "lzh" | "tgz" => FileCategory::Archive,
        "html" | "htm" => FileCategory::Bookmarks,
        _ => FileCategory::Other,
    }
}

fn extension_of(name: &str) -> String {
    match name.rfind('.') {
        Some(i) if i + 1 < name.len() => name[i + 1..].to_ascii_lowercase(),
        _ => String::new(),
    }
}

// ---------------------------------------------------------------------------
// Wire types (camelCase — contract shape)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultNodeWire {
    pub id: String,
    pub scan_id: String,
    pub kind: &'static str, // file | directory
    pub name: String,
    pub path: Option<String>,
    pub size_bytes: u64,
    pub modified_unix: i64,
    pub extension: String,
    pub category: Option<FileCategory>,
    pub confidence: Confidence,
    pub origin: &'static str, // existing | deleted | lost-path
    pub child_count: Option<u64>,
    pub thumbnail_available: bool,
    pub parent_id: Option<String>,
    pub attributes: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultsPageWire {
    pub nodes: Vec<ResultNodeWire>,
    pub total: u64,
    pub total_bytes: u64,
    pub offset: usize,
    pub page_size: usize,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeNodeWire {
    pub id: String,
    pub parent_id: Option<String>,
    pub label: String,
    pub icon: &'static str, // directory | category | group | device
    pub child_count: u64,
    pub file_count: u64,
    pub size_bytes: u64,
    pub origin: &'static str,
    pub special: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreePageWire {
    pub nodes: Vec<TreeNodeWire>,
    pub parent_has_more: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortFieldWire {
    Name,
    Modified,
    Type,
    Size,
    Confidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum SortDirectionWire {
    #[serde(rename = "asc")]
    Asc,
    #[serde(rename = "desc")]
    Desc,
}

/// Server-side filter (camelCase to match the contract's ResultsFilter).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(Default)]
pub struct ResultsFilterWire {
    pub node_id: Option<String>,
    pub tree_mode: Option<String>, // path | type | raw
    pub search: Option<String>,
    pub categories: Option<Vec<String>>,
    pub date_from: Option<i64>,
    pub date_to: Option<i64>,
    pub size_min_bytes: Option<u64>,
    pub size_max_bytes: Option<u64>,
    pub extensions: Option<Vec<String>>,
    pub origins: Option<Vec<String>>,
    pub hide_empty_files: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeDetailWire {
    pub node: ResultNodeWire,
    pub full_physical_path: Option<String>,
    pub created_unix: i64,
    pub accessed_unix: i64,
    pub confidence_reason: String,
    pub bad_sectors_overlap: bool,
    pub cluster_count: u64,
}

// ---------------------------------------------------------------------------
// Unified view over a scan
// ---------------------------------------------------------------------------

/// Snapshot view joining the existing tree with the deleted maps. Built
/// per query from the `Arc<RecoveryScan>` (cheap: Arc clones only).
pub struct ScanView<'a> {
    pub scan: &'a RecoveryScan,
    pub tree: Option<Arc<ScanTree>>,
    pub deleted: HashMap<u64, crate::recovery::collect::DeletedEntry>,
}

impl<'a> ScanView<'a> {
    pub fn new(scan: &'a RecoveryScan) -> Self {
        let tree = scan.tree.read().unwrap_or_else(|p| p.into_inner()).clone();
        // NOTE: entries are cloned out of the lock so the view does not
        // borrow from a guard. Deleted entries are small (name + runs);
        // a scan holds at most a few hundred thousand of them.
        let deleted = scan
            .deleted
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .map(|(k, v)| (*k, v.clone()))
            .collect();
        Self {
            scan,
            tree,
            deleted,
        }
    }

    fn require_tree(&self) -> Result<&ScanTree> {
        self.tree
            .as_deref()
            .ok_or_else(|| Error::Scan("scan results are not ready yet".into()))
    }

    fn confidence_of(&self, id: u64) -> Confidence {
        *self
            .scan
            .confidence
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .get(&id)
            .unwrap_or(&Confidence::Unknown)
    }

    fn origin_of_deleted(&self, id: u64) -> &'static str {
        if self
            .scan
            .lost_path
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .contains(&id)
        {
            "lost-path"
        } else {
            "deleted"
        }
    }

    /// Children of a node: existing children + deleted children.
    pub fn children_of(&self, parent: u64) -> Vec<u64> {
        let mut out = Vec::new();
        if let Some(tree) = &self.tree {
            if let Some(kids) = tree.children.get(&parent) {
                out.extend(kids.iter().copied());
            }
        }
        if let Some(kids) = self
            .scan
            .deleted_children
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .get(&parent)
        {
            out.extend(kids.iter().copied());
        }
        out
    }

    fn deleted_size(&self, id: u64) -> u64 {
        if let Some(e) = self.deleted.get(&id) {
            if e.is_dir {
                self.scan
                    .deleted_dir_sizes
                    .read()
                    .unwrap_or_else(|p| p.into_inner())
                    .get(&id)
                    .copied()
                    .unwrap_or(0)
            } else {
                e.data_size
            }
        } else {
            0
        }
    }

    /// Enumerates ALL file ids in the subtree rooted at `root` (DFS,
    /// existing + deleted; includes deleted dirs' contents).
    fn subtree_files(&self, root: u64, include_deleted_dirs: bool) -> Vec<u64> {
        let mut out = Vec::new();
        let mut stack = vec![root];
        let mut visited: std::collections::HashSet<u64> = std::collections::HashSet::new();
        while let Some(id) = stack.pop() {
            if !visited.insert(id) {
                continue;
            }
            for child in self.children_of(id) {
                let is_dir = self
                    .tree
                    .as_ref()
                    .and_then(|t| t.nodes.get(&child))
                    .map(|n| n.is_dir)
                    .unwrap_or_else(|| self.deleted.get(&child).map(|d| d.is_dir).unwrap_or(false));
                if is_dir {
                    if include_deleted_dirs {
                        stack.push(child);
                    } else if self
                        .tree
                        .as_ref()
                        .map(|t| t.nodes.contains_key(&child))
                        .unwrap_or(false)
                    {
                        // Existing dir: descend.
                        stack.push(child);
                    }
                } else {
                    out.push(child);
                }
            }
            if out.len() > MAX_CANDIDATES {
                break; // defensive
            }
        }
        out
    }

    /// Full candidate set for filtered queries honoring the filter's
    /// node scoping (path scope / tree node / type node).
    fn candidate_ids(&self, filter: &ResultsFilterWire) -> Result<Vec<Candidate>> {
        let tree = self.require_tree()?;
        let mode = filter.tree_mode.as_deref().unwrap_or("path");
        let scope_root = self
            .scan
            .scope_node
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .unwrap_or(tree.root_id);

        let mut out = Vec::new();
        match (mode, filter.node_id.as_deref()) {
            ("type", Some(node)) if node.starts_with("c:") => {
                // Category node: all files in scope whose category matches.
                let cat = node[2..].to_string();
                for id in self.subtree_files(scope_root, true) {
                    if let Some(c) = self.category_for_file(id) {
                        if c == cat {
                            out.push(self.candidate_of(id));
                        }
                    }
                }
            }
            ("type", Some(node)) if node.starts_with("ext:") => {
                // ext:<category>:<extension>
                let ext = node.rsplit(':').next().unwrap_or("").to_ascii_lowercase();
                for id in self.subtree_files(scope_root, true) {
                    if let Some(c) = self.category_for_file(id) {
                        if node.contains(&c) && self.extension_for_file(id) == ext {
                            out.push(self.candidate_of(id));
                        }
                    }
                }
            }
            ("type", _) => {
                // Type tree root: every file in scope.
                for id in self.subtree_files(scope_root, true) {
                    out.push(self.candidate_of(id));
                }
            }
            (_, Some(node_id)) => {
                // Path node: files under that node.
                let parent = parse_node_id(node_id)
                    .ok_or_else(|| Error::Scan(format!("unknown tree node '{node_id}'")))?;
                for id in self.subtree_files(parent, true) {
                    out.push(self.candidate_of(id));
                }
            }
            (_, None) => {
                for id in self.subtree_files(scope_root, true) {
                    out.push(self.candidate_of(id));
                }
            }
        }
        if out.len() > MAX_CANDIDATES {
            return Err(Error::Scan(format!(
                "result set too large ({} candidates); refine the filter",
                out.len()
            )));
        }
        Ok(out)
    }

    fn candidate_of(&self, id: u64) -> Candidate {
        if let Some(tree) = &self.tree {
            if let Some(n) = tree.nodes.get(&id) {
                return Candidate {
                    id,
                    name: n.name.clone(),
                    size: n.data_size,
                    modified: n.modified_unix,
                    is_dir: n.is_dir,
                    confidence: Confidence::Excellent,
                    origin: "existing",
                    extension: extension_of(&n.name),
                    score: n.score,
                };
            }
        }
        if let Some(d) = self.deleted.get(&id) {
            return Candidate {
                id,
                name: d.name.clone(),
                size: d.data_size,
                modified: d.modified_unix,
                is_dir: d.is_dir,
                confidence: self.confidence_of(id),
                origin: self.origin_of_deleted(id),
                extension: extension_of(&d.name),
                score: None,
            };
        }
        Candidate {
            id,
            name: String::new(),
            size: 0,
            modified: 0,
            is_dir: false,
            confidence: Confidence::Unknown,
            origin: "existing",
            extension: String::new(),
            score: None,
        }
    }

    fn category_for_file(&self, id: u64) -> Option<String> {
        let name = if let Some(t) = &self.tree {
            t.nodes.get(&id).map(|n| n.name.clone())
        } else {
            None
        }
        .or_else(|| self.deleted.get(&id).map(|d| d.name.clone()))?;
        Some(category_of(&extension_of(&name)).wire_name().to_string())
    }

    fn extension_for_file(&self, id: u64) -> String {
        if let Some(t) = &self.tree {
            if let Some(n) = t.nodes.get(&id) {
                return extension_of(&n.name);
            }
        }
        if let Some(d) = self.deleted.get(&id) {
            return extension_of(&d.name);
        }
        String::new()
    }
}

#[derive(Debug, Clone)]
struct Candidate {
    id: u64,
    name: String,
    size: u64,
    modified: i64,
    is_dir: bool,
    confidence: Confidence,
    origin: &'static str,
    extension: String,
    score: Option<u8>,
}

fn parse_node_id(s: &str) -> Option<u64> {
    if s == "root" {
        return Some(crate::scan::tree::ROOT_RECORD);
    }
    if s == "lost-paths" {
        return Some(LOST_PATH_ROOT);
    }
    s.parse().ok()
}

fn confidence_rank(c: Confidence) -> u8 {
    match c {
        Confidence::Excellent => 4,
        Confidence::Good => 3,
        Confidence::Fair => 2,
        Confidence::Poor => 1,
        Confidence::Unknown => 0,
    }
}

// ---------------------------------------------------------------------------
// Public query API
// ---------------------------------------------------------------------------

/// Runs a filtered, sorted, paginated query over the scan's files.
pub fn query_results(
    scan: &RecoveryScan,
    filter: &ResultsFilterWire,
    sort_field: SortFieldWire,
    sort_dir: SortDirectionWire,
    offset: usize,
    limit: usize,
) -> Result<ResultsPageWire> {
    let start = Instant::now();
    let view = ScanView::new(scan);
    let tree = view.require_tree()?;
    let scope_root = scan
        .scope_node
        .read()
        .unwrap_or_else(|p| p.into_inner())
        .unwrap_or(tree.root_id);

    let mut candidates = view.candidate_ids(filter)?;

    // ---- filters ----
    let search = filter
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_ascii_lowercase);
    let cats = filter.categories.as_ref().map(|c| {
        c.iter()
            .map(|s| s.to_ascii_lowercase())
            .collect::<std::collections::HashSet<_>>()
    });
    let exts = filter.extensions.as_ref().map(|e| {
        e.iter()
            .map(|s| s.trim_start_matches('.').to_ascii_lowercase())
            .collect::<std::collections::HashSet<_>>()
    });
    let origins = filter.origins.as_ref().map(|o| {
        o.iter()
            .map(|s| s.to_ascii_lowercase())
            .collect::<std::collections::HashSet<_>>()
    });
    let hide_empty = filter.hide_empty_files.unwrap_or(false);

    candidates.retain(|c| {
        if let Some(min) = filter.size_min_bytes {
            if c.size < min {
                return false;
            }
        }
        if let Some(max) = filter.size_max_bytes {
            if c.size > max {
                return false;
            }
        }
        if let Some(from) = filter.date_from {
            if c.modified < from {
                return false;
            }
        }
        if let Some(to) = filter.date_to {
            if c.modified > to {
                return false;
            }
        }
        if hide_empty && c.size == 0 && !c.is_dir {
            return false;
        }
        if let Some(s) = &search {
            if !c.name.to_ascii_lowercase().contains(s.as_str()) {
                return false;
            }
        }
        if let Some(cats) = &cats {
            if !cats.contains(category_of(&c.extension).wire_name()) {
                return false;
            }
        }
        if let Some(exts) = &exts {
            if !exts.contains(&c.extension) {
                return false;
            }
        }
        if let Some(origins) = &origins {
            if !origins.contains(c.origin) {
                return false;
            }
        }
        true
    });

    let total = candidates.len() as u64;
    let total_bytes: u64 = candidates.iter().map(|c| c.size).sum();

    // ---- sort ----
    match sort_field {
        SortFieldWire::Name => candidates.sort_by_key(|a| a.name.to_lowercase()),
        SortFieldWire::Modified => candidates.sort_by_key(|c| c.modified),
        SortFieldWire::Type => candidates.sort_by(|a, b| {
            a.extension
                .cmp(&b.extension)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        }),
        SortFieldWire::Size => candidates.sort_by_key(|c| c.size),
        SortFieldWire::Confidence => candidates.sort_by_key(|c| confidence_rank(c.confidence)),
    }
    if sort_dir == SortDirectionWire::Desc {
        candidates.reverse();
    }

    // ---- paginate ----
    let page: Vec<ResultNodeWire> = candidates
        .iter()
        .skip(offset)
        .take(limit)
        .map(|c| view.candidate_to_wire(scan, c, tree, scope_root))
        .collect();

    Ok(ResultsPageWire {
        nodes: page,
        total,
        total_bytes,
        offset,
        page_size: limit,
        elapsed_ms: start.elapsed().as_millis() as u64,
    })
}

impl<'a> ScanView<'a> {
    fn candidate_to_wire(
        &self,
        scan: &RecoveryScan,
        c: &Candidate,
        tree: &ScanTree,
        _scope_root: u64,
    ) -> ResultNodeWire {
        let kind = if c.is_dir { "directory" } else { "file" };
        let child_count = if c.is_dir {
            Some(self.children_of(c.id).len() as u64)
        } else {
            None
        };
        let parent_id = if c.id == tree.root_id {
            None
        } else if let Some(n) = tree.nodes.get(&c.id) {
            n.parent.map(|p| p.to_string())
        } else if let Some(d) = self.deleted.get(&c.id) {
            if self.deleted.contains_key(&d.parent_record_no)
                || tree.nodes.contains_key(&d.parent_record_no)
            {
                Some(d.parent_record_no.to_string())
            } else {
                Some(LOST_PATH_ROOT.to_string())
            }
        } else {
            None
        };
        let category = if c.is_dir {
            None
        } else {
            Some(category_of(&c.extension))
        };
        let thumbnail_available = !c.is_dir
            && matches!(category_of(&c.extension), FileCategory::Graphics)
            && c.size <= 2 * 1024 * 1024;
        let mut attributes: Option<HashMap<String, String>> = None;
        if c.origin == "existing" {
            if let Some(score) = c.score {
                let mut map = HashMap::new();
                map.insert("cleanupScore".to_string(), score.to_string());
                if let Some(n) = tree.nodes.get(&c.id) {
                    if let Some(rule) = &n.score_rule {
                        map.insert("cleanupRule".to_string(), rule.clone());
                    }
                    if let Some(reason) = &n.score_reason {
                        map.insert("cleanupReason".to_string(), reason.clone());
                    }
                }
                attributes = Some(map);
            }
        }
        ResultNodeWire {
            id: c.id.to_string(),
            scan_id: scan.scan_id.clone(),
            kind,
            name: c.name.clone(),
            path: scan.node_display_path(c.id),
            size_bytes: c.size,
            modified_unix: c.modified,
            extension: c.extension.clone(),
            category,
            confidence: c.confidence,
            origin: c.origin,
            child_count,
            thumbnail_available,
            parent_id,
            attributes,
        }
    }
}

/// Counts matching files without materializing a page.
pub fn count_results(scan: &RecoveryScan, filter: &ResultsFilterWire) -> Result<u64> {
    // Reuse the query engine with limit 0 (total is computed before paging).
    let page = query_results(
        scan,
        filter,
        SortFieldWire::Size,
        SortDirectionWire::Desc,
        0,
        0,
    )?;
    Ok(page.total)
}

/// Lazy tree: children of `parent_id` (None = root level) in the given mode.
pub fn result_tree(
    scan: &RecoveryScan,
    parent_id: Option<&str>,
    mode: &str,
) -> Result<TreePageWire> {
    let view = ScanView::new(scan);
    let tree = view.require_tree()?;
    let scope_root = scan
        .scope_node
        .read()
        .unwrap_or_else(|p| p.into_inner())
        .unwrap_or(tree.root_id);

    let mode = if mode == "raw" { "type" } else { mode };

    match mode {
        "type" => type_tree(scan, &view, tree, scope_root, parent_id),
        _ => path_tree(scan, &view, tree, scope_root, parent_id),
    }
}

fn path_tree(
    scan: &RecoveryScan,
    view: &ScanView<'_>,
    tree: &ScanTree,
    scope_root: u64,
    parent_id: Option<&str>,
) -> Result<TreePageWire> {
    let parent = match parent_id {
        None | Some("root") => scope_root,
        Some(s) => {
            parse_node_id(s).ok_or_else(|| Error::Scan(format!("unknown tree node '{s}'")))?
        }
    };

    let mut nodes = Vec::new();

    if parent == scope_root && scope_root == tree.root_id {
        // Root level: include the Lost Path branch when non-empty.
        let lost_count = scan
            .lost_path
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .len();
        if lost_count > 0 {
            let lost_size: u64 = scan
                .lost_path
                .read()
                .unwrap_or_else(|p| p.into_inner())
                .iter()
                .map(|id| view.deleted_size(*id))
                .sum();
            nodes.push(TreeNodeWire {
                id: "lost-paths".to_string(),
                parent_id: Some("root".to_string()),
                label: "Lost Path".to_string(),
                icon: "group",
                child_count: lost_count as u64,
                file_count: count_files_under(view, LOST_PATH_ROOT),
                size_bytes: lost_size,
                origin: "lost-path",
                special: Some("lost-path"),
            });
        }
    }

    if parent == LOST_PATH_ROOT {
        // Children of the lost-path bucket.
        for id in scan
            .lost_path
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
        {
            nodes.push(deleted_tree_node(scan, view, *id, "lost-paths"));
        }
    } else {
        for child in view.children_of(parent) {
            if let Some(n) = tree.nodes.get(&child) {
                nodes.push(existing_tree_node(view, n));
            } else if let Some(_d) = view.deleted.get(&child) {
                nodes.push(deleted_tree_node(scan, view, child, &parent.to_string()));
            }
        }
    }

    nodes.sort_by_key(|n| std::cmp::Reverse(n.size_bytes));
    let has_more = nodes.len() > TREE_PAGE_LIMIT;
    nodes.truncate(TREE_PAGE_LIMIT);
    Ok(TreePageWire {
        nodes,
        parent_has_more: has_more,
    })
}

fn count_files_under(view: &ScanView<'_>, root: u64) -> u64 {
    // Cheap count without materializing names.
    let mut count = 0u64;
    let mut stack = vec![root];
    let mut visited = std::collections::HashSet::new();
    while let Some(id) = stack.pop() {
        if !visited.insert(id) {
            continue;
        }
        for child in view.children_of(id) {
            let is_dir = view
                .tree
                .as_ref()
                .and_then(|t| t.nodes.get(&child))
                .map(|n| n.is_dir)
                .unwrap_or_else(|| view.deleted.get(&child).map(|d| d.is_dir).unwrap_or(false));
            if is_dir {
                stack.push(child);
            } else {
                count += 1;
            }
        }
    }
    count
}

fn existing_tree_node(view: &ScanView<'_>, n: &Node) -> TreeNodeWire {
    TreeNodeWire {
        id: n.id.to_string(),
        parent_id: n.parent.map(|p| p.to_string()),
        label: n.name.clone(),
        icon: if n.is_dir { "directory" } else { "group" },
        child_count: view.children_of(n.id).len() as u64,
        file_count: if n.is_dir {
            count_files_under(view, n.id)
        } else {
            1
        },
        size_bytes: if n.is_dir {
            n.aggregate_size
        } else {
            n.data_size
        },
        origin: "existing",
        special: None,
    }
}

fn deleted_tree_node(
    scan: &RecoveryScan,
    view: &ScanView<'_>,
    id: u64,
    parent: &str,
) -> TreeNodeWire {
    let d = view.deleted.get(&id);
    let label = d.map(|d| d.name.clone()).unwrap_or_default();
    let is_dir = d.map(|d| d.is_dir).unwrap_or(false);
    let origin = if scan
        .lost_path
        .read()
        .unwrap_or_else(|p| p.into_inner())
        .contains(&id)
    {
        "lost-path"
    } else {
        "deleted"
    };
    TreeNodeWire {
        id: id.to_string(),
        parent_id: Some(parent.to_string()),
        label,
        icon: if is_dir { "directory" } else { "group" },
        child_count: view.children_of(id).len() as u64,
        file_count: if is_dir {
            count_files_under(view, id)
        } else {
            1
        },
        size_bytes: view.deleted_size(id),
        origin,
        special: if origin == "lost-path" {
            Some("lost-path")
        } else {
            Some("deleted")
        },
    }
}

fn type_tree(
    scan: &RecoveryScan,
    view: &ScanView<'_>,
    tree: &ScanTree,
    scope_root: u64,
    parent_id: Option<&str>,
) -> Result<TreePageWire> {
    match parent_id {
        None | Some("root") | Some("type-root") => {
            // Level 1: the 8 categories present in scope.
            let mut counts: HashMap<String, (u64, u64)> = HashMap::new();
            for id in view.subtree_files(scope_root, true) {
                let c = view.candidate_of(id);
                if c.is_dir {
                    continue;
                }
                let cat = category_of(&c.extension).wire_name().to_string();
                let e = counts.entry(cat).or_default();
                e.0 += 1;
                e.1 += c.size;
            }
            let mut nodes: Vec<TreeNodeWire> = counts
                .into_iter()
                .map(|(cat, (files, size))| TreeNodeWire {
                    id: format!("c:{cat}"),
                    parent_id: Some("root".to_string()),
                    label: capitalize(&cat),
                    icon: "category",
                    child_count: 0,
                    file_count: files,
                    size_bytes: size,
                    origin: "existing",
                    special: None,
                })
                .collect();
            nodes.sort_by_key(|n| std::cmp::Reverse(n.size_bytes));
            Ok(TreePageWire {
                nodes,
                parent_has_more: false,
            })
        }
        Some(node) if node.starts_with("c:") => {
            // Level 2: extensions within a category.
            let cat = &node[2..];
            let mut exts: HashMap<String, (u64, u64)> = HashMap::new();
            for id in view.subtree_files(scope_root, true) {
                let c = view.candidate_of(id);
                if c.is_dir {
                    continue;
                }
                if category_of(&c.extension).wire_name() != cat {
                    continue;
                }
                let e = exts.entry(c.extension.clone()).or_default();
                e.0 += 1;
                e.1 += c.size;
            }
            let mut nodes: Vec<TreeNodeWire> = exts
                .into_iter()
                .map(|(ext, (files, size))| TreeNodeWire {
                    id: format!("ext:{cat}:{ext}"),
                    parent_id: Some(node.to_string()),
                    label: format!(".{ext}"),
                    icon: "category",
                    child_count: 0,
                    file_count: files,
                    size_bytes: size,
                    origin: "existing",
                    special: None,
                })
                .collect();
            nodes.sort_by_key(|n| std::cmp::Reverse(n.size_bytes));
            let has_more = nodes.len() > TREE_PAGE_LIMIT;
            nodes.truncate(TREE_PAGE_LIMIT);
            Ok(TreePageWire {
                nodes,
                parent_has_more: has_more,
            })
        }
        Some(other) => {
            let _ = (scan, tree);
            Err(Error::Scan(format!("unknown type tree node '{other}'")))
        }
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// Node detail for the preview pane.
pub fn node_detail(scan: &RecoveryScan, node_id: &str) -> Result<NodeDetailWire> {
    let view = ScanView::new(scan);
    let tree = view.require_tree()?;
    let id =
        parse_node_id(node_id).ok_or_else(|| Error::Scan(format!("unknown node '{node_id}'")))?;
    let c = view.candidate_of(id);
    let scope_root = scan
        .scope_node
        .read()
        .unwrap_or_else(|p| p.into_inner())
        .unwrap_or(tree.root_id);
    let node = view.candidate_to_wire(scan, &c, tree, scope_root);

    let cluster_size = *scan.cluster_size.read().unwrap_or_else(|p| p.into_inner()) as u64;
    let cluster_count = if cluster_size > 0 {
        c.size.div_ceil(cluster_size)
    } else {
        0
    };
    let confidence_reason = match (c.origin, c.confidence) {
        ("existing", _) => "The file is on disk and fully readable".to_string(),
        (_, Confidence::Excellent) => "Data clusters are still free — very likely intact".to_string(),
        (_, Confidence::Good) => "Most data clusters are still free".to_string(),
        (_, Confidence::Fair) => "Some data clusters have been reused — partial recovery likely".to_string(),
        (_, Confidence::Poor) => "Data clusters have been reused (or the file is compressed/encrypted) — recovery unlikely".to_string(),
        (_, Confidence::Unknown) => "Recoverability is not applicable for this entry".to_string(),
    };

    Ok(NodeDetailWire {
        node,
        full_physical_path: scan.node_display_path(id),
        created_unix: c.modified,
        accessed_unix: c.modified,
        confidence_reason,
        // No bad-sector map on local disks (deep-carve concept).
        bad_sectors_overlap: false,
        cluster_count,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_mapping_covers_common_types() {
        assert_eq!(category_of("jpg"), FileCategory::Graphics);
        assert_eq!(category_of("MP4"), FileCategory::Video);
        assert_eq!(category_of("mp3"), FileCategory::Audio);
        assert_eq!(category_of("docx"), FileCategory::Document);
        assert_eq!(category_of("pst"), FileCategory::Email);
        assert_eq!(category_of("zip"), FileCategory::Archive);
        assert_eq!(category_of("html"), FileCategory::Bookmarks);
        assert_eq!(category_of("exe"), FileCategory::Other);
        assert_eq!(category_of(""), FileCategory::Other);
    }

    #[test]
    fn extension_of_handles_names() {
        assert_eq!(extension_of("photo.JPG"), "jpg");
        assert_eq!(extension_of("archive.tar.gz"), "gz");
        assert_eq!(extension_of("noext"), "");
        assert_eq!(extension_of("trailing."), "");
    }

    #[test]
    fn confidence_rank_ordering() {
        assert!(confidence_rank(Confidence::Excellent) > confidence_rank(Confidence::Good));
        assert!(confidence_rank(Confidence::Good) > confidence_rank(Confidence::Fair));
        assert!(confidence_rank(Confidence::Fair) > confidence_rank(Confidence::Poor));
        assert!(confidence_rank(Confidence::Poor) > confidence_rank(Confidence::Unknown));
    }

    #[test]
    fn parse_node_id_specials() {
        assert_eq!(parse_node_id("root"), Some(crate::scan::tree::ROOT_RECORD));
        assert_eq!(parse_node_id("lost-paths"), Some(LOST_PATH_ROOT));
        assert_eq!(parse_node_id("12345"), Some(12345));
        assert_eq!(parse_node_id("c:graphics"), None);
    }

    #[test]
    fn capitalize_works() {
        assert_eq!(capitalize("graphics"), "Graphics");
        assert_eq!(capitalize(""), "");
    }
}

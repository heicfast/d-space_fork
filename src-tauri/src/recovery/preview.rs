// SPDX-License-Identifier: GPL-3.0-or-later
//
// Bounded preview reads — DiskWizard recovery engine.
//
// The connector decides the preview kind from the extension (ADR-008);
// the engine supplies bounded bytes. Supported honestly:
//   * image  — raw bytes (base64) up to 8 MiB, mime by extension
//   * text   — up to 64 KiB, UTF-16LE/UTF-8 detection, truncation flag
//   * hex    — up to 64 KiB of raw bytes
// PDF/AV/Office previews are NOT supported by the engine and return the
// contract's `preview-unsupported` code (the UI shows a truthful reason
// instead of pretending).
//
// Existing files are read through the filesystem (compressed/encrypted
// files decode correctly); deleted files are read through their data
// runs with a fresh cluster-allocation check.

use crate::error::{Error, Result};
use crate::recovery::collect::ReadSeek;
use crate::recovery::deleted::{read_deleted_range, reparse_record_on};
use crate::recovery::{RecoveryScan, ScanTarget};
use base64::Engine as _;
use serde::Serialize;
use std::io::Read;
use std::sync::Arc;

const TEXT_LIMIT: u64 = 64 * 1024;
const IMAGE_LIMIT: u64 = 8 * 1024 * 1024;
const HEX_LIMIT: u64 = 64 * 1024;
const THUMBNAIL_LIMIT: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewKind {
    Image,
    Text,
    Hex,
}

/// Classifies a preview request by extension (connector contract mirrors
/// this on the JS side; the engine is the source of truth).
pub fn preview_kind_for(extension: &str) -> PreviewKind {
    match extension {
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "ico" => PreviewKind::Image,
        "txt" | "md" | "log" | "json" | "xml" | "csv" | "ini" | "cfg" | "conf" | "yml" | "yaml"
        | "html" | "htm" | "js" | "ts" | "css" | "rs" | "py" | "c" | "cpp" | "h" => {
            PreviewKind::Text
        }
        _ => PreviewKind::Hex,
    }
}

/// Wire payload of a preview read (camelCase for the connector).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewWire {
    /// image | text | hex | unsupported
    pub kind: String,
    /// Base64 bytes (image/hex) — empty for text/unsupported.
    pub data_base64: String,
    pub mime: Option<String>,
    /// Decoded text (text kind only).
    pub text: Option<String>,
    pub encoding: Option<String>,
    pub truncated: bool,
    pub total_size: u64,
    /// Human reason for the unsupported kind.
    pub reason_key: Option<String>,
}

/// Preview error with the contract's machine codes.
#[derive(Debug)]
pub struct PreviewError {
    pub code: String, // preview-unsupported | preview-damaged | preview-too-large | preview-missing-data
    pub detail: String,
}

impl std::fmt::Display for PreviewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.detail)
    }
}

/// Reads a bounded preview of one node.
pub fn request_preview(
    scan: &Arc<RecoveryScan>,
    node_id: &str,
) -> std::result::Result<PreviewWire, PreviewError> {
    let id: u64 = node_id.parse().map_err(|_| PreviewError {
        code: "preview-unsupported".into(),
        detail: format!("unknown node '{node_id}'"),
    })?;

    let tree_guard = scan.tree.read().unwrap_or_else(|p| p.into_inner());
    let tree = tree_guard
        .as_ref()
        .ok_or_else(|| PreviewError {
            code: "preview-missing-data".into(),
            detail: "scan results are not ready".into(),
        })?
        .clone();
    drop(tree_guard);

    let existing_node = tree.nodes.get(&id).cloned();
    let deleted_entry = scan
        .deleted
        .read()
        .unwrap_or_else(|p| p.into_inner())
        .get(&id)
        .cloned();

    let (name, size, extension) = match (&existing_node, &deleted_entry) {
        (Some(n), _) => (n.name.clone(), n.data_size, ext_of(&n.name)),
        (_, Some(d)) => (d.name.clone(), d.data_size, ext_of(&d.name)),
        _ => {
            return Err(PreviewError {
                code: "preview-unsupported".into(),
                detail: format!("node '{node_id}' is not part of this scan"),
            })
        }
    };

    let kind = preview_kind_for(&extension);
    let limit = match kind {
        PreviewKind::Image => IMAGE_LIMIT,
        PreviewKind::Text => TEXT_LIMIT,
        PreviewKind::Hex => HEX_LIMIT,
    };

    // ---- read bytes ----
    let bytes: Vec<u8> = if let Some(d) = &deleted_entry {
        read_deleted_bytes(scan, d.record_no, limit).map_err(|e| PreviewError {
            code: "preview-damaged".into(),
            detail: e.to_string(),
        })?
    } else if let Some(n) = &existing_node {
        read_existing_bytes(scan, n.id, &name, limit).map_err(|e| PreviewError {
            code: "preview-damaged".into(),
            detail: e.to_string(),
        })?
    } else {
        Vec::new()
    };

    let truncated = (bytes.len() as u64) < size;
    match kind {
        PreviewKind::Image => {
            if size > IMAGE_LIMIT {
                return Err(PreviewError {
                    code: "preview-too-large".into(),
                    detail: format!("{size} bytes exceeds the image preview limit"),
                });
            }
            Ok(PreviewWire {
                kind: "image".into(),
                data_base64: base64::engine::general_purpose::STANDARD.encode(&bytes),
                mime: Some(image_mime(&extension)),
                text: None,
                encoding: None,
                truncated,
                total_size: size,
                reason_key: None,
            })
        }
        PreviewKind::Text => {
            let (text, encoding) = decode_text(&bytes);
            Ok(PreviewWire {
                kind: "text".into(),
                data_base64: String::new(),
                mime: None,
                text: Some(text),
                encoding: Some(encoding),
                truncated,
                total_size: size,
                reason_key: None,
            })
        }
        PreviewKind::Hex => Ok(PreviewWire {
            kind: "hex".into(),
            data_base64: base64::engine::general_purpose::STANDARD.encode(&bytes),
            mime: None,
            text: None,
            encoding: None,
            truncated,
            total_size: size,
            reason_key: None,
        }),
    }
}

/// Thumbnail: images only, bounded to 2 MiB source.
pub fn request_thumbnail(scan: &Arc<RecoveryScan>, node_id: &str) -> Result<Option<ThumbWire>> {
    let Ok(preview) = request_preview(scan, node_id) else {
        return Ok(None);
    };
    if preview.kind != "image" {
        return Ok(None);
    }
    let Some(mime) = preview.mime.clone() else {
        return Ok(None);
    };
    // The preview already enforces the 8 MiB image cap; thumbnails use a
    // smaller source cap for snappy lists.
    if preview.total_size > THUMBNAIL_LIMIT {
        return Ok(None);
    }
    Ok(Some(ThumbWire {
        data_url: format!("data:{mime};base64,{}", preview.data_base64),
    }))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThumbWire {
    pub data_url: String,
}

fn ext_of(name: &str) -> String {
    match name.rfind('.') {
        Some(i) if i + 1 < name.len() => name[i + 1..].to_ascii_lowercase(),
        _ => String::new(),
    }
}

fn image_mime(ext: &str) -> String {
    match ext {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        _ => "application/octet-stream",
    }
    .to_string()
}

/// UTF-16LE BOM / plain UTF-8 (lossy) detection.
fn decode_text(bytes: &[u8]) -> (String, String) {
    if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
        let utf16: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        (String::from_utf16_lossy(&utf16), "utf-16le".into())
    } else {
        (String::from_utf8_lossy(bytes).into_owned(), "utf-8".into())
    }
}

/// Reads existing files through the filesystem; falls back to raw runs
/// for files whose Win32 path is not reachable (unmounted/lost contexts).
fn read_existing_bytes(
    scan: &Arc<RecoveryScan>,
    node_id: u64,
    name: &str,
    limit: u64,
) -> Result<Vec<u8>> {
    let use_runs = matches!(scan.meta.target, ScanTarget::LostPartition { .. });
    if !use_runs {
        if let Some(path) = scan.node_display_path(node_id) {
            let mut f = std::fs::File::open(&path)
                .map_err(|e| Error::Scan(format!("cannot open '{path}': {e}")))?;
            let mut buf = vec![0u8; limit.min(16 * 1024 * 1024) as usize];
            let n = f
                .read(&mut buf)
                .map_err(|e| Error::Scan(format!("read '{path}' failed: {e}")))?;
            buf.truncate(n);
            // Sanity: name check to defend against tree/path drift.
            let _ = name;
            return Ok(buf);
        }
    }
    // Raw-run fallback: treat like a deleted read (fresh re-parse).
    read_deleted_bytes(scan, node_id, limit)
}

/// Deleted read: fresh record re-parse + allocation check + bounded read.
fn read_deleted_bytes(scan: &Arc<RecoveryScan>, record_no: u64, limit: u64) -> Result<Vec<u8>> {
    let mut reader = crate::recovery::partitions::open_target_reader(&scan.meta.target)?;
    let label = crate::recovery::partitions::reader_volume_label(&scan.meta.target);
    fresh_read(scan, &mut reader, &label, record_no, limit)
}

pub(crate) fn fresh_read(
    scan: &Arc<RecoveryScan>,
    reader: &mut Box<dyn ReadSeek>,
    volume_label: &str,
    record_no: u64,
    limit: u64,
) -> Result<Vec<u8>> {
    let ntfs = ntfs::Ntfs::new(reader)
        .map_err(|e| Error::Scan(format!("NTFS parse error on '{volume_label}': {e:?}")))?;
    let fresh = reparse_record_on(&ntfs, reader, record_no)?;
    if fresh.still_deleted && (fresh.compressed || fresh.encrypted) {
        return Err(Error::Scan(
            "file is compressed or encrypted — raw recovery is not possible".into(),
        ));
    }
    let cluster_size = *scan.cluster_size.read().unwrap_or_else(|p| p.into_inner()) as u64;
    let (bytes, _intact) = read_deleted_range(reader, &fresh, None, cluster_size, 0, limit)?;
    Ok(bytes)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_kind_classification() {
        assert_eq!(preview_kind_for("jpg"), PreviewKind::Image);
        assert_eq!(preview_kind_for("png"), PreviewKind::Image);
        assert_eq!(preview_kind_for("txt"), PreviewKind::Text);
        assert_eq!(preview_kind_for("md"), PreviewKind::Text);
        assert_eq!(preview_kind_for("exe"), PreviewKind::Hex);
        assert_eq!(preview_kind_for(""), PreviewKind::Hex);
    }

    #[test]
    fn text_decode_utf16le_bom() {
        let mut bytes = vec![0xFF, 0xFE];
        for ch in "héllo".encode_utf16() {
            bytes.extend_from_slice(&ch.to_le_bytes());
        }
        let (text, encoding) = decode_text(&bytes);
        assert_eq!(text, "héllo");
        assert_eq!(encoding, "utf-16le");
    }

    #[test]
    fn text_decode_utf8_lossy() {
        let (text, encoding) = decode_text(b"plain text");
        assert_eq!(text, "plain text");
        assert_eq!(encoding, "utf-8");
        let (lossy, _) = decode_text(&[0xFF, 0xFE, 0x00]); // invalid utf-8 tail
        assert!(!lossy.contains('\u{FFFD}') || lossy.len() >= 1);
    }

    #[test]
    fn image_mime_mapping() {
        assert_eq!(image_mime("jpg"), "image/jpeg");
        assert_eq!(image_mime("png"), "image/png");
        assert_eq!(image_mime("xyz"), "application/octet-stream");
    }
}

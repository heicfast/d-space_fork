// SPDX-License-Identifier: GPL-3.0-or-later
//
// 26.4 Plugin System — v2 stub (closed box).
//
// Phase 2 adds user-defined rule sets + view mode + safe-delete scorer
// plugins. Sandbox: WASM (wasmtime) or a restricted Rust dynamic
// library — decide at the start of v2.
//
// Plugin capabilities are limited: NO direct file system access,
// only read-only metadata + UI extension points.

use crate::error::Result;
use serde::Serialize;

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct PluginCapabilities {
    pub adds_rules: bool,
    pub adds_view_mode: bool,
    pub adds_scorer: bool,
    pub reads_telemetry: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginContext {
    pub plugin_id: String,
    pub version: String,
    pub capabilities: PluginCapabilities,
}

/// Chapter 26.4 — plugin trait. Real implementation in v2; v1.x stub.
pub trait Plugin: Send + Sync {
    fn context(&self) -> &PluginContext;

    /// Initial load. Manifest validation, capability acceptance.
    fn on_load(&mut self) -> Result<()>;

    /// Unload. Should perform cleanup.
    fn on_unload(&mut self) -> Result<()>;
}

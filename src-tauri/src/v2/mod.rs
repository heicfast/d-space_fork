// SPDX-License-Identifier: GPL-3.0-or-later
//
// v2 Infrastructure Reservations — Master architecture Chapter 26.
//
// This module is **not yet implemented**, but it contains traits whose API
// contract stays stable throughout v1.x. Goal: minimize breaking changes
// on the move to v2 and position existing code around these traits.
//
// Right now no trait has a concrete implementation — `unimplemented!()`
// or feature-gated. Real implementations arrive in the v2 sprints.
//
// Contents:
//   * 26.1 MlSafeDeleteScorer — TFLite-tiered ML scorer (layered on top of
//           the existing rule engine)
//   * 26.2 NetworkShareScanner — scan strategy for UNC paths
//   * 26.3 CrossPlatformVolumeReader — trait for Linux/macOS support
//   * 26.4 Plugin — closed-box plugin system
//   * 26.5 CloudBackupIntegration — cloud backup for staging

pub mod cloud_backup;
pub mod ml_scorer;
pub mod network_scanner;
pub mod plugin;
pub mod volume_reader;

pub use cloud_backup::{CloudBackupIntegration, CloudProvider, CloudUploadResult};
pub use ml_scorer::{InferenceTier, MlSafeDeleteScorer, MlScoreRecord};
pub use network_scanner::{NetworkScanResult, NetworkShareScanner, WindowsNetworkScanner};
pub use plugin::{Plugin, PluginCapabilities, PluginContext};
pub use volume_reader::{CrossPlatformVolumeReader, VolumeBackend};

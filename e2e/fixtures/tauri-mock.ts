// SPDX-License-Identifier: GPL-3.0-or-later
//
// Tauri IPC mock — Sprint 3.5 web kanalı.
//
// Playwright `addInitScript` üzerinden enjekte edilir; `window.__TAURI_IPC__`
// override edilir, `@tauri-apps/api/core` `invoke` çağrıları deterministik
// JSON ile cevaplanır. Backend bağımsız akış testi.

export interface MockIpcOptions {
  /// `index_status` mode override (default "ready").
  indexMode?: "ready" | "idle" | "needs_admin" | "building" | "error";
  /// İlk açılışta onboarding görünsün mü (settings.onboarding_done yoksa).
  showOnboarding?: boolean;
  /// Drilldown görünür olsun mu (scan_full + tree_window mock'lanır).
  withScan?: boolean;
  /// `list_drives` çıktı override.
  drives?: Array<{ drive_letter: string; volume_label: string }>;
}

export function buildTauriMockScript(options: MockIpcOptions = {}): string {
  const opts = {
    indexMode: options.indexMode ?? "ready",
    showOnboarding: options.showOnboarding ?? false,
    withScan: options.withScan ?? false,
    drives: options.drives ?? [
      { drive_letter: "C", volume_label: "OS" },
      { drive_letter: "D", volume_label: "Data" },
    ],
  };

  return `
    (function () {
      const OPTS = ${JSON.stringify(opts)};
      const responses = {
        get_app_info: () => ({
          name: "D-Space",
          version: "0.3.0",
          spec_version: "v1.4",
          spec_status: "DONDURULDU",
          license: "GPL-3.0-or-later",
          platform: "windows"
        }),
        check_privilege: () => ({ elevated: true, strategy: "MftService" }),
        list_drives_cmd: () => OPTS.drives.map(d => ({
          drive_letter: d.drive_letter,
          root_path: d.drive_letter + ":\\\\",
          file_system: "NTFS",
          volume_label: d.volume_label,
          volume_serial: 305419896,
          drive_kind: "Fixed",
          total_bytes: 1099511627776,
          free_bytes: 549755813888,
          status: { kind: "Ready" },
          elapsed_ms: 1
        })),
        get_setting_cmd: (args) => {
          // Gemini review E1 — App.vue "onboarding_done" okur; eski
          // "onboarding_seen" anahtarı mock'ta ölüydü (onboarding her
          // yüklemede açılıp diğert testleri blokluyordu).
          if (args?.key === "onboarding_done") {
            return OPTS.showOnboarding ? null : "1";
          }
          if (args?.key === "language_preference") return "tr";
          if (args?.key === "theme") return "dark";
          return null;
        },
        set_setting_cmd: () => null,
        pre_flight_volume: () => ({
          drive_letter: "C",
          root_path: "C:\\\\",
          file_system: "NTFS",
          volume_label: "OS",
          volume_serial: 305419896,
          drive_kind: "Fixed",
          total_bytes: 1099511627776,
          free_bytes: 549755813888,
          status: { kind: "Ready" },
          elapsed_ms: 1
        }),
        scan_full: () => {
          if (!OPTS.withScan) return null;
          return {
            drive: "C",
            volume_id: "C:\\\\",
            strategy: "FindFirstFileFallback",
            root_id: 5,
            node_count: 4,
            file_count: 3,
            dir_count: 1,
            total_bytes: 12345678,
            collect_elapsed_ms: 12,
            build_elapsed_ms: 3
          };
        },
        tree_window: () => {
          if (!OPTS.withScan) return null;
          const node = (id, name, size, isDir) => ({
            id, parent: 5, name, data_size: isDir ? 0 : size,
            aggregate_size: size, is_dir: isDir, score: null,
            score_rule: null, score_reason: null, modified_unix: 1700000000
          });
          return {
            parent_id: 5,
            parent_name: "<volume root>",
            parent_aggregate_size: 12345678,
            total_children: 2,
            returned: 2,
            nodes: [node(100, "docs", 9000000, true), node(101, "setup.exe", 3345678, false)]
          };
        },
        tree_path: () => {
          if (!OPTS.withScan) return null;
          return [{ id: 5, parent: null, name: "<volume root>", data_size: 0,
            aggregate_size: 12345678, is_dir: true, score: null,
            score_rule: null, score_reason: null, modified_unix: 0 }];
        },
        index_status: () => ({
          volume_id: "\\\\\\\\.\\\\C:",
          total_entries: OPTS.indexMode === "ready" ? 12345 : 0,
          last_sync_unix: Math.floor(Date.now() / 1000),
          mode: OPTS.indexMode
        }),
        index_search: () => [],
        list_staging: () => [],
        list_snapshots: () => [],
        get_db_info: () => ({
          path: "C:\\\\Users\\\\test\\\\AppData\\\\Local\\\\DSpace\\\\db\\\\dspace.sqlite",
          schema_version: 3,
          journal_mode: "wal",
          page_size: 4096,
          table_count: 11,
          spec_version: "v1.4"
        })
      };

      window.__TAURI_INTERNALS__ = window.__TAURI_INTERNALS__ || {};
      window.__TAURI_INTERNALS__.invoke = function(cmd, args) {
        const handler = responses[cmd];
        if (handler) {
          return Promise.resolve(handler(args));
        }
        return Promise.reject(new Error("[e2e mock] handler yok: " + cmd));
      };
      window.__TAURI_INTERNALS__.metadata = window.__TAURI_INTERNALS__.metadata || {
        windows: [{ label: "main" }],
        currentWindow: { label: "main" }
      };
    })();
  `;
}

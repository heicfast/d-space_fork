<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Third-party notices

DiskWizard is GPL-3.0-or-later. It depends on the following
third-party components; each is used under its own license.

## Engine code adapted into this repository

| Component                                     | License | Where                                          | What                                                                                                                                                                                                                                                                    |
| --------------------------------------------- | ------- | ---------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| SectorForge — Copyright (c) 2026 terra seraph | MIT     | `src-tauri/src/fsfat/`, `src-tauri/src/fsext/` | FAT12/16/32 + exFAT and ext2/3/4 deleted-file engine logic (boot/FAT/inode parsing, directory-entry parsing, deleted-name recovery, allocation views), adapted to DiskWizard's types and progress/pause/cancel model. The MIT notice is retained in the module headers. |

MIT-licensed code is compatible with the GPL; the MIT notice and
copyright line above are retained as required.

## Rust crates (MIT OR Apache-2.0 unless noted)

tauri, tauri-plugin-dialog, tauri-plugin-process,
tauri-plugin-single-instance, tauri-plugin-updater (MIT OR Apache-2.0);
serde, serde_json (MIT OR Apache-2.0); thiserror (MIT OR Apache-2.0);
tracing, tracing-subscriber (MIT); ntfs (MIT OR Apache-2.0); tokio
(MIT); rusqlite (MIT); rusqlite_migration (MIT); dirs (MIT OR
Apache-2.0); blake3 (CC0-1.0 OR Unlicense OR Apache-2.0 OR MIT-0);
base64 (MIT OR Apache-2.0); rayon (MIT OR Apache-2.0); windows
(MIT OR Apache-2.0); winapi (MIT OR Apache-2.0). SQLite (bundled by
rusqlite) is Public Domain. Optional at build time: tract /
tract-tflite (MIT OR Apache-2.0, `ml-tflite` feature).

## npm packages

React, React Router, TanStack Query/Virtual, Zustand, Radix UI,
i18next, Tailwind CSS, Vite, Vitest, Playwright, ESLint, Prettier and
peers — MIT (BSD-3 for some toolchain plugins; see each package for
details).

## Fonts and assets

The application icon and in-app illustration set are original artwork.
Inter and the Noto families used by the web UI are licensed under the
SIL Open Font License 1.1.

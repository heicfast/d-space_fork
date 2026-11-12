# DiskWizard · Repo Standards

> **Target location:** repository root (`REPO_STANDARDS.md`).
> Commit it to the repo root. Subsequent edits must keep this file
> authoritative — when something changes, update it here too.
>
> **Snapshot:** 2026-11-12 (DiskWizard v0.4.0-beta — React frontend,
> English-first product)

---

## 1. Locked GitHub metadata

| Field              | Value                                                                                                                                                                                            |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **Owner/Repo**     | `heicfast/d-space_fork`                                                                                                                                                                          |
| **Visibility**     | public                                                                                                                                                                                           |
| **Default branch** | `main`                                                                                                                                                                                           |
| **License (SPDX)** | `GPL-3.0` (or-later)                                                                                                                                                                             |
| **Description**    | `Windows data recovery wizard and safe disk cleanup — raw NTFS recovery engine, MFT-fast scanning, 0-100 safe-to-delete scoring, staging+undo guarantee with cross-volume two-phase commit. Tauri v2 + React 19 + Rust.` |
| **Homepage**       | `https://github.com/heicfast/d-space_fork/releases`                                                                                                                                              |
| **Topics (20)**    | `blake3, data-recovery, developer-tools, disk-analyzer, disk-space, disk-usage, filesystem, mft, ntfs, open-source, rust, safe-delete, sqlite, staging, storage, tauri, tauri-v2, undo, react, windows` |

When any of this changes, update the table and mirror it to the remote.

---

## 2. README skeleton (DiskWizard template)

### 2.1 Section order (canonical)

1. **Header** — center-aligned, title + tagline
2. **🎬 Demo**
3. **Badge row** — CI/Release/Downloads (when present) → License →
   Status → Platform → Tech → DiskWizard
4. **📌 At a glance**
5. **🆕 What's new / What's done so far**
6. **🎯 Vision** (optional)
7. **✨ Key Features**
8. **🛠️ Tech Stack**
9. **🗺️ Roadmap**
10. **📥 Installation** + **🚀 Quick Start**
11. **🛡️ Security Scan Results** (when a release exists)
12. **🤝 Contributing**
13. **🎨 DiskWizard Family**
14. **💖 Sponsors**
15. **❤️ Support**
16. **📜 License**

### 2.2 Header pattern

```markdown
<div align="center">

<img src="<icon-path>" width="128" alt="DiskWizard logo" />

# DiskWizard

**Windows data recovery wizard and safe disk cleanup**

_MFT-fast scanning, safe-to-delete scoring, guaranteed undo_

</div>
```

### 2.3 Badge row

```
[CI] [Release] [Downloads]                       (when present)
[License: GPL-3.0+]
[Status: alpha/beta/stable — matching the actual state]
[Platform: Windows 10/11]
[Tauri v2] [React 19] [Rust stable] [SQLite] [BLAKE3]
[DiskWizard]
```

### 2.4 Language

- **English is the default and primary language** of the product, the
  README, commit messages, code comments and CI. The UI offers a
  manual switch to 简体中文.
- Technical terms (MFT, NTFS, two-phase commit) pass without
  explanation — the audience is developers and technical users.

---

## 3. Tech stack & status

- **Status:** v0.x — in sync with the newest version badge in the
  README
- **Core:** Tauri v2 (Rust + WebView2), `requireAdministrator`
  manifest (raw volume access)
- **Frontend:** React 19 + TypeScript + Vite, connector architecture
  (real IPC bridge by default; mock is an explicit test/dev opt-in)
- **Storage:** SQLite (scan cache, staging ledger, sessions)
- **Hashing:** BLAKE3
- **Filesystem:** MFT direct read (NTFS) incl. deleted records;
  staging+undo cross-volume two-phase commit
- **Target:** Windows 10 1809+ and Windows 11 only
- **Visualization:** squarified treemap (Size Map), result tree

---

## 4. License

- **GPL-3.0-or-later** (SPDX: `GPL-3.0`). The README badge and the
  `LICENSE` file stay consistent.

---

## 5. Commit message style

Conventional commits:

- `feat(readme): ...`, `fix(readme): ...`, `docs(readme): ...`
- `chore: ...` — config / FUNDING / dependency bump
- `feat(<area>): ...`, `fix(<area>): ...` — code (recovery, scan,
  staging, ui, connector, ...)

Language: English.

---

## 6. File hygiene

- Files whose names contain `:` or `\` are **never committed**.
- **Mandatory:** `README.md`, `LICENSE`, `.github/FUNDING.yml`
- **Preferred:** `.gitignore`, `docs/`
- `git status` review before every push.

---

## 7. Repo-specific notes

- **Safe-to-delete scoring (0-100)** — algorithm claims in the README
  must stay in sync with the code. If the scoring formula changes,
  version it in the README (e.g. "v2 scoring rubric").
- **Staging + undo guarantee** — the cross-volume two-phase commit
  claim is in the README; breaking it triggers a major bump.
- **Release hygiene** — after every release keep the homepage URL at
  `/releases` (not `/releases/tag/vX.Y.Z`) so it shows the latest.
- **MFT direct read / recovery** — requires administrator privileges;
  stated explicitly in the README Quick Start (the app manifests
  `requireAdministrator` and shows the UAC prompt at launch).
- **Connector rule** — production builds ship the REAL tauri
  connector only (`VITE_CONNECTOR=tauri` in the release workflow);
  a mock-connector build must never ship.

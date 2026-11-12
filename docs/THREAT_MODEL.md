<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# DiskWizard Threat Model

Master architecture chapter 27. Lesson carried over from AdminToolkit
and DTransfer: formalize the attacker model — it is also required for
ISO 27001 audits.

This document reflects the live state for v0.4 (beta). It will be
extended after a third-party pen-test in v2.0.

---

## 1. Attacker Model (chapter 27.1)

| Actor                                 | Capability                | Motivation                   | Phase 1 mitigation status                           |
| ------------------------------------- | ------------------------- | ---------------------------- | --------------------------------------------------- |
| **A1. Malware (outside any sandbox)** | User-space code execution | Stealing user data           | **Partial** — staging ACL v0.5 (T2)                 |
| **A2. Standard Windows user**         | Browser, file system      | License bypass, usage rights | **Waiting** — license server v2.0 (T4)              |
| **A3. Local administrator**           | System-wide access        | Erasing forensic evidence    | **Designed** — Event Log v0.5 (T3)                  |
| **A4. Network attacker (MITM)**       | Network sniffing          | License info, telemetry      | **Waiting** — telemetry v0.5, opt-in off by default |
| **A5. Hostile competitor**            | Public review platforms   | Reputation damage            | **Designed** — AV whitelist chapter 18.4            |

**Out of scope (v1.0):** kernel-mode malware, physical attacks
(cold-boot), supply-chain compromise of Rust crates (side module:
cargo audit v0.5).

---

## 2. Attack Surface (chapter 27.2)

| Surface                                                          | Risk                              | Phase mitigation                                                                                                              | Gap                                                               |
| ---------------------------------------------------------------- | --------------------------------- | ----------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------- |
| **Tauri IPC** (`invoke_handler`)                                 | Privilege escalation via commands | Tauri capability system, default `default.json`; all Rust commands typed + return `Result<T, Error>`                          | Capability narrowing v0.5                                         |
| **MFT raw read** (`\\.\C:`)                                      | Admin-only API, needs fallback    | Chapter 5.2A three-tier strategy: FindFirstFile fallback when not elevated                                                    | MFT Service named-pipe ACL v2.0                                   |
| **Staging folder** (`%LOCALAPPDATA%\DiskWizard\staging`)         | Race conditions, exfiltration     | Atomic `fs::rename` + cross-volume 2PC + WAL (chapter 12.3)                                                                   | ACL restriction + SQLite encryption (v0.5)                        |
| **SQLite DB** (`%LOCALAPPDATA%\DiskWizard\db\diskwizard.sqlite`) | Local tampering, schema downgrade | Forward-only migrations (rusqlite_migration), `permanent_deletes_forensic` ledger                                             | WORM log + Event Log v0.5                                         |
| **RestartManager probe** (chapter 34.4)                          | System handle-table disclosure    | On-demand only, hot-path isolation (chapter 34.5.1); RM PID + app name, no system-handle leak                                 | Deeper NtQuerySystemInformation probing unnecessary (RM suffices) |
| **Locked-file probe** (`detect.rs`)                              | DoS via CreateFileW spam          | Explicit user click in the UI; async via spawn_blocking                                                                       | No rate limit (v0.5 — 0.5 s debounce)                             |
| **Telemetry endpoint** (optional)                                | Data leak                         | **None**: no real endpoint yet; only the settings flag exists                                                                 | HTTPS + cert pinning v0.5                                         |
| **Auto-update** (Tauri updater)                                  | Supply chain                      | **Active** (since v0.3.0): Ed25519 signatures + latest.json endpoint; `.sig` generation enforced via `createUpdaterArtifacts` | Ed25519 signature check + tag-based releases                      |
| **Recovery engine** (v0.4)                                       | Raw-volume writes via repair      | Repair is dry-run by default; apply is consent-gated (typed phrase) + undo log + non-system volumes only                      | Event Log integration v0.5                                        |

---

## 3. Threat Scenarios (chapter 27.3)

### T1: Malware abuses an MFT Service

**Attacker:** A1 user-space malware. **Goal:** file deletion with
SYSTEM privileges.

**Mitigation (phase 1):** DiskWizard is a single-process Tauri
application; there is no separate MFT Service (chapter 5.2A layer 3
is a stub). Delete/staging operations run only in the user context.
If an MFT Service is added in v2.0, a named-pipe ACL
(`local user only`) plus a signed-binary check become mandatory.

**Status:** Out of scope (no layer 3).

### T2: Staging-folder exfiltration

**Attacker:** A1 user-space malware. **Goal:** copying sensitive
files from staging.

**Mitigation (phase 1):** staging lives under
`%LOCALAPPDATA%\DiskWizard\staging`. Default Windows ACLs restrict it
to the user. Malware running as another user cannot reach it through
normal means, but malware running **as the same user** can read the
folder — an accepted risk. SQLite-at-rest encryption arrives in v0.5.

**Status:** Default ACLs preserved; extra protection in v0.5.

### T3: Forensic-trace deletion

**Attacker:** A3 local administrator wanting to erase traces after
the fact.

**Mitigation (phase 1):** the `permanent_deletes_forensic` table
lives inside the DB, so an administrator can delete it directly. v0.5
adds Windows Event Log integration (every permanent delete is also
written to the OS event log, which persists on the Windows side even
if the admin manipulates the app log).

**Status:** Table present; OS-level ledger in v0.5.

### T4: License cracking

**Attacker:** A2 standard user wanting Pro features without paying.

**Mitigation (phase 1):** no licensing/monetization exists yet
(GPL-3.0-or-later, open source). A Pro tier in v2.0 would use an
Ed25519-signed license file, server-side activation and a 30-day
offline grace period. **GPL-compatible**: cracking cannot be fully
prevented; the goal is economic deterrence (chapter 22.6 — ethical
constraints).

**Status:** Not applicable in phase 1 (no Pro tier).

### T5: Anti-virus false-positive attack

**Attacker:** A5 hostile competitor. **Method:** reporting the
DiskWizard binary as malware.

**Mitigation (phase 1):** the chapter 18.4 EDR/AV certification
roadmap lands in v0.5 — VirusTotal monitoring, proactive AV
whitelist submissions, README warnings for unsigned installers.

**Status:** Waiting (the release workflow currently produces
unsigned MSI/NSIS).

---

## 4. Security Audit Roadmap (chapter 27.4)

| Phase           | Action                                                                         | Target date     |
| --------------- | ------------------------------------------------------------------------------ | --------------- |
| **v0.1 (done)** | Internal security review — this document                                       | Phase 1 alpha   |
| **v0.3 (done)** | Tauri updater Ed25519 chain, updater artifacts enforced                        | Pre-stable      |
| **v0.5**        | `cargo audit` CI gate, dependency review, SQLite encryption, WAL recovery test | Phase 2 end     |
| **v1.0**        | Chapter 18.2 code signing (EV certificate)                                     | Stable release  |
| **v1.5**        | Third-party pen-test (Pentest As A Service)                                    | Post-stable     |
| **v2.0**        | ISO 27001 preparation                                                          | Enterprise tier |

---

## 5. Open Risks and Accepted Design Tradeoffs

| Risk                                                                                                     | Impact                                                    | Why accepted                                                                                                           |
| -------------------------------------------------------------------------------------------------------- | --------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| Malware in the same user context reads the staging folder                                                | Data exfiltration                                         | Every desktop app shares this risk; Windows has no per-app sandbox. AppContainer review in v2.0                        |
| RestartManager discloses PID + app name to non-admin users                                               | Information leak (Windows' own dialogs disclose the same) | Accepted in spec chapter 34.4 — the user's "who holds this file?" need outweighs the leak                              |
| Telemetry opt-in flag is client-side; even an opting-in user sends nothing while no real endpoint exists | Misleading                                                | Chapter 18.3: once the real endpoint ships in v0.5, an honest `flag=1 → POST` chain is established                     |
| Unsigned MSI/NSIS (beta)                                                                                 | SmartScreen warning                                       | Chapter 18.2: EV certificate on the v1.0 road; until then users are warned in the README                               |
| Recovery engine writes recovered files to user-chosen destinations                                       | Overwrite risk                                            | Collision policy (skip/rename/overwrite) is explicit per job; Blake3 verify-on-write; nothing is ever written silently |

---

## 6. Update Rule

This document is updated by PR for every **major security decision**
and cross-referenced with the Discovery Log (`DISCOVERY_LOG.md`). The
spec is frozen; the threat model is alive.

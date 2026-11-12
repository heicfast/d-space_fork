# Session Prompt — Feature Harvest, Gap Analysis & Production Implementation

> Archived at session start (T1). The GitHub token supplied with this prompt has been
> redacted here: it exists only in the local environment / git config of the session
> sandbox, is never committed, never logged, and the user rotates it after the session.
> Read twice before acting. This file is the authoritative statement of the session SOP.

## Role

You are a senior systems engineer and product architect (25+ years) with deep expertise in filesystem internals, data-recovery engines, partition-table reconstruction, file carving, forensic tooling, Rust systems programming, and production-grade desktop application delivery. You have personally ported recovery capabilities from mature open-source projects into a commercial product, and you know that the difference between "we read the reference" and "we shipped the capability" is a disciplined extraction of features, algorithms, edge cases, error semantics, and output formats from each reference — followed by implementation, testing, and validation on real media.

Your mandate this session is to take the target repository and implement as much as possible of the best-of-breed feature set found across six reference repositories, each of which is a mature or specialized project in its domain. You are not building a clone of any one of them. You are harvesting the strongest capabilities, algorithms, UX behaviors, and output formats from each, and implementing them in the target product's existing tech stack and architecture — at production quality, with tests, with validation on real Windows, and with a documented map of what was taken from where.

The target and its references:

- Target (implement here): https://github.com/heicfast/d-space_fork.git
- Base reference A, mature, same tech stack, UI features to match: https://github.com/terraseraph/sectorforge.git
- Base reference B, mature, same tech stack, UI features to match: https://github.com/htoklu/halim-recovery.git
- Feature reference C, partition & carving: https://github.com/cgsecurity/testdisk.git
- Feature reference D, forensic engine library: https://github.com/sleuthkit/sleuthkit.git
- Feature reference E, NTFS deep recovery: https://github.com/mjgil-rust/ntfs-recover.git
- Feature reference F, MFT analysis & rescue: https://github.com/hasithahaththella/mft-rescue.git
- GitHub token (env/git config only, never committed; user will rotate after session): [REDACTED — supplied out-of-band]

Do not clone, reference, or modify any other repository. Read every repository end to end with parallel agents before proposing a single change. Every feature claim is backed by a reference citation. Every implementation is proven by a test and a validation artifact. No hallucination. No stubs. No shortcuts.

## 1. Phase 0 — Read all seven repositories end to end

Parallel agents are mandatory. The reading must be exhaustive, not sampled. Each agent produces a structured context sheet in a scratch area (not committed) that is consolidated before findings are produced.

Agent A — Target repository, current state:

- Every source file, module, test, build file, workflow, doc, asset, and configuration.
- The current feature set: every filesystem supported, every recovery mode, every output format, every UI surface, every CLI surface, every persistence format.
- The current architecture: module boundaries, layering, dependency direction, threading, lifecycle, error model.
- The current gaps: what the product claims, what it actually does, and what is missing relative to the reference projects' capabilities.
- Deliverable: worklog/TARGET-STATE.md — the complete current-state map.

Agent B — SectorForge (base reference, same tech stack):

- Every module, every algorithm, every UI feature, every output format, every workflow, every test, every doc.
- The UX surface: every screen, every operation, every state, every transition, every output the product presents to the user.
- Specifically: how SectorForge represents recovered files with their original directory structure preserved, containing only deleted files (not currently-existing files), as a post-scan option. This is a named feature the user wants matched. Read how SectorForge does it — the data model, the scan-time tracking, the post-scan filter, the tree construction, the output layout on disk, the UI presentation — and extract the exact behavior.
- Every algorithm worth porting: recovery, carving, classification, deduplication, filtering, tree reconstruction.
- Deliverable: worklog/SECTORFORGE-FEATURES.md — an enumerated feature map, each with the source file(s) and the behavioral contract, ready to be compared against the target.

Agent C — Halim Recovery (base reference, same tech stack):

- Every module, algorithm, feature, output format, workflow, test, and doc.
- Every UX behavior, every state, every error surface, every persistence behavior.
- Every algorithm worth porting.
- Deliverable: worklog/HALIM-FEATURES.md — an enumerated feature map with source citations.

Agent D — TestDisk & PhotoRec:

- Partition detection and recovery: MBR, GPT, extended partitions, backup GPT, backup boot sectors, MFT mirror repair, filesystem signature detection, deep-scan heuristics, boundary reconstruction.
- PhotoRec: file-signature registry, header/footer matching, format-specific validators, fragmented-file handling, output layout, the naming convention for carved files, the folder grouping by extension.
- Deliverable: worklog/TESTDISK-FEATURES.md — every algorithm, heuristic, and edge case, with source citations.

Agent E — The Sleuth Kit:

- The forensic library architecture: filesystem abstraction, mmls partition-layer parsing, fls directory enumeration (allocated and deleted), icat extent extraction, blkls unallocated-space extraction, ils inode listing.
- Supported filesystems: NTFS, FAT12/16/32, exFAT, ext2/3/4, HFS+, APFS, ISO9660, Yaffs2 — and for each, the metadata structures the library parses.
- Image formats: raw, E01, and any other documented.
- Deliverable: worklog/SLEUTHKIT-FEATURES.md — the filesystem-abstraction model, every parser's key structures, and every algorithm worth adopting.

Agent F — ntfs-recover (Rust):

- The MFT-corruption recovery path: the case where both $MFT and $MFTMirr are unusable, and how the tool reconstructs enough of the MFT to recover files.
- Every algorithm, every edge case, every error path, every output format.
- Deliverable: worklog/NTFS-RECOVER-FEATURES.md — the algorithm and the edge cases, with source citations.

Agent G — mft-rescue:

- MFT record analysis: deleted-record detection, $LogFile journal replay, $UsnJrnl analysis, MFT slack-space carving, parent-reference reconstruction, filename and path recovery.
- Every algorithm, every edge case, every output format.
- Deliverable: worklog/MFT-RESCUE-FEATURES.md — the algorithms and the edge cases, with source citations.

Agent H — Cross-reference and feature matrix:

- Consolidate A–G.
- Produce the feature matrix: every feature, algorithm, heuristic, output format, and UX behavior found across B–G, mapped to whether the target (A) already has it, partially has it, or lacks it.
- For each feature, record the source reference, the source file, the behavior, the prerequisites (dependencies, data structures, filesystem support), and the target's current status.
- Deliverable: worklog/FEATURE-MATRIX.md — the master extraction document. This is the deliverable that drives the rest of the session.

Nothing is implemented until the feature matrix is committed and pushed.

## 2. The named feature the user called out

The user has specifically called out one feature as the reference behavior to match:

Reconstructed files with their directory structure preserved — containing only deleted files, not the existing ones — available as an option after the scan finishes.

This is a first-class deliverable, not a secondary item. The extraction from Agent B must establish, with source citations, exactly how SectorForge does it:

- When, during the scan, does the product identify which files were deleted and which were not?
- How is the distinction represented in the data model — a flag on the node, a separate list, a scan-time classification, a post-scan filter?
- How does the product reconstruct the directory tree containing only the deleted files, preserving the original folder hierarchy?
- How is the option presented to the user after the scan finishes — a filter, a toggle, a separate view, an export option?
- How does the reconstruction behave when parent folders no longer exist, when a deleted file's parent was also deleted, when a deleted file's parent is a currently-existing folder, when the parent chain is broken, or when multiple deleted files share a parent?
- How does the output look on disk — the folder structure, the naming, the collision handling?
- Every edge case the reference handles, and how.

The target implementation must match this behavior — not approximately, not "close enough," but with the same observable contract on the same inputs. The implementation is validated by comparing the reconstructed tree for the same seeded disk against the reference's behavior where feasible, or against a specified expected tree where a direct comparison is not feasible.

## 3. The extraction taxonomy

Every feature extracted from a reference is categorized and recorded in worklog/FEATURE-MATRIX.md with:

Category, and examples of what belongs in each:

- Partition layer: MBR, GPT, extended partitions, backup GPT, backup boot sectors, protective MBR, partition signature detection, boundary heuristics, deep-scan heuristics, lost-partition candidate scoring, virtual-volume construction.
- Filesystem layer: per-filesystem metadata parsing, bootstrap, fixups, attribute parsing, runlist decoding, tree walks, deleted-record detection, orphan reconstruction, journal replay, snapshot awareness, compression handling, encryption detection.
- Recovery layer: recovered-file reading, extent reading, fragment handling, alternate data streams, hard links, sparse files, timestamps, attributes, permission and ACL preservation where applicable.
- Carving layer: signature registry, header/footer matching, grammar-aware validators, per-format size caps, fragmentation-aware recovery, deduplication, result merging with filesystem-parsed results, naming conventions, folder grouping.
- Data model: recovered-file node shape, tree shape, deleted/live classification, confidence grading, deduplication identity, provenance tracking, output layout.
- UX/UI layer: every screen, every operation, every filter, every view, every option, every transition, every state, every empty/loading/error/partial state, every output presentation. This includes the "reconstructed files with directory structure preserved, deleted only" option and every equivalent user-facing behavior.
- Output formats: every format the reference produces — file layout on disk, JSON, CSV, session persistence, report — and the exact semantics of each.
- Persistence: session save/load, checkpointing, resume, crash recovery.
- Performance: read sizing, concurrency, queue depth, buffering, cancellation, memory bounds — and the reference's measured or documented behavior for each.
- Error model: every error class, every code, every propagation path, every user-facing message.
- Edge cases: every edge case the reference documents or handles, with the reference's exact behavior.

For each feature, the matrix records:

- Feature — the capability as a one-line statement.
- Reference source — which repository, which file, which function or section.
- Behavior — the exact observable contract, including edge cases.
- Prerequisites — what must exist in the target for this to be implemented (data structures, parsers, UI surfaces).
- Target status — PRESENT, PARTIAL, MISSING, NOT-APPLICABLE (with reason).
- Priority — P0 (core recovery capability, must ship), P1 (high-value), P2 (valuable, lower priority), P3 (nice-to-have).
- Notes — any caveat, licensing consideration, or divergence required by the target's architecture.

Licensing is checked before porting. Every reference repository's license is read and recorded. Any capability from a license that is incompatible with the target's license is either re-implemented from understanding of the algorithm (not copied) or excluded with a documented rationale. No code is copied verbatim from any reference repository; the algorithms are re-implemented from understanding, and the reference is cited for the algorithm's behavior.

## 4. Implementation waves

The work is structured in waves so that each wave delivers a coherent, tested, shippable increment. Each wave is committed and pushed as its own unit, with tests and validation artifacts.

Wave 1 — Foundations: every shared data structure the feature matrix requires (recovered-file node, tree model, deleted/live classification, provenance, dedup identity, output model); every parser primitive the matrix requires that the target lacks; the abstract access layer the parsers consume. Unit tests against synthetic fixtures.

Wave 2 — Filesystem parsers: every filesystem the matrix identifies as missing or partial, implemented to the reference's behavior with all edge cases; NTFS deep recovery including the ntfs-recover and mft-rescue algorithms; FAT, exFAT, ext4, HFS+, and any other filesystem the reference projects support that the target's scope includes. Validated against seeded fixtures with byte-for-byte comparison.

Wave 3 — Partition layer: MBR, GPT, extended partitions, backup structures, boundary heuristics, lost-partition candidate scoring, virtual-volume construction, deep-scan flow. Validated against deleted-partition and damaged-partition-table fixtures.

Wave 4 — Carving: signature registry, header/footer matching, grammar-aware validators, fragmentation-aware recovery, deduplication, result merging with filesystem-parsed results. Validated against seeded carved-format fixtures.

Wave 5 — Recovered-file tree and deleted-only reconstruction: the full data model including deleted/live classification and provenance; the reconstructed-files-with-directory-structure-preserved feature matching the reference's behavior exactly including every edge case; the output layout on disk with collision handling, path preservation, and the post-scan option. Validated against the seeded fixture and against the reference's behavior where feasible.

Wave 6 — UX and output: every screen, operation, filter, view, option, transition, and state the reference presents; every output format; the reconstructed-files option surfaced in the UI as the reference surfaces it; every empty/loading/error/partial state. Validated by walking every surface on real Windows and capturing evidence.

Wave 7 — Persistence, session, and performance: session save/load, checkpoint, resume, and the persistence format; read sizing, concurrency, buffering, cancellation, memory bounds to match or exceed the reference. Validated by measurement.

Wave 8 — Hardening: every edge case in the matrix handled with tests; every error path with a test; every unsupported-format path truthfully disabled with a documented reason; fuzz smoke on every parser; ASan/UBSan/TSan clean where the target's stack permits.

Wave 9 — Documentation and release: every feature documented with the reference cited; worklog/PARITY.md with each item resolved to IMPLEMENTED, DELIBERATELY-DIVERGENT (rationale), or NOT-APPLICABLE (reason); no unresolved MISSING at session end unless documented with precise scope and a follow-up plan in worklog/FOLLOW-UP.md; a release candidate tagged from the final state, built and validated.

## 5. Quality and validation standard

- Ground truth first: every feature tested against a seeded fixture (VHD/VHDX, disk image, or real media) with a seed manifest recording every file's path, size, timestamps, attributes, and SHA-256 before deletion or damage; generated deterministically and versioned.
- Real Windows validation: every user-visible surface validated on real Windows runners with the actual product, not a mock, not a Linux emulation.
- Byte-for-byte comparison where content is recovered: recovered files compared against the seed manifest by SHA-256; the recovered tree compared against the expected tree.
- Reference comparison where the reference is runnable; where not runnable, the reference's documented behavior is the standard, cited from the source.
- Captured evidence for every claim — logs, screenshots, comparison reports, captured outputs — in worklog/VALIDATION/.
- No fake anything: no fake progress, no fake results, no fake enumeration, no fake state. Every user-visible value is derived from the actual implementation.

## 6. Non-negotiable constraints

- Read before writing: every feature extracted before implemented; every implementation cites the reference algorithm in the commit message and code comment.
- Re-implement from understanding: never copy code verbatim from any reference repository; cite the reference for the algorithm's behavior; write the implementation in the target's style and architecture.
- Check licensing before porting; record every reference's license.
- No behavior change beyond the new features: every existing target feature continues to work identically; new features are additive or opt-in.
- No stubs, no TODOs, no placeholders: a feature is either completed, truthfully disabled with a documented reason, or moved to worklog/FOLLOW-UP.md with a precise scope.
- No masked failure: every error propagates; no retry loop that hides a dead disk; no silent zero-fill; no fake result.
- No hallucination: every claim about a reference is a source citation; every claim about the implementation is a test or a captured validation artifact.
- No shortcuts. Production grade only.
- Commit and push after every coherent unit: one feature, one verified implementation, one push, one feature-matrix update. Do not batch unrelated features.
- Pull frequently. Never force-push. Leave main green.
- Token hygiene: the GitHub token is used only from env/git config; never committed, never logged.
- Preserve audit trail: prior worklogs, diff trails, and audit artifacts in the target repository are preserved. Extend, never overwrite.

## 7. CI and release

- Windows CI: every feature's tests run on Windows including every user-visible surface; no continue-on-error; no skipped stages without a recorded reason; no flaky-test masking.
- Linux CI: every pure-logic and parser test that can run cross-platform runs on Linux; sanitizers clean.
- Fuzz smoke: every parser touched has a structure-aware harness; the smoke campaign passes.
- Release: a release candidate tagged from the final state, built on Windows, installed on a clean Windows machine, every new feature exercised against the seeded fixture; evidence in worklog/VALIDATION/.

## 8. Deliverables

1. worklog/TARGET-STATE.md, SECTORFORGE-FEATURES.md, HALIM-FEATURES.md, TESTDISK-FEATURES.md, SLEUTHKIT-FEATURES.md, NTFS-RECOVER-FEATURES.md, MFT-RESCUE-FEATURES.md — the Phase 0 context sheets, pushed before any code change.
2. worklog/FEATURE-MATRIX.md — the master extraction document.
3. Every P0 and P1 feature implemented, tested, and validated on real Windows, evidence in worklog/VALIDATION/.
4. Every P2 and P3 feature either implemented or documented in worklog/FOLLOW-UP.md with a precise scope.
5. The reconstructed-files-with-directory-structure-preserved feature implemented to match the reference's behavior exactly, every edge case handled, validated against a seeded fixture.
6. Every existing target feature verified to still work identically after the session, with regression tests.
7. Every supported filesystem and recovery mode documented in the product's docs, with the reference cited.
8. worklog/PARITY.md — the final feature matrix with every item resolved.
9. Full CI green on Windows and Linux for the right reason.
10. A release candidate tagged, built, installed on a clean Windows machine, and validated against the seeded fixture, with evidence captured.
11. worklog.md — the chronological handoff with every commit SHA, CI run link, validation artifact, deferred item with precise scope, and the feature matrix's final state.
12. All changes committed and pushed with detailed, reference-citing messages.

## 9. First 20 TODOs

- T1: Clone all seven repositories; record commits; save this prompt (token redacted) to worklog/; write the worklog entry.
- T2: Spawn parallel agents A–G; read every repository end to end; produce the seven Phase 0 context sheets; push before any code change.
- T3: Consolidate into worklog/FEATURE-MATRIX.md — the master extraction document; push as the gate document.
- T4: Establish the seeded validation fixture (VHD/VHDX or disk image with a seed manifest and known ground truth); version it; push.
- T5: Wave 1 — shared data structures, parser primitives, abstract access layer; unit tests; push.
- T6: Wave 2 — NTFS deep recovery (ntfs-recover and mft-rescue algorithms); seeded validation; push.
- T7: Wave 2 — FAT, exFAT, and the other filesystems the matrix identifies; seeded validation; push.
- T8: Wave 3 — partition layer (MBR, GPT, backup structures, lost-partition candidate scoring, virtual-volume construction); seeded validation; push.
- T9: Wave 4 — carving (signature registry, validators, fragmentation handling, deduplication, result merging); seeded validation; push.
- T10: Wave 5 — recovered-file tree data model, deleted/live classification, provenance; push.
- T11: Wave 5 — the reconstructed-files-with-directory-structure-preserved feature, matching the reference's behavior exactly, with every edge case; seeded validation against the reference; push.
- T12: Wave 6 — every UX surface the reference presents, including the reconstructed-files option; real Windows validation; push.
- T13: Wave 7 — session persistence, checkpoint, resume, and performance; validated against the reference; push.
- T14: Wave 8 — every edge case, every error path, every unsupported-format path; tests for each; fuzz smoke; push.
- T15: Wave 9 — documentation for every feature; worklog/PARITY.md; worklog/FOLLOW-UP.md for anything deferred; push.
- T16: Full CI sweep on Windows and Linux; fix every red for the right reason; push.
- T17: Regression sweep — every existing target feature verified to still work identically; push.
- T18: Release candidate — tag, build, install on clean Windows, exercise every new feature against the seeded fixture, capture evidence; push.
- T19: Final verification sweep — every P0/P1 feature resolved; every P2/P3 feature either implemented or documented; every claim evidenced; no stub; no fake; no hallucination; push.
- T20: Handoff — worklog.md complete with every commit SHA, every CI run link, every validation artifact, every deferred item with a precise scope, and the feature matrix's final state; final push.

## Final reminders

- Harvest, do not clone. The target is not a clone of any one reference. It is the best-of-breed feature set extracted from six mature projects, implemented in the target's stack and architecture.
- The named feature is the priority: reconstructed files with directory structure preserved, deleted only, as a post-scan option — matching the reference's behavior exactly, with every edge case.
- The feature matrix is the roadmap: every feature sourced, cited, prioritized, and tracked from extraction to implementation to validation. Nothing is implemented without an entry in the matrix.
- Licensing is checked: every reference's license read and recorded; no code copied; algorithms re-implemented from understanding; the reference cited for behavior.
- Validation is real: seeded fixtures with known ground truth; real Windows for every user-visible surface; byte-for-byte comparison where content is recovered; captured evidence for every claim.
- No stubs. No TODOs. No fake results. No hallucination. No shortcuts. Production grade only.
- Preserve the existing product: every existing target feature continues to work identically; new features are additive or opt-in.
- Work long-horizon: commit and push after every coherent unit; keep worklog.md current so an unexpected session stop loses nothing; leave main green at every push.

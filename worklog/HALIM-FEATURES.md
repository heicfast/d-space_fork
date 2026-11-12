# HALIM RECOVERY — Feature Harvest Report

## Header

- **Repo URL:** https://github.com/htoklu/halim-recovery
- **Local clone:** `/home/z/my-project/refs/halim-recovery` (depth-1, no network access used)
- **HEAD SHA:** `3b5d66d8537432f54b76489e717387f9da198635` (62 tracked files, verified via `git ls-files`)
- **Product version:** 0.5.1 — `src/HalimRecovery.Cli/Program.cs:9` `const string Version = "0.5.1"`; `src/HalimRecovery.App/App.xaml.cs:24` `AppVersion => "0.5.1"`
- **License:** MIT — `LICENSE` lines 1–21, "Copyright (c) 2026 Halim Toklu"; no NOTICE file; `THIRD_PARTY_NOTICES.md` covers dependencies
- **Tech stack:** .NET 9 / C# (`net9.0-windows` in all four csproj) · WPF + CommunityToolkit.Mvvm 8.4.2 GUI · console CLI frontend · hand-written NTFS/FAT32/exFAT parsers and carving validators (no DiscUtils, no GPL code — `docs/ARCHITECTURE.md` "Technology Decisions (PHASE 1)") · Win32 P/Invoke raw volume I/O · WMI disk enumeration · xUnit tests · Inno Setup installer · PowerShell VHD test lab
- **Date:** 2026-11-12
- **Agent:** C — Halim Recovery reference (feature-harvest session)

## Overview & Architecture

- **What it is:** free/open-source Windows deleted-file recovery. Reads the raw volume **read-only**, analyzes filesystem metadata and/or raw signatures, scores recoverability, previews, and writes selected files to a safe destination with SHA-256-verified reports (`README.md` lines 14–37).
- **Solution layout** (`HalimRecovery.sln`): `src/HalimRecovery.Core` (engine, zero UI dependency), `src/HalimRecovery.Cli`, `src/HalimRecovery.App` (WPF), `tests/HalimRecovery.Tests`, `testlab/` scripts.
- **Module map** (`docs/ARCHITECTURE.md` "Layers"):
  - `Disks/DiskEnumerator.cs` — physical disks, volumes, volume↔disk mapping
  - `IO/RawDiskReader.cs`, `IO/IByteSource.cs` — sector-aligned read-only stream + read abstractions
  - `FileSystems/FileSystemDetector.cs` + `FileSystems/Ntfs|Fat32|ExFat` — boot detection and quick scanners
  - `Carving/DeepScanner.cs`, `Carving/FormatSpecs.cs` — streaming carver + per-format validators
  - `Health/HealthScorer.cs` — GREEN/YELLOW/RED evidence scoring
  - `Preview/PreviewProvider.cs` — pre-recovery previews (best-effort, safe)
  - `Recovery/RecoveryEngine.cs`, `Recovery/PathSafety.cs` — safe writer + name sanitization
  - `Reporting/RecoveryReport.cs` — session reports (text + JSON)
  - `Scanning/ScanEngine.cs`, `Scanning/PauseToken.cs` — orchestration, progress, cancel, pause
  - `Search/NaturalQueryParser.cs` — offline rule-based query parser
  - `Logging/Log.cs` — rolling file logger; `TestSupport/SampleFiles.cs` — lab fixtures
- **Entry points:** `src/HalimRecovery.App/App.xaml:4` `StartupUri="MainWindow.xaml"` → `MainWindow.xaml.cs`; `src/HalimRecovery.Cli/Program.cs` top-level statements; test assembly `tests/HalimRecovery.Tests`.
- **Threading:** long ops wrapped in `Task.Run` + `IProgress<ScanProgress>` + `CancellationToken` (e.g. `ScanEngine.QuickScanAsync`); `RawDiskReader.ReadAt` serialized by `lock (_gate)`; UI thread never blocks (CONTRIBUTING.md rule 2; `.cursorrules` "Recovery Doğruluğu" section).
- **Pause model:** cooperative `PauseToken` (ManualResetEventSlim) — `Pause()/Resume()/WaitIfPaused(ct)` (`Scanning/PauseToken.cs`). Consumed **only** by `DeepScanner.Scan` (grep-verified); Quick Scan path never receives it.
- **Error model:** typed exceptions with Win32 mapping at open time; per-file failures recorded in `RecoveredItem.Error` (never swallowed); degradation paths log+continue (details in Error Model section).
- **Persistence:** no database. In-memory results per session; CLI `--json` export; per-recovery report files; daily log `%LOCALAPPDATA%\HalimRecovery\logs\halim-yyyyMMdd.log` (`Log.cs` `LogDirectory`/`LogFile`).
- **Memory discipline:** fixed buffers — DeepScan 8 MiB chunk (`DeepScanner.cs:17` `ChunkSize = 8 << 20`), MFT ~1 MiB record-aligned chunk, per-cluster reads in FAT/exFAT, 4 MiB recovery copy buffer (`docs/ARCHITECTURE.md` invariant 3).
- **Build & packaging:** `dotnet publish src/HalimRecovery.App -c Release -r win-x64 --self-contained -p:PublishSingleFile=true -o publish/app` (README lines 64–70); `IncludeNativeLibrariesForSelfExtract=true` prevents silent exit of the single-file WPF EXE (`HalimRecovery.App.csproj` comment; CHANGELOG 0.5.1); installer `ISCC.exe installer\HalimRecovery.iss` (Inno Setup 6, `PrivilegesRequired=admin`, EN+TR languages, optional desktop icon unchecked).
- **Admin model:** app manifest `requireAdministrator` (`src/HalimRecovery.App/app.manifest` requestedExecutionLevel) — raw volume access needs it.

## Feature Inventory (F-xx)

### Filesystems & scan depth

- **F-01 NTFS Quick Scan — deleted MFT records.** Streams $MFT (located via record 0's own data runs) in ~1 MiB record-aligned chunks.
  - Deleted filter (copy-exact): `!rec.InUse && !rec.IsDirectory && rec.BaseRecordNumber == 0 && rec.FileName.Length > 0 && rec.HasData` (`NtfsScanner.ScanDeletedFiles`).
  - Resident `$DATA` kept byte-exact in `MftRecord.ResidentData`; non-resident runs → `FileExtent(run.Lcn * bpc, run.ClusterCount * bpc)`, sparse runs (`Lcn < 0`) skipped.
  - Timestamps from `$STANDARD_INFORMATION` (attr 0x10); name from best-namespace `$FILE_NAME` (0x30) — Win32(1)/Win32AndDos(3) beat POSIX(0) beat DOS 8.3(2) (`MftRecord.Parse` namespace ranking).
  - Original path rebuilt by parent-record walk to root (record 5), guard ≤64 hops, self-reference guard; unknown ancestor → `@"?\" + partial` (`NtfsScanner.BuildPath`).
  - Extension records (non-zero `BaseRecordNumber`) excluded; directories excluded.
- **F-02 NTFS overwrite analysis via `$Bitmap`.** Record 6 loaded as BitArray (sanity bound ≤512 MiB); per-file `OverwrittenFraction = reused/total`, `-1` if bitmap unavailable (warn "…overwrite analysis disabled") (`NtfsScanner.LoadClusterBitmap`/`MeasureOverwrittenFraction`).
- **F-03 FAT32 Quick Scan.** FAT loaded masked `& 0x0FFFFFFF` (`LoadFat`); recursive directory walk following FAT chain; deleted entry = first byte `0xE5`; LFN entries (attr 0x0F) assembled — deleted LFN sequence bytes lost so disk order is reversed (`AssembleLfn`); short name first char replaced with `_` (`ReadShortName`); guards: cycle set `visited`, chain guard 1,000,000, depth cap 64.
  - Honest limitation in class doc: FAT chain zeroed on delete → "only the first cluster is known … Recovery assumes the file was stored contiguously".
  - HealthNote: "FAT32: cluster chain lost on delete; contiguous layout assumed".
- **F-04 FAT32 orphaned-directory harvesting.** Live-looking directory whose first cluster is FAT-free (or 0xE5 dir entry) → `WalkDeletedDirectory`: walks ≤256 contiguous clusters; first must look like a directory (`data[0] == '.' || data[0] == 0xE5`); stops at reallocated clusters; **all children recoverable regardless of their own deleted flag** — "Windows does not rewrite child entries when it deletes a whole tree" (`Fat32Scanner.WalkDeletedDirectory` doc + lines 130–139).
- **F-05 exFAT Quick Scan.** Boot `EXFAT   ` validation; entry-set walk: `0x85/0x05` file entry (deleted=0x05), `0xC0/0x40` stream extension (NoFatChain bit `0x02`, first cluster, dataLength), `0xC1/0x41` name entries (15 UTF-16 chars each).
  - Deleted = in-use bit cleared (0x85→0x05, 0xC0→0x40, 0xC1→0x41) (`ExFatScanner` class doc).
  - `NoFatChain` set → HealthNote "exFAT: NoFatChain set — file was stored contiguously (exact layout known)"; else "exFAT: FAT chain cleared on delete; contiguous layout assumed".
  - `LooksLikeDirectoryCluster` type-code whitelist stops straying into file data.
- **F-06 exFAT allocation bitmap as authoritative allocator.** Root entry `0x81` bitmap preferred over FAT (deleted files' FAT entries "often left stale by Windows") for reuse measurement and orphan-dir decisions; FAT is fallback (`ExFatScanner.LoadAllocationBitmap`/`IsClusterAllocated`).
- **F-07 Filesystem detection.** OEM string `NTFS    ` / `EXFAT   `; FAT32 type string at offset 82; FAT16/FAT12 at 54; else `Unknown` (`FileSystemDetector.Detect`). FAT16 is in `FileSystemKind` but unsupported for Quick Scan → honest `NotSupportedException`.

### Scan & recovery modes

- **F-08 Quick Scan orchestration.** `ScanEngine.QuickScanAsync` dispatches per FS, then `HealthScorer.Score(f, new ExtentByteSource(f, _reader))` for every result (`ScanEngine.cs` 44–61).
- **F-09 Deep Scan (signature carving).** 8 MiB streaming chunks, per-512-byte-sector signature probe, structural `Measure`, skip past carved file end (sector-aligned) to avoid re-detecting embedded content; bad-sector `IOException` → warn + skip one chunk, keep scanning; progress every 64 MiB (`DeepScanner.Scan`).
- **F-10 Raw image-file scanning.** `ScanEngine` ctor: if `File.Exists(driveLetterOrImage)` → open image file instead of `\\.\X:` (`ScanEngine.cs` 24–36). Used by offline regression (`testlab/Test-OfflineImages.ps1`).
- **F-11 Recovery engine.** `RecoveryStatus { Recovered, PartiallyRecovered, Failed, Skipped }`; streaming copy + inline `SHA256` (`TransformBlock` per 4 MiB buffer); `written == 0` → Failed and output deleted; `written < target` → PartiallyRecovered + error "Only {written}/{target} bytes were readable."; `MakeUnique` → "name (i).ext" ×9999 then `_{guid:N}` (`RecoveryEngine.RecoverOne`/`MakeUnique`).
- **F-12 Directory-structure preservation in output.** `preserveFolderStructure` recreates sanitized `OriginalPath` under destination (`RecoveryEngine.RecoverOne` 91–95); always `true` from CLI and GUI (`Program.cs:134`, `MainViewModel.cs:352`).
- **F-13 Destination safety.** Same-volume destination **blocked**: "Destination is on the SOURCE volume. Writing there can overwrite the very data you are trying to recover. Choose another drive."; same-physical-disk → warning "…Recover files to another drive whenever possible." (acknowledge to continue); free-space preflight throws "Insufficient space on destination: need X MiB, have Y MiB." (`RecoveryEngine.CheckDestination`/`Recover`).
- **F-14 Path safety.** Invalid chars/`/`/`\`/ctrl → `_`; `..` → `_`; trailing dots trimmed; reserved names CON/PRN/AUX/NUL/COM1–9/LPT1–9 prefixed `_`; 200-char cap; empty → `unnamed`; relative path drops `.`/`..` segments; `IsInsideRoot` containment check (`PathSafety.cs`).

### Health & confidence

- **F-15 Evidence-only health scoring** (`HealthScorer.Score`):
  - Evidence 1 — cluster reuse, 55%: `55 * (1 - reuse)`; unknown (`-1`) → flat 25 + note "Cluster allocation state unknown"; reuse 0 → "No clusters reused by other files"; else "{P0} of clusters have been reused — that data is overwritten".
  - Evidence 2 — structural validation, 35%: Valid 35 ("File signature and structure verified on disk") / HeaderOnly 15 / Mismatch 0 ("Content does not match the expected file format (likely overwritten)") / NotApplicable 18 ("No structural validator available for this file type" — neutral).
  - Evidence 3 — layout certainty, 10%: resident data 10 ("File data resides in filesystem metadata (exact recovery)"); contiguous-assumed 3; fragmented 6; else 10.
  - Bands: `>=75` Green, `>=40` Yellow, else Red; `reuse >= 0.99` forces Red and Confidence ≤10.
  - Carved files: validation verdict reused from carve-time notes ("structure validated" → Valid else HeaderOnly).
- **F-16 Honest carve labeling.** `carved_{sector:D10}.{ext}` naming, `OverwrittenFraction = 0`, mandatory HealthNote "Carved file: original name, path and timestamps are not recoverable from raw data" (`DeepScanner.TryCarveAt` 139–156).

### Preview

- **F-17 Pre-recovery preview** (`PreviewProvider.GetPreview`):
  - Images (jpg/jpeg/png/gif/bmp/webp): ≤32 MiB bytes to UI decoder; partial overwrite "may still render its upper portion".
  - Text (txt/log/csv/md/xml/json/ini): 64 KiB UTF-8, NULs stripped.
  - PDF: header version + "/Type /Page" count in first 256 KiB + "(Full rendering is available after recovery.)".
  - DOCX: `word/document.xml` text ≤4000 chars; XLSX: worksheet names + sharedStrings sample; ZIP/PPTX: first 50 entry names.
  - Media (mp3/mp4/mov/wav/m4a): metadata card (name/size/dates/source + "Media preview is available after recovery.").
  - Failure reasons copy-exact: "No data available on disk for this file.", "Image data is unreadable.", "Text data is unreadable.", "PDF data is unreadable.", "Data no longer looks like a PDF (content may be overwritten).", "DOCX structure incomplete: word/document.xml missing.", "DOCX archive is damaged and cannot be opened.", "XLSX archive is damaged and cannot be opened.", "Archive is damaged and cannot be listed.", "Preview failed: {ex.Message}".

### Search / filters / views

- **F-18 Post-scan filter dropdown.** `All / Images / Videos / Documents / Audio / Archives / High Confidence / Partial / Corrupted` (`MainViewModel.FilterOptions`) + free-text name-or-path search (`FilterPredicate`).
- **F-19 Offline natural-language smart filter** (`NaturalQueryParser.Parse`), TR+EN:
  - Categories: "fotoğraf/foto/resim/görsel/photo/picture/image"→Image; "video/film/movie/klip"; "müzik/ses/şarkı/music/audio/song"; "belge/doküman/document/evrak"; "arşiv/archive/sıkıştırılmış".
  - Extensions: 14 explicit (jpg…txt); extensions override category.
  - Time: "bugün/today"; "dün/yesterday"; "geçen hafta/last week" → −14 d; "geçen ay/last month" → −2 months (copy-exact `AddMonths(-2)`); "geçen yıl/last year" → −2 y.
  - Years 2000..now+1 with seasons: yaz/summer Jun 1–Sep 1; kış/winter Dec(prev)–Mar; bahar/spring Mar–Jun; sonbahar/güz/autumn/fall Sep–Dec; plain year → calendar year.
  - Leftover ≥3-char non-stopword → `NameContains` (e.g. "tatil", "fatura").
  - Class doc: "Purely rule-based — no cloud, no AI dependency; recovery works identically without it."

### CLI

- **F-20 CLI surface** (`Program.cs` usage block): `list-disks` · `quick-scan <drive> [--json <out.json>]` · `deep-scan <drive> [--json <out.json>]` · `recover <drive> --dest <folder> [--mode quick|deep] [--filter <name>] [--all]` · `gen-testfiles <directory> [--manifest <out.json>]`.
  - Exit codes: 0 ok; 1 unhandled error; 2 usage error; 3 access denied ("Run this tool from an elevated (Administrator) prompt."); 4 destination blocked; 5 all items failed.
  - Recover default selects non-Red files unless `--all`; `--filter` substring on FileName.
  - Scan console output: top-50 by Confidence with `[Health] %  size  path\name`, "… and N more", "TOTAL: N recoverable file(s) found.".
  - Banner notes: admin required; source only ever read; SSD/TRIM honesty line.

### Progress / cancellation UX

- **F-21 Telemetry model.** `ScanProgress { Phase, BytesProcessed, BytesTotal, FilesFound, Elapsed }` + computed `Percent`, `Speed`, `EstimatedRemaining` (null when indeterminate or >30 days) (`Models/ScanProgress.cs`).
  - Phase strings observed: "Analyzing MFT", "Scanning directories", "Deep scan (signature carving)", "Recovering {FileName}", "Paused", "Resumed".
- **F-22 Pause/Resume + Cancel.** GUI buttons while `IsScanning`; `CancelScan` → `_cts?.Cancel()`; `PauseResume` toggles `engine.Pause` (MainViewModel 155–164). **Caveat (verified):** PauseToken reaches only DeepScanner; Quick Scan does not pause. CLI passes `CancellationToken.None` (no console cancel).

### Session / persistence behavior

- **F-23 No session save/load/resume across runs.** Results live only in the in-memory `ObservableCollection<FileItemVm>`; `BackHome` cancels, disposes engine, clears results (MainViewModel 166–176). Only on-disk artifacts: CLI `--json` dump, recovery reports, logs. No scan-result cache, no project file, no resume-after-restart. (Absence stated plainly.)

### Disk model

- **F-24 Disk/volume discovery.** WMI `Win32_DiskDrive` (+ `MSFT_PhysicalDisk` MediaType 4 = SSD via `root\microsoft\windows\storage`); volumes via `DriveInfo.GetDrives()` skipping CDRom/Network/NoRootDirectory; volume→disk via `IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS` (no admin needed); GUI auto-selects first non-C volume (`DiskEnumerator.cs`, `MainViewModel.RefreshDrives`).
- **F-25 SSD/TRIM honesty surfaces.** Home-screen banner (see UX map); README "SSD / TRIM limitations — please read" ("No tool can promise '100% recovery'. Anyone who does is misleading you."); CLI banner note; docs/BENCHMARK.md "Honest limitations".

## Algorithms Worth Porting

1. **NTFS MFT two-pass scan with path reconstruction** — `src/HalimRecovery.Core/FileSystems/Ntfs/NtfsScanner.cs`.
   - Inputs: `RawDiskReader` (volume or image). Outputs: `List<RecoverableFile>` with names/paths/timestamps/extents/resident data.
   - Pass 1 builds `names[record] = (Name, Parent)` for every fixup-valid FILE record; pass 2 emits deleted files with `BuildPath` + `MeasureOverwrittenFraction`.
   - Edge cases: sparse runs; torn writes rejected by fixup; extension records skipped; self-referencing parent guard; missing parent → `?\"` prefix path.
2. **MFT fixup (update sequence array)** — `MftRecord.ApplyFixup` (lines 138–156): verify every sector-end USN equals the record USN (mismatch → torn/corrupt → reject), restore original words from USA. Test: `TornRecord_RejectedByFixup`.
3. **NTFS data-run decoder** — `DataRunDecoder.Decode`: header byte = size nibbles; 0x00 terminates; `offsetSize == 0` → sparse run (Lcn −1); signed delta LCN with sign extension; malformed input stops silently (never throws). Tests cover delta accumulation and garbage.
4. **$Bitmap overwrite fraction** — `NtfsScanner.MeasureOverwrittenFraction`: per-cluster bit test; `-1` when unknown. The honesty engine behind "how much is already gone".
5. **FAT32 orphan-directory harvesting** — `Fat32Scanner.WalkDeletedDirectory` + `HarvestDirectoryCluster`: recovers whole deleted trees; validates continuation clusters are still free; terminator 0x00 ends directory.
6. **exFAT deleted entry-set parsing** — `ExFatScanner`: type-code whitelist, deleted variants, NoFatChain exactness, allocation-bitmap-over-FAT precedence, `treatAllAsDeleted` recursion for deleted subtrees.
7. **Structural carving validators** — `Carving/FormatSpecs.cs`, each `Func<IByteSource, long, CarveMeasure?>`:
   - JPEG (`MeasureJpeg`): length-parsed segments (EXIF thumbnails skipped correctly), SOS → raw FFD9 scan with 1-byte overlap for split markers; fill/RST legal.
   - PNG (`MeasurePng`): chunk walk; first chunk must be IHDR len 13; ASCII-only types; ends at IEND.
   - GIF (`MeasureGif`): 0x3B trailer; global/local color tables `3 << ((flags & 7) + 1)`; sub-block skipper.
   - PDF (`MeasurePdf`): last `%%EOF` wins (incremental updates); stop 8 MiB after last EOF; `startxref` presence gates StructureValid.
   - ZIP/Office (`MeasureZip` + `ClassifyZip`): EOCD `PK\x05\x06`; central-directory pointer must hit `PK\x01\x02` (or 0xFFFFFFFF zip64); docx/xlsx/pptx classification by `word/`, `xl/`, `ppt/`, `[Content_Types].xml` in head 4096 and tail 64 KiB.
   - MP4/MOV (`MeasureMp4`, `LooksLikeMp4`): ftyp at bytes 4–7; box walk with 64-bit largesize (size==1), size==0 → unverifiable; moov/mdat presence; `qt` brand → mov ext.
   - MP3 (`MeasureMp3`, `Mp3FrameLength`): ID3 syncsafe skip; bare-frame sync detection; MPEG1/2/2.5 Layer3 bitrate+sample tables; frame len = 144|72 × bitrate/sampleRate + padding; ≥3 frames to carve, ≥10 for valid.
   - WAV (`MeasureWav`): RIFF size cap 4 GiB; first chunk `fmt `/`JUNK`/`LIST` sanity.
   - MaxLength caps per format (JPEG 128 MiB … MP4 8 GiB) bound measurement.
8. **Health scoring rubric** — `Health/HealthScorer.cs` (55/35/10 weights, explicit unknown=25, neutral=18, forced-Red ≥0.99 reuse). Fully explainable confidence contract.
9. **ExtentByteSource** — `IO/IByteSource.cs`: one contiguous read-only view over resident data or multi-extent runs; bridges extents on read; used by health validation, preview, recovery.
10. **Sector-aligned raw reader** — `IO/RawDiskReader.cs`: GENERIC_READ + FILE_SHARE_READ|WRITE, `IOCTL_DISK_GET_LENGTH_INFO` (GetFileSizeEx fallback for image files), position and native-buffer alignment, short-read tolerance, `ReadExact` throws.
11. **Natural-language filter parser** — `Search/NaturalQueryParser.cs`: bilingual deterministic rules (categories, extensions, relative/absolute time, seasons, stop-word-stripped name keywords).
12. **Safe-writer pattern** — `RecoveryEngine` + `PathSafety`: containment check, sanitized structure recreation, collision-safe naming, inline SHA-256, per-item status + errors, twin reports.

### Behavioral contracts (inputs → outputs, edge cases)

- **Contract A — `ScanEngine(string driveLetterOrImage, …)`** (`Scanning/ScanEngine.cs` ctor):
  - In: drive letter "E"/"E:" OR path to raw volume image. Out: engine with `Reader` (RawDiskReader), `FileSystem` (FileSystemKind), public `Pause` token.
  - Edge: image path wins when `File.Exists`; drive letter trimmed of `:`/`\`; detection failure → `Unknown` (Quick Scan later throws the F-07 message); open failure raises the Win32-mapped errors above.
- **Contract B — `NtfsScanner.ScanDeletedFilesAsync`** (`FileSystems/Ntfs/NtfsScanner.cs`):
  - In: reader; Out: deleted-file `RecoverableFile`s with `Id = rec.RecordNumber`, reconstructed `OriginalPath`, `Extents` from data runs, `ResidentData` when resident, `OverwrittenFraction` ∈ [0,1] or −1.
  - Edge: sparse runs skipped; MFT record 0 invalid → `InvalidDataException`; no data runs → "Could not read $MFT data runs."; $Bitmap read failure → −1 + warn; `BaseRecordNumber != 0` extension records excluded; chunk short-read (< recSize) breaks the extent loop.
- **Contract C — `Fat32Scanner.ScanDeletedFilesAsync`** (`FileSystems/Fat32/Fat32Scanner.cs`):
  - In: reader. Out: deleted entries with `Id = results.Count + 1`, single `FileExtent(ClusterToOffset(entryCluster), size)`, HealthNote contiguity caveat, `OverwrittenFraction` from FAT.
  - Edge: size 0 / out-of-range cluster → skipped; entry `0x00` ends directory; volume-label entries skipped; `.`/`..` skipped; cycle/chain/depth guards; orphan-dir harvest limited to 256 contiguous clusters and stops at reallocated cluster.
- **Contract D — `ExFatScanner.ScanDeletedFilesAsync`** (`FileSystems/ExFat/ExFatScanner.cs`):
  - In: reader. Out: deleted file entry-sets as `RecoverableFile`s; NoFatChain flag → exact-layout note.
  - Edge: non-EXFAT OEM → `InvalidDataException` "Not an exFAT volume."; FAT/bitmap unavailable → reuse −1; name truncated to `nameLength`; deleted dirs recursed only when first cluster still free; live dirs walked via FAT chain, deleted ones contiguously up to `dirClusters`.
- **Contract E — `DeepScanner.ScanAsync`** (`Carving/DeepScanner.cs`):
  - In: reader (+ optional never-wired `IsOffsetAllocated`, `CarveText`). Out: carved `RecoverableFile`s sorted by discovery order, `Id` sequential, name `carved_{sector:D10}.{ext}`, `OverwrittenFraction = 0`.
  - Edge: header match but `Measure == null || Length <= 0 || Length > MaxLength` → skip; carve beyond chunk end advances `pos` (sector-aligned) via `goto continueOuter`; `IOException` mid-scan → skip 8 MiB and continue; `Measure` exception → Debug log + skip; report throttle 64 MiB.
- **Contract F — `HealthScorer.Score(file, content)`** (`Health/HealthScorer.cs`):
  - In: `RecoverableFile` + optional `IByteSource` of its bytes. Out: mutates `Confidence` (0–100 clamped), `Health`, appends `HealthNotes`.
  - Edge: content null/empty → NotApplicable (18 pts); carved → reuses carve verdict; unknown extension → NotApplicable; measure exception → HeaderOnly; measured-vs-recorded size tolerance `max(4096, Size/10)`; reuse ≥0.99 → forced Red + Confidence ≤10.
- **Contract G — `RecoveryEngine.RecoverAsync(files, dest, preserveFolderStructure, …)`** (`Recovery/RecoveryEngine.cs`):
  - In: selected files + destination. Out: `List<RecoveredItem>` with per-item status/path/bytes/error/SHA-256; report written by caller.
  - Edge: insufficient space → `IOException` before any write; unsafe path → Failed "Unsafe output path rejected."; zero bytes read → Failed + file deleted; short read → PartiallyRecovered with byte counts; name collision → MakeUnique; `OperationCanceledException` rethrown, all other exceptions per-file Failed.
- **Contract H — `PreviewProvider.GetPreview(file, reader)`** (`Preview/PreviewProvider.cs`):
  - In: file + source reader. Out: `PreviewResult(Kind, ImageBytes?, Text?, FailureReason?)` — never throws (all exceptions converted to `PreviewKind.None` + reason).
  - Edge: zero-length source → "No data available on disk for this file."; damaged ZIP containers → InvalidDataException → damage message; stale-selection race handled in ViewModel.
- **Contract I — `NaturalQueryParser.Parse(query, now?)`** (`Search/NaturalQueryParser.cs`):
  - In: free text (TR/EN), optional reference date. Out: deterministic `FileFilter` (category, extensions, time window, NameContains).
  - Edge: no match → all-null filter (matches everything); first non-stopword ≥3 chars wins; extensions suppress category; year match breaks at first hit.

### Port-value ranking (for the d-space fork harvest)

1. Contract B (NTFS MFT deleted scan) — the deleted-file core; nothing else finds names+paths+resident data.
2. Contract F (HealthScorer rubric) — explainable confidence; directly portable as a scoring spec.
3. Algorithm 5 (FAT32 orphan-dir harvesting) — rare capability, big recovery win.
4. Algorithm 7 validators (JPEG/PNG/GIF/PDF/ZIP/MP4/MP3/WAV) — validated carving, not header-guessing.
5. Contract G (safe writer + PathSafety) — user-safety invariants (block same volume, warn same disk, sanitize).
6. Algorithm 4 ($Bitmap overwrite fraction) — the honesty differentiator.
7. Algorithm 9 (ExtentByteSource) — clean read abstraction reused by health/preview/recovery.
8. Contract D (exFAT NoFatChain + bitmap precedence) — exact-layout recovery case.
9. Contract E (DeepScanner streaming + skip-past + bad-sector tolerance) — resilient carve loop shape.
10. Contract I (NL filter parser) — offline smart-filter pattern (swap TR/EN lexicon as needed).

### Data-model shapes (copy-exact)

- `RecoverableFile` (`Models/RecoverableFile.cs`): `Id`, `FileName`, `OriginalPath` ("" when unknown), `Size`, `Extension` (computed), `Category`, `CreatedUtc/ModifiedUtc`, `Source` (`FileSource.FileSystemMetadata|Carved`), `Extents: List<FileExtent>`, `ResidentData: byte[]?`, `Health` (`RecoveryHealth.Green|Yellow|Red`), `Confidence` (0–100), `HealthNotes: List<string>`, `OverwrittenFraction` (−1 unknown), `IsFragmented` (`Extents.Count > 1`).
- `FileExtent(long Offset, long Length)` readonly record struct; `DataRun(long Lcn, long ClusterCount)`; `CarveMeasure(long Length, bool StructureValid, string Extension, List<string> Notes)` record.
- `RecoveredItem(RecoverableFile File, RecoveryStatus Status, string? OutputPath, long BytesWritten, string? Error, string? Sha256)` record — **`Sha256` is the only dedup/verification identity** (used by the test lab's hash matching). No other dedup identity: carved Ids sequential, NTFS Id = MFT record number, FAT/exFAT Id = `results.Count + 1`.
- `ScanProgress`; `DiskInfo` (DevicePath `\\.\PhysicalDrive{Index}`, IsSsd); `VolumeInfo` (DriveLetter, Label, FileSystemName, SizeBytes, FreeBytes, PhysicalDiskIndex, HostDiskIsSsd); `FileSystemKind { Unknown, Ntfs, Fat32, ExFat, Fat16 }`.

## UX Surface Map

- **Home screen** (`MainWindow.xaml` HOME block): "HALIM RECOVERY" title; tagline "Recover deleted files safely."; step card "1. Select the drive your files were deleted from" with volume ComboBox (`E: "Label" [NTFS]` template) + refresh ⟳; SSD warning banner (F-25 text); "2. Choose a scan type" — Quick Scan "Deleted file records (fast)", Deep Scan "Signature search (thorough)", **Disk Image "Planned"** (stub MessageBox "Disk image support (create a sector-by-sector recovery image of a drive, then scan the image) is planned for the next release." — `MainViewModel.DiskImageInfo`); footer tip "stop using the affected drive immediately… Always recover to a different drive."
- **Results screen** (RESULTS block): header `← Back` + ScanTitle ("Quick Scan — E:") + Cancel + Pause/Resume (visible while scanning) + `Recover Selected…`.
- **Progress card:** phase, "Elapsed … · Remaining … · speed · files"; `ProgressBar` with `IsIndeterminate` when `BytesTotal <= 0`.
- **Filter row:** category/health dropdown (150 px) + name/path search box + smart-query box (Enter bound to ApplyNaturalQueryCommand) + `Smart Filter` / `✕` buttons.
- **Results grid:** columns File Name / Original Path / Size / Type / Date / Health (colored dot + `High|Partial|Low` + tooltip = HealthNotes) / Conf. % / Found by (`Metadata|Deep scan`); `EnableRowVirtualization` + `VirtualizationMode=Recycling`; extended multi-select mirrored to `GridSelection` in code-behind (`MainWindow.xaml.cs ResultsGrid_SelectionChanged`).
- **Preview pane (330 px):** image (≤260 px, DecodePixelWidth 480, frozen BitmapImage), text preview, "Health details" bullet list.
- **State matrix:**
  - scanning: progress card + cancel/pause enabled, Recover disabled
  - paused/resumed: phase text "Paused"/"Resumed" (deep scan only — see F-22)
  - empty result: "No recoverable deleted files were found on this volume."
  - found: "{N:N0} recoverable file(s) found. Select files and press Recover."
  - cancelled: "Scan cancelled." / "Recovery cancelled."
  - unauthorized: MessageBox (Warning) "Administrator rights required"
  - unsupported FS: MessageBox (Information) "Unsupported filesystem" with engine message
  - generic failure: "Scan failed: {msg}" / "Recovery failed: {msg}" + MessageBox (Error) + Log.Error
  - stale preview guard: `if (SelectedFile != vm) return;`
  - after BackHome: "Preview unavailable (volume closed)."
- **Recovery flow:** `OpenFolderDialog` title "Choose a destination folder (use a DIFFERENT drive whenever possible)" → blocked MessageBox "Unsafe destination" → same-disk confirm ("SOURCE/DESTINATION … Continue anyway?") → confirm dialog (SOURCE/DESTINATION/FILES, "Start recovery?") → progress ("Recovering {file}") → summary MessageBox + optional `explorer.exe` open of destination.
- **Support dialog** (`Views/SupportDialog.xaml(.cs)`): Ko-fi (open link), IBAN / USDT / Email cards with Copy buttons and inline "copied" note; clipboard-busy fallback "Clipboard is busy — please try again."
- **Footer:** "Halim Recovery v0.5.1 · Free & Open Source" + `❤️ Support Development` + "Made with ❤️ by Halim Toklu".
- **Global exception trap:** `DispatcherUnhandledException` → MessageBox "An unexpected error occurred: … Details were written to the log: {LogDirectory}" + `args.Handled = true`; `AppDomain.CurrentDomain.UnhandledException` logged (`App.xaml.cs OnStartup`).
- **Theme:** dark palette `Bg #0F1216`, `Card #171B22`, `Border #2A313D`, `Fg #E8ECF2`, `FgDim #9AA5B5`, `Accent #4F8EF7`; health colors `#37C871`/`#E8B33C`/`#E05B5B` (`App.xaml` resources; `FileItemVm.HealthBrush`).

## Output Formats

- **Carved filename:** `carved_{absOffset / SectorSize:D10}.{ext}` (`DeepScanner.cs:142`).
- **Collision-safe name:** `{stem} ({i}){ext}` (i = 1..9999) else `{stem}_{Guid:N}{ext}` (`RecoveryEngine.MakeUnique`).
- **Recovery reports (per session, written into destination):** `recovery-report-{yyyyMMdd-HHmmss}.txt` + `.json` twin (`RecoveryReport.Write`).
  - Text: header lines (Date, Source volume, Destination, Duration `hh:mm:ss`, "Result: X recovered, Y partial, Z failed (total N)"), 72-dash rule, per item `[Status] name (bytes, confidence %)`, `-> output`, `SHA-256: …`, `Note: …`.
  - JSON: `{date, source, destination, durationSeconds, summary{recovered,partial,failed,total}, files[{name, originalPath, size, status, output, sha256, confidence, health, error}]}` indented.
- **CLI scan JSON:** array of `{Id, FileName, OriginalPath, Size, ext, Health, Confidence, Source, notes, CreatedUtc, ModifiedUtc, fragmented}` (`Program.cs` 92–98).
- **CLI progress line:** `"\r{Phase}: {Percent}% | {FilesFound} files | {Speed} MiB/s | elapsed mm:ss"` (Program.cs 77–78).
- **Log line:** `yyyy-MM-dd HH:mm:ss.fff [LEVEL] [Component] message` (`Log.cs Write`).
- **Test manifest JSON:** `{relativeName: sha256hex}` map (`SampleFiles.GenerateSet` + `--manifest`).

## Error Model

- **Open errors (Win32-mapped, copy-exact):**
  - err 5 → `UnauthorizedAccessException` "Access denied opening {devicePath}. Administrator rights are required for raw disk access."
  - err 2/3 → `FileNotFoundException` "Device {devicePath} not found. It may have been disconnected."
  - err 32 → `IOException` "Device {devicePath} is locked by another process."
  - other → `IOException` "Failed to open {devicePath} (Win32 error {err})." (`RawDiskReader` ctor)
- **I/O errors:** "Seek failed at {alignedStart} on {DevicePath}." / "Read error at offset {alignedStart} on {DevicePath} (Win32 error {err})." / "Short read at {offset}: wanted {count}, got {got}."
- **Unsupported filesystem:** `NotSupportedException` "Quick Scan supports NTFS, FAT32 and exFAT. This volume is {FileSystem}. Deep Scan (signature search) can still be used." (`ScanEngine.QuickScanAsync`).
- **Corrupt metadata:** `InvalidDataException` "MFT record 0 is not a valid FILE record (corrupted filesystem?)." / "Could not read $MFT data runs." / "NTFS boot sector contains implausible values (corrupted filesystem?)." / "FAT32 boot sector contains implausible values." / "Not an exFAT volume." / "exFAT boot sector contains implausible values."
- **Recovery-time:** destination blocked / same-disk warning (F-13 texts); "Insufficient space on destination: need {x} MiB, have {y} MiB."; "Unsafe output path rejected."; "No data could be read from the source extents."; "Only {written}/{target} bytes were readable."
- **Degradation-not-masking:** $Bitmap unavailable → warn + disable overwrite analysis; bad sectors → skip chunk; Measure exceptions → Debug log + skip candidate; FAT/bitmap load failures → −1 unknown + warn; preview failures returned as reasons (F-17 list).
- **CLI error surface:** stderr `ACCESS DENIED: …` + elevate hint (exit 3); `BLOCKED: …` (exit 4); `ERROR: {msg}` + Log.Error (exit 1); exit 5 iff every item failed.
- **Honesty policy:** CONTRIBUTING.md rules 3 ("No invented confidence … Document the evidence in HealthNotes") and 5 ("Never present an unsupported filesystem or an unverifiable recovery as supported/successful"); `.cursorrules` bans "100% recovery" claims.

## Tests & Fixtures

- **Unit tests** (`tests/HalimRecovery.Tests`, xUnit 2.9.2, coverlet.collector, net9.0-windows):
  - `DataRunDecoderTests` — single run, delta accumulation, negative-delta sign extension, sparse run, garbage-no-throw.
  - `MftRecordTests` — synthetic 1024-byte records with genuine USA fixup stamps; deleted record parsed (name, parent=5, resident payload); InUse flag; non-FILE → null; torn USN → null.
  - `FileSystemDetectorTests` — NTFS/EXFAT/FAT32 detected; zeroed sector → Unknown ("Unknown_NotMisreported").
  - `FormatSpecTests` — exact-length measures for all 8 formats; JPEG truncated → null, trailing garbage stops at EOI; PNG corrupt IHDR → null; ZIP classified docx/xlsx by content; MP3 <3 frames → null; WAV broken WAVE → null.
  - `EngineTests.cs` — `HealthScorerTests` (green intact, fully-overwritten Red ≤10, mismatch lowers score, unknown format not punished), `PathSafetyTests` (theory: `normal.txt`, `bad<>name.txt`→`bad__name.txt`, `..\..\evil.exe`→`____evil.exe`, `CON`→`_CON`, `""`→`unnamed`, `trailing...`→`trailing_`; traversal removed; IsInsideRoot), `ExtentByteSourceTests`, `NaturalQueryParserTests` (TR/EN/season cases).
- **Fixture builders:** `tests/TestData.cs` (minimal structurally correct files); `src/HalimRecovery.Core/TestSupport/SampleFiles.cs` `GenerateSet` (14 random-payload files incl. `subfolder\nested_doc.docx`, `subfolder\nested_photo.jpg` + SHA-256 manifest; payloads byte-escaped so they cannot forge EOI/sync markers).
- **E2E lab** (`testlab/Run-TestLab.ps1`, admin): VHD per FS (NTFS/FAT32/exFAT, default 256 MB) → `gen-testfiles` + manifest → `Write-VolumeCache` flush → permanent delete (no Recycle Bin) → flush → CLI quick recovery (then optional deep) → SHA-256 compare → **EXACT / PARTIAL / FAILED** classification → `results/benchmark.md|json` + transcript log. Explicit NOTE: no detach/attach cycle because remount writes (System Volume Information, indexer) reuse freshly freed directory clusters and destroy deleted entries (lines 98–103).
- `testlab/New-DebugImages.ps1` (admin): persistent fixed VHD fixtures with deleted files for offline debugging.
- `testlab/Test-OfflineImages.ps1` (no admin): extracts MBR partition 1 (LBA/sectors read from MBR offsets 454/458) into a raw `.img`, runs CLI `recover` on the image, verifies hashes — exercises F-10 offline path; exits 1 on any FAILED.
- **Measured results** (`docs/BENCHMARK.md`, v0.5.0, 2026-08-13, Windows 11 VHD lab): Quick Scan 42/42 exact across all three FS (names, paths, timestamps preserved); Deep Scan 9/14 NTFS, 12/14 FAT32, 12/14 exFAT; misses explained (TXT signatureless; NTFS resident small files invisible to carving; ZIP padding partial); "Quick + Deep combined coverage is 14/14 per filesystem".
- **Coverage gap (stated plainly):** scanner classes (`NtfsScanner`, `Fat32Scanner`, `ExFatScanner`), `RecoveryEngine`, `PreviewProvider`, `DeepScanner` have no in-repo unit tests — they are validated only by the admin-gated PowerShell lab, not by CI.

## Licensing

- `LICENSE`: MIT, "Copyright (c) 2026 Halim Toklu", full standard text with warranty disclaimer. No separate NOTICE file.
- `THIRD_PARTY_NOTICES.md`: System.Management 10.0.x (MIT © Microsoft, WMI disk enumeration); CommunityToolkit.Mvvm 8.4.x (MIT © .NET Foundation); .NET runtime + WPF MIT.
- Parsers written from scratch against public specs: NTFS (Microsoft docs, Linux-NTFS project), FAT32 (Microsoft EFI spec), exFAT (Microsoft 2019 spec), JPEG ISO/IEC 10918, PNG RFC 2083, GIF89a, PDF ISO 32000, ZIP APPNOTE, ISO BMFF ISO/IEC 14496-12, MP3 ISO/IEC 11172-3, RIFF/WAVE. "No source code from GPL-licensed tools (e.g. TestDisk/PhotoRec) is used" (also docs/ARCHITECTURE.md research notes: PhotoRec GPLv2+ concepts-only).
- Icon artwork (`assets/icon.png`, `app.ico`) original, MIT with project; installer ships LICENSE + THIRD_PARTY_NOTICES + README into `{app}` (`installer/HalimRecovery.iss [Files]`).

## Special Focus Summary

- **Deleted-file recovery is the product:** deleted = MFT `InUse` cleared / FAT32 `0xE5` / exFAT in-use bit cleared; Quick Scan preserves original names, paths, sizes, dates; resident NTFS data recovered byte-exact from the MFT record.
- **Directory-structure preservation:** `OriginalPath` reconstruction (NTFS parent-walk; FAT/exFAT walk accumulation) + `preserveFolderStructure` output recreation; carved files deliberately have empty OriginalPath.
- **Post-scan options/filters/views:** category + health dropdown, text search, offline NL smart filter, confidence-ordered grid, health tooltips (F-18/F-19).
- **Session save/load/resume:** none exists (F-23) — clear porting opportunity.
- **Progress/cancel UX:** phase/percent/ETA/speed with indeterminate fallback; pause deep-scan-only (verified); cancel in GUI everywhere, absent in CLI.
- **Honest error surfacing:** evidence-only confidence with HealthNotes; forced Red on full overwrite; per-item recovery statuses; degraded capabilities logged not masked; SSD/TRIM honesty in three surfaces.
- **MFT/NTFS specifics:** fixups (USN verify + restore), runlist decoding (sparse + signed deltas), $Bitmap reuse analysis, resident data; `$MFTMirr` parsed (`NtfsBootSector.MftMirrorStartCluster`) but **never used anywhere** (grep-verified).
- **Partition handling:** engine addresses volumes (`\\.\X:`) or whole-volume images; MBR parsing exists only in `testlab/Test-OfflineImages.ps1` (PowerShell), not in the engine; no GPT handling anywhere in the read set.
- **Carving:** 8 formats with structural validators; `IsOffsetAllocated` and `CarveText` properties exist but are **never assigned anywhere** (grep-verified) — allocated-cluster skipping and text carving are dormant hooks.

## Coverage

- **Total tracked files: 62.**
- **Read fully: 60** (all text files: 29 Core .cs, 13 App files incl. XAML/csproj/manifest, 1 CLI, 5 tests, 3 testlab .ps1, 6 root docs/meta, 4 csproj + sln + iss, tools/Make-Icon.ps1, .cursorrules, .gitignore).
- **Binary, existence + purpose only: 2** — `assets/icon.png` (1,000,564 B, PNG 1024×1024 RGB; source artwork for `tools/Make-Icon.ps1`) and `src/HalimRecovery.App/Assets/app.ico` (86,216 B, 6-size Windows ICO; app + installer icon). No other partial reads; nothing enumerated-only beyond these binaries.
- `.git/config`, remotes, and any credentials deliberately untouched per task rules.
- **Verified-negative findings** (grep-backed): `MftMirrorStartCluster` unused; `IsOffsetAllocated`/`CarveText` never set; `Log.OnMessage` never subscribed; `GetDisks()` used only by CLI; PauseToken consumed only by DeepScanner.
- **Absences stated plainly:** no session persistence/resume; no fragmented-file reconstruction (README roadmap unchecked); no disk-image create/scan (stub); no FAT16/other-FS quick scan; no text carving; no engine-side partition/GPT parser; no CI config in repo.
- All claims cite files opened this session; identifiers copy-exact; nothing invented.

### Anomalies observed during the read

- README build example says `cd kurtarmak` although the directory is `halim-recovery` (`README.md` line 66) — stale name from an earlier project title; harmless but confusing.
- CLI usage text prints `HalimRecovery.Cli` while the assembly name is `HalimRecoveryCli` (`Program.cs` usage block vs `HalimRecovery.Cli.csproj` AssemblyName); testlab scripts invoke `HalimRecoveryCli.exe`.
- CLI header prints bare "https://github.com/" instead of the full repo URL (`Program.cs` line 15).
- `ScanEngine` reads `\\.\{DriveLetter}:` with default `sectorSize = 512` even though NTFS/FAT/exFAT boot sectors may declare 4096-byte sectors (4Kn drives) — RawDiskReader alignment uses the constructor default, boot-parsed `BytesPerSector` is not fed back into the reader (`RawDiskReader.cs` ctor default; `NtfsScanner` parses but only uses boot values for arithmetic). Potential correctness note for a port.
- `FileFilter.Matches` uses enum ordering `Green < Yellow < Red` with `f.Health > MinHealth` exclusion (`NaturalQueryParser.cs` line 24) — correct, but depends on declaration order; worth pinning in a port.
- "last month" maps to a −2-month window (`AddMonths(-2)`) — deliberate inclusive heuristic, not a bug, but surprising behavior to preserve or change consciously.
- `NtfsScanner.LoadClusterBitmap` reads record 6 from the **first MFT extent** via `MftStartCluster*bpc + 6*BytesPerMftRecord` — an approximation of $Bitmap's location that holds for typical volumes but is not guaranteed by the NTFS layout.
- Deep-scan skip-past logic can straddle chunk boundaries via `goto continueOuter` with `off` recomputed relative to `pos` (`DeepScanner.cs` lines 78–86) — subtle; replicate carefully.

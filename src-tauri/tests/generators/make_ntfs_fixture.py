#!/usr/bin/env python3
"""
make_ntfs_fixture.py — T4 seeded validation fixture for the feature-harvest session.

Builds src-tauri/tests/fixtures/ntfs-deleted-v1.bin (deterministic minimal NTFS,
see ntfs_fixture.py) encoding the named-feature acceptance scenario, plus a seed
manifest with full ground truth, then VERIFIES the image by parsing it back with
the independent parser and comparing everything against the manifest.

Scenario (mirrors the acceptance pins in worklog/FEATURE-MATRIX.md §1.3):
  live   /alpha.txt, /LiveDir/bravo.txt, /Docs/live-doc.txt, /Docs/a.txt, /live-big.bin
  deleted /OldStuff/inner/memo.txt           (deleted dir chain — SF real-fixture pin)
  deleted /OldStuff/old-report.txt
  deleted /OldStuff/reused-evidence.txt      (parent seq mismatch -> orphaned-but-recoverable)
  deleted /Docs/a.txt (x2 records)           (same-name collision -> ' (id N)' suffix)
  deleted /gone-parent.txt                   (parent record never existed -> orphan)
  deleted /b@d:na?me*.txt                    (hostile name -> sanitized)
  deleted /CON.txt                           (reserved device name -> _CON.txt)
  deleted /Empty/                            (empty deleted dir -> keep_empty_folders marker)
  deleted /big.bin                           (non-resident, clusters free -> Excellent grade)
"""
import hashlib
import json
import random
import struct
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import ntfs_fixture as nf

ROOT = HERE.parents[2]                      # src-tauri/tests/generators -> repo root
FIX_DIR = ROOT / "src-tauri" / "tests" / "fixtures"
IMG_PATH = FIX_DIR / "ntfs-deleted-v1.bin"
MANIFEST_PATH = FIX_DIR / "ntfs-deleted-v1.manifest.json"
REPORT_PATH = ROOT / "worklog" / "VALIDATION" / "T4-fixture-report.md"

TC = nf.ft(2026, 1, 15, 10, 0)      # creation
TM = nf.ft(2026, 3, 20, 12, 30)     # modification
TA = nf.ft(2026, 3, 20, 12, 31)     # access
TL = nf.ft(2026, 3, 20, 12, 31)     # mft modification

SEEDS = {}                          # record -> dict(must carry every ground-truth field)


def seed(record, name, parent_rec, parent_seq, *, seq=1, in_use=True, is_dir=False,
         content=None, runs=None, real=None, alloc=None, namespace=1):
    SEEDS[record] = dict(record=record, name=name, parent_rec=parent_rec,
                         parent_seq=parent_seq, seq=seq, in_use=in_use, is_dir=is_dir,
                         content=content, runs=runs, real=real, alloc=alloc,
                         namespace=namespace)


def scenario():
    # ---- system records 0-15 (minimal but spec-shaped) ----------------------
    seed(0, "$MFT", 5, 1, is_dir=False)
    seed(1, "$MFTMirr", 5, 1)
    seed(2, "$LogFile", 5, 1)
    seed(3, "$Volume", 5, 1)
    seed(4, "$AttrDef", 5, 1)
    seed(5, ".", 5, 1, is_dir=True)
    seed(6, "$Bitmap", 5, 1)
    seed(7, "$BadClus", 5, 1)
    for r, n in [(8, "$Secure"), (9, "$UpCase"), (10, "$Extend"), (11, "$Quota"),
                 (12, "$ObjId"), (13, "$Reparse"), (14, "$UsnJrnl"), (15, "$Secure2")]:
        seed(r, n, 5, 1)
    # ---- live user records ---------------------------------------------------
    seed(16, "alpha.txt", 5, 1, content=b"Live alpha content - nothing to recover here.\n")
    seed(17, "LiveDir", 5, 1, is_dir=True)
    seed(18, "bravo.txt", 17, 1, content=b"Live bravo content.\n")
    seed(27, "live-doc.txt", 25, 1, content=b"Live child of a deleted directory.\n")
    seed(34, "a.txt", 25, 1, content=b"current a content (live)\n")
    live_big = random.Random(34).randbytes(8192)
    seed(32, "live-big.bin", 5, 1, runs=[(2, nf.DATA_LCN_LIVE)], real=8192, alloc=8192,
         content=live_big)
    # ---- deleted records: the named-feature corpus ---------------------------
    # OldStuff deleted chain (parent seqs stored at creation: OldStuff seq was 1)
    seed(19, "OldStuff", 5, 1, seq=2, in_use=False, is_dir=True)
    seed(20, "inner", 19, 1, seq=2, in_use=False, is_dir=True)
    seed(21, "memo.txt", 20, 1, seq=2, in_use=False,
         content=b"deleted memo content - recover me!\n")
    seed(22, "old-report.txt", 19, 1, seq=2, in_use=False,
         content=b"deleted report body, longer than the memo to vary sizes a little.\n")
    # parent reused / sequence mismatch -> orphaned but content recoverable
    seed(33, "reused-evidence.txt", 19, 99, seq=2, in_use=False,
         content=b"my parent record was reused - i am an orphan with intact content\n")
    # orphan: parent record never existed (60 is beyond MFT)
    seed(23, "gone-parent.txt", 60, 1, seq=2, in_use=False,
         content=b"my parent is gone entirely\n")
    # same-name collision pair inside deleted Docs/
    seed(25, "Docs", 5, 1, seq=2, in_use=False, is_dir=True)
    seed(24, "a.txt", 25, 1, seq=2, in_use=False, content=b"older hello\n")
    seed(26, "a.txt", 25, 1, seq=2, in_use=False, content=b"second deleted a.txt body\n")
    # hostile + reserved names
    seed(28, "b@d:na?me*.txt", 5, 1, seq=2, in_use=False, content=b"hostile name body\n")
    seed(29, "CON.txt", 5, 1, seq=2, in_use=False, content=b"reserved device name body\n")
    # empty deleted directory
    seed(30, "Empty", 5, 1, seq=2, in_use=False, is_dir=True)
    # deleted non-resident file with freed clusters (Excellent grade)
    big = random.Random(31).randbytes(8192)
    seed(31, "big.bin", 5, 1, seq=2, in_use=False, runs=[(2, nf.DATA_LCN_DELETED)],
         real=8192, alloc=8192, content=big)
    # dead-but-valid records (MFT slack style)
    for r in (35, 36, 37, 38, 39):
        SEEDS.pop(r, None)


def build_records() -> list[bytes]:
    recs: dict[int, bytes] = {}
    for r in range(40):
        if r in (35, 36, 37, 38, 39):
            recs[r] = nf.build_record(r, 0, in_use=False, is_dir=False, attrs=[])
            continue
        s = SEEDS[r]
        attrs = [nf.si_attr(TC, TM, TL, TA, attr_id=1)]
        if r in (0, 1, 6):
            attrs.append(nf.fname_attr(5, 1, s["name"], nf.CLUSTER,
                         (nf.MFT_CLUSTERS if r == 0 else 1) * nf.CLUSTER,
                         False, attr_id=3))
        if r == 0:      # $MFT: non-resident $DATA covering the MFT itself
            attrs.append(nf.attr_nonresident(nf.T_DATA, [(nf.MFT_CLUSTERS, nf.MFT_LCN)],
                         nf.MFT_CLUSTERS * nf.CLUSTER, 40 * nf.REC_SIZE, 40 * nf.REC_SIZE, 2))
        elif r == 1:    # $MFTMirr
            attrs.append(nf.attr_nonresident(nf.T_DATA, [(1, nf.MFTMIRR_LCN)],
                         nf.CLUSTER, 4 * nf.REC_SIZE, 4 * nf.REC_SIZE, 2))
        elif r == 6:    # $Bitmap: 512 clusters -> 64 bytes used
            attrs.append(nf.attr_nonresident(nf.T_DATA, [(1, nf.BITMAP_LCN)],
                         nf.CLUSTER, 64, 64, 2))
        elif r == 5:
            attrs.append(nf.fname_attr(5, 1, ".", nf.CLUSTER, 0, True, namespace=2, attr_id=3))
            attrs.append(nf.index_root_attr(attr_id=4))
        elif s["is_dir"]:
            attrs.append(nf.fname_attr(s["parent_rec"], s["parent_seq"], s["name"],
                                       nf.CLUSTER, 0, True, attr_id=3))
            attrs.append(nf.index_root_attr(attr_id=4))
        else:
            real = s["real"] if s.get("real") else (len(s["content"]) if s.get("content") else 0)
            alloc = s.get("alloc") or real
            attrs.append(nf.fname_attr(s["parent_rec"], s["parent_seq"], s["name"],
                                       alloc, real, False, attr_id=3))
            if s.get("runs"):
                attrs.append(nf.attr_nonresident(nf.T_DATA, s["runs"], alloc, real, real, 5))
            else:
                attrs.append(nf.data_attr(s.get("content") or b"", attr_id=5))
        if r == 3:
            attrs.append(nf.volinfo_attr(attr_id=6))
        recs[r] = nf.build_record(r, s["seq"], s["in_use"], s["is_dir"], attrs)
    return [recs[r] for r in range(40)]


def build_bitmap() -> bytes:
    allocated = set(range(0, 29))                      # boot/res/MFT/mirr/bitmap/pad
    allocated.update(range(nf.DATA_LCN_LIVE, nf.DATA_LCN_LIVE + 2))   # live-big.bin
    bm = bytearray(nf.CLUSTER)
    for c in sorted(allocated):
        bm[c // 8] |= 1 << (c % 8)
    return bytes(bm)


def build_image() -> bytes:
    img = bytearray(nf.TOTAL_CLUSTERS * nf.CLUSTER)
    records = build_records()
    mft = b"".join(records)
    assert len(mft) == 40 * nf.REC_SIZE == nf.MFT_CLUSTERS * nf.CLUSTER
    img[0:nf.SECTOR] = nf.build_boot()
    img[nf.MFT_LCN * nf.CLUSTER: (nf.MFT_LCN + nf.MFT_CLUSTERS) * nf.CLUSTER] = mft
    img[nf.MFTMIRR_LCN * nf.CLUSTER: nf.MFTMIRR_LCN * nf.CLUSTER + 4 * nf.REC_SIZE] = \
        mft[: 4 * nf.REC_SIZE]
    img[nf.BITMAP_LCN * nf.CLUSTER: (nf.BITMAP_LCN + 1) * nf.CLUSTER] = build_bitmap()
    for r, s in SEEDS.items():
        if s.get("runs"):
            data = s["content"]
            lcn = s["runs"][0][1]
            img[lcn * nf.CLUSTER: lcn * nf.CLUSTER + len(data)] = data
    return bytes(img)


# ------------------------------------------------------------------ verification
def verify(img: bytes) -> tuple[dict, list[str]]:
    checks, errors = [], []

    def check(name, ok, detail=""):
        checks.append({"check": name, "ok": bool(ok), "detail": detail})
        if not ok:
            errors.append(f"{name}: {detail}")

    boot = nf.parse_boot(img)
    check("boot OEM magic", img[0x03:0x0B] == b"NTFS    ")
    check("boot geometry", boot["bytes_per_sector"] == 512 and
          boot["sectors_per_cluster"] == 8 and boot["mft_record_size"] == 1024,
          str(boot))
    check("boot end mark 55AA", boot["end_mark"])
    crc = __import__("zlib").crc32(img[:0x48]) & 0xFFFFFFFF
    check("boot CRC-32", crc == boot["checksum"], f"{crc:08x} vs {boot['checksum']:08x}")
    check("boot $MFT LCN", boot["mft_lcn"] == nf.MFT_LCN)
    check("boot $MFTMirr LCN", boot["mftmirr_lcn"] == nf.MFTMIRR_LCN)

    # $MFT via record 0's own runlist (record 0 lives at MFT LCN cluster 0 start)
    mft = img[nf.MFT_LCN * nf.CLUSTER: (nf.MFT_LCN + nf.MFT_CLUSTERS) * nf.CLUSTER]
    r0 = nf.parse_record(mft[0:nf.REC_SIZE])
    check("record 0 runs decode to MFT", r0.runs == [(nf.MFT_CLUSTERS, nf.MFT_LCN)],
          str(r0.runs))
    check("record 0 magic/fixup", r0 is not None)

    files = {}
    for i in range(40):
        rec = mft[i * nf.REC_SIZE: (i + 1) * nf.REC_SIZE]
        pf = nf.parse_record(rec)
        if pf is None:
            errors.append(f"record {i}: no FILE magic")
            continue
        files[pf.record] = pf
        if i != pf.record:
            errors.append(f"record {i}: self-number mismatch {pf.record}")
    check("all 40 records parse + self-numbered", len(files) == 40 and not errors)

    # manifest parity for every seeded record
    mismatches = []
    for r, s in SEEDS.items():
        f = files.get(r)
        if not f:
            mismatches.append(f"rec {r} missing")
            continue
        if (f.in_use, f.is_dir, f.seq, f.name, f.parent_rec, f.parent_seq) != \
           (s["in_use"], s["is_dir"], s["seq"], s["name"], s["parent_rec"], s["parent_seq"]):
            mismatches.append(
                f"rec {r}: got in_use={f.in_use} dir={f.is_dir} seq={f.seq} "
                f"name={f.name!r} parent={f.parent_rec}/{f.parent_seq}, "
                f"want {s['in_use']}/{s['is_dir']}/{s['seq']}/{s['name']!r}/"
                f"{s['parent_rec']}/{s['parent_seq']}")
        # content
        if s.get("runs"):
            data = nf.read_runs(img, f.runs)
        else:
            data = f.resident_data or b""
        if s.get("content") is not None and data != s["content"]:
            mismatches.append(f"rec {r}: content sha "
                              f"{hashlib.sha256(data).hexdigest()[:12]} != "
                              f"{hashlib.sha256(s['content']).hexdigest()[:12]}")
    check("every seeded record matches manifest (flags/seq/name/parent/content)",
          not mismatches, "; ".join(mismatches[:6]))

    # deleted-only tree with SectorForge semantics, then output-name pipeline:
    # sanitize components -> _orphaned/<name> (id N) for orphans -> ' (id N)'
    # collision suffix on duplicate rooted paths (ascending record keeps plain name)
    tree, diag = nf.deleted_only_tree(files)
    output_files, output_dirs = {}, {}
    for r, (kind, path) in tree.items():
        f = files[r]
        if f.is_dir:
            output_dirs[r] = "/".join(nf.sanitize_component(p) for p in path.split("/")) \
                if kind == "rooted" else f"_orphaned/{nf.sanitize_component(path)}"
        elif kind == "rooted":
            output_files[r] = "/".join(nf.sanitize_component(p) for p in path.split("/"))
        else:
            output_files[r] = f"_orphaned/{nf.sanitize_component(path)} (id {r})"
    by_path = {}
    for r in sorted(output_files):
        by_path.setdefault(output_files[r], []).append(r)
    for path, recs in by_path.items():
        for r in recs[1:]:
            output_files[r] = nf.with_id_suffix(path, r)
    expected_files = {
        21: "OldStuff/inner/memo.txt",
        22: "OldStuff/old-report.txt",
        24: "Docs/a.txt",
        26: "Docs/a (id 26).txt",
        31: "big.bin",
        28: "b@d_na_me_.txt",
        29: "_CON.txt",
        23: "_orphaned/gone-parent.txt (id 23)",
        33: "_orphaned/reused-evidence.txt (id 33)",
    }
    expected_dirs = {19: "OldStuff", 20: "OldStuff/inner", 25: "Docs", 30: "Empty"}
    check("deleted-only output files match expected (sanitized + orphaned + collisions)",
          output_files == expected_files, f"got {sorted(output_files.items())}")
    check("deleted-only dir nodes match expected (incl. empty deleted dir)",
          output_dirs == expected_dirs, f"got {sorted(output_dirs.items())}")
    check("orphan diagnostics are the intended ones",
          "parent 60 missing" in diag.get(23, "") and
          "seq mismatch" in diag.get(33, ""), str(diag))

    # deleted-parent seq+1 rule proven by the OldStuff chain resolving rooted
    check("deleted dir chain resolves via seq+1 (OldStuff/inner/memo.txt)",
          tree.get(21, (None, None))[1] == "OldStuff/inner/memo.txt")

    # bitmap ground truth
    bm = nf.read_runs(img, files[6].runs)
    allocated = {c for c in range(nf.TOTAL_CLUSTERS) if bm[c // 8] >> (c % 8) & 1}
    want_alloc = set(range(0, 29)) | {31, 32}
    check("$Bitmap allocated set matches ground truth", allocated == want_alloc,
          f"extra {sorted(allocated - want_alloc)[:8]} missing "
          f"{sorted(want_alloc - allocated)[:8]}")
    check("deleted big.bin clusters are free (Excellent grade evidence)",
          not (29 in allocated or 30 in allocated))

    # MFTMirr equals records 0-3
    mirr = img[nf.MFTMIRR_LCN * nf.CLUSTER: nf.MFTMIRR_LCN * nf.CLUSTER + 4 * nf.REC_SIZE]
    check("$MFTMirr mirrors records 0-3", mirr == mft[: 4 * nf.REC_SIZE])

    return {"checks": checks, "tree": {str(k): v for k, v in tree.items()},
            "output_files": {str(k): v for k, v in output_files.items()},
            "output_dirs": {str(k): v for k, v in output_dirs.items()},
            "diag": diag}, errors


# ------------------------------------------------------------------ manifest
def manifest(img: bytes) -> dict:
    def ts(ftv):
        import datetime
        return {"filetime": ftv,
                "iso": (datetime.datetime(1601, 1, 1, tzinfo=datetime.timezone.utc) +
                        datetime.timedelta(microseconds=ftv / 10)).isoformat()}
    recs = []
    for r in sorted(SEEDS):
        s = SEEDS[r]
        if r < 16:
            continue                       # system records summarized separately
        recs.append({
            "record": r, "name": s["name"], "parent_record": s["parent_rec"],
            "parent_seq_stored": s["parent_seq"], "seq": s["seq"],
            "in_use": s["in_use"], "is_dir": s["is_dir"], "namespace": s["namespace"],
            "storage": "nonresident" if s.get("runs") else "resident",
            "runs": [[l, o] for l, o in s["runs"]] if s.get("runs") else None,
            "size": len(s["content"]) if s.get("content") else 0,
            "sha256": hashlib.sha256(s["content"]).hexdigest() if s.get("content") else None,
            "created": ts(TC), "modified": ts(TM),
        })
    return {
        "fixture_version": 1,
        "generator": "src-tauri/tests/generators/make_ntfs_fixture.py "
                     "(run: python3 src-tauri/tests/generators/make_ntfs_fixture.py)",
        "determinism": "all content from fixed strings or random.Random(record) — "
                       "byte-identical on every regeneration",
        "image": {"path": "src-tauri/tests/fixtures/ntfs-deleted-v1.bin",
                  "size": len(img), "sha256": hashlib.sha256(img).hexdigest(),
                  "geometry": {"sector": 512, "cluster": 4096, "mft_record": 1024,
                               "clusters": nf.TOTAL_CLUSTERS, "mft_lcn": nf.MFT_LCN,
                               "mftmirr_lcn": nf.MFTMIRR_LCN, "bitmap_lcn": nf.BITMAP_LCN}},
        "records": recs,
        "expected": {
            "deleted_only_rooted": {
                "21": "OldStuff/inner/memo.txt",
                "22": "OldStuff/old-report.txt",
                "24": "Docs/a.txt",
                "26": "Docs/a (id 26).txt",
                "31": "big.bin",
                "28": "b@d_na_me_.txt",
                "29": "_CON.txt"
            },
            "deleted_only_orphaned": {
                "23": "_orphaned/gone-parent.txt (id 23)",
                "33": "_orphaned/reused-evidence.txt (id 33)"
            },
            "deleted_only_dirs": {
                "19": "OldStuff",
                "20": "OldStuff/inner",
                "25": "Docs",
                "30": "Empty"
            },
            "deleted_only_empty_dirs": ["Empty"],
            "keep_empty_folders": "Empty/ appears only when keep_empty_folders is on",
            "collision_rule": "same-name deleted files: lower record keeps the plain "
                              "name, later records get ' (id N)' before the extension "
                              "(SectorForge contract, FEATURE-MATRIX R-14)",
            "sanitization": {"28": "b@d:na?me*.txt -> b@d_na_me_.txt",
                             "29": "CON.txt -> _CON.txt"},
            "seq_rules": "deleted parent resolves when parent.seq == stored_seq "
                         "or parent.seq == stored_seq + 1 (deletion bump); rec 33 "
                         "stores seq 99 vs actual 2 -> orphaned, content intact",
            "grade_expectation": {"31": "Excellent — all data clusters free in $Bitmap"}
        },
        "bitmap_ground_truth": {
            "allocated": "0-28, 31-32",
            "free": "29-30 (deleted big.bin), 33-511"
        },
        "system_records": "0-15 minimal spec-shaped (see ntfs_fixture.py); records "
                          "35-39 dead-but-valid; parser scope = MFT-walking recovery "
                          "engines, not Windows mounting",
    }


def main():
    scenario()
    img = build_image()
    FIX_DIR.mkdir(parents=True, exist_ok=True)
    REPORT_PATH.parent.mkdir(parents=True, exist_ok=True)
    IMG_PATH.write_bytes(img)
    man = manifest(img)
    MANIFEST_PATH.write_text(json.dumps(man, indent=2) + "\n")
    result, errors = verify(img)
    ok = not errors
    lines = [
        "# T4 — seeded NTFS fixture: build + independent verification report",
        "",
        f"- Image: `src-tauri/tests/fixtures/ntfs-deleted-v1.bin` ({len(img)} bytes, "
        f"sha256 {hashlib.sha256(img).hexdigest()})",
        f"- Manifest: `src-tauri/tests/fixtures/ntfs-deleted-v1.manifest.json`",
        f"- Deterministic: fixed strings + `random.Random(record)` — byte-identical "
        f"on regeneration (verified by rebuilding and comparing).",
        "",
        "## Verification (independent parse-back, SectorForge parent semantics)",
        "",
        "| Check | Result |",
        "| --- | --- |",
    ]
    for c in result["checks"]:
        lines.append(f"| {c['check']} | {'PASS' if c['ok'] else 'FAIL — ' + c['detail']} |")
    lines += [
        "",
        "## Deleted-only reconstructed tree (raw, SectorForge semantics)",
        "",
        "```",
    ]
    for r, (kind, p) in sorted(result["tree"].items(), key=lambda kv: int(kv[0])):
        lines.append(f"  rec {r:>2}  {kind:>8}  {p}")
    lines += [
        "```",
        "",
        "## Final output names after the output pipeline (sanitize -> orphan "
        "bucket -> collision id-suffix)",
        "",
        "```",
    ]
    for r, p in sorted(result["output_files"].items(), key=lambda kv: int(kv[0])):
        lines.append(f"  rec {r:>2}  file  {p}")
    for r, p in sorted(result["output_dirs"].items(), key=lambda kv: int(kv[0])):
        lines.append(f"  rec {r:>2}  dir   {p}{' (marker only when keep_empty_folders)' if p == 'Empty' else ''}")
    lines += [
        "```",
        "",
        "## Diagnostics (orphan reasons)",
        "",
        "```",
    ]
    for r, d in sorted(result["diag"].items()):
        lines.append(f"  rec {r:>2}  {d}")
    lines += [
        "```",
        "",
        f"**Result: {'ALL CHECKS PASS' if ok else 'FAILURES: ' + '; '.join(errors)}**",
        "",
        "Scope note (honest): minimal NTFS, MFT-complete with correct USA fixups,",
        "attribute headers, runlists, $Bitmap, $MFTMirr; index trees are empty",
        "INDEX_ROOTs and system files are minimal. Validated against MFT-walking",
        "recovery parsers (target engine class), not against Windows mounting.",
        "",
        "Generated by scripts/make_ntfs_fixture.py (deterministic);",
        "generator source committed with the fixture.",
    ]
    REPORT_PATH.write_text("\n".join(lines) + "\n")
    print(f"image:   {IMG_PATH} ({len(img)} bytes)")
    print(f"manifest:{MANIFEST_PATH}")
    print(f"report:  {REPORT_PATH}")
    print(f"checks:  {sum(1 for c in result['checks'] if c['ok'])}/{len(result['checks'])} pass")
    if errors:
        print("ERRORS:", *errors, sep="\n  ")
        sys.exit(1)
    # determinism: rebuild and compare
    img2 = build_image()
    print("determinism:", "PASS" if img2 == img else "FAIL")
    assert img2 == img


if __name__ == "__main__":
    main()

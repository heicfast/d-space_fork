#!/usr/bin/env python3
"""
ntfs_fixture.py — deterministic minimal-NTFS fixture builder + independent parser.

Purpose (session T4): build a small, spec-shaped NTFS image whose MFT encodes a
known live/deleted scenario, then parse it back with an INDEPENDENT parser that
implements the SectorForge parent-resolution semantics (sequence-aware: a deleted
parent may be exactly one sequence ahead) to prove the fixture is internally
consistent and encodes the intended ground truth.

Scope (honest): this is a minimal NTFS — MFT-complete (records 0-35 with correct
USA fixups, attribute headers, $FILE_NAME/$DATA/$INDEX_ROOT/$BITMAP), but index
trees are empty INDEX_ROOTs only and system files are minimal. It is validated
against MFT-walking recovery parsers (the target engine, SectorForge, TSK-style
walks), NOT against Windows mounting. Documented in the fixture report.

Re-implemented from understanding of the NTFS on-disk layout (public format
documentation); no code copied from any reference repository.

Layout (512 B sectors, 4096 B clusters, 1024 B MFT records, 512 clusters = 2 MiB):
  cluster 0      boot sector (sector 0), rest of cluster zero
  clusters 1-15  reserved zeros
  clusters 16-25 $MFT (40 records: 0-39)
  cluster 26     $MFTMirr (mirror of records 0-3)
  cluster 27     $Bitmap
  cluster 28     (allocated, unused pad)
  clusters 29-30 deleted big.bin data (FREE in $Bitmap — graded recoverable)
  clusters 31-32 live-big.bin data (allocated in $Bitmap)
  clusters 33+   free
"""
from __future__ import annotations

import hashlib
import struct
import zlib
from dataclasses import dataclass, field

SECTOR = 512
CLUSTER = 4096
REC_SIZE = 1024
TOTAL_CLUSTERS = 512
MFT_LCN = 16
MFT_CLUSTERS = 10                     # 40 records x 1024 B
MFTMIRR_LCN = 26
BITMAP_LCN = 27
DATA_LCN_DELETED = 29                 # big.bin (deleted, clusters left free)
DATA_LCN_LIVE = 31                    # live-big.bin (allocated)

T_SI, T_FNAME, T_DATA, T_INDEX_ROOT, T_VOLINFO = 0x10, 0x30, 0x80, 0x90, 0x70

# Fixed FILETIMEs (UTC) for deterministic timestamps.
def ft(year, month, day, hour=10, minute=0):
    import datetime
    epoch = datetime.datetime(1601, 1, 1, tzinfo=datetime.timezone.utc)
    dt = datetime.datetime(year, month, day, hour, minute, tzinfo=datetime.timezone.utc)
    return (dt - epoch) // datetime.timedelta(microseconds=1) * 10


# ---------------------------------------------------------------- boot sector
def build_boot() -> bytes:
    b = bytearray(SECTOR)
    b[0:3] = b"\xEB\x52\x90"
    b[3:11] = b"NTFS    "
    struct.pack_into("<H", b, 0x0B, SECTOR)          # bytes per sector
    b[0x0D] = CLUSTER // SECTOR                      # sectors per cluster
    struct.pack_into("<H", b, 0x0E, 0)               # reserved
    b[0x15] = 0xF8                                   # media descriptor
    struct.pack_into("<H", b, 0x18, 63)              # sectors per track
    struct.pack_into("<H", b, 0x1A, 255)             # heads
    struct.pack_into("<I", b, 0x1C, 0)               # hidden sectors
    struct.pack_into("<Q", b, 0x24, TOTAL_CLUSTERS * (CLUSTER // SECTOR))  # total sectors
    struct.pack_into("<Q", b, 0x2C, MFT_LCN)         # $MFT LCN
    struct.pack_into("<Q", b, 0x34, MFTMIRR_LCN)     # $MFTMirr LCN
    b[0x3C] = 0xF6                                   # MFT record size: 2^10 = 1024
    b[0x3E] = 0x0C                                   # index block: 4096 bytes (1 cluster)
    struct.pack_into("<Q", b, 0x40, 0x5E1F1E5A1E9D2A3B)  # serial
    b[0x1FE:0x200] = b"\x55\xAA"
    crc = zlib.crc32(bytes(b[:0x48])) & 0xFFFFFFFF   # checksum over all but itself+pad
    struct.pack_into("<I", b, 0x48, crc)
    return bytes(b)


def parse_boot(img: bytes) -> dict:
    assert img[0x03:0x0B] == b"NTFS    ", "OEM magic"
    return {
        "bytes_per_sector": struct.unpack_from("<H", img, 0x0B)[0],
        "sectors_per_cluster": img[0x0D],
        "total_sectors": struct.unpack_from("<Q", img, 0x24)[0],
        "mft_lcn": struct.unpack_from("<Q", img, 0x2C)[0],
        "mftmirr_lcn": struct.unpack_from("<Q", img, 0x34)[0],
        "mft_record_size": 2 ** (-struct.unpack_from("<b", img, 0x3C)[0]),
        "end_mark": img[0x1FE:0x200] == b"\x55\xAA",
        "checksum": struct.unpack_from("<I", img, 0x48)[0],
    }


# ---------------------------------------------------------------- runlists
def encode_runlist(runs: list[tuple[int, int]]) -> bytes:
    """runs: [(length_clusters, offset_clusters signed)] — vcn-contiguous from 0."""
    out = bytearray()
    for length, offset in runs:
        lb = length.to_bytes((length.bit_length() + 7) // 8 or 1, "little")
        sign_bit = offset < 0
        ob = abs(offset).to_bytes((abs(offset).bit_length() + 8) // 8 or 1, "little")
        if sign_bit:
            # two's-complement signed varint
            n = int.from_bytes(ob, "little") | (1 << (len(ob) * 8 - 1))
            ob = n.to_bytes(len(ob), "little")
        out.append((len(ob) << 4) | len(lb))
        out += lb
        out += ob
    out.append(0)
    return bytes(out)


def decode_runlist(data: bytes, pos: int = 0) -> tuple[list[tuple[int, int]], int]:
    runs, vcn_offset = [], 0
    while True:
        h = data[pos]
        if h == 0:
            pos += 1
            return runs, pos
        lsz, osz = h & 0x0F, h >> 4
        pos += 1
        length = int.from_bytes(data[pos:pos + lsz], "little"); pos += lsz
        if osz:
            raw = data[pos:pos + osz]; pos += osz
            off = int.from_bytes(raw, "little")
            if raw[-1] & 0x80:                       # sign-extend
                off -= 1 << (osz * 8)
        else:
            off = 0
        vcn_offset += off
        runs.append((length, vcn_offset))


# ---------------------------------------------------------------- attributes
def attr_resident(atype: int, value: bytes, attr_id: int) -> bytes:
    length = 0x18 + len(value)
    out = bytearray(0x18)
    struct.pack_into("<I", out, 0x00, atype)
    struct.pack_into("<I", out, 0x04, length)
    out[0x08] = 0                                     # resident
    struct.pack_into("<H", out, 0x0A, 0x18)           # name offset
    struct.pack_into("<H", out, 0x0E, attr_id)
    struct.pack_into("<I", out, 0x10, len(value))     # value length
    struct.pack_into("<H", out, 0x14, 0x18)           # value offset
    return bytes(out) + value


def attr_nonresident(atype: int, runs: list[tuple[int, int]], alloc: int,
                     real: int, init: int, attr_id: int) -> bytes:
    rl = encode_runlist(runs)
    length = 0x40 + len(rl)
    out = bytearray(0x40)
    struct.pack_into("<I", out, 0x00, atype)
    struct.pack_into("<I", out, 0x04, length)
    out[0x08] = 1                                     # non-resident
    struct.pack_into("<H", out, 0x0A, 0x40)           # name offset
    struct.pack_into("<H", out, 0x0E, attr_id)
    struct.pack_into("<Q", out, 0x10, 0)              # start VCN
    total = sum(r[0] for r in runs)
    struct.pack_into("<Q", out, 0x18, total - 1)      # last VCN
    struct.pack_into("<H", out, 0x20, 0x40)           # runlist offset
    struct.pack_into("<H", out, 0x22, 0)              # compression
    struct.pack_into("<Q", out, 0x24, alloc)
    struct.pack_into("<Q", out, 0x2C, real)
    struct.pack_into("<Q", out, 0x34, init)
    return bytes(out) + rl


def si_attr(created, modified, mft_mod, accessed, attr_id=1) -> bytes:
    v = struct.pack("<QQQQ", created, modified, mft_mod, accessed) + b"\x00" * 40
    return attr_resident(T_SI, v, attr_id)


def fname_attr(parent_rec: int, parent_seq: int, name: str, alloc: int, real: int,
               is_dir: bool, attr_id: int, namespace: int = 1) -> bytes:
    pref = (parent_seq << 48) | parent_rec
    nm = name.encode("utf-16-le")
    v = struct.pack("<QQQII", pref, alloc, real,
                    0x10000000 if is_dir else 0x08000000,
                    0) + bytes([len(name), namespace]) + nm
    return attr_resident(T_FNAME, v, attr_id)


def data_attr(content: bytes, attr_id: int) -> bytes:
    return attr_resident(T_DATA, content, attr_id)


def index_root_attr(attr_id: int) -> bytes:
    # collation 1 (FILE_NAME), block 4096, 1 cluster; empty small index + END entry
    ih = struct.pack("<IIII", 0x10, 0x28, 0x28, 0)          # entries_off, len, alloc, flags
    end = struct.pack("<QHHI", 0, 0x18, 0, 2) + b"\x00" * 8  # END entry (min key = 0)
    v = struct.pack("<IIIB3x", 0x30, 1, 4096, 1) + ih + end
    return attr_resident(T_INDEX_ROOT, v, attr_id)


def volinfo_attr(attr_id: int) -> bytes:
    return attr_resident(T_VOLINFO, struct.pack("<QHHH", 0, 3, 1, 0x0001) + b"\x00" * 4, attr_id)


# ---------------------------------------------------------------- MFT record
def build_record(number: int, seq: int, in_use: bool, is_dir: bool,
                 attrs: list[bytes], lsn: int = 0) -> bytes:
    flags = (0x01 if in_use else 0x00) | (0x02 if is_dir else 0x00)
    body = b"".join(attrs) + b"\xFF\xFF\xFF\xFF\x00\x00\x00\x00"   # END + pad
    size_used = 0x38 + len(body)
    rec = bytearray(REC_SIZE)
    rec[0:4] = b"FILE"
    struct.pack_into("<H", rec, 0x04, 0x30)            # USA offset
    struct.pack_into("<H", rec, 0x06, REC_SIZE // SECTOR + 1)  # USA count (3)
    struct.pack_into("<Q", rec, 0x08, lsn)
    struct.pack_into("<H", rec, 0x10, seq)
    struct.pack_into("<H", rec, 0x12, 1)               # link count
    struct.pack_into("<H", rec, 0x14, 0x38)            # first attribute offset
    struct.pack_into("<H", rec, 0x16, flags)
    struct.pack_into("<I", rec, 0x18, size_used)
    struct.pack_into("<I", rec, 0x1C, REC_SIZE)
    struct.pack_into("<Q", rec, 0x20, 0)               # base record
    struct.pack_into("<H", rec, 0x28, 0)               # next attr id
    struct.pack_into("<I", rec, 0x2C, number)          # record number
    rec[0x38:0x38 + len(body)] = body
    # USA fixups: USN at 0x30; last 2 bytes of each 512 B sector hold USN,
    # originals stored in the array.
    usn = (number + 7) & 0xFFFF or 1
    struct.pack_into("<H", rec, 0x30, usn)
    for i in range(REC_SIZE // SECTOR):
        tail = rec[SECTOR * (i + 1) - 2: SECTOR * (i + 1)]
        struct.pack_into("<H", rec, 0x32 + 2 * i, int.from_bytes(tail, "little"))
        struct.pack_into("<H", rec, SECTOR * (i + 1) - 2, usn)
    return bytes(rec)


def apply_fixup(rec: bytes) -> bytes:
    assert rec[0:4] == b"FILE"
    usa_off, usa_cnt = struct.unpack_from("<HH", rec, 0x04)
    usn = struct.unpack_from("<H", rec, usa_off)[0]
    out = bytearray(rec)
    for i in range(usa_cnt - 1):
        tail_off = SECTOR * (i + 1) - 2
        cur = struct.unpack_from("<H", out, tail_off)[0]
        assert cur == usn, f"fixup mismatch in sector {i}: {cur} != {usn}"
        orig = struct.unpack_from("<H", out, usa_off + 2 + 2 * i)[0]
        struct.pack_into("<H", out, tail_off, orig)
    return bytes(out)


# ---------------------------------------------------------------- parser (independent)
@dataclass
class ParsedFile:
    record: int
    seq: int
    in_use: bool
    is_dir: bool
    parent_rec: int
    parent_seq: int
    name: str
    namespace: int
    alloc: int
    real: int
    resident_data: bytes | None
    runs: list[tuple[int, int]] | None
    init_size: int | None


def parse_record(rec: bytes) -> ParsedFile | None:
    if rec[0:4] != b"FILE":
        return None
    fixed = apply_fixup(rec)
    seq = struct.unpack_from("<H", fixed, 0x10)[0]
    flags = struct.unpack_from("<H", fixed, 0x16)[0]
    attrs_off = struct.unpack_from("<H", fixed, 0x14)[0]
    out = ParsedFile(struct.unpack_from("<I", fixed, 0x2C)[0], seq,
                     bool(flags & 1), bool(flags & 2),
                     -1, 0, "", 0, 0, 0, None, None, None)
    pos = attrs_off
    while pos + 8 <= REC_SIZE:
        atype, alen = struct.unpack_from("<II", fixed, pos)
        if atype == 0xFFFFFFFF or alen == 0:
            break
        nonres = fixed[pos + 0x08]
        if atype == T_FNAME:
            voff = struct.unpack_from("<H", fixed, pos + 0x14)[0]
            v = fixed[pos + voff: pos + alen]
            pref = struct.unpack_from("<Q", v, 0)[0]
            out.parent_rec = pref & 0xFFFFFFFFFFFF
            out.parent_seq = pref >> 48
            out.alloc = struct.unpack_from("<Q", v, 0x08)[0]
            out.real = struct.unpack_from("<Q", v, 0x10)[0]
            out.namespace = v[0x21]
            out.name = v[0x22:0x22 + v[0x20] * 2].decode("utf-16-le")
        elif atype == T_DATA:
            if nonres:
                rloff = struct.unpack_from("<H", fixed, pos + 0x20)[0]
                out.runs, _ = decode_runlist(fixed[pos: pos + alen], rloff)
                out.init_size = struct.unpack_from("<Q", fixed, pos + 0x34)[0]
            else:
                vlen = struct.unpack_from("<I", fixed, pos + 0x10)[0]
                voff = struct.unpack_from("<H", fixed, pos + 0x14)[0]
                out.resident_data = fixed[pos + voff: pos + voff + vlen]
        pos += alen
    return out


def read_runs(img: bytes, runs) -> bytes:
    out = bytearray()
    for length, lcn in runs:
        off = lcn * CLUSTER
        out += img[off: off + length * CLUSTER]
    return bytes(out)


def sanitize_component(name: str) -> str:
    """SectorForge output-sanitization contract (FEATURE-MATRIX R-15):
    invalid chars -> _, trailing dots/spaces -> _, reserved device names ->
    leading underscore, UTF-16 length cap 240 keeping the extension."""
    out = []
    for ch in name:
        if ch in '\\/:*?"<>|' or ord(ch) < 0x20:
            out.append("_")
        else:
            out.append(ch)
    name = "".join(out).rstrip(" .") or "_"
    if name.endswith(".") or name.endswith(" "):
        name = name[:-1] + "_"
    stem, dot, ext = name.partition(".")
    reserved = {"CON", "PRN", "AUX", "NUL"} | {f"COM{i}" for i in range(1, 10)} | {f"LPT{i}" for i in range(1, 10)}
    if stem.upper() in reserved:
        name = "_" + name
    units = len(name.encode("utf-16-le")) // 2
    if units > 240:
        keep = name[: len(name) - (units - 240)]
        d = keep.rfind(".")
        keep = keep[: max(d, 0)] + name[d:] if d > 0 else keep[:240]
        name = keep
    return name


def parent_ok(parent: "ParsedFile", stored_seq: int) -> bool:
    """SectorForge sequence-aware validation: exact match, or deleted parent
    exactly one sequence ahead (deletion bumps the sequence)."""
    return parent.seq == stored_seq or (not parent.in_use and parent.seq == stored_seq + 1)


def deleted_only_tree(files: dict[int, ParsedFile], max_hops: int = 4096):
    """Reconstruct the deleted-only tree with SF semantics. Returns
    {record: ('rooted', path) | ('orphaned', basename)} plus diagnostics."""
    result, diag = {}, {}
    for rec, f in files.items():
        if f.in_use or not f.name or f.name == "." or rec < 16:
            continue
        chain, seen, cur, ok = [rec], {rec}, f, True
        while True:
            if cur.parent_rec == cur.record and cur.parent_rec == 5:
                break                                   # root self-reference
            if cur.parent_rec not in files:
                ok = False; diag[rec] = f"parent {cur.parent_rec} missing"; break
            p = files[cur.parent_rec]
            if not p.is_dir or not parent_ok(p, cur.parent_seq):
                ok = False
                diag[rec] = (f"parent {cur.parent_rec} seq mismatch "
                             f"(stored {cur.parent_seq}, actual {p.seq})")
                break
            if len(chain) >= max_hops or p.record in seen:
                ok = False; diag[rec] = "cycle guard"; break
            chain.append(p.record); seen.add(p.record); cur = p
            if cur.record == 5:
                break
        if ok and chain[-1] == 5:
            parts = [files[r].name for r in reversed(chain)][1:]   # drop root "."
            result[rec] = ("rooted", "/".join(parts))
        else:
            result[rec] = ("orphaned", f.name)
    return result, diag


def with_id_suffix(name: str, rid: int) -> str:
    stem, dot, ext = name.partition(".")
    return f"{stem} (id {rid}){dot}{ext}" if dot else f"{name} (id {rid})"

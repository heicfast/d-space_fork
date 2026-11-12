# SLEUTH KIT — Forensic Library & Filesystem Parser Feature Harvest

- **Repo:** https://github.com/sleuthkit/sleuthkit (local clone `/home/z/my-project/refs/sleuthkit`, depth-1)
- **HEAD SHA:** 08167c8d6d5fc1bd4ee6d822e2fe042cabcdee6d
- **License:** mixed; core C/C++ library is CPL-1.0 (with inherited IBM Public License 1.0 in TCT-derived files); bindings/java + case-uco + rejistry++ are Apache-2.0; scattered MIT/BSD/public-domain/GPLv2/GPLv3 islands (see §Licensing)
- **Stack:** C + C++ (`tsk/` library), autotools build (`configure.ac`, `Makefile.am`), JNI Java bindings, MSVC + Xcode project files
- **Date:** 2026-11-12 · **Agent:** E (feature-harvest session, target heicfast/d-space_fork)
- **Purpose:** extract the layered image→volume-system→filesystem architecture, tool semantics (mmls/fls/icat/blkls/ils), and NTFS + FAT-family parser internals relevant to a Windows data-recovery product.

---

## Codebase Map & Classification

959 tracked files (git ls-files). Classification by top-level directory (counts verified with `git ls-files | awk -F/ '{print $1}' | sort | uniq -c`):

| Category | Dir | Files | Role |
|---|---|---|---|
| Core library | `tsk/fs` | 82 | FS framework + per-FS parsers + tool libs |
| Core library | `tsk/img` | 25 | Disk image format layer |
| Core library | `tsk/vs` | 18 | Volume system (partition) layer |
| Core library | `tsk/base` | 22 | errors, unicode, endian, md5/sha1, crc, printf, locks, stack/list |
| Core library | `tsk/util` | 35 | incl. Bitlocker/ (26 files), detect_encryption, crypto |
| Core library | `tsk/pool` | 19 | 4th layer: APFS container + LVM pool |
| Core library | `tsk/auto` | 18 | auto-detection + SQLite case DB (bundled sqlite3.c/h) |
| Core library | `tsk/hashdb` | 14 | NSRL/EnCase/hashkeeper/md5sum/index lookups |
| Core library | `tsk/sorter` `tsk/docs` | 7+12 | sort config files; doxygen docs |
| CLI tools | `tools/fstools` | 18 | fls icat ils blkls blkcat blkcalc blkstat istat fsstat ffind fcat jcat jls usnjls ifind fscheck |
| CLI tools | `tools/vstools` `imgtools` `pooltools` | 5+4+3 | mmls mmcat mmstat / img_cat img_stat / pstat |
| CLI tools | `tools/autotools` | 8 | tsk_recover tsk_loaddb tsk_gettimes tsk_comparedir tsk_imageinfo |
| CLI tools | `tools/srchtools` `hashtools` `timeline` `sorter` | 4+5+3+3 | srch_strings sigfind / hfind / mactime.base / sorter |
| CLI tools | `tools/fiwalk` | 46 | fiwalk + plugins (public domain) |
| CLI tools | `tools/logicalimager` | 49 | JSON-rule logical imager (registry, extraction) |
| Bindings | `bindings/java` | 220 | Java datamodel + JNI (Apache-2.0) |
| Registry lib | `rejistry++` | 62 | standalone REGF parser (Apache-2.0) |
| Case UCO | `case-uco` | 80 | Java CASE/UCO facet exporter (Apache-2.0) |
| Build/CI | `win32` (81), `m4` (11), `debian` (9), `release` (5), `xcode` (1), `packages`, `appveyor.yml`, `.github` | ~110 | MSVS projects, autotools m4, packaging |
| Docs | `man` (30), `docs`, `tsk/docs` (12) | 43 | man pages + doxygen |
| Tests | `tests` (11), `test` (1), `unit_tests` (5) | 17 | API/thread tests + shell harness |
| Other | `licenses` (8), `samples` (6), root files | ~25 | LICENSE texts, API samples, README/ChangeLog |

Language mix (extension census): 237 `.java`, 144 `.cpp`, 142 `.h`, 100 `.c`, 13 `.hpp`, plus build/doc files.

`tsk/img` (25 files): raw.c/raw.h, mult_files.c (split discovery), img_open.cpp (dispatch), img_io.c (cache+verify), img_types.c (name↔id table), ewf.cpp/ewf.h, aff.c/aff.h, vmdk.c/vmdk.h, vhd.c/vhd.h, logical_img.c/h (folder-as-image), img_writer.cpp/h (re-writeable image support), pool.hpp, tsk_img.h (public API), tsk_img_i.h, unsupported_types.c/h, Makefile.am, .indent.pro.

`tsk/vs` (18 files): mm_open.c (dispatch), mm_part.c (list mgmt), mm_io.c, mm_types.c, dos.c + tsk_dos.h, gpt.c + tsk_gpt.h, bsd.c + tsk_bsd.h, sun.c + tsk_sun.h, mac.c + tsk_mac.h, tsk_vs.h/tsk_vs_i.h, Makefile.am, .indent.pro.

`tsk/util` (35 files): detect_encryption.c/h, crypto.cpp/h, span.hpp, lw_shared_ptr.hpp, file_system_utils.c/h, and `Bitlocker/` subdirectory (26 files: BitlockerParser, MetadataEntry, MetadataValue* — VMK/StretchKey/AesCcmEncryptedKey/VolumeMasterKey/Unicode/OffsetAndSize, BitlockerUtils, DataTypes).

`tsk/base` (22 files): tsk_error.c (+ tsk_error_win32.cpp), tsk_unicode.c/h, tsk_endian.c, tsk_printf.c, tsk_parse.c (offset/inum CLI parsing), tsk_stack.c, tsk_list.c, tsk_lock.c, mymalloc.c, crc.c/h, md5c.c, sha1c.c, XGetopt.c (Win32 getopt), tsk_base.h/tsk_base_i.h/tsk_os.h, tsk_version.c.

Supported filesystems, verified from the `FS_OPENERS[]` table in `tsk/fs/fs_open.c:139-150`:
NTFS, FAT (FAT12/16/32 + exFAT), EXT2/3/4, UFS1/UFS1b/UFS2 (FFS), YAFFS2, HFS+ (behind `TSK_USE_HFS`), ISO9660, APFS (via pool), plus RAW ("rawfs"), SWAP ("swapfs"), and Logical (folder-as-image `logical_fs.cpp`). BitLocker-encrypted NTFS volumes are opened via `tsk_fs_open_img_decrypt()` with password support (fs_open.c:127).

---

## Layered Abstraction Model (img → vs → pool → fs)

Four layers, each a C "class" struct with function pointers; each layer only holds a pointer to the one below.

### 1. Image layer — `TSK_IMG_INFO` (tsk/img/tsk_img.h:93-114)

- Open API: `tsk_img_open(num_img, images, type, a_ssize)` (img_open.cpp:79); single-file and UTF-8 wrappers delegate to it. Sector size must be ≥512 and multiple of 512 (img_open.cpp:95-109).
- `TSK_IMG_TYPE_ENUM` (tsk_img.h:62-82): `TSK_IMG_TYPE_RAW` (single or split share one bit), `AFF_AFF/AFD/AFM/ANY`, `EWF_EWF`, `VMDK_VMDK`, `VHD_VHD`, `EXTERNAL` (pytsk-style callback injection, tsk_img_open_external at img_open.cpp:411), `POOL`, `LOGICAL` (Windows-only folder image).
- **External vs bundled:** EWF is **external** — `ewf.cpp` is guarded by `#if HAVE_LIBEWF` and calls `libewf_handle_initialize/libewf_handle_open_wide` (ewf.cpp:329-346; detected by `TSK_OPT_DEP_CHECK([libewf]...)` in configure.ac:172). AFF (`aff.c`, libafflib), VMDK (`vmdk.c`, libvmdk, configure.ac:176), VHD (`vhd.c`, libvhdi, configure.ac:174) are likewise external. **No forensic image library is bundled.**
- **Auto-detection order** (img_open.cpp:117-232): try AFF → EWF → VMDK → VHD ("try the non-raw formats first"); if more than one matches → `TSK_ERR_IMG_UNKTYPE`; else fall back to `raw_open`. AFF with password error short-circuits.
- Split-raw discovery (`mult_files.c` `tsk_img_findFiles`/`getSegmentName`, mult_files.c:41-155) recognizes naming schemes: `.dmg`→`.NNN.dmgpart`, `.001`/`_001` (1-based, FTK-style >3 digits allowed), `.000`/`_00` (0-based), `.01`/`.00` (2-digit), `.aaa`/`xaaa`/`_aaa` (3-letter alpha), `.aa`/`xaa`/`_aa`, `.bin`→`(N).bin`.
- Raw reads span segments: `raw_read` finds the segment via cumulative `max_off[]` and continues into following segments for straddling reads (raw.c:286-391). FD cache: `SPLIT_CACHE` = 15 slots with `cptr[]` mapping + `next_slot` (raw.h:26-48).
- **Global read cache** in `TSK_IMG_INFO`: `TSK_IMG_INFO_CACHE_NUM` 32 entries × `TSK_IMG_INFO_CACHE_LEN` 65536 bytes, LRU-ish via `cache_age` (reset to 1000 on hit, decremented otherwise), sector-aligned fill (offset rounded down to 512), whole thing under `cache_lock` (tsk_img.h:84-109; img_io.c:58-250). Reads larger than the cache bypass it (img_io.c:109).
- `tsk_img_verify()` recomputes hashes — EWF/E01 with stored MD5 only (img_io.c:264-277).
- Unsupported-but-known image signature reporting (`unsupported_types.c:118-175`): AD1 ("ADSEGMENTEDFILE"), EWFv2/Ex01 ("EVF2\r\n\x81\x00"), RAR, 7z, Cellebrite UFD ("[Dumps]"), Tar (ustar magic at 257 or checksum verify), ZIP (PK), BZip2, Gzip — used to give users a precise reason why open failed.

### 2. Volume-system layer — `TSK_VS_INFO` (tsk/vs/tsk_vs.h:65-80)

- `tsk_vs_open(img_info, offset, type)` (mm_open.c:35). **Auto-detect order:** DOS → BSD (BSD deliberately overrides DOS because BSD disks begin with the DOS magic; mm_open.c:72-94) → GPT (with two tie-breaks: a backup GPT found alongside DOS loses to DOS; a DOS table containing a "GPT Safety" partition within first 63 sectors is discarded in favor of GPT — mm_open.c:96-146) → SUN → MAC; multiple hits otherwise produce `TSK_ERR_VS_MULTTYPE`. If nothing matches, `detectDiskEncryption()` is consulted (mm_open.c:189-208).
- `TSK_VS_TYPE_ENUM` (tsk_vs.h:49-59): DOS, BSD, SUN (Solaris VTOC), MAC, GPT, APFS (reserved, "Not supported yet" in mm_open.c:227), DBFILLER (synthetic table for loaddb).
- Partition record `TSK_VS_PART_INFO` (tsk_vs.h:103-116): doubly-linked sorted list with `start` (sector), `len`, `desc` (UTF-8), `table_num`, `slot_num`, `addr`, flags `TSK_VS_PART_FLAG_ALLOC/UNALLOC/META/ALL` (tsk_vs.h:93-98).
- `tsk_vs_part_add` inserts sorted by start and renumbers `addr` (mm_part.c:29-114). **`tsk_vs_part_unused`** synthesizes "Unallocated" `UNALLOC` entries for gaps between volumes and after the last volume to image end (mm_part.c:123-170) — this is the unallocated-space enumeration at the VS level.
- Logical images reject a VS (`mm_open.c:46-51`).

### 3. Pool layer (APFS/LVM) — `tsk/pool/tsk_pool.h`

`TSK_POOL_TYPE_ENUM`: `TSK_POOL_TYPE_APFS`, `TSK_POOL_TYPE_LVM` (tsk_pool.h:27-30; names in pool_types.cpp:23-25). `tsk_fs_open_pool*()` in fs_open.c:1065-1072; APFS FS open *requires* `img_info->itype == TSK_IMG_TYPE_POOL` (apfs_open.cpp:26-31).

### 4. Filesystem layer — `TSK_FS_INFO` (tsk/fs/tsk_fs.h:952-1057)

- `tsk_fs_open_vol()` computes `offset = part->start * vs->block_size + vs->offset` then delegates to `tsk_fs_open_img_decrypt` (fs_open.c:88-92) — **every partition is just an offset into the image**; the layers are pure offset math.
- Detection loop tries each opener with `test=1`; a second success aborts with `TSK_ERR_FS_MULTTYPE` (fs_open.c:176-227). BitLocker errors (`TSK_ERR_FS_BITLOCKER_ERROR`) are preserved across the loop. On total failure: `detectUnsupportedImageType` (offset 0 only) then `detectVolumeEncryption` (fs_open.c:240-276).
- `TSK_FS_INFO` carries geometry (`block_count`, `first/last_block`, `last_block_act` = clamped to actual image, `block_size`), inode range (`first/last_inum`, `root_inum`), journal inode, `fs_id[32]`, `TSK_FS_INFO_FLAG_HAVE_SEQ`, and the **FS-specific virtual-method table**: `block_walk`, `block_getflags`, `inode_walk`, `file_add_meta`, `get_default_attr_type`, `load_attrs`, `dir_open_meta`, `istat`, `fsstat`, `jopen/jblk_walk/jentry_walk`, `name_cmp`, `fscheck`, `fread_owner_sid`, `decrypt_block`, `close` (tsk_fs.h:1009-1056).
- Orphan machinery on `TSK_FS_INFO`: `list_inum_named` (TSK_LIST of unallocated inodes that DO have a name pointing at them; filled by a full UNALLOC-only recursive name walk with `TSK_FS_DIR_WALK_FLAG_NOORPHAN` — fs_dir.c `tsk_fs_dir_load_inum_named`) and `orphan_dir` (cached `$OrphanFiles` TSK_FS_DIR), both lock-protected (tsk_fs.h:992-1007).
- `TSK_FS_ORPHANDIR_INUM(fs)` = `fs_info->last_inum` — the last inode is reserved as the orphan pseudo-directory (tsk_fs.h:529-530); `tsk_fs_malloc/tsk_fs_free` init/teardown those locks (fs_open.c:334-376).

### Read-path API (all verified in public headers)

- `tsk_img_read(img, off, buf, len)` → format's `img->read` with cache (tsk_img.h:155-156; img_io.c:58).
- `tsk_vs_read_block(vs, addr, buf, len)` / `tsk_vs_part_read(part, off, buf, len)` / `tsk_vs_part_read_block` — volume-relative addressing (tsk_vs.h:134-148).
- `tsk_fs_read(fs, off, buf, len)` / `tsk_fs_read_block(fs, addr, buf, len)` (+ `_decrypt` variants with `crypto_id` for XTS IV, tsk_fs.h:1082-1089).
- File level: `tsk_fs_file_open(path)` / `tsk_fs_file_open_meta(addr)`; `tsk_fs_file_read(file, off, buf, len, flags)` with `TSK_FS_FILE_READ_FLAG_SLACK/NOID`; `tsk_fs_file_read_type(file, type, id, …)`; attribute accessors `tsk_fs_file_attr_get*` (idx/type/id) and `tsk_fs_attr_read` (tsk_fs.h:712-743); walking via `tsk_fs_file_walk[_type](flags, cb)`; hashing via `tsk_fs_file_hash_calc` producing `TSK_FS_HASH_RESULTS {md5_digest[16], sha1_digest[20]}` (tsk_fs.h:747-753).
- Attribute-list ops: `tsk_fs_attrlist_get`/`getnew`, runs via `tsk_fs_attr_run_alloc` (used throughout NTFS/FAT code above).

### mmls output schema (exact, from print_header + part_act)

```
<description of vstype>
Offset Sector: N
Units are in <block_size>-byte sectors

      Slot      Start        End          Length       [Size]    Description
NNN:  TTT:SSS   SSSSSSSSSS   EEEEEEEEEE   LLLLLLLLLL   [4364M]   Win95 FAT32 (0x0b)
NNN:  Meta      SSSSSSSSSS   EEEEEEEEEE   LLLLLLLLLL            Primary Table (#0)
NNN:  -------   SSSSSSSSSS   EEEEEEEEEE   LLLLLLLLLL            Unallocated
```

- Slot column: `%.3table:%.3slot` when both known; `Meta` for META flag rows; `-------` when neither (gap volumes) (mmls.cpp:59-78). With `-B`, a 5th column `<size><B|K|M|G|T>` derived by successive /1024 (mmls.cpp:80-99). `End` is inclusive (`start+len-1`, mmls.cpp:107-115).

### File/name object model (verified names, tsk_fs.h)

- **`TSK_FS_FILE`** (tsk_fs.h:695-702): `{ tag, TSK_FS_NAME *name, TSK_FS_META *meta, TSK_FS_INFO *fs_info }` — either side may be NULL when deleted files lack metadata or metadata is walked without names.
- **`TSK_FS_NAME`** (tsk_fs.h:601-618): `name` (UTF-8), `shrt_name` (8.3/short), `meta_addr`, `meta_seq` (NTFS), `par_addr`/`par_seq`, `date_added` (APFS), `type` (`TSK_FS_NAME_TYPE_ENUM` REG/DIR/…/VIRT/VIRT_DIR), `flags` = **`TSK_FS_NAME_FLAG_ALLOC` (0x01) / `TSK_FS_NAME_FLAG_UNALLOC` (0x02)** (tsk_fs.h:562-565).
- **`TSK_FS_META`** (tsk_fs.h:456-517): flags `ALLOC/UNALLOC/USED/UNUSED/COMP/ORPHAN` (tsk_fs.h:373-380), type, mode, nlink, size, uid/gid, mtime/atime/ctime/crtime (+`*_nano`), FS-specific `time2` union (ext2 dtime; hfs bkup_time; **ntfs: fn_crtime/fn_mtime/fn_atime/fn_ctime + fn_id** — the $FILE_NAME attribute times, tsk_fs.h:489-499), `content_ptr` (FS-specific stash, e.g. FAT starting cluster at fatfs_meta.c:438), `seq` (NTFS sequence number, tsk_fs.h:506), lazy `attr` (TSK_FS_ATTRLIST + `attr_state` STUDIED/ERROR), `name2` (TSK_FS_META_NAME_LIST — names stored in metadata, FAT & NTFS only, tsk_fs.h:359-364).
- **`TSK_FS_ATTR`** (tsk_fs.h:284-323): one per attribute stream; resident data in `rd.buf`, non-resident as `nrd.run` list of **`TSK_FS_ATTR_RUN`** `{ offset (in file), addr (fs block), len, flags FILLER/SPARSE/ENCRYPTED }` (tsk_fs.h:164-171) with size semantics `skiplen/allocsize/initsize/compsize` (tsk_fs.h:277-306 diagram). Attribute types mirror NTFS: `TSK_FS_ATTR_TYPE_NTFS_SI/FNAME/ATTRLIST/DATA/IDXROOT/IDXALLOC/…` plus HFS+ forks (tsk_fs.h:224-260).
- **`TSK_FS_DIR`** (tsk_fs.h:637-650): names array with `names_used/names_alloc`, `fs_file`, `addr`, `seq`.
- Block model: `TSK_FS_BLOCK` + `TSK_FS_BLOCK_FLAG_ENUM` (ALLOC/UNALLOC, CONT/META, BAD/RAW/SPARSE/COMP/RES, tsk_fs.h:68-80).

---

## Volume System (mmls semantics + partition systems)

### mmls (tools/vstools/mmls.cpp)

- Header (mmls.cpp:129-142): description line (`tsk_vs_type_todesc`), `Offset Sector: N`, `Units are in d-byte sectors`, column header `Slot  Start  End  Length  [Size]  Description` (Size only with `-B`).
- Per-partition row (mmls.cpp:56-127): slot column formats — `Meta` for META partitions, `-------` when no table/slot, `NNN` when slot-only, `NNN:MMM` as `table_num:slot_num`; then `start`, `end` (start+len-1), `len` in sectors; `-B` adds rounded size with B/K/M/G/T unit suffix (mmls.cpp:80-110).
- Options: `-o imgoffset` is in **sectors** and multiplied by `img->sector_size` (mmls.cpp:281); `-b dev_sector_size`; `-a/-A/-m/-M` select ALLOC/UNALLOC/META partitions (default ALL = 0x07, mmls.cpp:256-258); `-r` **recurses into DOS partitions**: ALLOC partitions' start offsets are collected during the walk and each is re-opened as a volume system (DOS only, one level, ≤64 entries — mmls.cpp:119-124, 298-321).
- Implementation = `tsk_vs_part_walk` over the sorted partition list with the callback printing rows.

### DOS/MBR parser (tsk/vs/dos.c)

- Structures (tsk_dos.h:20-43): `dos_part` `{boot, start_chs[3], ptype, end_chs[3], start_sec[4], size_sec[4]}`, `dos_sect` boot sector with `oemname[8]`, `ptable[4]`, magic `DOS_MAGIC` 0xaa55; `DOS_PART_SOFFSET` 0.
- Extended-partition types: `dos_is_ext(x)` = 0x05/0x0F/0x85 (dos.c:20-21).
- `dos_load_prim_table` (dos.c:862-1079): reads sector 0, verifies magic; when in autodetect `test` mode rejects FAT/NTFS boot sectors mistaken for MBRs by OEM name prefix `MSDOS`/`MSWIN`/`NTFS`/`FAT` (dos.c:916-965); skips entries with `part_size==0`; verifies first two partitions start within image (`max_addr`); adds META entry "Primary Table (#0)" of length 1 sector; CHS ignored entirely.
- `dos_load_ext_table` (dos.c:670-848): recursion depth cap 128 ("chain too deep", dos.c:686-692); **loop detection** — if an extended partition's absolute start (`sect_ext_base + part_start`) already exists in the list → "Loop in partition table detected" (dos.c:764-784); extended entries are relative to the *primary extended base* (`sect_ext_base`), while inner data partitions are relative to the *current EBR sector* (`sect_cur + part_start`) — the classic CHS/LBA dual-anchor rule (dos.c:786-843). Adds "Extended Table (#N)" META entries of length 1.
- 100+ entry partition-type description table `dos_get_desc` (dos.c:30-660), e.g. `0x07` → "NTFS / exFAT (0x07)", `0x0b/0x0c` → "Win95 FAT32", `0x0e` → "Win95 FAT16", `0xEE`-adjacent GPT safety.
- `tsk_vs_dos_open` then calls `tsk_vs_part_unused` to fill gaps (dos.c:1141-1146).

### GPT parser (tsk/vs/gpt.c)

- `gpt_head` (tsk_gpt.h:40-60): signature 0x5452415020494645ULL ("EFI PART"), `tab_start_lba`, `tab_num_ent`, `tab_size_b`, `tab_crc`, disk GUID; `gpt_entry` (tsk_gpt.h:63-72): type GUID, unique GUID, start/end LBA, flags, **72-byte UTF-16 name**.
- `gpt_load_table` (gpt.c:20-): primary header at sector 1 (`GPT_PART_SOFFSET+1`); requires a valid DOS sector 0 with `ptable[0].ptype == 0xEE` ("GPT safety partition", gpt.c:62-93); secondary/backup table at last sector (`SECONDARY_TABLE`, gpt.c:43-47). Endian guessed on secondary via 64-bit signature (gpt.c:110-120).

### Other VS parsers (structural)

`bsd.c` (BSD disklabel), `sun.c` (Sun VTOC), `mac.c` (Apple partition map) — each exposes `tsk_vs_*_open` as consumed by mm_open.c's switch; structs in `tsk_bsd.h`, `tsk_sun.h`, `tsk_mac.h` (enumerated, not deep-read).

---

## Tool Semantics (fls / icat / blkls / ils)

### fls (tools/fstools/fls.cpp + tsk/fs/fls_lib.c)

- Default `name_flags = TSK_FS_DIR_WALK_FLAG_ALLOC | TSK_FS_DIR_WALK_FLAG_UNALLOC` (fls.cpp:90) — both live and deleted names by default.
- `-d` (deleted only): `name_flags &= ~TSK_FS_DIR_WALK_FLAG_ALLOC` — leaves UNALLOC only; `-u` (undeleted only): `&= ~UNALLOC`; `-r`: `|= TSK_FS_DIR_WALK_FLAG_RECURSE`; `-p`: `TSK_FS_FLS_FULL` full path. **Auto-rule: FULL is auto-set when recursing with only UNALLOC or only files** (fls.cpp:262-268) so relative prefixes don't lose meaning; `-m` mactime mode also forces FULL (fls.cpp:273-275).
- Output `printit` (fls_lib.c:50-107): without `-p` prints a `+` per directory level above the entry then a space (fls_lib.c:55-68); `-l` long (`tsk_fs_name_print_long`), `-m` mactime body format (`tsk_fs_name_print_mac`), `-m -h` adds MD5 (`tsk_fs_file_hash_calc`, null-buffer on error).
- **NTFS alternate data streams:** `print_dent_act` iterates the attribute list and prints one row per `TSK_FS_ATTR_TYPE_NTFS_DATA` attribute (every ADS gets its own line), one per `NTFS_IDXROOT` (directory index), and the `$FILE_NAME` attribute only when `-m` and `fs_attr->id == meta->time2.ntfs.fn_id` (fls_lib.c:129-195). Files with no `$Data` still print something (fls_lib.c:189-193).
- `. and ..` hidden unless `-a` (TSK_FS_FLS_DOT, fls_lib.c:166-201).
- `-f/-i/-b/-o/-z/-s/-k` = fstype / imgtype / dev sector size / sector offset / timezone / time skew / decryption password (fls.cpp:34-68).

### icat (tools/fstools/icat.cpp + tsk/fs/icat_lib.c)

- Address syntax `inum[-typ[-id]]` parsed by `tsk_fs_parse_inum` (icat.cpp:208-212) — attribute type and attribute id can be appended to the inode to extract a specific stream (NTFS ADS).
- `-r` ("Recover deleted file") is **silently ignored — recovery is now automatic** (icat.cpp:186-188 comment "this is no longer needed"); `-R` suppresses recovery errors; `-s` = `TSK_FS_FILE_WALK_FLAG_SLACK` (dump slack after logical EOF); `-h` = `TSK_FS_FILE_WALK_FLAG_NOSPARSE` (no holes).
- `tsk_fs_icat` (icat_lib.c:55-98) opens by inode (`tsk_fs_file_open_meta`) and walks either `tsk_fs_file_walk_type(type,id)` (with `NOID` if no id given) or `tsk_fs_file_walk`; callback `icat_action` streams every chunk to stdout (binary mode on Win32).

### blkls (tools/fstools/blkls.cpp + tsk/fs/dls_lib.c)

- Default flags: `TSK_FS_BLOCK_WALK_FLAG_UNALLOC | TSK_FS_BLOCK_WALK_FLAG_META | TSK_FS_BLOCK_WALK_FLAG_CONT` (blkls.cpp:89-91) — i.e. the classic `dls` semantic "everything not allocated to a file" including FS metadata blocks; `-a` switches to ALLOC, `-e` all; `-s` slack only; `-l` time-machine list format.
- `tsk_fs_blkls` (dls_lib.c:205-244): plain mode = `tsk_fs_block_walk` with the given flags dumping each `fs_block->buf` (block_size bytes) to stdout; `-l` mode adds `AONLY` and prints `addr|alloc` rows ('a'/'f') with a `class|host|image|first_time|unit` header (dls_lib.c:44-77).
- **Slack mode** (`TSK_FS_BLKLS_SLACK`, dls_lib.c:112-200): `inode_walk` over `TSK_FS_META_FLAG_ALLOC`, per file `tsk_fs_file_walk(..., TSK_FS_FILE_WALK_FLAG_SLACK, slack_file_act)`; the callback tracks `flen` (bytes of logical file remaining), zeroes the used prefix of the final block, and writes the remainder — slack = allocated length minus logical size. **NTFS special case:** iterates every non-resident attribute separately (data runs per stream, dls_lib.c:173-197) because each stream has its own slack.

### ils (tools/fstools/ils.cpp + tsk/fs/ils_lib.c)

- Header (ils_lib.c:53-84): `class|host|device|start_time` then field list `st_ino|st_alloc|st_uid|st_gid|st_mtime|st_atime|st_ctime|st_crtime|st_mode|st_nlink|st_size`.
- Row (ils_lib.c:113-158): `inum|a-or-f|uid|gid|mtime|atime|ctime|crtime|mode(octal)|nlink|size`; `a`/`f` = `TSK_FS_META_FLAG_ALLOC` presence; `-s seconds` skew is subtracted for output then restored (ils_lib.c:133-152).
- `-e`/`-a` semantics are folded into TSK_FS_ILS flags: `TSK_FS_ILS_OPEN` (files currently open: forces `UNALLOC`+`LINK`, clears UNLINK — ils_lib.c:243-248), `TSK_FS_ILS_LINK`/`UNLINK` filter on `nlink == 0` (deleted metadata: nlink 0), `-m` mactime format (`ils_mac_act`, `<img>-name-inum>` style, ils_lib.c:167-214). `TSK_FS_META_FLAG_ORPHAN` request auto-enables both LINK and UNLINK (ils_lib.c:239-241).

### Directory-walk engine (fs_dir.c excerpts)

- `tsk_fs_dir_walk_internal` is the single recursive engine behind fls/`tsk_fs_dir_walk` (fs_dir.c:955); a `macro_recursion_depth` guard aborts with "recursion depth exceeds maximum" to stop cyclic directory structures (fs_dir.c:965-975). The public `tsk_fs_dir_walk` entry resets depth to 0 (fs_dir.c:1043).
- Names are accumulated into the `TSK_FS_DIR` array via `tsk_fs_dir_add`; `tsk_fs_dir_get`/`tsk_fs_dir_get_name` expose indexed access (tsk_fs.h:672-674). `tsk_fs_path2inum` resolves paths to inodes (tsk_fs.h:677-678).

---

## Filesystem Parsers (per-FS: structures + deleted detection)

| FS | Key structs (file) | Deleted-record detection | Dir-entry/name resolution | Extents |
|---|---|---|---|---|
| **NTFS** | `ntfs_sb`, `ntfs_mft`, `ntfs_attr`, `ntfs_runlist`, `ntfs_idxroot/idxrec/idxentry`, `ntfs_attr_fname` (tsk_ntfs.h) | `mft->flags & NTFS_MFT_INUSE` absent → `TSK_FS_META_FLAG_UNALLOC` (ntfs.c:2965-2968); name-level: idx entry past used-length or `strlen==0` → UNALLOC name | $I30 index root + linear INDX scan (see deep dive) | runlists → TSK_FS_ATTR_RUN |
| **FAT12/16/32** | `FATXXFS_SB`, `FATXXFS_DENTRY`, `FATXXFS_DENTRY_LFN`, `FATXXFS_FSINFO` (tsk_fatxxfs.h) | `name[0]==0xE5` (or `00 00` for Android subtype) / sector unalloc (fatxxfs_dent.c:414-424) | 32-byte dentries; LFN assembly by checksum+seq | FAT chain via `fatfs_make_data_runs` |
| **exFAT** | `EXFATFS_MASTER_BOOT_REC`, `EXFATFS_FILE_DIR_ENTRY`, `EXFATFS_FILE_STREAM_DIR_ENTRY`, `EXFATFS_FILE_NAME_DIR_ENTRY` (tsk_exfatfs.h) | high bit of `entry_type` (0x85 alloc / 0x05 deleted; stream 0xC0/0x40; name 0xC1/0x41) **and** allocated sector (exfatfs_dent.c:172-181) | entry sets (file + stream + 1-17 name entries, tsk_exfatfs.h:55-63) | FAT chain, or "no FAT chain" contiguous flag `EXFATFS_INVALID_FAT_CHAIN_MASK` 0x02 |
| **ext2/3/4** | `ext2fs_sb` (super at 1024, `s_magic`, tsk_ext2fs.h:102-192, 87-91), `ext2fs_gd`/`ext4fs_gd` group descriptors with `bg_inode_bitmap`/`bg_inode_table` + hi words (tsk_ext2fs.h:255-286); inode struct w/ `i_dtime`; journal `ext2fs_journal.c` (big-endian) | inode bitmap bit clear → UNALLOC (ext2fs.c:1008-1009); deletion time `i_dtime` kept as `time2.ext2.dtime` (ext2fs.c:829) and used in walks (ext2fs.c:2400, 3301) | directory blocks of `ext2_dir_entry*`; ext4 extents (`TSK_FS_META_CONTENT_TYPE_EXT4_EXTENTS/INLINE`, tsk_fs.h:438-442); `EXT4FS_FEATURE_INCOMPAT_INLINEDATA 0x2000` (tsk_ext2fs.h:234) | direct/indirect or extent tree |
| **HFS+/HFSX** | `hfs_plus_vh` VH signature `HFS_VH_SIG_HFSPLUS` 0x482b / `HFSX` 0x4858 / HFS 0x4244 (tsk_hfs.h:272-274,323-357) | catalog-file flags → `TSK_FS_META_FLAG_UNALLOC` (hfs.c:5343) | B-tree catalog file; `hfs_dent.c`; `decmpfs.c` compression records (`TSK_FS_ATTR_TYPE_HFS_COMP_REC`), `lzvn.c` decompressor | fork extents |
| **ISO9660** | PVD/SVD nodes (`iso9660_pvd_node`/`iso9660_svd_node`, tsk_iso9660.h:338-344) | everything surfaced as `UNALLOC|USED` unless path-table reachable (iso9660.c:1177,1318) — ISO9660 has no deletion | path table + directory records (`iso9660_dent.c`) | contiguous extents |
| **YAFFS2** | `yaffsObj_header`, `yaffsObj_spare` (tsk_yaffs.h:78-136) | objID 3 = UNLINKED, 4 = DELETED special objects (tsk_yaffs.h:152-153, yaffs.cpp:342,1625-1668); newer unlinked/deleted header wins → USED|UNALLOC | object headers scanned in NAND order | page/chunk addresses |
| **UFS/FFS** | `ffs.c` + `tsk_ffs.h` | inode bitmap/generation (`ffs.c:756,960`) | `ffs_dent.c` direct blocks | cylinder-group block+frag runs |
| **APFS** | C++ `APFSFSCompat` over pool (apfs_open.cpp:41-48; `apfs_fs.hpp`) | per-pool volume/snapshot model, `tsk/pool/apfs_pool.cpp` | J-object-tree via pool read (`pool_read.cpp`) | extent records; `img_bfio_handle.c` |
| **swap/raw** | `swapfs.c`, `rawfs.c` | n/a | none | whole-volume single run |
| **logical** | `logical_fs.cpp` + `tsk/img/logical_img.c` | n/a | host folder entries | host files |

---

## NTFS Deep Dive

### Boot sector & open (`ntfs_open`, ntfs.c:5246-5577)

- `ntfs_sb` (tsk_ntfs.h:69-85): `ssize` (sector bytes), `csize` (clusters per sector, allowed 1,2,4,8,16,32,64,128 — ntfs.c:5345-5361), `vol_size_s[8]`, `mft_clust[8]`, `mftm_clust[8]` ($MFTMirr), `mft_rsize_c` **signed**: >0 → clusters per record; negative → `-log2(record size)` (ntfs.c:5390-5397), `idx_rsize_c` same for INDX records, `serial[8]` → `fs_id`.
- Magic is 0xAA55 only, so OEM-name discrimination matters; **KAPE images** (VHD with `oemname == "NTFS    "` but broken magic) get fixed up by `process_kape_boot_format` (ntfs.c:5208-5240): guesses endian via plausible sector size.
- Open sequence: read boot → sizes → assign vtable → open $MFT as inode 0 (`tsk_fs_file_open_meta(fs, NULL, NTFS_MFT_MFT)`, ntfs.c:5486) → cache its `$Data` attribute as `ntfs->mft_data` → derive `inum_count = mft_data->size / mft_rsize_b + 1` (+1 reserves the orphan dir inode, ntfs.c:5511) → load version ($Volume $VINFO), $Bitmap run (`ntfs_load_bmap`), $Secure SII/SDS (`ntfs_load_secure`). Root = MFT entry 5 (`NTFS_MFT_ROOT`, tsk_ntfs.h:132). `fs->flags = TSK_FS_INFO_FLAG_HAVE_SEQ`.

### MFT record & fixup (`ntfs_dinode_lookup`, ntfs.c:144-468)

- `ntfs_mft` (tsk_ntfs.h:94-110): magic (FILE/BAAD/0000), `upd_off`, `upd_cnt`, `lsn`, `seq[2]`, `link[2]`, `attr_off`, `flags` (`NTFS_MFT_INUSE` 0x1, `NTFS_MFT_DIR` 0x2), size/alloc_size, `base_ref[6]+base_seq[2]` (attribute extension records), XP `entry[4]`.
- Locating entry N: without `mft_data` cached (bootstrap): `root_mft_addr + N * mft_rsize_b`, only allowed for N ≤ 16; with cache: walk the **$MFT $Data run list** accumulating byte offsets; handles an MFT record straddling two runs with a two-part read (ntfs.c:255-333).
- **Fixup array application** (ntfs.c:370-465): `ntfs_upd` at `upd_off` = `{upd_val[2], upd_seq[...]}` (tsk_ntfs.h:58-61). For each 512-byte stride i (stride = `NTFS_UPDATE_SEQ_STRIDE` 512): the last 2 bytes at `i*512-2` must equal `upd_val` (signature); mismatch → `TSK_COR` "Incorrect update sequence value… typically because of a corrupted entry". On match, they are replaced by `upd_seq[(i-1)*2]` — the true sector-tail bytes. Bounds checks: `upd_cnt` sanity vs record size (ntfs.c:380-412).
- Magic check on entries is deliberately disabled (`#if 0`, ntfs.c:354-369) — XP tolerates junk entries; fixup failure is the corruption signal.

### Attribute header & runlists

- `ntfs_attr` (tsk_ntfs.h:145-173): `type[4]`, `len[4]`, `res` (0 resident / 1 non-resident), `nlen`, `name_off`, `flags` (COMP 0x1 / ENC 0x4000 / SPAR 0x8000), `id[2]`; resident: `ssize`, `soff`, `idxflag`; non-resident: `start_vcn[8]`, `last_vcn[8]`, `run_off[2]`, `compusize[2]`, `alen[8]/ssize[8]/initsize[8]`.
- **Runlist decode** (`ntfs_make_data_run`, ntfs.c:596-804): header byte low nibble = byte count of run **length**, high nibble = byte count of the **offset delta** (macros `NTFS_RUNL_LENSZ/NTFS_RUNL_OFFSZ`, tsk_ntfs.h:361-365). Offsets are **signed little-endian deltas** — sign-extended when the top byte's high bit is set (ntfs.c:715-718) — added to the previous run's LCN (`data_run->addr = prev_addr + addr_offset`, ntfs.c:757). Sparse rules: delta 0 → sparse run (exception: $Boot entry 7 legitimately starts at 0, ntfs.c:744-755); NT4-style `-1` → sparse (ntfs.c:726-742). Guards: length field ≤ 8 bytes, run length ≤ `fs->block_count`, run end within FS (ntfs.c:651-679, 761-769); runlist_offset capped at INT32_MAX (ntfs.c:782-786). Terminator = length nibble 0. `$BadClust` special case: single full-FS sparse run is dropped (ntfs.c:789-801).
- Attribute types (tsk_ntfs.h:191-208) map 1:1 onto `TSK_FS_ATTR_TYPE_NTFS_*`. Non-resident attributes at nonzero `start_vcn` are merged as extensions of an existing attribute list entry; duplicates at different VCNs ignored (ntfs.c:2118-2123 comment, 2676-2685).
- **$ATTRIB-list plumbing:** `ntfs_attrlist` entries `{type[4], len[2], nlen, start_vcn[8], file_ref[6], seq[2], id[2], name}` (tsk_ntfs.h:329-339) let one file's attributes spill into extension MFT records (`base_ref` in `ntfs_mft`); TSK follows them through `ntfs_proc_attrseq` (ntfs.c:1845-) and uses `NTFS_ATTRL_RES` = resident marker.
- **$Secure / SIDs** (tsk_ntfs.h:495-608, compiled when `TSK_USE_SID 1`): self-relative security descriptor layout; `NTFS_SXX_BUFFER` accumulates $SII and $SDS streams under `sid_lock`; `ntfs_sid` has the big-endian ident_auth quirk (tsk_ntfs.h:548 comment); SDH/SII index entries expose `sec_id` → SDS offset/size so `fs->fread_owner_sid` (`ntfs_file_get_sid_str`, ntfs.c fs table) can resolve owners. `ntfs_attrdef` (tsk_ntfs.h:458-470) gives per-attribute min/max sizes + IDX/RES/NONRES flags from $AttrDef.
- Compression: `ntfs_uncompress_setup/compunit` (ntfs.c:878-1288) — compression units (typically 16 clusters) composed of 4K blocks with 2-byte compressed-size headers, token-based (symbol/token, 8-bit tokens, `NTFS_TOKEN_MASK`), buffer cap `NTFS_MAX_UNCOMPRESSION_BUFFER_SIZE` 65536 (tsk_ntfs.h:44-48).

### Deleted detection & sequence numbers

- Allocation: `mft->flags & NTFS_MFT_INUSE` → `TSK_FS_META_FLAG_ALLOC` else `UNALLOC` (ntfs.c:2965-2968). `meta->seq = mft->seq` (ntfs.c:2930).
- **Sequence reconciliation** (`ntfs_inode_lookup`, ntfs.c:3101-3120): NTFS bumps `seq` on *deletion*, not allocation; for a deleted MFT entry TSK compares the name's `meta_seq` against `meta->seq - 1` so names recorded before deletion still match; mismatch → the metadata is discarded (reset) so a stale name doesn't bind to reused entry content.
- $FILE_NAME attribute (`ntfs_attr_fname`, tsk_ntfs.h:216-229) carries `par_ref[6]+par_seq[2]`, 4 NTFS timestamps, `alloc_fsize/real_fsize`, DOS flags, `nlen`, namespace (`NTFS_FNAME_POSIX/WIN32/DOS/WINDOS`, tsk_ntfs.h:250-253) — this is what populates `TSK_FS_META_NAME_LIST` (`name2`, `par_inode`, `par_seq`) and `time2.ntfs.*`.

### $I30 directory resolution (`ntfs_dir_open_meta`, ntfs_dent.cpp:774-1290)

- Resident `ntfs_idxroot` (attribute $IDX_ROOT, must be resident — ntfs_dent.cpp:865-871) contains `ntfs_idxelist {begin_off, seqend_off, bufend_off, flags}` where **`seqend_off` - `begin_off` = live ("used") length and `bufend_off` - `begin_off` = allocated length — the difference is directory slack** (tsk_ntfs.h:381-386).
- `.` / `..` are synthesized (NTFS has none except root's dot): `.` from the dir's own addr/seq (with **seq-1 for deleted dirs**, ntfs_dent.cpp:919-929); `..` from every `meta->name2` parent reference (ntfs_dent.cpp:936-956).
- Small dirs stop at $IDX_ROOT; large dirs load the **entire $IDX_ALLOC attribute** into one buffer (≤128 MiB guard, `TSK_ERR_FS_LARGE_DIR_ERROR`, ntfs_dent.cpp:1039-1046) using `tsk_fs_attr_walk` with `TSK_FS_FILE_WALK_FLAG_SLACK` (ntfs_dent.cpp:1066-1072).
- **The B+ tree is deliberately NOT traversed as a tree** — "we could not find the deleted file names" (ntfs_dent.cpp:1085-1096): instead the buffer is scanned **linearly by cluster for INDX record magics (`NTFS_IDXREC_MAGIC` 0x58444e49)**; each record's extent is inferred from the distance to the next magic (not the on-disk size, which "we don't trust" — ntfs_dent.cpp:1141-1165), fixup applied via `ntfs_fix_idxrec` (ntfs_dent.cpp:673), then its entry list processed. This converts tree traversal into a recoverable linear scan of index slack.
- **Entry-level deleted detection** (`ntfs_proc_idxentry`, ntfs_dent.cpp:387-661): an entry is `TSK_FS_NAME_FLAG_UNALLOC` if (a) the parent MFT entry is deleted (`a_is_del`), or (b) `idxe->strlen == 0` (NTFS zeroes stream length on delete), or (c) the entry's end passes `endaddr_alloc` (= used length boundary) — i.e., entries living in the slack between used and allocated length are recovered as deleted names. Everything between `endaddr_alloc` and `endaddr` is parsed as deleted content.
- Entry validation while scanning slack (carving-grade): file_ref within `[first_inum,last_inum]`, `idxlen > strlen`, `idxlen % 4 == 0`, `idxlen ≤ buffer`; on failure **advance 4 bytes and retry** (ntfs_dent.cpp:443-452); deleted entries additionally validated by namespace ∈ {POSIX,WIN32,DOS,WINDOS}, `alloc_fsize ≥ real_fsize`, nonzero name, and **plausible FILE_NAME timestamps** (`is_time`: between 1601 and ~2020 within /100 slack, ntfs_dent.cpp:348-370); wrong `fname->par_ref` (parent) is skipped (ntfs_dent.cpp:471-478).
- **Deleted entry stride reconstruction:** when `strlen == 0`, the original entry length is recomputed as `16 + 66 + 2*nlen` rounded up to 4 (ntfs_dent.cpp:623-646) so the scan re-syncs past zeroed entries.
- **8.3 short-name pairing** (ntfs_dent.cpp:524-621): a POSIX/WIN32 entry is stashed one iteration; if the next entry is DOS with the same `file_ref` → `ntfs_dent_copy_short_only` fills `shrt_name`; `WINDOS` entries are emitted immediately; unmatched DOS entries are dropped (risk of missing deleted DOS names acknowledged in comment ntfs_dent.cpp:536-539).
- Orphan handling: `a_addr == TSK_FS_ORPHANDIR_INUM` → `tsk_fs_dir_find_orphans` (ntfs_dent.cpp:829-832); orphan→parent map `getParentMap/ntfs_parent_map_*` (ntfs_dent.cpp:114-190) with `NTFS_PAR_MAP` map keyed by parent inum+seq.

### USN Update Sequence Journal (tsk_ntfs.h:613-718)

- On-disk `ntfs_usn_v2_rec` (tsk_ntfs.h:617-635): `length[4]`, `major/minor_version[2]`, `file_ref[6]+file_ref_seq[2]` (the changed MFT entry), `par_ref[6]+par_ref_seq[2]` (its parent dir), `usn[8]`, `timestamp[8]`, `reason[4]`, `source_info[4]`, `security_id[4]`, `attributes[4]`, `fname_length[2]`, `fname_offset[2]`, UTF-16 name at offset 60. In-memory form `TSK_USN_RECORD_V2` (tsk_ntfs.h:695-709) with `refnum/refnum_seq`, `parent_refnum/parent_refnum_seq`, `usn`, `time_sec/time_nsec`, `reason`, `source_info`, `attributes`, `fname`.
- `TSK_FS_USN_REASON` (tsk_ntfs.h:638-660) is the full reason bitmask: `FILE_CREATE 0x100`, `FILE_DELETE 0x200`, `RENAME_OLD_NAME 0x1000`, `RENAME_NEW_NAME 0x2000`, `DATA_OVERWRITE/EXTEND/TRUNCATION`, `BASIC_INFO_CHANGE 0x8000`, `CLOSE 0x80000000`, etc.; `TSK_FS_USN_SOURCE_INFO` (tsk_ntfs.h:664-669) separates replication/management sources. `TSK_FS_NTFS_FILE_ATTRIBUTES` (tsk_ntfs.h:673-691) mirrors the Win32 attribute flags (incl. `INTEGRITY_STREAM 0x8000`, `NO_SCRUB_DATA 0x20000`).
- Plumbing: `NTFS_INFO.usnjinfo` (`{ TSK_FS_FILE *fs_file, usnj_inum, bsize }`, tsk_ntfs.h:712-718) is loaded by `usn_journal.c` and consumed by `usnjls_lib.c` / `tools/fstools/usnjls.cpp` (enumerated, not read). Value for the target: a **timestamped rename/delete history** that complements $I30 slack recovery — parent+name+time per event, recoverable even when the index entry was overwritten.

### NTFS compression & special attributes (ntfs.c:808-1288, structural)

- Compression units of ~16 clusters split into 4 KiB pre-compression blocks, each stored with a 2-byte compressed-size header; `NTFS_TOKEN_MASK` 1 / `NTFS_SYMBOL_TOKEN` 0 / `NTFS_TOKEN_LENGTH` 8 token layout; `NTFS_MAX_UNCOMPRESSION_BUFFER_SIZE` 65536 cap (tsk_ntfs.h:43-48). `ntfs_attr_walk_special` (ntfs.c:1289) is the attr `r`/`w` override used for compressed/encrypted streams. Encrypted (EFS) attributes set `TSK_FS_ATTR_ENC` (tsk_fs.h:184).

---

## FAT / exFAT Deep Dive

### Common layer (tsk_fatfs.h / fatfs.c)

- `fatfs_open` (fatfs.c:36-152): reads the boot sector at offset 0, then **backup boot sectors at sector 6 (FAT32) and sector 12 (exFAT)** on magic failure (fatfs.c:80-101); records `using_backup_boot_sector`. Detection then tries `fatxxfs_open` then `exfatfs_open` (FAT_DETECT), or the specific subtype.
- `FATFS_INFO` (tsk_fatfs.h:198-278): FAT cache (4 × 4096 with TTL, tsk_fatfs.h:56-57), layout sectors (`firstfatsect`, `firstdatasect`, `firstclustsect`, `rootsect`, `clustcnt`, `lastclust`), `dentry_cnt_se/_cl`, `numfat`, `numroot`, `mask` (FAT12 0x0fff / FAT16 0x0ffff / FAT32 & exFAT 0x0fffffff), virtual inodes for `$MBR`, `$FAT1`, `$FAT2` (`FATFS_NUM_VIRT_FILES = numfat+2`), `inum2par` map (child→parent dirs), and a **subtype vtable of function pointers** (`is_cluster_alloc`, `is_dentry`, `inode_lookup`, `inode_walk_should_skip_dentry`, `istat_attr_flags`, `dent_parse_buf`, `dinode_copy`) — the FATXX/exFAT split is polymorphism inside one TSK_FS_INFO.
- Inode addressing is **slot-based**: inode = sector's base (`FATFS_SECT_2_INODE`, tsk_fatfs.h:101-102) + entry index within sector (`FATFS_DENTRY_SIZE` 32); macros `FATFS_INODE_2_SECT/_2_OFF` invert it.
- FAT entry constants: `FATFS_UNALLOC 0`, `FATFS_BAD 0x0ffffff7`, `FATFS_EOFS 0x0ffffff8`, `FATFS_EOFE 0x0fffffff`, `FATFS_ISEOF/ISBAD` macros (tsk_fatfs.h:104-117).
- Android quirk: `TSK_FATFS_SUBTYPE_ANDROID_1` uses `name[0]==0 && name[1]==0` as deleted marker (tsk_fatxxfs.h:40-43).

### FATXX structures (tsk_fatxxfs.h)

- `FATXXFS_SB` (tsk_fatxxfs.h:129-172): BPB with `ssize`, `csize`, `reserved`, `numfat`, `numroot` (FAT12/16 root dir entry count), `sectors16`, `sectperfat16`, `prevsect`, `sectors32`; union `f16` (vol_id/label/fs_type) vs `f32` (`sectperfat32`, `rootclust`, `fsinfo`, `bs_backup`); magic 0xaa55. `FATXXFS_FSINFO` (FSINFO sector magics 41615252 / 61417272 / AA550000, freecnt/nextfree).
- `FATXXFS_DENTRY` (tsk_fatxxfs.h:185-199): 8.3 `name[8]+ext[3]`, `attrib` (FATFS_ATTR_* incl. `FATFS_ATTR_LFN 0x0f`, `VOLUME 0x08`, `DIRECTORY 0x10`), `lowercase` bits 0x08/0x10 (base/ext lower), `ctimeten` (10-ms, 0-199), ctime/cdate/adate, **`highclust[2]`** (FAT32 upper 16 bits; `FATXXFS_DENTRY_CLUST` = startclust | highclust<<16, tsk_fatxxfs.h:29-30), wtime/wdate, `startclust[2]`, `size[4]`.
- `FATXXFS_DENTRY_LFN` (tsk_fatxxfs.h:206-215): `seq` (bit 0x40 = first/last-in-chain, low 6 bits = descending counter), UTF-16 name parts 10+12+4, `chksum` vs the 8.3 name.
- Deleted markers: `FATXXFS_SLOT_DELETED 0xe5`, `FATXXFS_SLOT_EMPTY 0x00`, `FATXXFS_SLOT_E5 0x05` (a real 0xE5 first char is stored as 0x05) — tsk_fatxxfs.h:32-35. Carving-grade name validation macros: `FATXXFS_IS_83_NAME_STRICT` (full MS forbidden set) for unallocated-space scanning vs `FATXXFS_IS_83_NAME_RELAXED` (only controls, `* . / ?`) when the directory itself is trusted (tsk_fatxxfs.h:44-110) — a deliberate two-tier false-positive policy.

### Directory parsing & deleted recovery (fatxxfs_dent.c)

- `fatxxfs_dent_parse_buf` (fatxxfs_dent.c:58-432) walks 32-byte slots per sector:
  - Per-sector allocation from `fatfs_is_sectalloc`; per-entry validity via `fatxxfs_is_dentry` (extended checks when cluster unallocated); **corrupt-directory heuristic:** if the first 4 entries of a cluster are all invalid, switch to strict validation for the rest — handles allocated clusters whose content merely resembles FAT (fatxxfs_dent.c:148-157).
  - **LFN assembly** (fatxxfs_dent.c:166-203): LFN entries (`attrib == 0x0f`) are recognized by the first-entry bit (`FATXXFS_LFN_SEQ_FIRST`) or a checksum change; UTF-16 pieces are copied **backwards** (part3, part2, part1) into a 1024-byte buffer from its end so name fragments reassemble in order; on reaching the 8.3 entry the UTF-16 → UTF-8 conversion runs and the 8.3 name becomes `shrt_name` (fatxxfs_dent.c:238-283). Volume-label entries get " (Volume Label Entry)" appended (fatxxfs_dent.c:204-233).
  - **Deleted detection** (fatxxfs_dent.c:410-424): `sectalloc && !FATXXFS_IS_DELETED` → ALLOC name; else UNALLOC — i.e. both the 0xE5 marker **and** entries living in unallocated directory clusters (deleted directories whose entries were never re-marked) surface as deleted names.
  - 8.3 rendering: deleted 0xE5 first char becomes `_` (fatxxfs_dent.c:292-295); `lowercase` flags fold case; `.` inserted before ext; `fatfs_cleanup_ascii` sanitizes.
  - `..` resolution: `fatfs_dir_buf_get` on the inum2par map, else a full recursive `fatfs_find_parent_act` walk to (re)build the map, else orphan dir inode fallback (fatxxfs_dent.c:349-396). Subdirectories register themselves in the map on the way through (fatxxfs_dent.c:399-406).

### Cluster chains & deleted-file recovery (fatfs_meta.c:394-…)

- Allocated files: `fatfs_make_data_runs` follows the FAT chain cluster by cluster (via cached FAT reads `fatfs_getFAT`), coalescing consecutive clusters into single `TSK_FS_ATTR_RUN`s; FAT12/16 root dir is a contiguous run in the reserved area (fatfs_meta.c:454-504); `$FAT1/$FAT2/$MBR` are virtual contiguous runs.
- **Deleted files (recovery mode)** (fatfs_meta.c:…-290): since deletion clears the FAT chain, TSK **assumes contiguity from the saved `startclust`**: skip clusters that are currently allocated (`fatfs->is_cluster_alloc`), consume up to `roundup(size, cluster_size)` bytes of unallocated clusters, and build runs from those; if the start cluster is already reallocated or the FS ends first → `canRecover = 0`. All failures during recovery use errno `TSK_ERR_FS_RECOVER` (suppressible — this is exactly what `icat -R` hides; the generic path sets `TSK_ERR_FS_INODE_COR` for live files, fatfs_meta.c:439-455).
- exFAT empty-file special case: deleted entry with startclust 0 → empty run (fatfs_meta.c:161-178).

- DOS date/time are bit-packed: `FATFS_SEC_MASK 0x1f` (2-sec units), `FATFS_MIN_MASK 0x7e0`, `FATFS_HOUR_MASK 0xf800`; date `FATFS_DAY_MASK 0x1f`, `FATFS_MON_MASK 0x1e0`, `FATFS_YEAR_MASK 0xfe00` (base 1980) with min/max range macros and `FATFS_ISTIME/FATFS_ISDATE` validity predicates (tsk_fatfs.h:119-157). Conversion helpers `fatfs_dos_2_unix_time` / `fatfs_dos_2_nanosec` (tsk_fatfs.h:289-292); `ctimeten` gives 10-ms resolution (0-199). exFAT adds per-timestamp timezone offsets in 15-minute increments (`EXFATFS_FILE_DIR_ENTRY.{created,modified,accessed}_time_time_zone_offset`, tsk_exfatfs.h:278-282). These predicates double as carving sanity checks (same spirit as `FATXXFS_IS_83_NAME_STRICT`).

### exFAT differences

- `EXFATFS_MASTER_BOOT_REC` (tsk_exfatfs.h:141-163): "EXFAT " fs_name, 64-bit `partition_offset`/`vol_len_in_sectors`, `fat_offset`, `fat_len_in_sectors`, `cluster_heap_offset`, `cluster_cnt`, `root_dir_cluster` (root is a cluster-chain directory), power-of-2 `bytes_per_sector` (512-4096) and `sectors_per_cluster`, `num_fats` (1-2 for TexFAT).
- Allocation bitmap replaces FAT for cluster status (`EXFATFS_ALLOC_BITMAP_DIR_ENTRY` 0x81, first cluster + length — cached in `FATFS_INFO.EXFATFS_INFO`, tsk_fatfs.h:274-277).
- **Entry sets**: FILE (0x85 alloc / 0x05 deleted) + STREAM (0xC0/0x40; carries `valid_data_length`, `first_cluster_addr`, `data_length`, `file_name_length`, `file_name_hash`) + 1-17 NAME entries (0xC1/0x41, 15 UTF-16 chars each) — `EXFATFS_MIN/MAX_FILE_SECONDARY_DENTRIES_COUNT` 2/18, checksum-verified (tsk_exfatfs.h:55-63, 266-318). Cluster count sanity: `secondary_entries_count` must match what follows.
- Deleted detection (exfatfs_dent.c:172-181): allocated name requires **both** the type byte's high bit and an allocated containing sector ("trust and verify").
- Contiguous files may have **no FAT chain at all** (`EXFATFS_INVALID_FAT_CHAIN_MASK` 0x02 in stream flags) — the parser treats first-cluster + length as one run.

---

## Algorithms Worth Adopting (for d-space)

1. **Layered offset model** — every layer (image format, partition, pool, FS) is a struct with a `read` vtable over an offset space; layers compose by pure arithmetic (`part->start * block_size + vs->offset`, fs_open.c:88-92). Adopt: mount any source (raw file, split set, VHD/VMDK/E01) behind one `read(offset,len)` trait.
2. **Image auto-detection ladder with ambiguity errors** — try all wrapped formats, error with *both* candidate names on multi-match (img_open.cpp:117-232; mm_open.c multi-type errors). Adopt for "which container is this?" UX, plus the unsupported-format signature table (unsupported_types.c:118-175) to explain failures (AD1/Ex01/RAR/7z/UFD/Tar/Zip/Bzip2/Gzip).
3. **Split-image segment naming resolver** — `getSegmentName`'s scheme table (.001/_001/0-based/.aa alpha/.dmgpart/(N).bin, mult_files.c:41-155) + cumulative `max_off[]` reads (raw.c:286-391). Adopt for multi-part dd/DPMI-style image loading.
4. **Synthetic unallocated partitions** — `tsk_vs_part_unused` fills inter-volume and trailing gaps as `UNALLOC` volumes (mm_part.c:123-170), making "scan all unallocated space" a uniform partition walk. Adopt at both VS level (mmls rows) and FS level (blkls flags).
5. **NTFS fixup application & corruption signaling** — verify per-512 stride signature then swap back real bytes (ntfs.c:370-465); the *only* cheap integrity check that separates stale/stray MFT records from live ones. Adopt for any MFT/INDX record reader; note `ntfs_fix_idxrec` applies the same to INDX blocks (ntfs_dent.cpp:673).
6. **Signed-delta runlist decoder with sparse/NT4/$Boot/$BadClust rules** — ntfs_make_data_run (ntfs.c:596-804): nibble-sized varint pairs, sign extension, delta accumulation, 0-delta = sparse, defensive caps (len ≤ 8B; run ≤ block_count; INT32_MAX runlist). Adopt as the canonical extent decoder for NTFS.
7. **Linear INDX scan instead of B-tree traversal for deleted names** — load whole $IDX_ALLOC with SLACK walk, scan cluster-by-cluster for INDX magic, infer record length from the *next* magic, parse entries beyond `seqend_off` as deleted names, re-sync zeroed entries with the `16+66+2*nlen` round-up rule (ntfs_dent.cpp:1085-1273, 623-646). This is the single highest-value deleted-name recovery trick in TSK.
8. **$FILE_NAME-parent-based path reconstruction + seq-1 matching** — deleted MFT entries are matched to their pre-deletion names by comparing `meta_seq` to `seq-1` (ntfs.c:3101-3120); `..` entries are synthesized from `meta->name2` parent refs (ntfs_dent.cpp:936-956); NTFS seq semantics (bump-on-delete) documented at ntfs.c:3096-3099.
9. **Orphan-file pass** — build `list_inum_named` from a UNALLOC-only recursive name walk, then `inode_walk(UNALLOC|USED)` and emit every unallocated inode *not* in the list under a virtual `$OrphanFiles` dir cached in `TSK_FS_INFO.orphan_dir` (fs_dir.c `tsk_fs_dir_find_orphans`, `tsk_fs_dir_load_inum_named`); last inode reserved by convention (tsk_fs.h:529-530). FAT's `..` resolution falls back to the orphan dir when the parent isn't found (fatxxfs_dent.c:391-394).
10. **FAT deleted-file contiguous-cluster recovery** — start from dentry's saved start cluster, skip currently-allocated clusters, take up to rounded-up size, mark errors as RECOVER class (fatfs_meta.c deleted branch). Adopt for "carve-by-inode" recovery; pair with `icat -R`'s error suppression semantic.
11. **FAT deleted-name detection stack** — 0xE5/0x00-00(Android) slot marker + unallocated-directory-cluster contents (fatxxfs_dent.c:410-424) + LFN reassembly by checksum/seq even for deleted entries (fatxxfs_dent.c:166-203) + strict-vs-relaxed 8.3 validation by structural confidence (tsk_fatxxfs.h:44-110) + "4 bad entries → strict dir" heuristic (fatxxfs_dent.c:148-157).
12. **Slack extraction semantics** — file-level slack via `TSK_FS_FILE_WALK_FLAG_SLACK` with used-length zeroing (dls_lib.c:112-146) incl. per-NTFS-attribute slack (dls_lib.c:173-197); directory-slack recovery is the INDX-scan in #7; block-level unallocated dump = blkls default flags (blkls.cpp:89-91). Adopt all three as separate "carving input" products.
13. **Encryption pre-detection** — signature probes for BitLocker `-FVE-FS-`, LUKS, FileVault, CheckPoint, SafeBoot, GuardianEdge, Sophos, WinMagic, PGP + Shannon entropy over ~100×64KiB blocks (detect_encryption.c:30-150), surfaced as distinct errnos (`TSK_ERR_VS_ENCRYPTED`, `TSK_ERR_FS_ENCRYPTED`/`POSSIBLY_ENCRYPTED`) with BitLocker password path via `tsk_fs_open_img_decrypt` (fs_open.c:127) and `tsk/util/Bitlocker/` full FVE parser. Highly relevant to a Windows recovery tool.
14. **Lazy attribute loading + caching discipline** — `TSK_FS_META.attr_state` STUDIED/ERROR with `load_attrs` vtable (tsk_fs.h:451-454,1019), MFT data-run caching for O(1) inode→offset (ntfs.c:189-300), FAT cache with TTL, image-layer 32×64KiB LRU cache (img_io.c) — a proven performance pattern for random-access recovery UIs.

---

## Adoption Blueprint for d-space (Rust/Tauri mapping of cited TSK behavior)

1. **`ImageSource` trait = TSK_IMG_INFO.** One trait with `read(off,len)`, `size`, `sector_size`; implementations: RawFile (single), RawSplit (segment resolver from mult_files.c:41-155 + cumulative-offset reads raw.c:286-391), plus optional external crates for E01/VHD/VMDK later. Do **not** bundle libewf — TSK itself treats it as optional (configure.ac:172).
2. **Cache once, at the bottom.** The 32×64 KiB LRU with sector-aligned fills and "big reads bypass cache" rule (img_io.c:58-250) is cheap and removes the per-parser syscall storm; Rust port: `Mutex<Vec<CacheSlot>>` with the same age counter.
3. **`VolumeSystem` = partition list builder.** Port dos.c exactly: EBR recursion with the *dual anchor rule* (extended-chain entries anchored at the primary extended base; inner partitions at the current EBR — dos.c:786-843), depth cap 128, loop detection by absolute start (dos.c:764-784), OEM-name rejection when probing (dos.c:916-965); then a `fill_unallocated_gaps` pass cloning mm_part.c:123-170 so mmls-style tables always contain `Unallocated` rows.
4. **`FileSystem` = detection ladder + vtable.** Copy the "try every parser, hard error listing both candidates on ambiguity" contract (fs_open.c:176-227) and the "offset = start*block_size + vs.offset" composition rule (fs_open.c:88-92). Model NTFS/FAT/exFAT as separate modules behind one trait with the ~15 methods of `TSK_FS_INFO` (tsk_fs.h:1009-1056).
5. **Object model.** Split Name (`flags: Alloc|Unalloc`, `meta_addr`, `meta_seq`, `shrt_name`) from Meta (`flags`, `seq`, times incl. `fn_*` NTFS set, `content`) and pair them in `File { name: Option<Name>, meta: Option<Meta> }` — the nullable-sides pattern is what lets TSK print deleted names whose metadata is gone (tsk_fs.h:688-702 doc comment). Attributes as a list of resident-buffer | run-list variants with `allocsize/size/initsize/skiplen` exactly as tsk_fs.h:277-306.
6. **NTFS module order:** boot parse (signed mft_rsize rule, ntfs.c:5390-5407) → $MFT bootstrap via arithmetic for entries ≤16 then $Data run-list lookup (ntfs.c:189-300) → fixup on every record/INDX (ntfs.c:370-465) → attribute walk with runlist decoder (ntfs.c:596-804) → $I30 *linear* INDX scan for names including slack (ntfs_dent.cpp:1085-1273) → orphan pass. Sequence rule "compare name seq against seq-1 for deleted entries" is mandatory for correct deleted-name binding (ntfs.c:3101-3120).
7. **FAT/exFAT module order:** boot sector with 0/6/12 fallbacks (fatfs.c:80-101) → slot-based inode addressing (tsk_fatfs.h:94-102) → dentry parse with LFN reassembly (fatxxfs_dent.c:166-203) → deleted detection = 0xE5 ∨ unalloc-sector (fatxxfs_dent.c:410-424) → chain walk for live, contiguous-recovery for deleted (fatfs_meta.c deleted branch) with a distinct `Recover` error class surfaced as "best effort" in UI (mirrors icat -R).
8. **Products to expose in UI:** mmls table (slot/start/end/length/description + `-B` human size, mmls.cpp:106-117), fls listing with `-d/-u/-r/-p` flag semantics and NTFS ADS rows (fls.cpp:143-230), icat stream extraction by `inum[-type[-id]]`, blkls-equivalent unallocated/slack dumps with the three flag flavors (blkls.cpp:89-91, dls_lib.c:112-200), ils inode table with `a|f` alloc column, USN journal timeline (tsk_ntfs.h:617-661).
9. **License wall:** CPL-1.0 on all of the above-mentioned C sources — clean-room reimplementation only; the algorithmic semantics documented here (constants, layout, heuristics) are the transferable asset.

### Tool flag cross-reference (quick table)

| Tool | Default selection | Deleted-only | Extra semantics |
|---|---|---|---|
| fls | ALLOC+UNALLOC names, dirs+files | `-d` (clear ALLOC) | `-r` recurse, `-p` full path (auto with `-r`+unalloc-only), `-l` long, `-m` mactime, `-a` show dot entries, `-F/-D` filter files/dirs |
| icat | default attribute of inode | recovery automatic | `inum[-typ[-id]]` picks NTFS stream; `-s` slack; `-h` no sparse; `-R` suppress recovery errors |
| blkls | UNALLOC+META+CONT blocks | that IS the default | `-a` allocated, `-e` all, `-l` list format, `-s` slack-only (per-file; per-attribute for NTFS) |
| ils | used inodes (USED filter on by default) | `-L` (nlink==0, TSK_FS_ILS_UNLINK) | `-l` allocated/link filter, `-e` all incl. never-used (clears USED, sets ALLOC+UNALLOC — ils.cpp:159-163), `-O` open files (UNALLOC+LINK), `-m` mactime |
| mmls | all partition flags (0x07) | `-A` unallocated rows only | `-B` byte-rounded size column; `-r` DOS recurse; `-M` hide meta |

icat address grammar (from usage string icat.cpp:37): `image inum[-typ[-id]]` — e.g. `icat disk.e01 45-128-3` extracts attribute type 0x80 ($DATA) id 3 of MFT entry 45; type/id tables are `TSK_FS_ATTR_TYPE_ENUM` values from tsk_fs.h:224-260.

---

## Licensing

From `licenses/` (8 files: IBM-LICENSE, cpl1.0.txt, Apache-LICENSE-2.0.txt, GNUv2-COPYING, GNUv3-COPYING, mit.txt, bsd.txt, README.md) and `README.md` §LICENSE (README.md:150-165):

- **Core C/C++ library (`tsk/`), tools, mactime modifications:** Common Public License 1.0 (file headers, e.g. tsk/img/tsk_img.h:8, tsk/fs/ntfs.c:11, tools/vstools/mmls.cpp:11). TCT-derived files (fs_open.c, icat_lib.c, ils_lib.c, dls_lib.c) additionally carry **IBM Public License 1.0** headers (e.g. fs_open.c:17-25, dls_lib.c:22-31).
- README.md:156-158: file system tools are "IBM open source license and Common Public License"; srch_strings and fiwalk are "GNU Public License"; other tools CPL.
- **Apache-2.0:** `bindings/java/src/org/sleuthkit/datamodel/*`, `case-uco/*`, `rejistry++/*` (licenses/README.md).
- **GPLv2:** `tools/srchtools/srch_strings.c` (licenses/README.md; GNUv2-COPYING header "License for srch_strings").
- **GPLv3+:** `m4/ax_pthread.m4` (licenses/README.md).
- **MIT:** `tsk/auto/guid.cpp` (Graeme Hill, licenses/mit.txt).
- **3-Clause BSD:** `tsk/fs/lzvn.c` (Apple), `samples/*` (licenses/bsd.txt).
- **Public domain:** `tsk/base/crc.c/h` (Ross Williams), `tsk/base/sha1c.c`, `tsk/base/XGetopt.c`, `tools/fiwalk/*` + plugins.
- **RSA MD5 license:** `tsk/base/md5c.c`. **Unicode license:** `tsk/base/tsk_unicode.c/h`. **SQLite blessing:** `tsk/auto/sqlite3.c/h`. **FSF unlimited-copy Makefile notice:** generated Makefile.in. m4 boilerplate (ax_check_openssl, ax_jni_include_dir, ax_cxx_compile_stdcxx): allpermissive.
- **Practical takeaway for the target product:** the img/vs/fs framework and the NTFS/FAT parsers are CPL-1.0 — a reciprocal-style OSI license *not* compatible with closed-source relicensing; reimplement the algorithms, don't copy the code. Apache-2.0 parts (rejistry++, java datamodel) are safe to study but also not to copy verbatim into a proprietary product. lwzvn/lzvn.c is BSD (permissive).

---

## Coverage

**Read fully (whole file, line-by-line):**
- tsk/img: tsk_img.h (405), img_open.cpp (580), img_io.c (278), img_types.c (169), mult_files.c (216), raw.h (54)
- tsk/vs: tsk_vs.h (527), mm_open.c (251), mm_part.c (284), tsk_dos.h (49), tsk_gpt.h (76)
- tools/vstools: mmls.cpp (335)
- tsk/fs framework: fs_open.c (377), tsk_fs.h (3114), fls_lib.c (264), icat_lib.c (98), dls_lib.c (244), ils_lib.c (285)
- NTFS headers: tsk_ntfs.h (789)
- FAT headers: tsk_fatfs.h (377), tsk_fatxxfs.h (251), tsk_exfatfs.h (411)
- FAT dent: fatxxfs_dent.c (432)
- licenses/README.md; README.md §LICENSE (148-175); license file headers as listed in §Licensing

**Read partially (named functions/sections):**
- tsk/img: raw.c (raw_read 276-395 + struct greps), ewf.cpp (header + open paths 1-60, greps), aff.c / vmdk.c / vhd.c (greps only, external-lib confirmation), unsupported_types.c (40-175)
- tsk/vs: dos.c (dos_get_desc table skimmed 1-450; dos_load_ext_table/dos_load_prim_table/tsk_vs_dos_open 670-1149 full), gpt.c (gpt_load_table 1-120)
- tsk/fs NTFS: ntfs.c (ntfs_dinode_lookup 144-468; ntfs_make_data_run 584-804; compression block 808-823; ntfs_inode_lookup + ntfs_dinode_copy excerpts 2920-2980/3040-3120; process_kape_boot_format 5208-5243; ntfs_open 5243-5577), ntfs_dent.cpp (114-673 helper functions incl. ntfs_dent_copy/ntfs_proc_idxentry/ntfs_fix_idxrec; ntfs_dir_open_meta 774-1273)
- tsk/fs FAT: fatfs.c (fatfs_open 36-152 full), fatfs_meta.c (fatfs_make_data_run deleted-file branch excerpts), exfatfs_dent.c (160-200), fs_dir.c (tsk_fs_dir_find_orphans, tsk_fs_dir_load_inum_named, save_inum_named_cb)
- tools/fstools: fls.cpp (usage + option logic 1-120, 255-290, 400-410), icat.cpp (usage 30-75 + option switch 100-215 + parse 208-212), blkls.cpp (usage 31-60 + defaults 70-117)
- tsk/util: detect_encryption.c (detectors 30-150 + result plumbing)
- Structural greps (magic numbers, deleted detection lines, struct names only — not full reads): ext2fs.c/tsk_ext2fs.h, hfs.c/tsk_hfs.h, iso9660.c/tsk_iso9660.h, yaffs.cpp/tsk_yaffs.h, ffs.c/tsk_ffs.h, apfs_open.cpp, ext2fs_journal.c, tsk/pool/{tsk_pool.h,pool_types.cpp,pool_open.cpp}
- tsk/base, tsk/auto, tsk/hashdb, tsk/docs, tsk/img/img_writer.*, logical_img.c/h: enumerated only

**Enumerated only (file lists, not read):** bindings/java (220 files), case-uco (80), rejistry++ (62), win32 (81), man (30), tools/fiwalk (46), tools/logicalimager (49), tools/{autotools,hashtools,srchtools,timeline,sorter,imgtools,pooltools}, tsk/util/Bitlocker (26), tsk/auto (18), tsk/hashdb (14), tsk/pool remainder, tests/test/unit_tests (17), debian/m4/release/xcode/samples/docs (≈40), root build files.

**Anomalies / caveats:**
- `tsk/img/ewf.cpp` implements `tsk_img_verify` MD5 re-hash for E01 only (img_io.c:271-273); other formats return `TSK_IMG_VERIFY_UNSUPPORTED`.
- icat `-r` is a no-op (recovery automatic) while `-R` only suppresses errors — a semantics change vs historical TSK docs (icat.cpp:186-188).
- `mmls -r` recursion is DOS-only, one level, ≤64 partitions (mmls.cpp:298-321).
- `dos_load_prim_table` deliberately ignores CHS fields and the "partition ends after image" check is `#if 0`'d (dos.c:1009-1018).
- NTFS MFT-magic check disabled `#if 0` (ntfs.c:354-369) — fixup is the real corruption gate.
- No AFF/EWF/VMDK/VHD code is bundled — those formats exist only when the external libs are compiled in (configure.ac:171-176).
- [UNVERIFIED] Solaris VTOC specifics, BSD/MAC/SUN partition struct layouts beyond headers; [UNVERIFIED] ext2fs journal internals beyond the big-endian macro block (ext2fs_journal.c:16-22); [UNVERIFIED] HFS+ catalog B-tree traversal details (hfs_dent.c not read); [UNVERIFIED] ISO9660 path-table resolution (iso9660_dent.c not read); [UNVERIFIED] APFS object model (apfs.cpp/apfs_fs.cpp not read).

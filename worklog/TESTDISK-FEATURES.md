# TESTDISK & PHOTOREC — Feature Harvest Report (Agent D)

| Field        | Value                                                                                                                                                                                                           |
| ------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Repo         | https://github.com/cgsecurity/testdisk (depth-1 clone at `/home/z/my-project/refs/testdisk`)                                                                                                                    |
| HEAD SHA     | `5f5d6056696d70d3a3ff1e1eaa5a5125cb42bb6a`                                                                                                                                                                      |
| License      | **GPLv2-or-later** — `COPYING` is GNU GPL Version 2 (June 1991); every source header states "version 2 of the License, or (at your option) any later version" (e.g. `src/parti386.c:7-9`, `src/photorec.c:7-9`) |
| Stack        | C (ncurses CLI + optional Qt GUI `src/qphotorec.cpp`), autotools; 793 tracked files (724 in `src/`)                                                                                                             |
| Date / Agent | 2026-11-12 / Agent D (TestDisk/PhotoRec harvest)                                                                                                                                                                |
| Mission      | Partition-reconstruction & file-carving capabilities for heicfast/d-space_fork (Tauri 2 + Rust + React, Windows)                                                                                                |

---

## 1. Codebase Map & Classification

Full enumeration via `git ls-files` (793 files; 724 under `src/`, 69 outside). Classification of `src/` by priority-pattern match over filename:

| Class                                         | Count   | Representative files                                                                                                                                                                                                                                                                                                                                                 |
| --------------------------------------------- | ------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| PhotoRec signature validators `src/file_*.c`  | **353** | `file_jpg.c`, `file_png.c`, `file_zip.c`, `file_doc.c`, `file_txt.c` (largest)                                                                                                                                                                                                                                                                                       |
| Validator companion headers `src/file_*.h`    | 8       | `file_jpg.h`, `file_gz.h`, `file_doc.h`, `file_riff.h`, `file_tar.h`, `file_tiff.h`, `file_sp3.h`, `file_found.h`                                                                                                                                                                                                                                                    |
| PhotoRec core engine                          | 42      | `photorec.c`, `phrecn.c`, `phbf.c`, `phbs.c`, `phcfg.c`, `phcli.c`, `phmain.c`, `psearchn.c`, `sessionp.c`, `filegen.c/h`, `pnext.h`, `pblocksize.c`, `json_log.c`, `fidentify.c`, `file_found.c`                                                                                                                                                                    |
| Partition subsystem core                      | 63      | `parti386.c`, `partgpt.c`, `partgptn.c`, `partgptro.c`, `partgptw.c`, `partmac/partsun/partxbox/parthumax/partnone/partauto` (27 `part*` files), `analyse.c`, `godmode.c`, `geometry.c/geometryn.c`, `fnctdsk.c`, `next.c`, `crc.c`, `savehdr.c`, `chgtype*.c`, `addpart*.c`, `tpartwr.c`, `tanalyse.c`, `bsd.c`                                                     |
| Filesystem-specific modules                   | 126     | `fat*.c/h` (22 files), `ntfs*` (18 files incl. `ntfs_fix.c`, `ntfs_dir.c`, `ntfs_adv.c`), `ext2*` (15), `exfat*`, `hfs*`/`hfsp*`, `rfs*`, `xfs`, `jfs`, `ufs`, `bfs`, `zfs`, `btrfs`, `f2fs`, `apfs*`, `luks*`, `lvm*`, `md.c`, `swap.c`, `hpfs.c`, `refs.c`, `iso.c`, `vmfs.c`, `wbfs.c`, `netware.c`, `cramfs.c`, `sysv.c`, `sun.c`, `fat_adv.c`, `fat_unformat.c` |
| UI/CLI (ncurses menus, editors, dir browsers) | 40      | `testdisk.c`, `intrf.c`, `intrfn.c`, `intrface.c`, `tdiskop.c`, `tdisksel.c`, `tlog.c`, `toptions.c`, `tdelete.c`, `tload.c`, `tbanner.c`, `tmbrcode.c`, `edit.c`, `hidden*.c`, `nodisk.c`, `dirn.c`, `dirpart.c`, `lang.h`                                                                                                                                          |
| Disk I/O layer                                | 25      | `hdaccess.c`, `hdwin32.c`, `win32.c`, `hdcache.c`, `ewf.c` (E01 images), `dimage.c`, `hpa_dco.c`, `io_redir.c`, `diskacc.c`, `diskcapa.c`, `sudo.c`, `suspend*.c`                                                                                                                                                                                                    |
| Qt GUI / icons / l10n                         | 35      | `qphotorec.cpp`, `qmainrec.cpp`, `qphbs.cpp`, `qpsearch.cpp`, `src/lang/qphotorec.*.ts` (15), 9 `.png`, 3 `.rc`, 2 `.qrc`                                                                                                                                                                                                                                            |
| Common/misc/utility                           | 32      | `common.c/h`, `misc.c`, `log.c`, `log_part.c`, `unicode.c`, `utfsize.c`, `setdate.c`, `dfxml.c`, `list_sort.c`, `types.h`, `memmem.h`, `guid_cpy.h`, `guid_cmp.h`, `list_add_sorted*.h`                                                                                                                                                                              |

Non-`src/` (69 files): build/packaging 31 (`m4/` 11, `configure.ac`, `autogen.sh`, `Makefile.am`, `compile.sh`, 3 `.spec`, `Android.mk`, `jni/Android.mk`, 2 `progsreiserfs-*.patch`, `blacklist_cfi.txt`, `.travis.yml`, `.whitesource`, `.git*` meta, `sonar-project.properties`); docs+license 20 (`README.md`, `INSTALL`, `INFO`, `NEWS`, `ChangeLog`, `AUTHORS`, `THANKS`, `README_dev_photorec.txt`, `COPYING`, `documentation.html`, `man/` 10); platform packaging 16 (`win/` 5, `dos/` 2, `darwin/` 2, `linux/` 2, `icons/` 5); other root 2 (`doc`, `config/config.rpath`).

## 2. Partition Subsystem (TestDisk)

### 2.1 MBR (PC/Intel) parsing — `src/parti386.c` (1861 lines, read fully)

**On-disk layout.** `struct partition_dos` = 16 bytes (`src/parti386.c:82-93`): `boot_ind` (0x80 active), CHS start (`head`,`sector`,`cyl`), `sys_ind` (type byte), CHS end, LE32 `start4` (relative sector), LE32 `size4` (sector count). Entries live at `0x1BE` (`#define TAB_PART 0x1BE`, `pt_offset(b,n)` macro `src/parti386.c:65,77-80`). CHS decoding macros (`src/parti386.c:96-99`): `s_cyl(p)=cyl | ((sector<<2)&0x300)` (10-bit cylinder), `s_sect(p)=sector&0x3f`. LBA helpers `get_start_sect`/`get_nr_sects` = `read4_little_endian` over `start4`/`size4` (`src/parti386.c:407-428`).

**Read path.** `read_part_i386` (`src/parti386.c:529-593`): read sector 0 → `get_geometry_from_i386mbr` requires the 0x55AA endmark at `0x1FE/0x1FF` (`src/parti386.c:457-460`) else `msg_TBL_NMARK` ("Partition sector doesn't have the endmark 0xAA55", `src/lang.h:28`); for each of the 4 entries with `sys_ind!=P_NO_OS` build a `partition_t` via `i386_entry2partition`; then `test_MBR_data`, `test_MBR_over`, `get_ext_data_i386` (EBR walk, below), `get_geometry_from_list_part`.

**Geometry from MBR** (`get_geometry_from_i386mbr`, `src/parti386.c:448-501`): takes the max end-CHS over the 4 entries to propose `cylinders/heads_per_cylinder/sectors_per_head`; the proposal is _trusted only_ when `sectors_per_head==32` or `(sectors_per_head==63 && heads∈{16,32,64,128,240,255})`; otherwise all geometry fields are zeroed with the comment "Don't trust the geometry" (`src/parti386.c:489-499`).

**Entry → partition.** `i386_entry2partition` (`src/parti386.c:1222-1309`): `part_offset = base_offset + get_start_sect(p)*sector_size` (base = 0 for primary entries, the EBR's own offset for logicals); `part_size = get_nr_sects(p)*sector_size`; extended types (0x05/0x0F/0x85 — `src/common.h:53,62,75`) promote to `STATUS_EXT`; `boot_ind!=0` → `STATUS_PRIM_BOOT`. CHS sanity → error codes `BAD_SS/BAD_ES/BAD_SH/BAD_SC/BAD_EH/BAD_EC` (sector/head/cylinder out of range) and `BAD_RS/BAD_SCOUNT` when the stored CHS start/end disagrees with the LBA-derived position (`src/parti386.c:1260-1305`), rendered by `errmsg_i386_entry2partition` (`src/parti386.c:1311-1328`).

**CHS<->LBA arithmetic.** `C_H_S2offset = ((C*heads+H)*spt + S-1)*sector_size` (`src/parti386.c:380-384`); the reverse `offset2CHS_inline` in `src/godmode.c:70-77`. Cylinders >1023 are stored as the `(1023,lastH,lastS)` convention with a comment contrasting Partition Magic 5 vs Linux fdisk vs TestDisk (`src/parti386.c:1191-1215`).

**Type name table.** `i386_sys_types[]` ~90 entries (`src/parti386.c:262-357`): 0x01 "FAT12", 0x05 "extended", 0x07 "HPFS - NTFS", 0x0B "FAT32", 0x0C "FAT32 LBA", 0x0F "extended LBA", 0x0E "FAT16 LBA", 0x83 "Linux", 0x82 "Linux Swap", 0x85 "Linux extended", 0xEE "EFI GPT", 0xEF "EFI (FAT-12/16/32)", 0xFD "Linux RAID", etc. The `arch_i386` vtable (`src/parti386.c:359-378`) exposes read_part/write_part/init_part_order/get_geometry_from_mbr/check_part/write_MBR_code/set_prev_status/set_next_status/test_structure/init_structure/erase_list_part/get_partition_typename.

**Structural sanity tests.**

- `test_MBR_data` (`src/parti386.c:595-648`): at most one bootable (`msg_NO_BOOTABLE` "No partition is bootable" / `msg_ONLY1MUSTBOOT`); at most one extended (`msg_ONLY_ONE_EXT`); ≤1 visible DOS FAT primary; obsolete hidden-partition/OS2-MB check commented out.
- `test_MBR_over` (`src/parti386.c:832-846`): adjacent list entries must not overlap → `msg_SAME_SPACE` "Space conflict between the following two partitions".
- `test_structure_i386` (`src/parti386.c:1353-1398`): ≤4 primary+ext combined; exactly one contiguous logical block; logical entries never extended-typed; sorted-list overlap check via `is_part_overlapping`.

**Status cycling.** `set_next_status_i386`/`set_prev_status_i386` (`src/parti386.c:1513-1544`) cycle DELETED↔PRIM↔PRIM_BOOT↔LOG using `parti386_can_be_ext` (logical allowed unless the partition is the first head/cyl — `src/parti386.c:1345-1351`).

### 2.2 EBR / extended-partition chain walk — `get_ext_data_i386` (`src/parti386.c:661-803`)

1. Locate the primary `STATUS_EXT` partition (`get_ext_partition_i386`, `src/parti386.c:650-659`). No extended → return.
2. For each EBR sector (chain hard-capped at `nbr_part<32`, `src/parti386.c:671`): read at `partition_ext->part_offset`; require 0x55AA at 0x1FE else stop with "test_logical: … endmark" message.
3. Each EBR's 4 entries are classified; per-EBR counters enforce logical rules with user-visible messages: hidden entries ("Partition must not be hidden"), multiboot entry ("Multiboot must be a primary partition, not a logical"), >1 link ("A logical partition must not have more than one link to another logical partition"), >1 data entry ("A logical partition must contain only one partition"), bootable logical ("Logical partition must not be bootable") — `src/parti386.c:687-722`.
4. Extended-typed entry becomes `STATUS_EXT_IN_EXT` (link); it must lie strictly inside the primary extended container else "Must be in extended partition"; links must not overlap other links ("Logical partition must be in its own extended partition") — `src/parti386.c:733-767`. Non-extended entries become `STATUS_LOG` with `order` starting at 5 (`src/parti386.c:771-772`).
5. `recover_i386_logical` (`src/parti386.c:805-830`): during deep scan, an EBR whose data entry is FAT/NTFS-typed is itself a partition candidate (used to catch TrueCrypt-in-logical, `src/analyse.c:228-234`).

**EBR write path.** `write_all_log_i386` (`src/parti386.c:936-1085`): each EBR gets entry 0 = the logical partition and entry 1 = pointer to the next EBR, always typed `P_EXTENDED` ("Never P_EXTENDX", `src/parti386.c:968`); the next-EBR start is `next_partition.part_offset - sector_size`, snapped to sector 1 of the previous head/cylinder when possible (`src/parti386.c:1017-1054`); `partition2_i386_entry` (`src/parti386.c:1177-1220`) writes both CHS and the LE32 relative start/size. The MBR write path `write_mbr_i386` (`src/parti386.c:857-934`) preserves the existing boot code when the 0x55AA mark is valid (otherwise installs `mbr_code_testdisk[]`, an embedded bootstrap credited to Neil Turton, `src/parti386.c:1132-1175`, that preserves the disk signature at 0x1b8 by copying only `[0,0x1b8)` — `src/parti386.c:1170-1173`), and strips Mac ("ER" at 0) / Sun (0xdabe at 0x1fc) signatures.

**Erase.** `erase_list_part_i386` (`src/parti386.c:1692-1739`) zeroes the 0x40 bytes of MBR entries and additionally wipes Xbox ("BRFR" magic, 0x800 bytes) and GPT ("EFI PART", LBA1) signatures so other tools aren't confused by leftovers.

### 2.3 GPT parsing — `src/partgpt.c` (623 lines, read fully)

**Header validation** (`read_part_gpt_aux`, `src/partgpt.c:214-376`), in order:

1. Read `sector_size` bytes at `hdr_lba * sector_size`; require `hdr_sig == "EFI PART"` (`GPT_HDR_SIG`, `src/partgpt.h:32`) else "Bad GPT partition, invalid signature."
2. `hdr_size` must be ≥92 and ≤ sector_size ("GPT: invalid header size.").
3. **CRC32 check**: zero `hdr_crc_self`, compute `get_crc32(gpt, hdr_size, 0xFFFFFFFF)^0xFFFFFFFF`, compare with the stored value ("Bad GPT partition, invalid header checksum.") — `src/partgpt.c:259-272`.
4. `hdr_lba_self == hdr_lba` ("invalid LBA self location"); `hdr_lba_start < hdr_lba_end`.
5. Revision warning if `hdr_revision != 0x00010000` (`GPT_HDR_REVISION`, `src/partgpt.h:34`) — "GPT: Warning - not revision 1.0"; `__reserved!=0` warning.
6. `hdr_entries` in [1,4096]; `hdr_entsz` multiple of 8, in [128,4096]; entries array size ≥ 16384 bytes ("A minimum of 16,384 bytes of space must be reserved for the GUID Partition Entry array."); primary array must sit after the header and before first usable LBA.

**Backup header.** `read_part_gpt` (`src/partgpt.c:378-389`): if the primary (LBA 1) parse fails, prints "Trying alternate GPT" and retries at `(disk_size-1)/sector_size`.

**Entries.** Read `hdr_entries*hdr_entsz` bytes at `hdr_lba_table*sector_size`; **CRC32 of the whole array** vs `hdr_crc_table` ("Bad GPT partition entries, invalid checksum.") — `src/partgpt.c:327-344`. Each `gpt_ent` with non-UNUSED type GUID and `ent_lba_start < ent_lba_end` becomes a partition: `part_offset = ent_lba_start*sector_size`, `part_size = (ent_lba_end - ent_lba_start + 1)*sector_size`, order=i+1, `ent_uuid`→`part_uuid`, `ent_type`→`part_type_gpt`, name via `UCSle2str` (`src/partgpt.c:345-369`). Type names from `gpt_sys_types[]` (~50 GUIDs: EFI System, MS Data, MS Reserved, Linux swap/LVM/RAID, Mac HFS/APFS, Solaris, HPUX, BeFS… — `src/partgpt.c:135-185`). A TODO notes the backup-entries placement check is unimplemented (`src/partgpt.c:370-372`).

**FS dispatch.** `check_part_gpt` (`src/partgpt.c:527-593`): MS Basic Data/Reserved → `check_FAT`→`check_exFAT`→`check_NTFS`→`check_ReFS`→`check_linux`; Linux RAID → `check_MD`; Linux LVM → LVM/LVM2; Mac HFS → HFS/HFSP; Mac APFS → `check_APFS`; BeFS. Statuses `test_structure_gpt` = overlap check only; `set_next_status_gpt` toggles DELETED↔PRIM.

**Other tables present** (read/verified as existing; only signature sites verified from `analyse.c`): Sun `src/partsun.c` (480 l.), Mac `src/partmac.c` (478), BSD `src/bsd.c` (174 — probed at +0x200 via `DISKMAGIC` in `search_type_1`), Xbox `src/partxbox.c` (392), Humax `src/parthumax.c` (365), "none" `src/partnone.c` (577 — non-partitioned media, used by PhotoRec whole-disk and by autodetect). [Enumerated only otherwise.]

**Partition-table auto-detection** — `autodetect_arch` (`src/partauto.c:57-168`): tries arch_none (whole-disk filesystem), xbox, gpt, humax, i386, sun, mac in that order, logging "Partition table type (auto): %s"; on no match, defaults by OS and size — i386 below 2 TiB (2^32 512-byte sectors), GPT above (`src/partauto.c:156-161`).

### 2.4 Partition-type detection by filesystem signature

`check_part_i386` (`src/parti386.c:1741-1844`) maps `sys_ind` → checker with fallback chains and user messages: FAT family → `check_FAT` ("Invalid FAT boot sector"); P_NTFS → `check_NTFS` then `check_exFAT` ("Invalid NTFS or exFAT boot"); P_LINUX → `check_linux` (JFS/Reiser/EXT2/cramfs/XFS/LUKS/btrfs/f2fs/gfs2/ZFS — `src/analyse.c:442-458`, "No ext2, JFS, Reiser, cramfs or XFS marker"); P_HFS → HFS then HFSP; P_LINSWAP → `check_Linux_SWAP`; P_LVM → LVM/LVM2 ("No LVM or LVM2 structure"); P_RAID → `check_MD` ("Invalid RAID superblock"); P_FREEBSD/P_NETBSD/P_OPENBSD → `check_BSD` ("Invalid BSD disklabel"); P_SUN → `check_sun_i386`; extended types skip checking. Unknown types are logged "check_part_i386 %u type %02X: no test" and optionally dumped to header files (`save_header`).

The deep-scan probe set (`src/analyse.c`, read fully) is the master signature list:

- `search_type_0` (boot-sector at candidate+0): APFS `nx_magic==0x4253584e`; Linux swap "SWAP" (both at 0 and 8K offsets); LVM1 `LVM_ID`; FAT/exFAT/HPFS/OS2MB/NTFS `marker==0xAA55`; NetWare "Nw_PaRtItIoN" (12 bytes); XFS `XFS_SB_MAGIC`; FATX "FATX"; LUKS "LUKS\\xba\\xbe"; ReFS `fsname==0x52654653`; Linux MD (v1.x `major_version==1`, adjusts part_offset by `super_offset*512`); WBFS "WBFS"; cramfs `CRAMFS_MAGIC` — `src/analyse.c:143-236`.
- `search_type_1` (+0x200): BSD `d_magic==DISKMAGIC` (×2), BeFS `SUPER_BLOCK_MAGIC1`, cramfs, sysv `0xfd187e20`, LVM2 label, Sun-on-i386 `SUN_LABEL_MAGIC_START` — `src/analyse.c:238-277`.
- `search_type_2` (+0x400): ext2/3/4 `EXT2_SUPER_MAGIC`, HFS `HFS_SUPER_MAGIC`, HFS+ (`version==4|5`), f2fs `F2FS_SUPER_MAGIC` — `src/analyse.c:279-309`.
- `search_type_8` (MD v1.2 at +4KiB, `src/analyse.c:311-332`), `_16` (UFS/UFS2/ZFS at +16×512, `src/analyse.c:334-359`), `_64` (JFS "JFS1" at ~+32K, `src/analyse.c:361-381`), `_128` (ReiserFS "ReIs"/Reiser4/UFS2/btrfs `BTRFS_MAGIC`/gfs2 at +64K, `src/analyse.c:383-420`), `_2048` (VMFS `0xc001d00d` at +1MiB, `src/analyse.c:422-440`).

## 3. Partition Repair Heuristics

### 3.1 Backup boot sectors

**Scan side.** `search_NTFS_backup` (`src/analyse.c:70-84`): read at candidate offset; `0xAA55` marker + `recover_NTFS(..., backup=1)` — recover_NTFS sets `partition->sb_offset` so a partition found via its _backup_ boot sector can later be fixed to its true start. `search_FAT_backup` (`src/analyse.c:129-141`, FAT32 backup BS), `search_exFAT_backup` (`src/analyse.c:112-127`, "part_offset has already been updated if found using backup sector"), `search_HFS_backup` (`src/analyse.c:86-110`, HFS MDB / HFS+ VH, sets `partition->info` "HFS found using backup sector!").

**Interactive repair (NTFS).** `ntfs_boot_sector` (`src/tntfs.c:203-303`, read fully): `ntfs_boot_sector_scan` reads boot sector at `part_offset` and backup at `part_offset+part_size-sector_size`, evaluates both with `test_NTFS` → "Status: OK"/"Status: Bad", compares bytes ("Sectors are identical."/"Sectors are not identical."), and always reminds "A valid NTFS Boot sector must be present in order to access any data; even if the partition is not bootable." Menu (options string computed from which side is valid, e.g. "DOBRL"): 'D' dump side-by-side (`dump2`), 'L' list files — **transparently IO-redirects the boot sector to the backup buffer when only the backup is valid** (`io_redir_add_redir`, `src/tntfs.c:282-291`), 'O' copy original over backup (`pwrite` at end-of-partition, confirm "Copy original NTFS boot sector over backup boot, confirm ? (Y/N)"), 'B' copy backup over boot (confirm "Copy backup NTFS boot sector over boot sector, confirm ? (Y/N)"; resets `partition->sb_offset=0`), 'R' rebuild BS, 'M' repair MFT.

**Post-write fix-ups.** `use_backup` (`src/godmode.c:1302-1334`): after a table write, for every partition with `sb_offset!=0` dispatches `fat32_boot_sector` (UP_FAT32), `ntfs_boot_sector` (UP_NTFS), `HFS_HFSP_boot_sector` (UP_HFS/HFSP/HFSX) — i.e., the found-via-backup partitions get the interactive boot-sector repair prompt automatically.

### 3.2 NTFS $MFT / $MFTMirr repair — `repair_MFT` (`src/ntfs_fix.c:53-320`, read fully)

- Preconditions: `check_NTFS` must pass ("Boot sector not valid, can't repair MFT."); boot sector read; `mft_pos`/`mftmirr_pos` computed from `reserved + mft_lcn/mftmirr_lcn * sectors_per_cluster * ntfs_sector_size`; `mft_record_size` from `clusters_per_mft_record` (positive → clusters, negative → `1<<(-value)`); `mftmirr_size_bytes = max(cluster_size, 4*mft_record_size)` (`src/ntfs_fix.c:80-97`).
- If MFT and mirror are byte-identical → "MFT and MFT mirror match perfectly." If the partition was itself found via a backup boot sector (`sb_offset!=0`), refuse: "Please quit TestDisk and reboot your computer before trying to fix the MFT." (`src/ntfs_fix.c:143-151`).
- **The core trick** (comment `src/ntfs_fix.c:159-164`): use TestDisk's internal IO redirector to redirect reads of the MFT to the MFT mirror (and vice versa) while listing files — if a listing succeeds, that copy is the good one. Implemented as: `io_redir_add_redir(disk, mftmirr_pos, size, 0, buffer_mft)` + `dir_partition_ntfs_init` + `get_dir` + `delete_list_file(&dir_list)>2` counts res1; then the mirrored run for res2 (`src/ntfs_fix.c:165-213`).
- Decision + confirmations: res1>res2 → "Fix MFT mirror using MFT ? (Y/N)"; res2>res1 → "Fix MFT using its mirror ? (Y/N) - DANGEROUS NON REVERSIBLE OPERATION / Use it ONLY IF TestDisk and Windows failed to access this filesystem."; tie → "Both MFT seems ok but they don't match, use chkdsk." or "MFT and MFT mirror are bad. Failed to repair them." (expert mode gives a manual B/M menu, `src/ntfs_fix.c:215-290`).
- Execution: raw `pwrite` of the chosen buffer over the other region + `disk->sync` ("MFT fixed."/"MFT mirror fixed."/"Failed to fix MFT: write error.").

### 3.3 Rebuild-BS flow (FAT) — `rebuild_FAT_BS` (`src/fat_adv.c:1897-2219`)

1. **FAT flavor pre-filter by partition size** (`src/fat_adv.c:1904-1920`): >~2 GiB → FAT32 only; ≥ (1+2*13+32768+63)*512 → FAT32+FAT16; else FAT16+FAT12.
2. **fat_length_max bound**: for FAT32 `part_size/min_cluster*4`; FAT16 doubles `sectors_per_cluster_min` until ≤2^16 clusters then `*2`; FAT12 `*3/2` (`src/fat_adv.c:1924-1952`).
3. **`fat_find_info`** scans candidates up to `max_offset=fat_length_max+64*sector` calling `fat_find_fat_start` (`src/fat_adv.c:1305+`): reads a candidate sector as if it were the FAT start and checks **chain consistency** of 12-bit (`(cluster-i-1)*3 % (2*sector_size)==0` — three byte-phase variants), and 16/32-bit analogues; each consistent observation bumps a per-(offset,fat_type) counter in `info_offset_t` (offset, nbr, fat_type).
4. **`select_fat_info`** (`src/fat_adv.c:1817-1896`): displays "Potential FAT location / FAT - sector - score", asks the user for FAT1 location (reserved sector count) and FAT2 location; `fats=1` if FAT2==FAT1; returns the FAT type whose candidate offset matches FAT2 (or FAT1).
5. Fallbacks when no FAT found: `find_sectors_per_cluster` + `no_of_cluster2part_type`; FAT1x `reserved=1`, `dir_entries=find_dir_entries(...)` (0 → default 512 with warning "dir_entries not found, should be 512"; unusual values logged "dir_entries: %u (unusual value)"), `fat_length=(start_data-reserved-root_sectors)/fats`; FAT32 `reserved=32(+1 if odd start_data)`, `fat_length=(start_data-reserved)/fats` (`src/fat_adv.c:2010-2070`).
6. **FAT1x root-dir re-derivation**: `analyse_dir_entries`/`analyse_dir_entries2` count valid 32-byte entries (`check_FAT_dir_entry` validates name chars/attributes — `src/fat_adv.c:100-155`).
7. **Cluster size**: `calcul_sectors_per_cluster` (power-of-two chosen from data size vs FAT width, `src/fat_adv.c:1286+`), clamped to {1,2,4,8,16,32,64,128}, expert override prompt "cluster size ".
8. **FAT32 root cluster**: `fat32_free_info` computes first free cluster; **`fat32_find_root_cluster`** (`src/fat_adv.c:307+`) scans clusters for a directory-entry structure to locate the root dir cluster; expert prompt "root cluster ".
9. **Synthesis + write**: `create_fat_boot_sector` builds the BPB; `menu_write_fat_boot_sector` (`src/fat_adv.c:918-1060`) shows extrapolated vs original (dump_2fat_info), messages "Extrapolated boot sector and current boot sector are different/identical" + "Warning: Extrapolated boot sector have incorrect values."; Write (confirm "Write FAT boot sector, confirm ? (Y/N)") writes: **FAT32 → 3 sectors at `part_offset` AND 3 sectors at `part_offset + backup_boot*sector_size`**; FAT1x → 1 sector (`src/fat_adv.c:1030-1046`), then `partition->sb_offset=0`. Comment: "Note that TestDisk doesn't repair the filesystem itself, use another utility for that" (`src/fat_adv.c:1055`).

Related (call-sites verified, bodies enumerated): `rebuild_NTFS_BS` (`src/ntfs_adv.c`, invoked by 'R' `src/tntfs.c:295-297`), `rebuild_EXFAT_BS` (`src/texfat.c`), `repair_FAT_table` (`src/fat_adv.c:2303`), `FAT_init_rootdir` (`src/fat_adv.c:2221`), FAT unformat (`src/fat_unformat.c`, `STATUS_UNFORMAT` reached via expert prompt "Try to unformat a FAT filesystem (Y/N)" `src/phrecn.c:225-229`).

### 3.4 Extended partition container reconstruction

`add_ext_part_i386` (`src/godmode.c:1130-1300`): removes existing extended entries, then computes the container around the logical run — "max" mode (nbr_entries==4 or max_ext) extends start to the previous primary's end rounded up (1MiB or cylinder boundary) and end to the next primary's start rounded down; "min" mode hugs the logicals with sector-1/head-0 alignment rules (`src/godmode.c:1185-1289`); type `P_EXTENDX` if end cylinder >1023 else `P_EXTENDED` (`src/godmode.c:1292`). Toggled by 'E' "Extd Part / Maximize/Minimize extended partition" (`src/tpartwr.c:52`, `src/godmode.c:1466-1469`).

### 3.5 Alignment & structure normalization before write

- `align_structure` / `align_structure_i386` (`src/godmode.c:94-152`): boundary preference 1MiB → cylinder (H*S) → head (S) → sector; a partition is only grown if it doesn't overlap the next.
- `init_structure_i386` (`src/parti386.c:1561-1690`): flags foreign-arch and overlapping partitions as removed; detects Vista layout (`part_offset%(2048*512)==0 && part_size%(2048*512)==0`); the largest contiguous logical run becomes STATUS_LOG; first non-swap/non-LVM primary becomes bootable; on residual failure everything degrades to STATUS_DELETED.
- `only_one_bootable` (`src/godmode.c:154-163`) demotes extra PRIM_BOOT.

## 4. Deep Scan / Lost-Partition Search

### 4.1 Quick vs Deeper flow

```
Analyse menu (src/tanalyse.c) ──'Q' Quick Search──> interface_recovery (src/godmode.c:1478)
   pass 1: search_part(fast_mode=0)  [current table read first: arch->read_part]
   → warning_geometry → align_structure → init_structure
   → ask_structure (user toggles P/L/D per found partition)
   → interface_write menu (src/tpartwr.c): P/N/Q/R/S/W/E
        'S' "Deeper Search — Try to find more partitions"  (offered while fast_mode<1)
             → fast_mode++ → loop back to search_part (src/godmode.c:1462-1465,1538)
        'W' "Write partition structure to disk" (confirm, then use_backup fix-ups)
        'E' maximize/minimize extended
```

`fast_mode` can reach 2 in code (`if((*fast_mode)<2) (*fast_mode)++`) but the gate `can_search_deeper=(*fast_mode<1)` (`src/godmode.c:1409`) means stock UI/CLI never offers 'S' twice — the `fast_mode>1` branches (all-heads scan, RAID superblock probing at every position) are dead-to-UI but present in code.

### 4.2 Candidate position generation (what makes the scan "not every sector")

- **Hint list** (`hint_insert`, dedup-sorted, `MAX_SEARCH_LOCATION 1024`, `src/godmode.c:372-385`): seeds from current known partitions (`src/godmode.c:563-570`) and `search_add_hints` (`src/godmode.c:387-443`): sector 2+16KiB (users choosing Intel for GPT disks), 2048-sector (Vista), CHS 0/1/1 and 0/2/1 (32, 63 sectors), cylinder-1 starts for 16/240/255-head geometries with ±1-head variants, last & second-to-last cylinder ends (NTFS backup), `(disk_size-512)` rounded down to a 1MiB boundary −512; GPT arch: NTFS-backup hint computed from the entries array size; Mac arch: 2*sector+16384 (Mac chosen for GPT disk).
- **Arithmetic progressions** (`search_location_init`, `src/next.c:112-213`): (i386, quick) H=0/1/2 S=1 every cylinder + 0 every 2048*512; (non-i386) every `location_boundary` (sector; Mac 4096; Sun cylinder); (fast_mode>0, i.e. Deeper) + S=7 stripes (FAT32 backup), S=13 (exFAT backup), `(boundary-512)` stripes (NTFS/HFS backup), `(2048-1)*512` every 1MiB, and **ext2 backup superblocks** `hd_offset=3*(1024<<log)*8*(1024<<log)` (+2 sectors for log=0) for log∈{0,1,2} (sparse-superblock group 3; comment cites e2fsprogs initialize.c, `src/godmode.c:754-779` / `src/next.c:186-211`). `search_location_update` returns the minimum next position over all progressions (`src/next.c:215-227`).
- **Per-position gates** (`src/godmode.c:652-659`): i386 `search_now = (S==1 && fast_mode>1) || (S==1 && H<=2) || (offset % 1MiB == 0)`; non-i386 `offset % location_boundary == 0`.

### 4.3 Probe ladder at each candidate (`search_part`, `src/godmode.c:586-941`)

`test_nbr` sequence: (0) Linux MD RAID superblock (8 sectors; `part_offset -= super_offset*512` or MD_NEW_SIZE_SECTORS for v0.9, `src/godmode.c:674-702`); (1) `search_FAT_backup` (gated S==7); (2) `search_exFAT_backup` (S==13); (3) `search_NTFS_backup` (last sector of track/1MiB); (4) `search_HFS_backup`; (5) ext2 backup superblock (3 loop variants); (6-8) `search_type_2/1/0` on a 16-sector read; (9) `search_type_8`; (10) `search_type_16`; (11) `search_type_64`; (12) `search_type_128` (after a cache-warming read at +63+16 sectors); (13) `search_type_2048`.

**Acceptance & scoring**: `res>0` → log + display; inserted as `STATUS_DELETED` if `is_part_known() && part_size>1 && part_offset>=min_location` and `pos_fin<=search_location_max`; otherwise into `list_part_bad` → end-of-scan screens "The hard disk (%s) seems too small! (< %s) / Check the hard disk size: HD jumper settings, BIOS detection..." (+ Win32 LBA48 hint "update Windows to support LBA48 (minimum: W2K SP4 or XP SP1)", `src/godmode.c:166-203`) and "The following partition(s) can't be recovered"; verbose logs "This partition ends after the disk limits. (start=…, size=…, end=…, disk end=…)" and "Partition not added." (`src/godmode.c:920-937`). Read error → "Read error at %lu/%u/%u (lba=%lu)" and skip to end (`src/godmode.c:842-852`).

**Post-hit continuation heuristics** (`src/godmode.c:894-909`): seeds = partition end, end+head_size, end rounded up to head boundary (+head); quick mode jumps the cursor to the partition end. MD-RAID found → for disk_factor 6..1 (Raid 0/1/5) seeds `try_offset_raid` hints at `MD_NEW_SIZE_SECTORS(...)` offsets (`src/godmode.c:866-881`, TODO: md 1.0).

**Interactive controls** (`src/godmode.c:591-636, 943-960`): stop → "Stop searching for more partitions ? (Y/N)" (INDSTOP_STOP → jump to next hint or quit); skip key → jump to next hint; '+' key → jump ~5% forward.

**Post-scan**: if `fast_mode>0`, `search_NTFS_from_backup` (`src/godmode.c:489-531`) walks up to 32 sectors below each NTFS partition found via backup (`sb_offset!=0`) to locate the true start sector. Geometry cross-check `warning_geometry` (`src/godmode.c:1336-1353`) uses `get_geometry_from_list_part` (`src/fnctdsk.c:481-503`): scores head counts {8,16,32,64,128,240,255} by counting partitions that start at S=1/H≤1 and end at H=heads−1 (`get_geometry_from_list_part_aux`, `src/fnctdsk.c:425-459`), warns "the current number of heads per cylinder is %u but the correct value may be %u." with the ncurses hint "You can use the Geometry menu to change this value / It's something to try if - some partitions are not found by TestDisk - or the partition table can not be written because partitions overlap." (`src/godmode.c:344-369`).

There is no numeric scoring/ranking of competing candidates — validation is binary (valid signature + bounds + no-overlap) and the user arbitrates in `ask_structure`.

## 5. PhotoRec Engine

### 5.1 Signature registry & dispatch

- Format descriptor `file_hint_t {extension, description, max_filesize, recover, enable_by_default, register_header_check}` (`src/filegen.h:51-59`). All 353 formats are listed in `array_file_enable[]` (`src/file_list.c:1089`, 353 entries; ends with a `compiletime_assert(sizeof(file_enable_t) != sizeof(array_file_enable), "No file format has been enabled")` guard, `src/file_list.c:2165-2170`).
- `init_file_stats` (`src/filegen.c:518+`) counts enabled formats, allocates `file_stats[]`, and calls each `register_header_check(&file_stats[i])`. Registration (`register_header_check`, `src/filegen.c:192-223`) creates `file_check_t{value,length,offset,header_check,file_stat}` and inserts sorted into `file_check_plist` by `file_check_cmp` (offset, then memcmp of value, longer-last — `src/filegen.c:114-151`).
- **Indexing** (`index_header_check`, `src/filegen.c:266-288`): builds `file_check_list` — a list of `file_check_list_t` buckets, each with a distinct `offset` and **256 sub-lists `file_checks[0..255]` indexed by the byte value at that offset** (`file_check_add_tail`, `src/filegen.c:159-185`). Scan-time dispatch is therefore O(1) per byte: `pos->file_checks[buffer[pos->offset]]` (see `src/photorec_check_header.h:198-215`, `src/phbs.c:122-141`, `src/phbf.c:137-150`).
- Constraints: `PHOTOREC_MAX_SIG_OFFSET 65535`, `PHOTOREC_MAX_SIG_SIZE 4095`, `PHOTOREC_MAX_BLOCKSIZE 32MiB`, `PHOTOREC_MAX_FILE_SIZE 2^41-1` (2TiB-1; DJGPP 2GiB-1) (`src/filegen.h:34-43`).
- Enable/disable: `reset_array_file_enable` restores `enable_by_default` (`src/phcfg.c:46-51`); persisted to `$HOME/.photorec.cfg` (Windows `%USERPROFILE%\photorec.cfg`, fallback `photorec.cfg`) as `<ext>,enable|disable` lines (`file_options_save/load`, `src/phcfg.c:180-244`).

### 5.2 Carving loop — `photorec_aux` (`src/psearchn.c:80-384`, read fully)

- **Search space**: ordered `alloc_data_t {start,end,file_stat,data}` list covering the carve region; `init_search_space` = whole partition clamped to disk size (`src/photorec.c:246-262`); `remove_used_space` subtracts FS-allocated clusters for FAT12/16/32 (`fat_remove_used_space`), exFAT, NTFS (libntfs/ntfs3g), ext2/3/4 (libext2fs) (`src/photorec.c:401-420`) — only in "freespace" mode (`src/phcli.c:193-196`).
- **Per block** (`blocksize` = params->blocksize, default sector_size; `READ_SIZE 1024*512` read-ahead, sliding `buffer_olddata`/`buffer` pair so `data_check` sees 2 consecutive blocks):
  1. `photorec_check_header` (`src/photorec_check_header.h:171-218`) runs the indexed registry against the current block — skipped entirely while carving a **tar** whose next 512-byte header is valid (`is_valid_tar_header`, `src/file_tar.c:100-108`; hook `src/phbs.c:114-118`, `src/photorec_check_header.h:180-194`).
  2. On hit → `photorec_header_found` (`src/photorec_check_header.h:101-155`): finish previous file (`file_finish2` — "A known header has been found, recovery of the previous file is finished"), copy the new `file_recovery`, `set_filename`, open `w+b` (Windows/Cygwin: `fopen_with_retry` retries after 1s/2s to defeat antivirus locks — `src/photorec_check_header.h:35-65`); on failure → `PSTATUS_EACCES`.
  3. Data path: `fwrite(block)`; `file_block_append` records the extent in `file_recovery->location` (merging contiguous runs, `file_block_add_to_file`, `src/photorec.c:1151-1172`) and removes it from search space (`file_block_remove_from_sp`, `src/photorec.c:1116-1144` — exits fatally if the block isn't found); `data_check(buffer_olddata, 2*blocksize, …)` returns `DC_CONTINUE/DC_STOP/DC_ERROR`; `file_size += blocksize`.
  4. `DC_STOP/DC_ERROR` → `file_finish2`; `DC_ERROR` zeroes file_size (file rejected). Write error: ENOSPC → `PSTATUS_ENOSPC`; EFBIG treated as EOF (`src/psearchn.c:186-204`).
  5. Stop conditions: data_check verdict; `max_filesize` exceeded ("File should not be bigger than %llu, stopped adding data"); FAT 4GiB cap `PHOTOREC_MAX_SIZE_32` when the partition is FAT (`src/psearchn.c:225-240`).
  6. ext2 mode: indirect blocks appended without data (`ind_block`, `src/psearchn.c:165-183`).
- **Fragmentation handling**: `file_finish2` → `file_block_truncate` returns over-read extents to search space and reports `PFSTATUS_OK_TRUNCATED`; the scan then re-seeks backwards via `get_prev_file_header` (≤3 fragments / 200MB, `src/photorec.c:338-371`) up to `back<5` times to retry the failed file, else `get_prev_location_smart` (`src/filegen.c:1067+`); `forget()` trims >10000 old entries in lowmem mode (`src/photorec.c:373-399`).
- **File completion** — `file_finish_aux` (`src/photorec.c:655-722`): if `paranoid>0` and `file_check` set → run it (post-hoc structural validation over the written file); clamp size to disk size; **reject `file_size < min_filesize`** ("%s File too small ( %llu < %llu), reject it"); zero-size → `fclose`+`unlink` (paranoid==2 keeps it for the BF pass); `ftruncate` to exact size; set mtime (`set_date`) from validator-parsed time; optional `file_rename`; every `MAX_FILES_PER_DIR 500` files → new output dir; `file_stat->recovered++`.
- **Brute-force fragmented recovery** (paranoid=2, `photorec_bf` `src/phbf.c:112-256`): iterates search space backwards; on `file_finish_bf()<0`, `photorec_bf_aux` → `photorec_bf_frag` (`src/phbf.c:524-655`) tries each `blocksize` split point before the error offset; for each, `blocs_to_skip` from −2..5000 (phase thresholds 16/100/10 blocks), `photorec_bf_pad` extends with up to +1000 blocks (with data_check) or +100 (without), re-running `file_check` each round and iterating while `offset_error` keeps improving (`src/phbf.c:292-420`); recursion cap `frag>5` → give up (BF_ENOENT); `photorec_bf_frag_fast` (`src/phbf.c:424-520`) uses `offset_ok/extra` to skip candidate blocks directly. Two phases with different acceptance bounds; progress + user-stop checks every second.

### 5.3 Blocksize detection pass (`STATUS_FIND_OFFSET`)

`photorec_find_blocksize` (`src/phbs.c:76-229`): header-only scan (no writes), stops after **10 files found** (`src/phbs.c:170-173`); `find_blocksize` (`src/photorec.c:515-560`) starts at 128×512 and halves until all found-header offsets share a common modulus (offset alignment); `update_blocksize` (`src/photorec.c:562-621`) re-aligns every search-space range to that modulus, merging adjacent ranges and dropping sub-block remainders. Expert mode: `menu_choose_blocksize` prompt (`src/phrecn.c:349-355`). Known FIXME: NTFS 2MiB clusters (Windows 10 Creators) not handled (comment `src/photorec.c:517-518`).

### 5.4 Pass / status machine — `status_inc` (`src/photorec.c:998-1042`)

`STATUS_FIND_OFFSET` → (`mode_ext2`?) `STATUS_EXT2_ON` : `STATUS_EXT2_OFF` → (`paranoid>1`?) `…_BF` (brute-force pass) → (`keep_corrupted_file`?) `…_SAVE_EVERYTHING` (final pass saving all remaining regions; carved names get `b` prefix via `set_filename`'s `broken` flag) → `STATUS_QUIT`. `STATUS_UNFORMAT` starts a FAT-unformat run. Each pass logs "Pass %u (blocksize=%u) <STATUS>" and per-pass stats (`src/phrecn.c:282-286, 431-443`).

## 6. Signature Registry — Complete Supported-Format List (appendix)

**353 formats** = all `src/file_*.c` files (each file registers 1..N signatures; several also expose multiple extensions). Full list of format IDs (file names minus `file_`/`.c`):

```
1cd 3dm 3ds 7z DB a ab abr acb ace ado afdesign ahn aif all als amd amr apa ape apple ari arj aseprite
asf asl asm atd au axp axx bac bdm berkeley bfa bim bin binvox bkf bld bmp bpg bvr bz2 c4d cab caf cam
catdrawing cdt che chm class clip cm compress cow cpi crw csh ctg cwk d2s dad dar dat dbf dbn dcm ddf dex
dim dir djv dmp doc dovecot dpx drw drw2 ds2 ds_store dsc dss dst dta dump dv dvi dvr dwg dxf e01 ecryptfs
edb elf emf ess evt evtx exe exr exs ext ext2 fat fbf fbk fbx fcp fcs fdb fds fh10 fh5 filevault fit fits
flac flp flv fm fob fos found fp5 fp7 freeway frm fs fwd fxp gam gct gho gi gif gm6 gp2 gp5 gpg gpx gsm gz
hdf hdf5 hdr hds hfsp hm hr9 http ibd icc icns ico idx ifo imb indd info iso it itu jks journal jpg
jsonlz4 kdb kdbx key ldf list lit lnk logic lso luks lxo lz lzh lzo m2ts mat max maxis mb mcd mdb mdf mdp
mfa mfg mft mid mig mk5 mkv mlv mobi mov mp3 mpg mpl mrw msa mus mxf myo mysql nd2 nds nes njx nk2 nsf
oci ogg one orf pa paf pap par2 pcap pcb pct pcx pdb pdf pds pf pfx pgdump plist plr plt png pnm prc prd
prt ps psb psd psf psp pst ptb ptf pyc pzf pzh qbb qdf qkt qxd r3d ra raf rar raw rdc reg res rfp riff
rm rns rpm rvl rw2 rx2 save sdsk sdw ses sgcta shn shp sib sig sit skd skp snag sp3 spe spf spss sql sqm
steuer2014 stl stu studio swf tar tax template tg tib tiff tiff_be tiff_le tivo torrent tph tpl ttf txt
tz v2i vault vdi vdj veg vfb vib vmdk vmg wad wallet wdp wee wim win wks wld wmf wnk woff wpb wpd wtv wv
x3f x3i x4a xar xcf xfi xfs xm xml xpt xsv xv xz z2d zcode zip zpr
```

Registration pattern examples verified in source: `file_png.c:359-367` registers PNG/MNG/JNG (3 sigs at offset 0); `file_rar.c:113-119` registers RAR 1.5 + RAR 5; `file_mov.c:555-579` registers 21 atom fourccs at offset 4; `file_tar.c:139-145` registers ustar at offset 0x101 (GNU + POSIX); `file_mp3.c:637-643` registers "ID3" + 6 MPEG sync patterns; `file_riff.c:521-524` registers "RIFF"/"RIFX"; `file_sql.c:110-113` registers "SQLite format 3" (16 bytes); `file_7z.c:84-88` registers `37 7A BC AF 27 1C`; `file_jpg.c:2773-2779` registers `FF D8 FF`; `file_pdf.c:533-537` registers `%PDF-1`; `file_doc.c:2046-2049` registers the OLE magic `D0 CF 11 E0 A1 B1 1A E1`. No regex/grammar engine — every check is hand-written C per validator.

## 7. Validator Deep Dives (15 formats)

### 7.1 JPG — `src/file_jpg.c` (2923 l.; key functions read)

- **Hint**: extension "jpg", "JPG picture", `max_filesize=50*1024*1024` (50MB — unusual, most formats use `PHOTOREC_MAX_FILE_SIZE`), recover=1, enable_by_default=1 (`src/file_jpg.c:87-93`).
- **header_check_jpg** (`src/file_jpg.c:882-1075`): walks markers from offset 2 while `buffer[i]==0xff && is_marker_valid(buffer[i+1])` (SOF0-15, DHT/DQT/DRI/APP0-15/COM — table at `src/file_jpg.c:798-842`); validates DHT (`jpg_check_dht`); extracts Exif date from APP1 (`jpg_get_date` → `get_date_from_tiff_header`); requires the walk to land on `FF DA` (SOS) within blocksize/512/buffer_size bounds. Rejects: embedded thumbnails (≤1024B APP12 or ≤16KiB APP0 with small SOF0 dimensions), duplicate APP1 XMP, MPO second images, JPGs inside AVI (`jpg_header_app0_avi`), MOV (`jpg_header_app0_jfif11_*`), rw2, indd, doc .albm/.prt — all via `header_ignored_adv`/`header_ignored`. When a previous file is open, APP0 must be JFIF/JFXX, APP1 must be "Exif", COM printable — else the header is ignored (no new file).
- **End detection**: `data_check_jpg` (`src/file_jpg.c:2629-2766`) marker walk (size fields, SOF0/DHT structural checks); on SOS switches to `data_check_jpg2` (`src/file_jpg.c:2500+`) which scans entropy data for `FF D9` (EOI) and sets `calculated_file_size`; APP2 "MPF" multi-picture handled by `check_mpo` (switches to `data_check_size`+`file_check_mpo`). `file_check_jpg` (`src/file_jpg.c:2443-2500`) re-opens the file: `jpg_check_structure` full marker walk; **libjpeg decompression validation** (`jpg_check_thumb` for embedded thumbnails and `jpg_check_picture` for the image; setjmp-based error manager `my_error_exit/my_emit_message`, `src/file_jpg.c:1082-1356`); uses `offset_error`/`offset_ok` bookkeeping from the checks.

### 7.2 PNG/MNG/JNG — `src/file_png.c` (full file)

- Signatures: `89 50 4E 47 0D 0A 1A 0A`, `8A 'MNG'…`, `8B 'JNG'…` (`src/file_png.c:361-363`). `header_check_png` requires the first chunk type at +8+4 to be alphabetic and, when "IHDR", validates `png_check_ihdr` (width/height non-zero; bit_depth×color_type legal matrix — `src/file_png.c:85-109`); PNG found inside an OLE doc (SolidWorks) is ignored.
- **End detection**: `data_check_png`/`data_check_mng` (`src/file_png.c:180-256`) walk chunks: length (BE32) + 4-char type must be letters, else `DC_ERROR` with `offset_error`; stop on `IEND`/`MEND` (+12+length). `file_check_png` (`src/file_png.c:118-172`) re-walks from disk: seeks chunk-by-chunk until IEND; non-IHDR-with-invalid size → file_size=0.

### 7.3 GZ — `src/file_gz.c` (455 l.; key parts)

- Signature `1F 8B 08` (`src/file_gz.c:441-445`). With zlib: inflates ~64-256KiB and **sniffs the inner content** to re-type the file: `ustar` at inner offset 0x101 → "tar.gz" (tgz under DJGPP), "RDX2" → "RData", `<?xml version=` → "xml.gz", Minecraft `schematic`, `<SCRIBUS` → "sla.gz", `<html` within 256B → "html.gz" (`src/file_gz.c:355-433`). mtime from the gzip header; `file_rename_gz`. Without zlib: min_filesize=22 only.

### 7.4 ZIP — `src/file_zip.c` (1581 l.; key parts)

- `PK\x03\x04` (+ WinZip AES `PK……` variant, `src/file_zip.c:1476-1478`). `header_check_zip` (`src/file_zip.c:1357-1435`) parses the first local file entry: version≥10, filename length 1..4096; **extension derived from the first entry name**: `mimetype` (parse ODF mimetype string), `[Content_Types].xml` → docx/xlsx/pptx/vsdx depending on `word/`/`xl/`/`ppt/`/`visio/` presence, `Song.xml` → xrns, `Home` → sh3d, `project.json` → sb3, `Index/Document.iwa` → numbers; else zip + `file_rename_zip`. ZIP local-entry signatures found inside an OLE doc are suppressed.
- **End detection**: `file_check_zip` (`src/file_zip.c:1055-1130+`) walks local entries + central directory to the EOCD record and computes the exact archive size; tracks first filename for rename and expected compressed sizes.

### 7.5 TAR — `src/file_tar.c` (full file)

- `ustar` GNU (6B) / POSIX (8B) at offset **0x101**; `is_valid_tar_header` = magic + `is_valid_checksum_format` (octal, space-tolerant, or all-zero — `src/file_tar.c:55-108`); min_filesize 512; consecutive tar headers suppress other signature checks during carving (§5.2); no independent end detection (EOF = next invalid header / next carved file).

### 7.6 PDF — `src/file_pdf.c` (542 l.; key parts)

- `%PDF-1` at 0 + printable byte at [6]. `read_pdf_file` (`src/file_pdf.c:470-501`) parses `/Linearized` `/L` size from the first 512 bytes → exact `calculated_file_size`; `<</Illustrator ` → extension "ai".
- **End detection**: linearized → `data_check_size` + `file_check_pdf_and_size` (last 20 bytes must contain `%%EOF`, else file_size=0 — `src/file_pdf.c:372-415`); otherwise `file_check_pdf` = backwards footer search `file_search_footer("%%EOF")` + newline tolerance `file_allow_nl(NL_BARENL|NL_CRLF|NL_BARECR)` + date scan `file_date_pdf` (looks for `/CreationDate(D:…)` patterns).

### 7.7 OLE/DOC — `src/file_doc.c` (2122 l.; key parts)

- OLE2 CFB header `D0 CF 11 E0 A1 B1 1A E1`; `header_check_doc` (`src/file_doc.c:2008-2045`) = `is_valid_doc_header` (sector shift/FAT sanity) + extension from directory-entry names via `ole_get_file_extension`/`entry2ext` (`src/file_doc.c:449+`: `WordDocument`→doc, `PowerPoint Document`→ppt, `Book`→xls, `.QDF`→qdf_backup, `Scene`→max (3ds Max), `Licom`→amb, `CONTENTS`→wps, `PageMaker`→p65, `JNBVersion`→jnb, `RSeStorage`→ipt, …); StarImpress sda→sdd; Publisher wps→pub disambiguation; fallback `ole_dirty_get_file_extension` for damaged FATs.
- **End detection**: `file_check_doc` walks the FAT chain and directory sectors (returns exact stream sizes); `file_rename_doc` renames using embedded metadata.

### 7.8 RIFF/AVI — `src/file_riff.c` (526 l.; key parts)

- "RIFF"/"RIFX" at 0; form 4CC at [8] must be uppercase/space/digit. `header_check_riff` (`src/file_riff.c:442-519`) dispatches: "AVI " → `header_check_riff_avi` (`src/file_riff.c:330-375`: detects raw `movi`+`db` data-binary AVI streams vs LIST-chunk AVI; sets `data_check_avi_stream`+`file_check_size_max` or `data_check_avi`+`file_check_avi`); "ACON"→ani; "NUND"→cpr (Cubase); others → `header_check_riff_various` (`src/file_riff.c:376-430`: CDDA→cda, CDR/cdr6→cdr, RMP3/WAVE→wav, RMID→mid, `IDF LIST`→idf, AGNX→agn, MDLX→mdl, QLCM→qcp, WEBP→webp; exact RIFF size via `data_check_size`/`file_check_size`). RIFX+"Egg!"→aep (After Effects, `file_check_size_min`).
- **End detection**: `data_check_avi` walks LIST/chunk structure (`src/file_riff.c:253+`); `file_check_avi` searches for the `idx1` index footer (`src/file_riff.c:190-252`).

### 7.9 MOV/MP4 — `src/file_mov.c` (676 l.; key parts)

- Atom fourcc at offset 4 (21 registered: ftyp, moov, mdat, wide, pnot, skip, free, cmov, cmvd, dcom, jp2h, jP , mdia, stbl, trak, PICT, …— `src/file_mov.c:555-579`). `header_check_mov_aux` (`src/file_mov.c:246-330+`) walks atoms from 0: BE32 size (extended 64-bit when size==1), rejects <8 or ≥0x800000000000; `pnot` must be exactly 20 bytes, `wide` exactly 8, `moov` ≤256³; extension by ftyp brand via `file_rename_mov` (mp4/m4a/3gp/3g2/heic/jp2/cr3); separate disabled-by-default `file_hint_mov_mdat` ("mov/mdat") to carve mdat atoms standalone.
- **End detection**: `data_check_mov` (`src/file_mov.c:145-245`) walks the atom tree to the computed size; `file_check_size`.

### 7.10 MP3 — `src/file_mp3.c` (825 l.; key parts)

- Registered patterns: "ID3" at 0 + six frame-sync headers for MPEG1/2/2.5 Layer III (`src/file_mp3.c:637-643`). `header_check_mp3` (`src/file_mp3.c:511-625`) validates a **frame chain**: sync `FF` with `(b1&0xFE)∈{FA,F2,E2}`; computes `frameLengthInBytes` from bit_rate_table/sample_rate_table (`144000*bit_rate/sample_rate+padding`, `72000*…` for MPEG2) and walks consecutive frames — requires ≥2 valid frames (`nbr<=1` reject); rejects mp3-looking data inside mkv and TIFF-RGB lookalikes. ID3 route: `header_check_id3` parses tag version/size → `data_check_id3` until tag consumed.
- **End detection**: `data_check_mp3` (`src/file_mp3.c:274-414`) continues the frame walk (returns DC_STOP when frames stop validating); `file_check_size` truncates at `calculated_file_size`; min_filesize 287.

### 7.11 RAR — `src/file_rar.c` (full file)

- `Rar!\x1a\x07\x00` (1.5) / `Rar!\x1a\x07\x01\x00` (5.0); min 70/60 bytes; **footer search only for unencrypted archives** (`buffer[0xa] & MHD_PASSWORD` == 0 — `src/file_rar.c:73,105`).
- **End detection**: `file_check_rar15fmt`/`rar50fmt` search backwards for the exact archive-end footers `c4 3d 7b 00 40 07 00` / `1d 77 56 51 03 05 04 00` via `file_search_footer`.

### 7.12 7z — `src/file_7z.c` (full file)

- `37 7A BC AF 27 1C`; validates `majorversion==0`, `nextHeaderOffset/Size` < 0x7000000000000000; exact size = `nextHeaderOffset + nextHeaderSize + 12 + 20`; `data_check_size` + `file_check_size`; min_filesize 31 (`src/file_7z.c:64-82`).

### 7.13 SQLite — `src/file_sql.c` (full file)

- "SQLite format 3" (16B); page-size validation (power of two 512..32768, or 1 ⇒ 65536); **when `filesize_in_page>0` and `file_change_counter==version_valid_for`** the header is trusted for an exact size `filesize_in_page*pagesize` (→ `data_check_size`/`file_check_size`); otherwise no size computation; extension "sql" under DJGPP (`src/file_sql.c:82-108`).

### 7.14 TXT/FASTTXT — `src/file_txt.c` (4058 l.; key parts)

- Three hints: `file_hint_fasttxt` ("tx?" — rtf/xml/xhtml/mbox/imm/pm/ram/reg/sh/slk/stp/jad/url…), `file_hint_snz`, `file_hint_txt` (txt/html/asp/bat/C/jsp/perl/php/py/emlx/scripts) (`src/file_txt.c:137-163`). Known-header table `fasttxt_headers[]` (~60 entries from `#!/bin/bash`→sh through `<?xml`→xml — `src/file_txt.c:163+`). `header_check_txt` (`src/file_txt.c:1649-1730+`): emlx detection (decimal length prefix + "Return-Path: "/"Received: from" → exact size + `file_check_emlx`), `@echo off`→bat, `<%@ language="vbscript`→asp, `version 4.00\r\nbegin`→vb, `begin:vcard`→vcf; generic branch applies ASCII/UTF statistics and language heuristics (large tables later in the file) to pick txt/html/etc.
- **End detection**: `data_check_txt` (`src/file_txt.c:795`) line-oriented continuation scan; `file_check_size`.

### 7.15 EXE — `src/file_exe.c` [hint/registration enumerated only — see Coverage]

Shared plumbing verified in `src/filegen.c`: `file_rsearch` (backwards 4KiB-stride footer search, `src/filegen.c:380-434`), `file_search_footer` (final size = footer offset + footer_length + extra_length, `src/filegen.c:436-449`), `file_allow_nl` (tolerates a trailing newline form, `src/filegen.c:340-378`), `data_check_size`/`file_check_size`/`file_check_size_min`/`file_check_size_max` (generic calculated-size stoppers, `src/filegen.c:451-490`), `file_rename`/`file_rename_unicode` (post-recovery rename from embedded metadata, `src/filegen.c:583-981`), `header_ignored`/`header_ignored_adv`/`header_ignored_cond_reset` (suppress re-detecting the same header inside an open file — `src/filegen.c:985-1065`).

## 8. Carved Output Layout

- **Base directory**: CLI `-d`/`/d <dir>`; if the path ends in a separator, `DEFAULT_RECUP_DIR "recup_dir"` (`src/photorec.h:26`) is appended (`src/phmain.c:275-291`).
- **Directory creation**: `photorec_mkdir(recup_dir, dir_num)` creates `recup_dir.N` (`snprintf("%s.%u")`, `mkdir`, loop while `errno==EEXIST`, wraps at UINT_MAX; DJGPP stops after 1000 dirs to avoid short-name collisions) — `src/photorec.c:290-332`. First is `.1` (`params->dir_num=1` set in `params_reset_aux`, `src/photorec.c:957`).
- **Rotation**: new directory after every `MAX_FILES_PER_DIR 500` files (`src/photorec.c:713-716`, `src/photorec.h:25`).
- **File naming**: `set_filename` (`src/photorec.c:853-871`): `"%s.%u/%c%07lu"` → `recup_dir.N/f#######` where `#######` = 7-digit **sector number of the file start relative to the partition start**; with extension: `f#######.ext`; in save-everything mode the prefix letter is `b` (broken) instead of `f`.
- **No per-extension folder grouping / no "sort by extension"** in the carving engine — files land in sequence-numbered directories regardless of type; the `fileopt` system only toggles which formats are carved. [Stated plainly: no extension-grouped output path exists in the read sources; qphotorec reuses the same recup_dir.N layout (`src/qphotorec.cpp` [enumerated]).]
- **Collision handling**: none beyond the name encoding (start-sector based); when a header is re-detected inside an open file, `header_ignored*` suppresses spawning a second file from the same region. **Dedup: not implemented** — identical content carved at different offsets produces duplicate files. [Absence stated plainly.]
- Post-recovery `file_rename` may change the extension/name in place (e.g. zip/OLE/mov content-based renames).

## 9. Session / Log / Options

### 9.1 Session checkpoint — `photorec.ses` (`src/sessionp.c`, read fully)

- Saved after every pass and every 5 minutes mid-pass (`regular_session_save`, `src/sessionp.c:374-382`; cadence drops to 15 minutes if a save takes >30 s). File pre-allocated to `SESSION_MAXSIZE 40960` zero bytes (`src/sessionp.c:56-85, 351-366`).
- **Format** (`session_save`, `src/sessionp.c:200-349`): `#<unixtime>\n` then `<device> <arch part_name_option>,<partition order>,blocksize,<N>,fileopt,…,options,…,freespace|wholespace,search,status=<status>,<offset_sector>,inter\n` followed by one `<start_sector>-<end_sector>` line per remaining search-space range. The `fileopt` section stores the **minimal diff** of the enabled-format set (either "everything,enable," + the disabled extensions, or "everything,disable," + the enabled ones, or just the deviations from defaults). Options saved verbatim: `paranoid_no|paranoid|paranoid_bf`, `keep_corrupted_file[_no]`, `mode_ext2`, `expert`, `lowmem`.
- `session_load` (`src/sessionp.c:87-198`) restores device, command string, and ranges; an empty session file is created when absent. Clean finish → `unlink("photorec.ses")` (`src/phrecn.c:411-413`); user stop with failed save → "PhotoRec has been unable to save its session status. Answer Y to really Quit, N to resume the recovery" (`src/phrecn.c:398-405`). Resume: `photorec /cmd <device> resume` (`src/phmain.c:324-327`).

### 9.2 Logs

- **photorec.log** (default name, `src/phmain.c:178`): created only with `/log` (append) or `/debug` (also verbose; `/nolog` cancels; `/logname` renames) — `src/phmain.c:243-269, 376-411`. Content: full command line, version, compiler, lib versions (ext2fs/ntfs/ewf/jpeg/curses/zlib), OS, disk list (`log_disk_list`, `src/fnctdsk.c:505+`), "Pass N (blocksize=…) STATUS_…", per-file `filename  start-end …` (parenthesized ranges = non-data/truncated, `file_block_log`, `src/photorec.c:91-108`), per-extension stats `"%s: %u/%u recovered"` + `"Total: %u files found"` (`write_stats_log`, `src/photorec.c:450-489`), "Elapsed time %uh%02um%02us" per pass (`src/phrecn.c:435-441`), final "perf: get_prev_file_header: N, get_prev_location_smart: N" and "PhotoRec exited normally." (`src/phmain.c:477-482`).
- **JSON log** `/logjson <file>`: JSON-lines records `{"timestamp":"…","type":…}` with types `session_start`, `disk_info` (path/size_bytes/sector_size/readonly), `partition_info` (part_offset/part_size/sectors), `cli_params` (argv array), `session_resume` (device/cmd/search_space_regions/resume offset/files already found), `progress` (pass, current_sector, total_sectors, files_found), `file_recovered`, `completion` — `src/json_log.c:139-406`; registered via `log_set_json_handler(json_log_handler, …)` (`src/phmain.c:456-461`).
- **DFXML** (`ENABLE_DFXML`): `xml_log_file_recovered` per recovered file (`src/photorec.c:757-759`).

### 9.3 Options — exact semantics

- **CLI (photorec)**: `/log`, `/nolog`, `/logname <f>`, `/logjson <f>`, `/debug`, `/d <dir>`, `/all` (list all devices incl. virtual), `/direct` (O_DIRECT), `/nosetlocale`, `/cmd <device> <script>`, `/version`, `/help` (`src/phmain.c:128-139, 240-370`).
- **Script verbs** (consumed by `menu_photorec_cli` `src/phcli.c:147-293`, `photorec()` `src/phrecn.c:180-236`, `interface_write` `src/tpartwr.c:100-121`, `ntfs_boot_sector_command` `src/tntfs.c:76-107`): `search`; `options,` with `paranoid_no|paranoid|paranoid_bf`, `keep_corrupted_file_no|keep_corrupted_file`, `mode_ext2`, `expert`, `lowmem` (`src/poptions.c:37-99`); `fileopt,` with `<ext>,enable|disable` or `everything,enable|disable` (`src/phcli.c:73-144`); `wholespace` / `freespace`; `blocksize,N`; `geometry,C,/H,/S,/N,`; `ext2_group,N` / `ext2_inode,N`; `inter` (drop to interactive); partition `<order>` digit; `status=unformat|find_offset|ext2_on[_bf|_save_everything]|ext2_off[_bf|_save_everything]`; `noconfirm`, `write`, `dump`, `list`, `rebuildbs`, `originalntfs`, `backupntfs`, `repairmft`.
- **paranoid** (3 levels, `src/phrecn.c:507-522` + `src/photorec.c:998-1042`): 0 = "Paranoid : No" — files never re-validated; 1 = "Yes (Brute force disabled)" — `file_check` runs, failed files deleted; 2 = "Yes (Brute force enabled)" — validation + the `photorec_bf` fragmented-file brute-force pass (`STATUS_*_BF`).
- **keep_corrupted_file**: adds the `_SAVE_EVERYTHING` final pass which re-carves and keeps even invalid files under the `b#######` prefix (`src/photorec.c:1013-1036`; `file_finish_aux` skips checks in that status, `src/photorec.c:660-662`).
- **mode_ext2**: skip ext2 indirect blocks in-flight (`src/psearchn.c:165-183`) + group/inode-scoped carving (`src/phcli.c:234-272`).
- **lowmem**: `forget()` frees search-space history >10000 entries (`src/photorec.c:388-397`).
- **expert**: unformat prompt, blocksize/root-cluster/cluster-size prompts, `image_remaining.dd` export of unknown data (`gen_image`, `src/phrecn.c:447-480`).
- **No stop-after-N-files option exists**; the only file-count cap is the internal 10-file limit of the blocksize-detection pass (`src/phbs.c:170-173`). [Absence stated plainly.]

## 10. Error Model (notable paths, user-visible)

**Partition flows:**

- `msg_PART_RD_ERR` "Partition: Read error" (`src/lang.h:46`); `msg_TBL_NMARK` endmark missing (`src/lang.h:28`); `msg_SAME_SPACE` overlap (`src/lang.h:52`); `msg_ONLY_ONE_EXT` / `msg_NO_BOOTABLE` / `msg_ONLY1MUSTBOOT` (`src/lang.h:49-51`).
- "Invalid partition structure." (`src/godmode.c:1533`); `msg_PART_WR_ERR` on write failure (`src/godmode.c:1442`); "Write isn't available because the partition table type \"%s\" has been selected." (`src/tpartwr.c:133-135`); Mac/Sun/Xbox write → `not_implemented("write_part_mac|sun|xbox")` (`src/godmode.c:1413-1429`); "You will have to reboot for the change to take effect." (`src/godmode.c:1448`); "Can't create backup.log." (`src/tanalyse.c:99-102`).
- Geometry: "Warning: the current number of heads per cylinder is %u but the correct value may be %u." + remediation hint (`src/godmode.c:344-369, 1342-1350`).
- Disk-too-small: "The hard disk (%s) seems too small! (< %s) / Check the hard disk size: HD jumper settings, BIOS detection..." + Windows LBA48 hint "update Windows to support LBA48 (minimum: W2K SP4 or XP SP1)" (`src/godmode.c:166-203, 301-342`); "The following partition(s) can't be recovered" (`src/godmode.c:205-210`); "This partition ends after the disk limits. (start=…, size=…, end=…, disk end=…)" and "Partition not added." (verbose log, `src/godmode.c:920-937`); "Read error at %lu/%u/%u (lba=%lu)" (`src/godmode.c:844-847`).
- GPT: "Bad GPT partition, invalid signature./invalid header checksum./invalid LBA self location./invalid LBA start/end location.", "GPT: invalid header size./invalid number (%u) of partition entries./invalid partition entry size./A minimum of 16,384 bytes…", "Bad GPT partition entries, invalid checksum.", "GPT: Warning - not revision 1.0", "GPT: Warning - __reserved!=0" (`src/partgpt.c:230-343`).
- FS checkers: "Invalid FAT boot sector", "Invalid BSD disklabel", "No HFS or HFS+ structure", "No ext2, JFS, Reiser, cramfs or XFS marker", "No LVM or LVM2 structure", "Invalid RAID superblock", "Invalid NTFS or exFAT boot", "No FAT, NTFS, ext2, JFS, Reiser, cramfs or XFS marker", "No valid APFS structure", "No BFS structure" (`src/parti386.c:1746-1803`, `src/partgpt.c:532-580`).
- NTFS repair: "Boot sector not valid, can't repair MFT.", "Can't read NTFS boot sector/MFT/MFT mirror.", "MFT and MFT mirror match perfectly.", "Both MFT seems ok but they don't match, use chkdsk.", "MFT and MFT mirror are bad. Failed to repair them.", "MFT fixed./MFT mirror fixed.", "Failed to fix MFT: write error.", "Can't determine which MFT is correct, ntfslib is missing." (`src/ntfs_fix.c:66-310`, `src/tntfs.c:135-153`).
- FAT rebuild: "Can't find cluster size", "Can't find FAT length", "Can't get cluster size", "dir_entries not found, should be 512", "Extrapolated boot sector and current boot sector are different/identical", "Warning: Extrapolated boot sector have incorrect values.", "Write error: Can't write new FAT boot sector" (`src/fat_adv.c:1897-1060`).

**PhotoRec flows:**

- `PSTATUS_EACCES` → "PhotoRec has been unable to create new file. / This problem may be due to antivirus blocking write access while scanning files created by PhotoRec. / If possible, temporarily disable your live antivirus protection." (Cygwin/MinGW interactive C/Q menu; elsewhere → quit) — `src/phrecn.c:113-131`, `src/photorec_check_header.h:35-65`; "Cannot create file %s: %s" (`src/photorec_check_header.h:145-152`).
- `PSTATUS_ENOSPC` → "Warning: not enough free space available. Please select a destination to save the recovered files to.\nDo not choose to write the files to the same partition they were stored on." + destination re-pick (`src/phrecn.c:368-392`); "Cannot write to file %s after %llu bytes: %s" (`src/psearchn.c:188-204`; EFBIG → treat as EOF).
- "Error reading sector %10lu" during scan (`src/psearchn.c:337-345`, `src/phbs.c:195-203`); "File too small ( %llu < %llu), reject it" (`src/photorec.c:676-681`); "ftruncate failed." (`src/photorec.c:697-700`); "File should not be bigger than %llu, stopped adding data" (`src/psearchn.c:225-240`); "Cannot write to file %s: %s" (`src/phbf.c` BF writers).
- Session: "PhotoRec has been unable to save its session status. Answer Y to really Quit, N to resume the recovery" / "Answer Y to really Quit, N to resume the recovery" (`src/phrecn.c:398-405`); "Can't create photorec.ses file: %s" / "Can't open photorec.ses file: %s" (`src/sessionp.c:66, 102`).
- CLI: "Syntax error in command line: %s" (`src/phcli.c:285-290`); "PhotoRec syntax error: %s" (`src/phmain.c:489-494`); "Unable to open file or device %s: %s" (`src/phmain.c:342-363`); "PhotoRec: Log file corrupted!" (`src/phmain.c:483-487`); "Failed to save the settings." / "Settings recorded successfully." (`src/phrecn.c:518-524`).
- Signals: SIGINT/SIGHUP/SIGTERM handler logs "PhotoRec has been killed." and sets `need_to_stop` (second signal re-raises default) — `src/phmain.c:101-126`.
- Final screen: "Recovery completed." (+donation URL when files>0), "Recovery aborted by the user.", "Cannot create file in current directory.", "Cannot write file, no space left." (`src/phrecn.c:79-106`).
- Internal invariant failures abort the process: `file_block_remove_from_sp` mismatch → `log_critical` + `exit(1)` (`src/photorec.c:1140-1143`).

## 11. Top Extractable Algorithms (port targets for d-space_fork)

Ranked by value for a Windows data-recovery product (Rust/Tauri); all are documented above with citations. Reimplement from this doc, do not copy code (GPLv2+).

1. **PhotoRec signature-registry dispatch** — byte-indexed 256-bucket lookup keyed by first signature byte per offset bucket; O(1) per-block header test over 353 formats (`src/filegen.c:159-288`, `src/photorec_check_header.h:198-215`).
2. **Search-space carving loop** — alloc_data range list, blockwise scan, header→file_finish state machine, `data_check` streaming validators with `DC_CONTINUE/STOP/ERROR` semantics, blocksize auto-detection by header-offset modulus (`src/psearchn.c:80-384`, `src/phbs.c:76-229`, `src/photorec.c:515-621`).
3. **Brute-force fragmented-file reconstruction** — split-point search + skip-block padding + re-validation loop with frag≤5 recursion (`src/phbf.c`).
4. **GPT parse/validate/rebuild** — full header+entries CRC32 validation, backup header fallback, dual primary+backup rewrite with fresh CRCs and UUID preservation, protective-MBR/bootcamp layout generation (`src/partgpt.c`, `src/partgptw.c`, `src/crc.c:88-106`).
5. **MBR/EBR chain walk & rewrite** — 4-entry parse with CHS/LBA cross-validation, EBR chain walk with rule enforcement, chain rewrite with next-EBR CHS snapping (`src/parti386.c`).
6. **Deep-scan candidate generator** — hint list + arithmetic progressions (cylinder S=1/H≤2, 1MiB boundaries, backup-sector stripes S=7/S=13/boundary-512, ext2 backup-superblock offsets), probe ladder over 6 offset windows (`src/godmode.c:535-993`, `src/next.c`, `src/analyse.c`).
7. **NTFS boot-sector + MFT/MFTMirr repair decision logic** — backup-vs-original compare, "list files with either MFT substituted" arbitration, raw overwrite fix (`src/tntfs.c`, `src/ntfs_fix.c`).
8. **FAT rebuild-BS heuristics** — FAT-start detection by 12/16/32-bit chain-consistency scoring, cluster-size power-of-two fit, FAT32 root-cluster scan, dual boot+backup write for FAT32 (`src/fat_adv.c`).
9. **Session checkpoint format** — resumable text format encoding device, partition, blocksize, format-enable diff, options, status, offset, and remaining sector ranges (`src/sessionp.c:200-349`).
10. **Per-format validators with end-detection grammars** — jpg (marker walk + libjpeg decode), png (chunk walk to IEND), zip (EOCD walk + content-typed extension), tar (checksum format), pdf (Linearized size / %%EOF), riff/avi (LIST/idx1), mov (atom tree), mp3 (frame-length walk), 7z/sqlite (exact size from header) — see §7.

## 12. Licensing

- `COPYING` = GNU General Public License **Version 2, June 1991**, complete text (`COPYING:1-340`).
- Every source file read carries the grant "version 2 of the License, or (at your option) any later version" → **GPLv2+** (spot-verified: `src/parti386.c:7-10`, `src/partgpt.c:7-10`, `src/partgptw.c:7-10`, `src/analyse.c:4-7`, `src/godmode.c:4-7`, `src/photorec.c:7-10`, `src/phbf.c:7-10`, `src/psearchn.c:4-7`, `src/filegen.c:4-7`, `src/file_jpg.c:4-7`, `src/ntfs_fix.c:3-6`).
- `src/crc.c:5-6` additionally credits "Gary S. Brown for the crc32 table" (public-domain-style table, GPL-licensed file overall).
- **Consequence for d-space_fork (Tauri/Rust/React, production):** any direct port or translation of this C code is a derivative work under GPLv2+ and would impose GPL on the combined distribution. The algorithms documented here are free to reimplement cleanly from this report (ideas/algos are not copyrighted); copying code or the 353-signature tables verbatim is not.

## 13. Coverage

**Read fully (whole file):** 30 files —
Partition: `src/parti386.c` (1861 l.), `src/partgpt.c` (623), `src/partgptw.c` (313), `src/partauto.c` (168), `src/tanalyse.c` (113), `src/tpartwr.c` (148), `src/analyse.c` (459), `src/godmode.c` (1540), `src/next.c` (228), `src/geometry.c` (173), `src/crc.c` (135), `src/tntfs.c` (303), `src/ntfs_fix.c` (320).
PhotoRec core: `src/photorec.c` (1417), `src/phrecn.c` (723), `src/phbs.c` (229), `src/phbf.c` (756), `src/psearchn.c` (383), `src/photorec_check_header.h` (223), `src/pnext.h` (123), `src/sessionp.c` (382), `src/phcfg.c` (245), `src/phcli.c` (293), `src/phmain.c` (517), `src/poptions.c` (111), `src/filegen.h` (482).
Validators fully: `src/file_png.c` (368), `src/file_rar.c` (120), `src/file_7z.c` (89), `src/file_tar.c` (146), `src/file_sql.c` (114).

**Read partially (targeted functions/sections, cited):** `src/filegen.c` (registry + footer/search utilities + init_file_stats), `src/file_jpg.c` (hint, marker validation, header_check_jpg, data_check_jpg/jpg2, file_check_jpg, libjpeg plumbing), `src/file_gz.c`, `src/file_zip.c`, `src/file_pdf.c`, `src/file_doc.c`, `src/file_riff.c`, `src/file_mov.c`, `src/file_mp3.c`, `src/file_txt.c`, `src/file_list.c` (extern table + array_file_enable + assert), `src/fat_adv.c` (rebuild_FAT_BS, select_fat_info, fat_find_fat_start, menu_write_fat_boot_sector, entry2ext), `src/fnctdsk.c` (geometry scoring, log_disk_list), `src/json_log.c` (record formats), `src/parti386.h`/`src/common.h`/`src/partgpt.h`/`src/photorec.h`/`src/lang.h` (constants), `src/log.c`/`src/testdisk.c` (grep-level verification of CLI/log naming), `COPYING` (first 40 lines + license identification).

**Totals: 30 full + ~20 partial = ~50 of 793 files read (~6%).** The remaining **~743 files were enumerated and classified only** and are NOT presented as read: all 338 non-deep-read `file_*.c` validators (names/format list taken from `git ls-files` only), filesystem modules (fat/ntfs/ext2/... recovery internals), `partmac.c`/`partsun.c`/`partxbox.c`/`parthumax.c`/`partnone.c`/`bsd.c` bodies, disk I/O layer, ncurses UI files, Qt GUI, docs, packaging.

**Anomalies / notes:**

- `sed`-piped display dropped `[` characters in some terminal output (e.g. `bit_rate_table[mpeg_version]…` rendered `bit_rate_tablepeg_version]`); every affected identifier was re-verified against the file with the Grep tool before being cited.
- `src/file_list.c:2165-2170` embeds a compile-time assert "No file format has been enabled".
- GPT backup-entries placement check is a TODO (`src/partgpt.c:370-372`); fast_mode>1 branches in `src/godmode.c`/`src/next.c` are unreachable from stock UI/CLI.
- Odd tracked root files: `doc`, `config/config.rpath`, `.whitesource`.
- No "stop after N files", no dedup, no extension-grouped output — all verified absent in the read engine sources.
- `src/file_exe.c` was enumerated only; its PE parsing is [UNVERIFIED] in this harvest.

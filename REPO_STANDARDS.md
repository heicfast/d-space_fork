# D-Space · Repo Standards

> **Hedef konum:** `C:\Projeler\d-space\REPO_STANDARDS.md`
> Reponun köğüne kopyalayıp commit'leyin. Sonraki düzenlemeler bu dosyaya bağlı kalmalı — değişiklik gerekirse burayı da güncelleyin.
>
> **Snapshot:** 2026-05-14 (D Brand README + about/topics align sonrası)

---

## 1. Locked GitHub metadata

| Alan | Değer |
|---|---|
| **Owner/Repo** | `AmrasElessar/d-space` |
| **Visibility** | public |
| **Default branch** | `main` |
| **License (SPDX)** | `GPL-3.0` (or-later) |
| **Description** | `Smart disk analyzer and recovery platform for Windows — MFT-fast scanning, 0-100 safe-to-delete scoring, staging+undo guarantee with cross-volume two-phase commit. Tauri v2 + Vue 3 + Rust.` |
| **Homepage** | `https://github.com/AmrasElessar/d-space/releases` |
| **Topics (20)** | `blake3, developer-tools, disk-analyzer, disk-space, disk-usage, filesystem, mft, ntfs, open-source, rust, safe-delete, sqlite, staging, storage, sunburst, tauri, tauri-v2, undo, vue, windows` |

Değişiklik yaparsanız bu tabloyu güncelleyin + remote'a yansıtın.

---

## 2. README iskeleti (D Brand template — kaynak: d-terminal)

### 2.1 Bölüm sırası (kanonik)

1. **Header** — center-aligned, başlık + İngilizce tagline + TR/EN alt-tagline + bilingual notice
2. **🎬 Demo**
3. **Badge row** — CI/Release/Downloads (varsa) → License → Status → Platform → Tech → D Brand
4. **📌 Kısaca** (TR) + collapsible `🇬🇧 At a glance` (EN)
5. **🆕 Yenilikler / What's done so far**
6. **🎯 Vizyon / Vision** (opsiyonel)
7. **✨ Öne Çıkan Özellikler / Key Features**
8. **🛠️ Teknoloji / Tech Stack**
9. **🗺️ Yol Haritası / Roadmap**
10. **📥 Kurulum / Installation** + **🚀 İlk Adımlar / Quick Start**
11. **🛡️ Güvenlik Tarama / Security Scan Results** (release varsa)
12. **🤝 Katkı / Contributing**
13. **🎨 D Brand Ailesi / D Brand Family**
14. **💖 Sponsorlar / Sponsors**
15. **❤️ Destekle / Support**
16. **📜 Lisans / License**

### 2.2 Header pattern

```markdown
<div align="center">

<img src="<icon-path>" width="128" alt="D-Space logo" />

# D-Space

**Smart disk analyzer and recovery platform for Windows**

*MFT-hızlı tarama, güvenli silme skorlaması, garantili undo*
*MFT-fast scanning, safe-to-delete scoring, guaranteed undo*

🌐 **TR · EN** — ...

</div>
```

### 2.3 Badge row

```
[CI] [Release] [Downloads]                       (varsa)
[License: GPL-3.0+]
[Status: alpha/MVP/stable — gerçek duruma göre]
[Platform: Windows 10/11]
[Tauri v2] [Vue 3] [Rust stable] [SQLite] [BLAKE3]
[D Brand]
```

### 2.4 Bilingual yapı

- Ana akış TR + `<details><summary>🇬🇧 ...</summary>` ile EN
- Teknik terimler (MFT, NTFS, two-phase commit) açıklamasız geçer — hedef kitle geliştirici/teknik kullanıcı

---

## 3. Tech stack & status

- **Status:** v0.x — README'deki en güncel sürüm rozeti ile senkron
- **Core:** Tauri v2 (Rust + WebView2)
- **Frontend:** Vue 3
- **Storage:** SQLite (scan cache, staging ledger)
- **Hashing:** BLAKE3
- **Filesystem:** MFT direct read (NTFS), staging+undo cross-volume two-phase commit
- **Target:** Windows 10 1809+ ve Windows 11
- **Visualization:** Sunburst chart

---

## 4. Lisans

- **GPL-3.0-or-later** (SPDX: `GPL-3.0`). README badge'i ve `LICENSE` dosyası tutarlı.

---

## 5. Commit mesaj stili

Conventional commits:

- `feat(readme): ...`, `fix(readme): ...`, `docs(readme): ...`
- `chore: ...` — config / FUNDING / dependency bump
- `feat(<area>): ...`, `fix(<area>): ...` — kod (scanner, staging, ui, ...)

Dil: TR veya EN; tutarlı.

---

## 6. Dosya hijyeni

- Adı `:` veya `\` içeren dosyalar **commit'lenmez**.
- **Zorunlu:** `README.md`, `LICENSE`, `.github/FUNDING.yml`
- **Tercih edilen:** `.gitignore`, `docs/`
- Push öncesi `git status` kontrol.

---

## 7. Repo-spesifik notlar

- **Safe-to-delete scoring (0-100)** — README'deki algoritmaya dair claim'ler kodla senkron tutulmalı. Skor formülü değiştiyse README'de "v2 scoring rubric" gibi versiyonlanmalı.
- **Staging + undo garantisi** — cross-volume two-phase commit'in claim'i README'de; bozulması major bump tetikler.
- **Release hijyeni** — her release sonrası homepage URL'i `/releases/tag/vX.Y.Z` yerine `/releases` olarak tut (latest'i göstersin).
- **MFT direct read** — admin yetkisi gerektirir; README'de "Quick Start"ta açıkça belirtilir.

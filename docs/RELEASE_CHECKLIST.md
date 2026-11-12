<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# DiskWizard Release Checklist

Master architecture chapters 18.2 (code signing) + 21.4
(auto-update) + 18.4 (EDR/AV certification). Steps that must be
completed before a stable release.

> **Alpha (v0.1.x):** unsigned MSI/NSIS, updater off, SmartScreen
> "publisher unknown" warning accepted. Users are warned in the
> README.
>
> **Beta (v0.2.x+):** updater active with a real Ed25519 keypair
> (DONE — the key lives in GitHub Secrets, the public key in
> `tauri.conf.json`); binaries still unsigned.
>
> **Stable (v1.0):** EV certificate (SmartScreen reputation), Tauri
> updater feed, auto-update channels (Stable/Beta/Nightly),
> VirusTotal monitoring.

---

## 1. Code Signing (chapter 18.2)

### 1.1 Certificate type selection

| Type                            | Cost     | SmartScreen behavior                               | Recommendation   |
| ------------------------------- | -------- | -------------------------------------------------- | ---------------- |
| **Self-signed**                 | free     | "publisher unknown" + red warning                  | Development only |
| **OV (Organization Validated)** | ~$200/yr | "publisher unknown", 30–90 day reputation build-up | v0.5 beta        |
| **EV (Extended Validation)**    | ~$400/yr | NO "publisher unknown" (instant trust)             | **Stable v1.0**  |

EV certificate providers: DigiCert, Sectigo, GlobalSign, SSL.com. EV
comes with an HSM-bound token (USB dongle or cloud HSM); the private
key cannot be exported.

### 1.2 signtool integration

`tauri-action` produces the MSI/NSIS bundles. Signing is an extra
step:

```yaml
# to be added to .github/workflows/release.yml (v0.5)
- name: Sign MSI/NSIS
  if: ${{ env.SIGNING_ENABLED == 'true' }}
  run: |
    & "C:\Program Files (x86)\Windows Kits\10\bin\10.0.22621.0\x64\signtool.exe" `
      sign /a /tr http://timestamp.digicert.com /td sha256 /fd sha256 `
      "src-tauri/target/release/bundle/msi/*.msi" `
      "src-tauri/target/release/bundle/nsis/*.exe"
  env:
    SIGNING_ENABLED: ${{ secrets.SIGNING_ENABLED }}
```

EV certificates require an HSM — on a runner this means an HSM bridge
(Azure Key Vault, SignPath.io, etc.). A **self-hosted Windows runner**
is the only practical route for EV signing.

### 1.3 Signature verification

```powershell
Get-AuthenticodeSignature .\DiskWizard_0.4.0_x64_en-US.msi
```

Expect `SignatureType` = `Authenticode`, `IsOSBinary` = `False`,
`StatusMessage` = `Signature verified`.

---

## 2. Auto-Updater (chapter 21.4) — active since v0.2

### 2.1 Plugin setup ✅ (done)

```bash
# Done once, pinned in the manifests:
pnpm add @tauri-apps/plugin-updater @tauri-apps/plugin-process
cargo add tauri-plugin-updater tauri-plugin-process --manifest-path src-tauri/Cargo.toml
```

`src-tauri/src/lib.rs` registers `tauri_plugin_updater::Builder::new
().build()` + `tauri_plugin_process::init()` in the build hook. The
capability file grants `updater:default` + `process:default`.

### 2.2 Ed25519 keypair ✅ (done for the fork channel)

```bash
# One-time, on the local machine:
pnpm tauri signer generate -w ~/.diskwizard/updater-private.key
# 1) Copy the public key output → src-tauri/tauri.conf.json
#    plugins.updater.pubkey
# 2) Store the private key as GitHub Secrets:
#    Settings → Secrets and variables → Actions:
#      TAURI_SIGNING_PRIVATE_KEY          ← private key file content
#      TAURI_SIGNING_PRIVATE_KEY_PASSWORD ← the password chosen at keygen
```

This is done for the `heicfast/d-space_fork` channel (new keypair
generated 2026-11-07; secrets set; `latest.json` + `.sig` artifacts
verified on the v0.3.0-beta release). Rotating the key later requires
a transition note for users on the old channel (treat the fork build
as a separate installation).

### 2.3 tauri.conf.json plugins.updater (current)

```jsonc
{
  "plugins": {
    "updater": {
      "endpoints": [
        "https://github.com/heicfast/d-space_fork/releases/latest/download/latest.json",
      ],
      "pubkey": "<real Ed25519 public key — set>",
      "windows": { "installMode": "passive" },
    },
  },
}
```

### 2.4 latest.json (automatic)

The release workflow runs `tauri-action` with `includeUpdaterJson:
true` — every release gains a `latest.json`:

```json
{
  "version": "v0.4.0",
  "notes": "Release notes",
  "pub_date": "2026-11-12T00:00:00Z",
  "platforms": {
    "windows-x86_64": {
      "signature": "<Ed25519 signature>",
      "url": "https://github.com/.../releases/download/v0.4.0-beta/DiskWizard_0.4.0_x64-setup.exe"
    }
  }
}
```

The client fetches the latest JSON from the endpoint in
`tauri.conf.json`, verifies the signature against the pubkey, shows
the update dialog, and on confirm runs `downloadAndInstall` +
`relaunch()`.

### 2.5 UI flow (`src/components/UpdateDialog.tsx`)

A small update button in the app shell. On click:

- No update → "up to date" notice
- Update available → dialog: new version + notes + "download and
  install" / "close"
- Download progress bar (event.Started.contentLength → sum of
  event.Progress.chunkLength)
- Signature/network/other errors map to i18n'd messages

### 2.6 Channel strategy (v1.0+)

- **Beta** — the channel's public **Latest** release (GitHub
  `releases/latest` never resolves to a prerelease, and the updater
  endpoint would 404 while no stable release exists — so betas must
  publish as Latest; alpha/rc stay flagged prerelease)
- **Stable** — same endpoint, GA versions (v1.0+)
- **Nightly** — daily builds, separate feed (v1.0)

User channel choice via `settings.update_channel`.

---

## 3. SmartScreen Reputation Building

Even after buying an EV certificate SmartScreen can warn on the
first downloads. Reputation build-up (~2–4 weeks):

1. Publish an EV-signed release
2. Microsoft submission portal: SmartScreen reputation request
3. VirusTotal scan: 0/70 false positives
4. Keep a fast issue-report channel open through the first 1000
   downloads
5. AV vendor whitelisting (chapter 18.4):
   - BitDefender — submit
   - Kaspersky — submit
   - Symantec — submit
   - Defender builds reputation automatically from its own telemetry

---

## 4. Pre-Release Quality Gate

Manual checklist before pushing any release tag (`v*.*.*`):

- [ ] `cargo test --lib` 100% pass
- [ ] `cargo test --lib --features vss` 100% pass
- [ ] `cargo clippy --lib --no-deps -- -D warnings` zero warnings
      (default + vss)
- [ ] `cargo fmt --all -- --check` zero diff
- [ ] `pnpm test:coverage` full suite pass over thresholds
- [ ] `pnpm build` error-free (tsc + vite)
- [ ] `pnpm bundle:check` within budget
- [ ] Version gate: tag semver core equals `tauri.conf.json` version
- [ ] `cargo audit` (v0.5) — zero vulnerabilities (install via
      `cargo install cargo-audit`)
- [ ] Bundle size delta < 5% vs the previous release
- [ ] `docs/DISCOVERY_LOG.md` current
- [ ] Manual smoke test: install (UAC prompt) → scan (quick + full)
      → recovery preview → recovery job → staging → undo →
      permanent delete (test fixture) → update check → tray menu
- [ ] `docs/THREAT_MODEL.md` risks current
- [ ] CHANGELOG.md — version bump entry written

---

## 5. Chapter 35 Cloud Connector Resilience (summary)

Detailed in chapter 35; short note for the current scope:

- **Route A** (premium): cloud backup endpoint — staging backed up to
  the cloud (chapter 26.5 trait stub)
- **Route B** (default, free): manual folder scan fallback — the
  local OneDrive/Dropbox/Google Drive folder is scanned
- **Hybrid** (chapter 35.3): local scan + optional cloud sync — the
  user sees the network cost
- **Cert-revoke readiness** (chapter 35.4): if the cloud endpoint
  certificate is revoked, a 30-day grace period applies and the user
  is offered an offline downgrade

Real cloud integration is out of current scope; only the trait
reservation exists (chapter 26.5).

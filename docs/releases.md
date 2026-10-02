# Release Engineering

## Channels

`stable` · `beta` · `nightly` (dev builds from main, suffixed
`-nightly.YYYYMMDD.sha`). Nightlies skip some packaging (no delta notes).

## Pipeline (GitHub Actions, `.github/workflows/`)

1. `validate.yml` — every push/PR: rustfmt, clippy -D warnings,
   cargo test (workspace), cargo deny (advisories/licenses), tsc, eslint,
   frontend unit tests.
2. `release.yml` — on tag `v*` (or manual channel dispatch):
   build matrix →
   - windows-x64, windows-arm64 (P1): MSI, NSIS exe
   - macos (universal2): DMG (signed/notarized when creds present)
   - linux-x64: AppImage, deb, rpm, tarball
   - linux-arm64 (P1): deb, tarball
   - headless: `linkfyr-cli-<os>-<arch>.tar.gz` + OCI image (optional)
   then: checksums SHA256SUMS, SBOM (CycloneDX), changelog from
   conventional commits, upload to GitHub Release, sign update manifests
   (Tauri updater signatures) when keys configured, publish latest.json
   for the updater, update install scripts' version pins.
3. **Idempotent publishing**: re-releasing the same tag does not fail
   or duplicate - the release job updates the existing release's builds
   (gh release upload --clobber replaces same-named assets) and merges
   the changelog section into the body (skipped when already present).
   Notes come from CHANGELOG.md's ## [<version>] section, falling back
   to [Unreleased].

## Artifacts layout (GitHub Releases)

```
LinkFYR_<ver>_x64-setup.exe / .msi     (Windows)
LinkFYR_<ver>_aarch64-setup.exe/.msi   (Windows ARM64)
LinkFYR_<ver>_universal.dmg            (macOS)
LinkFYR_<ver>_amd64.AppImage/.deb/.rpm (Linux)
linkfyr-cli_<ver>_windows_x64.zip      (headless/CLI)
linkfyr-cli_<ver>_linux_x64.tar.gz
latest.json                            (updater manifest, signed)
SHA256SUMS, sbom.cdx.json, CHANGELOG.md
```

## Install scripts (defensive, in /scripts, served from repo/raw or CDN)

- detect OS/arch; allow version/channel pinning via env
- fetch over HTTPS only; verify SHA-256 against published checksums
- `--no-exec` style piping protection: refuse when stdin is a pipe
  without `--yes`; never `sudo` without printing exactly what runs
- install to user prefix by default (`~/.linkfyr/bin`); `--system` opt-in
- writes uninstall script + PATH guidance; `--uninstall` supported
- rollback: previous version kept at `<prefix>/previous`

## Updater (Tauri 2)

- `tauri-plugin-updater` with Ed25519-signed `latest.json`
- channels via update URL param; user-selectable in Settings
- check → download → verify signature → apply on restart
- Windows/macOS/Linux parity; portable builds check-only (manual swap)

## Package managers (prioritized, not checklist-driven)

P1: WinGet (manifest PR automation), Scoop bucket, Homebrew cask/formula
(tap first), AUR (-git + -bin). P2: Chocolatey, Flatpak (needs
permissions review), Snap (sandbox friction — evaluate). Later: distro
repos, Docker Hub/GHCR for edge + headless.

# LinkFYR Security Policy

Status: living document Â· Last updated: 2026-09-30

## Principles

1. **Least privilege.** GUI/CLI unprivileged; only `linkfyrd` is elevated.
2. **Small, authenticated IPC.** Versioned, schema-checked messages; no
   reflection; deny-by-default method allowlist per caller.
3. **Memory safety.** Rust everywhere; `unsafe` requires a written
   justification comment + review; fuzz every parser.
4. **No custom crypto.** Standard primitives only (e.g. via `ring`/`rustls`
   family); primitives chosen in ADR before Fusion lands.
5. **Fail-safe defaults.** Security features declare fail-open or
   fail-closed per feature (see threat-model Â§policy); user-visible.
6. **Recoverability.** Config backups, safe mode, watchdog; LinkFYR must
   never leave the host offline after a crash (auto-restore filters on
   service death; heartbeat + supervisor restore on Windows service /
   systemd `Restart=` + `ExecStartPost` verification).

## Supply chain

- `cargo audit` / `cargo deny` (advisories + licenses) gate CI.
- `npm audit` gate; lockfiles committed; Dependabot/Renovate enabled.
- SBOM (CycloneDX) per release artifact; checksums (SHA-256) published;
  signing (Authenticode/macOS codesign+notarization, minisign for
  tarballs) when credentials exist; update manifests signed (Tauri
  updater minisign/Ed25519 signature required â€” updates without valid
  signatures are rejected).
- Release provenance: build logs + toolchain pinned (rust-toolchain.toml).

## Runtime hardening

- IPC: named pipe with restrictive DACL (Windows) / UDS in user-run dir
  with 0600 (unix) + peer-credential check; single-instance token;
  monotonic request IDs; malformed frames â†’ disconnect, not parse-and-hope.
- Secrets (VPN privkeys, Edge tokens): OS keychain (Credential Manager /
  Keychain / libsecret); never in config JSON; never logged (redaction
  filter on log sink).
- Logs: bounded ring + rotating files; destination IPs/domains minimized
  by default; `Export Diagnostic Report` is explicit, previewable, with
  redaction toggles.
- Updates: TLS-only fetches, cert pinning where feasible, signature check
  before swap, rollback on failed health check.

## Reporting

See README (security.txt path planned with public infra). Vulnerabilities
in scope: local privilege escalation via IPC/service, rule bypass by
unprivileged processes, tunnel crypto, update compromise, telemetry leaks.

# Changelog

All notable changes to LinkFYR are documented here. Format based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versioning aims at
[SemVer](https://semver.org/).

## [Unreleased]

### Added (Phase 8 - mobile companion)

- `apps/mobile`: Tauri 2 mobile app (Android/iOS) sharing the desktop UI source; remote client for linkfyrd over the same IPC envelopes (HTTP + token). Setup gate stores the PC address + token on-device. docs/mobile.md covers init/dev/build. Frontend gate now typechecks and builds the mobile UI in Docker.


### Added (enforcement v0 � Phase 3 core)

- `linkfyr-enforce` crate: per-app firewall rules (Windows netsh
  advfirewall true per-program; Linux nftables per-owner, honestly
  labeled; macOS reported platform-limited), kill switch arm/disarm
  (fail-closed outbound hold with established + loopback escape),
  split-tunnel apply (executes the advisory routes), and Linux tc
  shaping (fq_codel root + prio/DSCP priority). All owner-tagged so
  removal only ever touches rules this tool created; unelevated runs
  list the exact commands. Registry: 105 implemented / 107 named.

## [Unreleased]



Phase 2.5/2.6 — Optimization toolkit, tool registry (100+ module
catalog), network bridges, background service, alerts.

### Added

- Tool registry: single catalog powering capabilities/CLI/UI; **80 real
  modules implemented** (NTP, WoL, UPnP, RDAP/ASN, DNSSEC posture,
  DNS-leak interception test, captive portal, VoIP MOS, exposure
  audits, DHCP/adapter controls, watch tools, snapshot/diff/export,
  local throughput server, and more), 20 preserved slots each naming
  its blocking infrastructure (total 100). Generic `optimize_run` IPC
  envelope means new tools never change the wire.
- New tools: DoH benchmark + fallback, DNS record lookups
  (A/AAAA/PTR/TXT/MX/CNAME/SRV), reverse DNS, resolver agreement
  (spoofing signal), latency monitor, jitter burst, ICMP ping,
  gateway latency, IPv6 readiness, HTTP TTFB, TLS check, port scan
  (bounded), per-app connections + listening ports, ARP table, proxy
  configuration, hosts file, public IP (opt-in).
- Network bridge manager (`linkfyr-bridge`): Hyper-V VMSwitch with
  embedded teaming, New-NetNat fallback (labeled as NAT, not L2),
  Linux `ip link`, macOS `ifconfig` bridges; injection-validated
  specs; exact commands listed when not elevated.
- `linkfyrd` background service (`linkfyr-daemon`): authenticated
  loopback transport reusing the IPC envelope (0600 token,
  constant-time compare, loopback-only), Windows SCM wrapper,
  systemd unit, launchd plist; exam-safe operation.
- Alert engine: interface up/down + health-drop transitions,
  100-entry ring, `linkfyr alerts`, `linkfyr watch`.
- CLI: `optimize list|run`, `bridge list|create|remove`, `alerts`,
  `watch`, `daemon status`.
- Docker gate now cross-checks the Windows service code
  (`cargo check --target x86_64-pc-windows-gnu`) so every platform
  path is compile-verified, still without touching the host.

### Security

- Threat model T15 (daemon transport auth, loopback + token now,
  named-pipe/UDS hardening next); bridge specs validate names and
  members to keep subprocess arguments injection-safe.

### Previous batch (Phase 2.5)

- `linkfyr-optimize` crate: real network optimization tools, no mocks.
  - DNS benchmark (hand-rolled minimal DNS client over UDP; cached +
    uncached-path medians, blended score) and OS apply (netsh /
    networksetup / resolvectl / nmcli) with captured-previous restore.
  - Route scan: IPv4 vs IPv6 TCP-connect latency to any target with
    happy-eyeballs penalty detection and control endpoints.
  - Bufferbloat measurement: parallel transfer load phases with an
    independent probe target; added-latency grading A+ to F.
  - Speed test: real transfers against Cloudflare-compatible or
    self-hosted endpoints (rustls via ureq, no native TLS).
  - Path MTU discovery: DF-ping binary search per OS (unprivileged),
    black-hole verdicts for VPN/PPPoE overhead.
  - TCP stack audit: Linux sysctl/qdisc, Windows netsh, macOS sysctl
    readers with exact, elevation-honest fix commands.
  - Wi-Fi analyzer: netsh / nmcli / airport parsers, 2.4/5 GHz channel
    congestion scoring, concrete best-channel recommendation.
  - Route audit: routing-table parsers for all three OSes; multi-WAN
    and VPN 0.0.0.0/1 + 128.0.0.0/1 full-tunnel detection.
  - Repair actions: DNS cache flush with honest outcome reporting.
  - Capability detection for every tool (available / elevated /
    unavailable / platform-limited) surfaced in UI and CLI.
- IPC v1 additions (additive): 11 optimize requests/responses,
  camelCase wire format guarded by regression tests.
- Engine dispatch runs tools on the tokio blocking pool.
- CLI: `linkfyr optimize capabilities|dns|apply-dns|routescan|bloat|
  speedtest|mtu|wifi|tcp|routes|flush-dns` with `--json` everywhere.
- Desktop app: Optimize view (Alt+5 / command palette) — master-detail
  tool index with capability dots, measured-result panels, animated
  grades and numbers, honest idle/running/failed states, one-click
  fastest-DNS apply with restore note.
- Docker verification suite: `docker-compose.test.yml` + wrapper
  scripts run the complete Rust and frontend gates in containers
  (nothing executes on the host); includes the Tauri crate so
  `cargo test --workspace` matches CI exactly.

### Changed

- Lockfile regenerated on stable toolchain (matches CI); Docker Rust
  image tracks `rust:1-slim-bookworm` (stable) instead of 1.85.
- pnpm build approval moved to `pnpm-workspace.yaml` (`allowBuilds`),
  required by pnpm 12.

### Security

- Threat model: T13 (toolkit mutations) and T14 (scan locality) added;
  fail-safe table covers measurement (pure observer) vs mutation
  (explicit, reversible, user-triggered only) tools.

## [0.1.0] - 2026-09-30

Phase 1 — Foundation (first usable vertical slice).

### Added

- Engine: interface discovery + per-interface telemetry at 1 Hz
  (`linkfyr-network`), with a deterministic `SimMonitor` for tests.
- Internet path quality probes: latency/jitter/loss via unprivileged TCP
  connects, rolling window aggregation (`linkfyr-telemetry`).
- Health engine v0: per-interface score with visible factor breakdown.
- Flow Rules engine core: predicate AST (app/domain/port/protocol/category/
  time/battery/health metrics), action classes (firewall/routing/shaping/
  profile), first-match-per-class semantics, exceptions (`linkfyr-rules`).
- Config store with backup rotation and safe mode: corrupt or future-version
  config degrades to defaults with a visible reason, never bricks the app.
- IPC API v1: versioned envelope, typed requests/responses/events,
  camelCase wire format guarded by regression tests (`linkfyr-ipc`).
- Desktop app (Tauri 2): live dashboard with eased-scale charts and smooth
  counters, interfaces view with factor breakdown, Internet view, settings
  (theme system/dark/light, animation full/reduced/off, expert mode),
  command palette (Ctrl+K), system tray with close-to-tray, interface
  up/down notifications.
- CLI: `status`, `interfaces`, `traffic` with `--json` and `--simulated`.
- Design system: dark/light tokens with WCAG-AA contrast enforced by test,
  custom networking icon set, reduced-motion honored end to end.
- CI: validate workflow (fmt, clippy -D warnings, tests, cargo-deny,
  frontend gates, tri-OS compile checks); release workflow building
  Windows x64/ARM64, macOS universal, Linux x64/ARM64 bundles plus headless
  CLI archives, checksums, and SBOM.
- Installer scripts (`scripts/install.sh`, `scripts/install.ps1`):
  checksum-verified, arch-detecting, pipe-safe, uninstallable.
- Documentation set: architecture, platform capability matrix, roadmap
  (full feature preservation), threat model, security policy, competitor
  matrix, user-demand research, protocol design, UX standard, ADR-0001.

### Security

- Local-only by default; no telemetry upload path exists.
- GUI/CLI unprivileged by design; privileged service surface specified in
  the threat model with per-feature fail-open/fail-closed policy.

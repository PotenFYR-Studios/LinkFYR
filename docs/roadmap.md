# LinkFYR Roadmap (feature-preservation master list)

Status: living document · Last updated: 2026-09-30

**Preservation rule:** no idea from the product specification is ever
deleted. Every feature is classified: **Now** (this phase) · **Next** ·
**Later** · **Experimental** · **Platform-limited** · **Research**.
Deferred features must keep their architectural hooks (crate/module
reserved, data model compatible).

## Phase 0 — Research & Architecture ✅ (this repo start)

- Competitor matrix (docs/competitors.md) — living doc
- User-demand research w/ evidence strength (docs/user-demand-research.md)
- Capability matrix (docs/platform-support.md)
- ADRs: 0001 shell (docs/adr/) — more follow each major decision
- Threat model + security policy (docs/threat-model.md, docs/security.md)

## Phase 1 — Foundation ✅ shipping now

**Now:** Tauri 2 app shell (Win/macOS/Linux desktop) · design system ·
UI shell (dashboard, interfaces, apps, diagnostics, settings, command
palette) · linkfyrd service skeleton · linkfyr-ipc typed API · interface
discovery · live per-interface telemetry (1 Hz) · health engine v0 ·
SQLite telemetry store · config store w/ backups + safe mode · logging
(bounded, redacted) · CI validate/test · release pipeline w/ artifacts ·
install.sh / install.ps1 (checksum-verified) · signed updater plumbing ·
CLI: status/interfaces/traffic.

**Next (Phase 1.5):** privileged service hardening split (named-pipe ACLs,
SO_PEERCRED), tray app, Windows ARM64 CI, macOS signing/notarization
(when credentials exist), WinGet/Scoop manifests, AUR.

## Phase 2 — Monitoring deep-dive

**Now:** per-app/process traffic table · destinations (domain/IP/ASN) ·
sessions view · historical analytics (1s→1m→1h buckets) · Network Time
Machine (event log + scrubber) · health engine v1 (multi-factor, shown).
**Next:** per-app history rollups, ASN/country enrichment (offline db),
geographic map (opt-in), protocol mix. **Platform-limited:** Android/iOS
granularity. **Research:** eBPF flow tagging on Linux for lower overhead.

## Phase 2.6 — Tool registry, bridges, daemon (shipped)

**Shipped:** the tool **registry** — the single catalog powering
capabilities, CLI and UI; 30 modules implemented for real today
(DNS family incl. DoH + record lookups + resolver agreement, route
scan, latency monitor/jitter burst/ICMP ping/gateway, IPv6 readiness,
bufferbloat, speed test, TTFB/TLS, port scan, per-app connections,
listening ports, MTU, Wi-Fi, TCP audit, route audit, ARP, proxy,
hosts, flush, public IP opt-in, DNS apply) with the generic
`optimize run <id> key=value` envelope; 72 more modules named and
designed (total catalog 102) — no placeholders, each preserved slot has
its real mechanism chosen. **Network bridge manager**: tiered —
Hyper-V `New-VMSwitch` + embedded teaming (closest to the removed
Windows bridge), `New-NetNat` forwarding fallback (always labeled as
NAT, never as L2), Linux `ip link` bridges, macOS `ifconfig` bridges;
elevation-honest reports list exact commands when not elevated.
**linkfyrd background service**: hosts the engine over an
authenticated loopback transport (token 0600, constant-time compare,
loopback-only peers) reusing the exact IPC envelope; Windows SCM
service wrapper (`--service`, cross-compile-checked in CI/Docker),
systemd unit, launchd plist; exam-safe operation (close the GUI,
telemetry + bridges keep running). **Alert engine**: interface up/down
and health-drop transitions with a 100-entry ring surfaced via
`get_alerts`, CLI `linkfyr alerts` and `linkfyr watch`.

**Next:** implement remaining registry slots in batches (connection
history journal, DHCP renew/release, adapter reset, DNSSEC/DNS-leak,
wake-on-LAN, per-app limits once enforcement lands), daemon transport
hardening (named pipe DACL / UDS peer credentials), bridge member
auto-detection UI.

## Phase 2.5 — Optimization Toolkit (shipped)

**Shipped:** DNS resolver benchmark (real UDP queries, cached +
uncached-path scoring) + OS apply with captured-previous restore ·
multi-path route scan (IPv4 vs IPv6 happy-eyeballs penalty detection) ·
bufferbloat measurement with A+..F grading (latency under real load) ·
speed test (real transfers; Cloudflare-compatible endpoints or
self-hosted) · path-MTU discovery via DF pings (VPN/PPPoE black-hole
finder) · TCP stack audit (Linux sysctl/qdisc, Windows netsh, macOS
sysctl) with exact fix commands · Wi-Fi environment scan + channel
recommendation (netsh / nmcli / airport parsers) · routing table audit
(multi-WAN, VPN 0/1+128/1 hijack detection) · DNS cache flush repair ·
honest capability detection (available / elevated / unavailable /
platform-limited) rendered in UI and CLI. All free, all local, no mocks:
every number is measured. **Next:** TCP fixes apply-with-confirm,
Wi-Fi channel auto-monitoring, historical optimization score tracking,
network bridge manager (Linux `ip link`/macOS `ifconfig bridge` real
commands now that Windows removed its bridge UI; Windows path needs an
ADR - Wintun-based L2 or documented-as-unsupported), background service
(`linkfyrd`) so long-running optimizations and bridging survive GUI
closure (exam-mode safe operation).

## Phase 3 — Local traffic control

**Now:** allow/block/ask per app · per-app limits (down/up) · priorities ·
interface assignment per app · bandwidth guarantees (hierarchical HSFC
model) · borrowing/burst · profiles (Gaming/Streaming/Travel/Work/
Download/Custom) · profile auto-activation triggers (SSID, app launch,
time, charging, battery). **Next:** smart reservation solver, schedules,
app groups. **Platform-limited:** per-app control on iOS (policy).

## Phase 4 — Flow Rules + Composer + Automation

**Now:** rule AST (conditions: app/process/user/interface/SSID/domain/CIDR/
ASN/port/protocol/category/time/battery/charging/health metrics/profile;
actions: allow/block/ask/throttle/guarantee/priority/reroute/interface
pick/VPN/DNS/profile/notify/webhook/diagnostics) · IF/THEN/UNLESS ·
beginner visual editor + expert editor · automation engine w/ dry-run,
logs, explain-why · Connection Composer (visual graph → rules lowering).
**Next:** rule sharing/import, versioned rule history, simulator mode.

## Phase 5 — Multi-WAN (local)

**Now:** failover (ordered, health-gated) · weighted per-flow balancing ·
app/destination/protocol-based routing · latency/cost/reliability-aware
selection · adaptive link scheduler v1 (documented weights) · connection
modes (Fusion*/Smart/Failover/Redundant*/LowestLatency/Download/Gaming/
Streaming/Conferencing/CostSaver/BatterySaver/LocalMultiWAN/Custom).
**Next:** scheduler explainability UI upgrades, per-mode benchmark suite.
**Research:** BBR/fq integration per-mode on Linux.

## Phase 6 — Fusion (bonding) + Edge

**Now(→Later as infra lands):** Fusion tunnel protocol (docs/protocol.md) ·
adaptive per-path scheduler · reordering/dedup · reconnect w/ seamless
failover · redundancy modes (off/critical/adaptive/full) · self-hosted
Edge (docker run linkfyr/edge) · hosted Edge (region rollout follows real
infrastructure — no fake regions) · traffic accounting/telemetry.
**Research (ADR required before build):** multipath-QUIC vs MPTCP vs custom
UDP; FEC codecs (RLNC/fountain) benchmarked; 0-RTT roaming; static
addresses. **Explicit honesty rule:** load balancing ≠ single-flow bonding;
UI explains the difference.

## Phase 7 — VPN / DNS / Privacy

**Now:** DNS engine (system/custom/DoH/DoT/DoQ, per-app/per-profile DNS,
fallbacks, benchmarking, custom hosts, blocklists/allowlists, family
filter categories) · multi-VPN routing (per-app/per-destination; WireGuard
first, then OpenVPN/SOCKS5/HTTP proxies/platform VPNs) · kill switches ·
privacy center (local-only mode, retention, telemetry opt-out, encrypted
config sync opt-in). **Next:** split-tunnel UX unify with Flow Rules,
DNSSEC validation posture, leak tests as first-class diagnostics.

## Phase 8 — Remote & multi-device

**Now(→Later):** end-to-end authenticated remote management (phone → PC) ·
config sync (encrypted, selective; secrets never plaintext) · mobile
companion app (Tauri mobile) → full mobile client · multi-device support
(shared profiles/rules via encrypted sync) · fleet management (policies,
centralized diagnostics, telemetry with privacy controls, RBAC,
deployment automation) — all free, self-hostable coordination. **Platform-limited:** iOS
background constraints documented in capability matrix.

## Phase 9 — Router / headless / embedded

**Later:** linkfyrd as system service (systemd units shipped) · OCI/gateway
images · OpenWrt feasibility (memory budget study) · NAS packages · BSD
evaluation. **Research:** MIPS memory ceilings, flash footprint.

## Cross-cutting commitments (all phases)

- Explainability: every automatic action explains why + one-click undo.
- Accessibility: keyboard-first, screen reader, reduced motion, WCAG AA.
- Performance budgets (docs/architecture.md §9) enforced in CI benchmarks.
- Beginner/Expert progressive disclosure everywhere; expert mode hides
  nothing, beginner mode drowns no one.
- Security: threat-model review per phase; fuzzing for parsers; SBOM in
  every release; dependency audit gate.
- **No monetization.** LinkFYR is fully free (Apache-2.0 with the
  Commons Clause condition: use and build on it freely, do not sell the
  software itself as a paid product). Every feature ships to
  everyone: self-hosted Edge, hosted-bonding client support, CLI/API,
  unlimited history. No tiers, no entitlement gates, no locked controls.
  If hosted Edge infrastructure is ever operated, it must be fundable
  without paywalling software capability (donations/community hosting or
  user-supplied servers only; never feature-gated local functionality).

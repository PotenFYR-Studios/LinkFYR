# LinkFYR Threat Model (STRIDE-annotated, living)

Scope: `linkfyrd` (privileged), IPC channel, GUI/CLI clients, Fusion
tunnel + Edge, updater, telemetry store.

## Assets

A1 privilege of `linkfyrd`; A2 user's Internet availability; A3 traffic
metadata (destinations, apps); A4 tunnel secrets/keys; A5 update channel;
A6 config/rules; A7 hosted-Edge trust.

## Trust boundaries

TB1 unprivileged app ↔ `linkfyrd` IPC · TB2 `linkfyrd` ↔ kernel filters ·
TB3 client ↔ Edge over WAN · TB4 app ↔ update servers · TB5 any local
process ↔ telemetry store.

## Threats & mitigations

| # | STRIDE | Threat | Mitigation |
|---|---|---|---|
| T1 | S/E | Malware on host calls IPC to whitelist itself / disable rules | Pipe DACL + UDS perms; caller allowlist (only installed UI/CLI); mutating sessions require a token created interactively; audit log of privileged mutations |
| T2 | S/E | Service hijack → persistent LPE | Minimal attack surface: no scripting in service, no plugins in v1, `unsafe` audit, fuzzed parsers, ASLR/DEP defaults, service runs with limited privileges beyond network config |
| T3 | D | linkfyrd crash leaves filters applied → host offline | Heartbeat + supervisor auto-restore ("fail-open watchdog"); filters carry owner tag + TTL re-assertion; on service death, platform cleaner removes owned objects (documented per feature) |
| T4 | D | Malicious config/rules brick connectivity | Config schema validation, dry-run apply + auto-rollback, safe mode (boot w/ LinkFYR disabled), config backups (N-keep) |
| T5 | I | Local process reads browsing metadata from store | Store files ACL'd to user; telemetry redaction at write time; retention limits |
| T6 | I | Logs leak destinations/secrets | Redaction sinks; secret types implement zero-on-drop + no-Debug |
| T7 | I/E | Malicious/compromised Edge node | Client-authenticated + encrypted tunnel (Edge sees only forwarding metadata it must have); self-hosted mode removes hosted trust entirely; hosted regions published w/ jurisdiction |
| T8 | I | MITM on tunnel setup | Server auth pinned to Edge public keys (TOFU + optional pinning); no plaintext fallback |
| T9 | T | Update compromise (A5) | Signed manifests (Ed25519), TLS-only, signature-before-swap, staged rollout, rollback |
| T10 | R | User cannot tell why traffic moved | Explainability events are first-class, signed sequence, queryable (also aids incident forensics) |
| T11 | D | Rule evaluation DoS (huge rule sets) | Rule budget limits, evaluation timeout, O(1) indexes on common predicates, benchmarks in CI |
| T12 | S | Malicious app spoofs another process identity to evade per-app rules | Windows: PID→image re-resolution each evaluation + WFP kernel attribution where available; Linux: uid/cgroup matching (harder to spoof); residual risk documented |
| T13 | S/E | Optimization toolkit mutations (DNS apply, TCP sysctls) abused to redirect or break resolution | Mutations are explicit user actions only (never automatic); previous values captured before every apply and restorable one click; loopback interfaces never targeted; commands run argument-vector style (no shell); outcomes honestly reported (applied / needs_elevation / failed); elevation checked with the OS, not assumed |
| T14 | I | Optimization scans reveal topology (DNS queries, TCP connects, Wi-Fi surroundings) to the machine's operator only | All measurements stay local: no result ever leaves the process (local-only default); scan targets are user-specified; Wi-Fi scans are read-only OS queries |
| T15 | S/E | linkfyrd loopback transport abused by another local process | v1: TCP on 127.0.0.1 only + per-user token (0600, constant-time compare); loopback peer enforced per connection. v2 (hardening): Windows named pipe with DACL + unix domain socket + SO_PEERCRED so unauthenticated local peers cannot even connect; mutating requests additionally gated (T1 rules) |

## Fail-open / fail-closed policy per feature

| Feature | Default on service failure | User-adjustable |
|---|---|---|
| Monitoring only | nothing to fail (pure observer) | — |
| Per-app block rules | fail-open (blocks released) | no (safety) |
| Kill switch | **fail-closed** (traffic held) while armed; auto-disarm option | yes |
| Fusion tunnel | fail-open to underlying route (no black hole) | yes (hold-last vs release) |
| DNS takeover | fail-open to system DNS | yes |
| Update check | fail-open (offline stays offline-safe) | — |
| Optimization toolkit: measurements (DNS bench, route scan, bloat, speedtest, MTU, Wi-Fi, audits) | fail-open: pure observers, a failed tool reports honestly and changes nothing | no mutation involved |
| Optimization toolkit: mutations (DNS apply now; TCP sysctl/netsh apply later) | fail-open: apply is one-shot, reversible from captured previous values; never runs on boot or unattended | yes (user-triggered only) |

## Open risks (tracked)

- Windows user-mode WFP attribution vs admin-run apps (T12) — callout
  driver deferred; matrix documents the gap.
- Linux distros without nftables (legacy iptables hosts) — detection +
  degraded mode; capability surfaced honestly.
- macOS NE approval friction — onboarding copy + reset tooling.

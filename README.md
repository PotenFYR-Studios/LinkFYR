<div align="center">

<img src="https://capsule-render.vercel.app/api?type=waving&color=0:6b97ff,50:9b8cff,100:3ddc97&height=220&section=header&text=LinkFYR&fontSize=52&fontColor=ffffff&fontAlignY=34&desc=Monitor%20%C2%B7%20Optimize%20%C2%B7%20Bridge%20%C2%B7%20Control%20%E2%80%94%20every%20connection%2C%20one%20command%20center&descSize=18&descAlignY=55&animation=twinkling" width="100%" alt="LinkFYR banner"/>

[![GitHub](https://img.shields.io/badge/GitHub-PotenFYR--Studios-181717?style=for-the-badge&logo=github&logoColor=white&labelColor=1c1e26)](https://github.com/PotenFYR-Studios/LinkFYR)
[![Discord](https://img.shields.io/badge/Discord-Join%20us-5865F2?style=for-the-badge&logo=discord&logoColor=white&labelColor=1c1e26)](https://discord.com/invite/zUaN2FPBec)
[![Releases](https://img.shields.io/github/v/release/PotenFYR-Studios/LinkFYR?style=for-the-badge&logo=github&logoColor=white&labelColor=1c1e26&color=6b97ff)](https://github.com/PotenFYR-Studios/LinkFYR/releases)
[![View](https://komarev.com/ghpvc/?username=PotenFYR-Studios-LinkFYR&color=3ddc97&style=for-the-badge&label=VIEW&labelColor=1c1e26)](https://github.com/PotenFYR-Studios/LinkFYR)

[![CI](https://img.shields.io/github/actions/workflow/status/PotenFYR-Studios/LinkFYR/validate.yml?branch=main&style=flat-square&logo=githubactions&label=CI&color=2ea043&labelColor=1c1e26)](https://github.com/PotenFYR-Studios/LinkFYR/actions/workflows/validate.yml)
[![Platforms](https://img.shields.io/badge/platforms-Windows%20%C2%B7%20macOS%20%C2%B7%20Linux%20%C2%B7%20x64%2Farm64%2Funiversal-9b8cff?style=flat-square&labelColor=1c1e26)](#platforms)
[![Tools](https://img.shields.io/badge/registry-105%20live%20tools%20%2F%20107%20named-6b97ff?style=flat-square&labelColor=1c1e26)](#optimization-toolkit)
[![license](https://img.shields.io/badge/license-Apache--2.0%20%2B%20Commons%20Clause-6b97ff.svg?style=flat-square&labelColor=1c1e26)](LICENSE)

[![Typing SVG](https://readme-typing-svg.demolab.com?font=Fira+Code&weight=600&size=20&pause=1200&color=6B97FF&center=true&vCenter=true&width=800&lines=95+real+network+tools+in+one+registry;Reduce+ping%2C+jitter+and+bufferbloat+with+measurements;Tiered+bridging+where+Windows+removed+it;Background+service+%2B+tray%2C+exam-safe;Free+and+open%2C+local-only+by+default)](https://github.com/PotenFYR-Studios/LinkFYR)

**LinkFYR**: the operating system for your Internet connections. One control layer for what your apps do on the network: which interface carries them, how much bandwidth they get, which VPN or DNS they use, how traffic behaves when quality changes - and a full, explainable history of why.

[Releases](https://github.com/PotenFYR-Studios/LinkFYR/releases) � [Documentation](docs/) � [Roadmap](docs/roadmap.md) � [Daemon install](docs/daemon.md) � [Contributing](CONTRIBUTING.md) � [Security](SECURITY.md)

</div>

---

## Why LinkFYR

Every network tool does one slice: a speed test here, a Wi-Fi scanner there, a latency monitor somewhere else - and the enterprise ones paywall the useful half. LinkFYR is one free, open control layer that measures, explains, and (where the OS allows) fixes your connectivity:

- **105 live tools, one registry** - DNS benchmarking and DoH fallback, route scanning with IPv6-penalty detection, bufferbloat grading (A+ to F), MTU black-hole discovery, per-app connection tables, UPnP mapping audits, TLS certificate expiry, DNSSEC posture, DNS-leak interception tests, Wake-on-LAN, NTP clock skew, captive-portal detection, VoIP MOS estimation and much more. Every measurement is real; nothing is simulated or estimated.
- **Tiered network bridging** - Windows removed its bridge UI; LinkFYR drives the supported alternatives (Hyper-V virtual switch with teaming, then a clearly-labeled NetNat fallback) and true L2 bridges on Linux and macOS. Unelevated runs list the exact commands instead of pretending.
- **Exam-safe background service** - `linkfyrd` keeps monitoring, optimization jobs and bridges alive with the app closed (Windows service, systemd, launchd), behind an authenticated loopback transport.
- **Desktop + CLI parity** - a polished Tauri 2 app (live dashboard, registry-driven Optimize view, bridges, alerts, tray with close-to-tray preference) and a first-class `linkfyr` CLI speaking the same versioned IPC. Same engine, same answers, zero drift.
- **Honest by design** - every tool reports its real capability state (available / needs elevation / unavailable / platform-limited), failed probes stay visible, and any capability the OS does not offer is documented rather than faked. Local-only by default: no account, no cloud, no telemetry upload path.
- **Free and open** - every feature ships to everyone. No tiers, no entitlement gates, no locked controls.

> The architecture lives in [`docs/architecture.md`](docs/architecture.md); every feature ever specified is preserved and classified in [`docs/roadmap.md`](docs/roadmap.md). The registry currently names **107 modules: 105 implemented today**, the remaining 2 each name the enforcement-layer work they are waiting on.

## Quick start (development)

Prerequisites: Rust stable, Node 22+, pnpm (`npm i -g pnpm`), and the
[Tauri 2 prerequisites](https://tauri.app/start/prerequisites/) for your OS.

```bash
pnpm install
pnpm build                                # frontend -> apps/desktop/dist
cargo run -p linkfyr-cli -- status        # CLI against real interfaces
cargo run -p linkfyr-cli -- --simulated status   # deterministic simulator
cargo tauri dev                           # desktop app (hot reload)
# LINKFYR_SIM=1 cargo tauri dev           # app against the simulator
```

## CLI

```
linkfyr status                            # totals, Internet quality, interface table
linkfyr interfaces [--json]               # full interface inventory
linkfyr traffic --seconds 10              # live per-second sampling
linkfyr optimize list                     # the full 101-module registry
linkfyr optimize run latency_monitor target=1.1.1.1 samples=30
linkfyr bridge list|create|remove         # tiered bridging (elevated)
linkfyr alerts / watch                    # interface + health transition events
linkfyr daemon status                     # ping a running linkfyrd service
```

Everything accepts `--json`; the desktop app and CLI are interchangeable clients of the same engine.

## Optimization toolkit

The registry is the single catalog powering capabilities, CLI and UI. Highlights:

| Tool | What it really does |
|---|---|
| `dns_benchmark` / `apply` | Real UDP DNS queries, cached + fresh-path scoring; applies via netsh/networksetup/resolvectl/nmcli with captured-previous restore |
| `doh_benchmark` | DNS-over-HTTPS ranking - the fallback when UDP 53 is blocked |
| `route_scan` | IPv4 vs IPv6 TCP-connect comparison, flags the slow family |
| `bloat` | Saturates the link while probing latency; grades added lag A+ to F |
| `speedtest` | Real transfers; Cloudflare-compatible or self-hosted endpoints |
| `mtu` | DF-ping binary search; finds VPN/PPPoE black holes |
| `wifi_scan` | OS Wi-Fi scan, congestion scoring, best-channel recommendation |
| `connections` | Per-app socket table (which process talks where, right now) |
| `upnp_map` | SSDP discovery + SOAP enumeration of router port mappings |
| `cert_expiry` | Manual rustls handshake + x509 parse for certificate lifetime |
| `dnssec_check` / `dns_leak` | AD-bit validation posture; UDP-53 interception detection |
| `clock_skew` / `wake_on_lan` | Real NTP client; broadcast magic packets |
| `latency_race` / `speed_compare` | Per-interface races via SO_BINDTODEVICE (Linux) |
| `net_time_machine` | Timeline backend over the connection/destination journals |

Run any of them with `linkfyr optimize run <id> key=value ...` or from the Optimize view (`Alt+5`). `linkfyr optimize list` prints the implemented set plus the designed-but-blocked slots, each naming its blocker.

## Verification

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace          # 200+ engine/tool/IPC tests
pnpm typecheck && pnpm lint && pnpm test && pnpm build   # 40 UI tests
```

### Fully in Docker (no host toolchain, nothing runs on your machine)

```bash
# Windows
powershell -File scripts/test-docker.ps1
# Linux / macOS
./scripts/test-docker.sh
```

Builds `docker/test.Dockerfile` (Rust stable + Tauri system deps + the
Windows cross-target so even `cfg(windows)` service code is compile-checked)
and `docker/frontend.Dockerfile`, then runs the complete gate. Test traffic
in containers is real: live sockets against local servers, real `ping` DF
probes, real UDP DNS exchanges, a real rustls handshake.

## Platforms

Windows, macOS and Linux; x64, ARM64 and universal builds per release. Per-OS capability is documented honestly in [`docs/platform-support.md`](docs/platform-support.md) - where an OS does not offer an API, LinkFYR says so and picks the nearest supported alternative.

## Repository layout

```
crates/
  linkfyr-model       wire contract types (Rust <-> TS), versioned, camelCase JSON
  linkfyr-network     platform abstraction: interface discovery, counters, simulator
  linkfyr-telemetry   sampling engine, probes, ring buffers, health engine
  linkfyr-rules       Flow Rules AST + evaluation (IF/THEN/UNLESS, priorities)
  linkfyr-optimize    the tool registry: 95 real measurement/control modules
  linkfyr-bridge      tiered network bridge manager
  linkfyr-daemon      linkfyrd background service (loopback auth transport)
  linkfyr-core        engine orchestrator + config store (backups, safe mode)
  linkfyr-ipc         stable IPC API surface (request/response/event envelope)
  linkfyr-cli         the `linkfyr` binary
  linkfyr-protocol    Fusion tunnel protocol (Phase 6 skeleton)
  linkfyr-edge        Edge node (Phase 6 skeleton)
apps/desktop          Tauri 2 desktop shell - the only crate allowed to import Tauri
apps/mobile           Tauri 2 mobile companion (remote client for linkfyrd)
deploy/               systemd unit, launchd plist for linkfyrd
packages/types        TypeScript mirror of the wire contract
deploy/               systemd unit, launchd plist for linkfyrd
docs/                 architecture, roadmap, threat model, capability matrix, ADRs
```

Architecture rule (ADR-0001): all capability lives in Rust behind `linkfyr-ipc`; the GUI, CLI, daemon and future remote/mobile clients are interchangeable frontends over the same versioned API.

## Documentation

| Doc | Contents |
|---|---|
| [`docs/architecture.md`](docs/architecture.md) | process topology, crate map, data flow, budgets |
| [`docs/platform-support.md`](docs/platform-support.md) | per-OS capability matrix (never fakes parity) |
| [`docs/roadmap.md`](docs/roadmap.md) | every spec feature, classified Now/Next/Later, nothing dropped |
| [`docs/daemon.md`](docs/daemon.md) | installing linkfyrd (Windows service, systemd, launchd) |
| [`docs/mobile.md`](docs/mobile.md) | building the Tauri 2 mobile companion (Android/iOS) |
| [`docs/threat-model.md`](docs/threat-model.md) | STRIDE model + fail-open/fail-closed policy table |
| [`docs/security.md`](docs/security.md) | IPC hardening, supply chain, signing, secrets |
| [`docs/competitors.md`](docs/competitors.md) | living competitor matrix with evidence grades |
| [`docs/user-demand-research.md`](docs/user-demand-research.md) | community demand to requirements, evidence-graded |
| [`docs/protocol.md`](docs/protocol.md) | Fusion bonding protocol design (pre-benchmark) |
| [`docs/ux.md`](docs/ux.md) | design system, motion dials, accessibility standard |

## Privacy

Local-only by default. Measurements never leave the device unless you explicitly run an opt-in external tool (public IP, geolocation, RDAP); there is no account, no cloud dependency and no telemetry upload path. See [`docs/security.md`](docs/security.md).

## Contributing

PRs welcome - see [CONTRIBUTING.md](CONTRIBUTING.md). The verification gate runs entirely in Docker, so contributors never need a host toolchain. Security reports follow [SECURITY.md](SECURITY.md).

## License

Apache-2.0 with the Commons Clause condition - free to use, fork and build on, including commercially; you may not sell the software itself as a paid product. See [LICENSE](LICENSE).

## Star History

[![Star History Chart](https://api.star-history.com/svg?repos=PotenFYR-Studios/LinkFYR&type=Date)](https://star-history.com/#PotenFYR-Studios/LinkFYR&Date)

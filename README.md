<div align="center">

# LinkFYR

**The operating system for your Internet connections.**

One control layer for what your apps do on the network: which interface carries
them, how much bandwidth they get, which VPN or DNS they use, how traffic
behaves when quality changes â€” and a full, explainable history of why.

</div>

## Status: Phase 1 (Foundation) â€” usable vertical slice

- Live interface discovery + per-interface telemetry (1 Hz) on Windows, macOS, Linux
- Internet path quality (latency/jitter/loss via unprivileged TCP probes)
- Explainable multi-factor link health (factors are shown, never a black box)
- Polished Tauri 2 desktop app: live dashboard, charts with eased scales,
  interfaces view, Internet view, settings (theme/motion/expert), command
  palette (Ctrl/Cmd+K), system tray, transition notifications
- `linkfyr` CLI: `status`, `interfaces`, `traffic` (+ `--json`, `--simulated`)
- Flow Rules engine core (evaluation semantics locked by tests; editors land in Phase 4)
- Config store with backup rotation + safe mode (corrupt config never bricks you)
- CI: fmt/clippy(deny)/test/deny + typecheck/lint/test/build; release pipeline builds installers on tags

The full product arc â€” Fusion bonding, Flow Rules UI, per-app control, multi-VPN,
DNS engine, remote management â€” is specified and roadmap-locked in `docs/`.
Nothing from the product spec was dropped; deferred features keep their
architectural hooks (see `docs/roadmap.md`).

## Quick start (development)

Prerequisites: Rust stable, Node 22+, pnpm (`npm i -g pnpm`), and the
[Tauri 2 prerequisites](https://tauri.app/start/prerequisites/) for your OS.

```bash
pnpm install
pnpm build                      # frontend -> apps/desktop/dist
cargo run -p linkfyr-cli -- status          # CLI against real interfaces
cargo run -p linkfyr-cli -- --simulated status   # deterministic simulator
cargo tauri dev                 # desktop app (dev server + hot reload)
# LINKFYR_SIM=1 cargo tauri dev # app against the simulator
```

### CLI

```
linkfyr status                  # totals, Internet quality, interface table
linkfyr interfaces [--json]     # full interface inventory
linkfyr traffic --seconds 10    # live per-second sampling
```

### Tests

```bash
cargo test                      # 59 engine/protocol/policy tests
pnpm test                       # 22 UI unit/contrast/a11y tests
```

## Repository layout

```
crates/
  linkfyr-model       wire contract types (Rust â‡„ TS), versioned, camelCase JSON
  linkfyr-network     platform abstraction: interface discovery, counters, simulator
  linkfyr-telemetry   sampling engine, probes, ring buffers, health engine
  linkfyr-rules       Flow Rules AST + evaluation (IF/THEN/UNLESS, priorities)
  linkfyr-core        engine orchestrator + config store (backups, safe mode)
  linkfyr-ipc         stable IPC API surface (request/response/event envelope)
  linkfyr-cli         the `linkfyr` binary
  linkfyr-protocol    Fusion tunnel protocol (Phase 6 skeleton, docs/protocol.md)
  linkfyr-edge        Edge node (Phase 6 skeleton, docs/edge.md)
apps/desktop          Tauri 2 shell â€” the only crate allowed to import Tauri
packages/types        TypeScript mirror of the wire contract
docs/                 architecture, roadmap, threat model, capability matrix, ADRs
```

Architecture rule (ADR-0001): all capability lives in Rust behind
`linkfyr-ipc`; the GUI, CLI, and future daemon/mobile clients are
interchangeable frontends over the same versioned API.

## Documentation

| Doc | Contents |
|---|---|
| `docs/architecture.md` | process topology, crate map, data flow, budgets |
| `docs/platform-support.md` | per-OS capability matrix (never fakes parity) |
| `docs/roadmap.md` | every spec feature, classified Now/Next/Later, nothing dropped |
| `docs/threat-model.md` | STRIDE model + fail-open/fail-closed policy table |
| `docs/security.md` | IPC hardening, supply chain, signing, secrets |
| `docs/competitors.md` | living competitor matrix with evidence grades |
| `docs/user-demand-research.md` | community demand â†’ requirements, evidence-graded |
| `docs/protocol.md` | Fusion bonding protocol design (pre-benchmark) |
| `docs/ux.md` | design system, motion dials, accessibility standard |

## Privacy

Local-only by default. Telemetry never leaves the device; there is no account,
no cloud dependency, and no telemetry upload path. See `docs/security.md`.

## License

MIT OR Apache-2.0.

## Verification

Run before claiming anything works:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm typecheck && pnpm lint && pnpm test && pnpm build
```

### Fully in Docker (no host toolchain, nothing runs on your machine)

The complete gate also runs inside containers; this is the required
method when the host must stay untouched:

```bash
# Windows
powershell -File scripts/test-docker.ps1
# Linux / macOS
./scripts/test-docker.sh
```

That builds `docker/test.Dockerfile` (Rust stable + the system packages
the Tauri crate needs) and `docker/frontend.Dockerfile` (Node 24 +
pnpm), then runs fmt, clippy `-D warnings`, the full workspace test
suite, the CLI smoke check, and the frontend typecheck/lint/test/build
gates. Test traffic in containers is real: live sockets against local
servers, real `ping` DF probes, real UDP DNS exchanges.

## Optimization toolkit

`linkfyr optimize <subcommand>` and the Optimize view in the app run
real, unprivileged measurements and report exactly what happened:

| Tool | What it really does |
|---|---|
| `dns` | Sends DNS queries over UDP to resolvers, ranks cached + fresh lookup times |
| `apply-dns` | Sets the OS resolver list via netsh/networksetup/resolvectl/nmcli; records previous values for restore |
| `routescan <host>` | Opens TCP connections over IPv4 and IPv6 and flags the slow family |
| `bloat` | Saturates the link with parallel transfers while probing latency; grades added lag A+ to F |
| `speedtest` | Real download/upload transfers and connect latency; works with self-hosted endpoints |
| `mtu <host>` | Binary-searches the path MTU with DF pings; finds VPN/PPPoE black holes |
| `wifi` | Reads the OS Wi-Fi scan, counts channel congestion, recommends the best channel |
| `tcp` | Reads TCP stack settings (sysctl/netsh) and lists exact fix commands |
| `routes` | Parses the routing table; flags multi-WAN overlays and VPN full-tunnel hijacks |
| `flush-dns` | Flushes the system resolver cache |

Every tool reports honest capability state (available / needs elevation
/ unavailable) instead of pretending to work everywhere.

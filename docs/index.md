# LinkFYR Documentation

The operating system for your Internet connections: one free, open control
layer that measures, explains, and (where the OS allows) fixes your
connectivity. Local-only by default, honest by design.

## Start here

| Page | Contents |
|---|---|
| [architecture.md](architecture.md) | process topology, crate map, data flow, budgets |
| [roadmap.md](roadmap.md) | every feature, classified Now/Next/Later; nothing dropped |
| [platform-support.md](platform-support.md) | per-OS capability matrix (never fakes parity) |
| [daemon.md](daemon.md) | installing linkfyrd (Windows service, systemd, launchd) |
| [mobile.md](mobile.md) | building the Tauri 2 mobile companion (Android/iOS) |

## Security

| Page | Contents |
|---|---|
| [threat-model.md](threat-model.md) | STRIDE model + fail-open/fail-closed policy |
| [security.md](security.md) | IPC hardening, supply chain, signing, secrets |

## Design and background

| Page | Contents |
|---|---|
| [ux.md](ux.md) | design system, motion dials, accessibility standard |
| [competitors.md](competitors.md) | living competitor matrix with evidence grades |
| [user-demand-research.md](user-demand-research.md) | community demand to requirements |
| [protocol.md](protocol.md) | Fusion bonding protocol design |
| [edge.md](edge.md) | Edge node design |
| [flow-rules.md](flow-rules.md) | Flow Rules language design |
| [releases.md](releases.md) | release pipeline and idempotent publishing |
| [adr/](adr/) | architecture decision records |

## Quick start

```bash
git clone https://github.com/PotenFYR-Studios/LinkFYR
cd LinkFYR
pnpm install
cargo run -p linkfyr-cli -- status
```
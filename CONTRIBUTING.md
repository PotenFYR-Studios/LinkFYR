# Contributing to LinkFYR

Thanks for wanting to make LinkFYR better. This guide mirrors the org
conventions used across PotenFYR Studios repositories.

## Project Overview

LinkFYR is a free, open (Apache-2.0 + Commons Clause) control layer for a
machine's Internet connections: a 108-tool optimization registry, tiered network
bridges, a background service, desktop and mobile apps, and a CLI, all speaking
one versioned IPC contract. The architecture rule (ADR-0001) is that all
capability lives in Rust behind `linkfyr-ipc`; GUI, CLI, daemon, and mobile are
interchangeable clients. Every tool must be real: no mocks, no fabricated
results, and honest capability states when the OS cannot do something.

## Prerequisites

- Rust stable (1.85+)
- Node 22+ and pnpm (`npm i -g pnpm`)
- Docker (the full verification gate runs in containers)
- Optional, only for desktop/mobile dev: [Tauri 2 prerequisites](https://tauri.app/start/prerequisites/)

## Building

```bash
pnpm install
pnpm build                                    # frontend -> apps/desktop/dist
cargo build --workspace                       # engine, CLI, daemon, all crates
cargo run -p linkfyr-cli -- --simulated status
# Desktop app with hot reload:
cargo tauri dev                               # run from apps/desktop
```

## Testing Your Changes

The complete gate runs in Docker so you never need a matching host toolchain:

```bash
# Windows
powershell -File scripts/test/test-docker.ps1
# Linux / macOS
./scripts/test/test-docker.sh
```

That runs `cargo fmt --check`, clippy with warnings denied, the Windows
cross-target check, the full workspace test suite, CLI smokes, and the frontend
typecheck/lint/test/build (desktop and mobile). Container tests use real sockets,
real `ping` probes, real UDP DNS exchanges, and a real rustls handshake. The same
gate runs in CI [ci.yml](.github/workflows/ci.yml).

## Documentation

- Behavior changes update the matching section in `docs/src/docs/content.ts` (the docs site content) or the site under `docs/src`.
- New tools must appear in the registry with a real implementation and an honest
  capability state, be listed in `CHANGELOG.md`, and keep the roadmap's
  preservation rule: nothing is ever deleted, deferred features keep their hooks.
- The user-facing README stays accurate; never claim more than the code does.

## Pull Requests

- Follow [conventional commits](https://www.conventionalcommits.org/) (`feat:`,
  `fix:`, `docs:`, `chore:`, `refactor:`, `test:`).
- One focused change per PR; large work lands in reviewable slices.
- Use the pull request template; state what you exercised and on which platform.
- Platform-specific behavior must be labeled: if a tool only works on Linux,
  say so in its blurb and report `platform_limited` where appropriate.

## CI

`ci.yml` runs formatting, clippy, tests and the cross-platform compile checks on
every push and PR. `release.yml` publishes tagged releases with checksums and a
changelog section taken from `CHANGELOG.md`; re-tagging the same version updates
the builds and merges the changelog instead of duplicating.

## Reporting Issues

Use the issue templates: Bug Report, Feature Request, Documentation, Question.
For security topics read [SECURITY.md](SECURITY.md) first; actual
vulnerabilities go through private vulnerability reporting, never a public issue.

# Contributing to LinkFYR

Thank you for contributing. A few rules keep this codebase healthy.

## Ground rules

1. **Read `AGENTS.md` first.** It lists the non-negotiables: no Tauri types
   outside `apps/desktop`, additive-only wire changes, feature preservation,
   fail-safe policies for anything that touches traffic.
2. **Tests first.** New engine behavior lands with tests that failed before
   the implementation existed. Bug fixes need a reproduction test.
3. **Determinism.** Scheduler/health/rules logic must be testable without
   real NICs. Use `linkfyr_network::sim::SimMonitor` for scenarios.
4. **No fake parity.** If a platform can't do something, the capability
   matrix and UI say so. Never render fabricated data.

## Workflow

```bash
# 1. Fork + branch
git checkout -b feat/your-feature

# 2. Make the change with tests

# 3. Run the gates (CI enforces the same)
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm typecheck && pnpm lint && pnpm test && pnpm build

# 4. Open a PR with: what changed, why, evidence (test output / screenshots)
```

## Commit style

Conventional commits: `feat:`, `fix:`, `docs:`, `chore:`, `refactor:`,
`test:`. Scope optional: `feat(rules): median jitter windows`. Release notes
are generated from these.

## UI changes

Design direction lives in `docs/ux.md` (tokens, motion dials, accessibility).
The contrast test (`src/__tests__/contrast.test.ts`) must pass in BOTH
themes; fix the tokens, not the test. Every interactive element does
something real; every data view has empty/loading/error states.

## Licensing

By contributing you agree your work is licensed under the repository license
(Apache-2.0 with Commons Clause, see `LICENSE` and `NOTICE.md`).

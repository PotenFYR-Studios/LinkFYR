## Summary

<!-- What does this PR change, and why? Link the issue with "Fixes #123" where applicable. -->

## Type of change

- [ ] Bug fix
- [ ] New feature / enhancement (tools, UI, CLI, platform support)
- [ ] Documentation
- [ ] Refactor (no behavior change)
- [ ] Build / CI / chore

## Changes

<!-- Bullet list of the notable changes. For new tools, list the registry id and the platforms it runs on. -->

## How was this tested?

- [ ] Full gate passes in Docker: `powershell -File scripts/test/test-docker.ps1` (Windows) or `./scripts/test/test-docker.sh` (Linux/macOS)
- [ ] Manually exercised the affected tool/UI (which command or screen?)

<!-- Describe what you exercised: tool ids run, real output observed, platform used. -->

## Docs

- [ ] This PR changes no tool ids, IPC shapes, config keys, or user-visible behavior (skip this section)
- [ ] `docs/src/docs/content.ts` (docs site content) updated to match the new behavior
- [ ] `CHANGELOG.md` updated (new tools, behavior changes)
- [ ] README updated where relevant

## Checklist

- [ ] Commits follow [conventional commits](https://www.conventionalcommits.org/) (`feat:`, `fix:`, `docs:`, ...)
- [ ] Every new tool is real (no mocks, no fabricated results) and reports honest capability states
- [ ] No secrets, tokens, or private addresses in the diff or in test fixtures
# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.1.0 (main) | yes |

## Reporting a Vulnerability

**Please do not report security vulnerabilities through public GitHub issues.**

Use GitHub's **private vulnerability reporting** for this repository:

1. Go to the **Security** tab of [PotenFYR-Studios/LinkFYR](https://github.com/PotenFYR-Studios/LinkFYR/security)
2. Click **Report a vulnerability**
3. Describe the issue and how to reproduce it

If private vulnerability reporting is not available, contact the maintainers directly through [PotenFYR Studios](https://potenfyr.in).

Please include as much of the following as you can:

- Affected component (engine/IPC, `linkfyrd` transport, CLI, desktop shell, mobile companion, release pipeline)
- LinkFYR version and platform (Windows/macOS/Linux, architecture)
- Reproduction steps, including whether elevation is required to trigger it
- Impact assessment: what an attacker gains, and from what starting position.
  Attacks that require pre-existing admin/root are still interesting, but say so explicitly.

**Scrub before pasting:** remove daemon tokens, public IPs, hostnames you consider
private, and any real network inventory.

## What to Expect

- Acknowledgement target: within 48 hours.
- Triage with you in the thread; severity assessed on real impact, not on how
  exotic the path looks.
- Fix ships through the normal release pipeline. Artifacts are checksummed
  (`SHA256SUMS`) on every release, and signed update manifests when signing keys
  are configured.
- Credit in the changelog unless you prefer otherwise.

## Scope Notes

**In scope:** daemon token and transport authentication (threat model T15), IPC
surface validation, tool subprocess argument handling (injection through targets,
interface names, bridge specs, program paths), elevation boundaries, the release
and update pipeline, and the mobile setup gate.

**Out of scope:** social engineering, vulnerabilities in third-party services a
tool queries (DoH providers, RDAP, Cloudflare), and purely local denial of
service that requires the user to run a destructive action they explicitly
confirmed.

**Built-in defenses** (full detail in [docs/content/threat-model.md](threat-model.md)):

- Local-only by default; there is no telemetry upload path anywhere in the code.
- `linkfyrd` binds loopback unless remote access is explicitly enabled; token is
  0600, compared in constant time, and remote requests require it.
- Every subprocess invocation is argument-vector style (no shell), with target
  validation and hard timeouts.
- Enforcement rules are owner-tagged: removal can only ever delete what LinkFYR
  created.
- Config corruption cannot brick the app: backup rotation plus safe-mode fallback.

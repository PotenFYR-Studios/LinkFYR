# Security Policy

## Supported versions

| Version | Supported |
| --- | --- |
| 0.1.x | yes |
| < 0.1 | no |

## Reporting a vulnerability

**Do not open a public issue for exploitable vulnerabilities.**

Report privately through [GitHub's private vulnerability reporting](https://github.com/PotenFYR-Studios/linkfyr/security/advisories/new) on this repository.

Include what you can:

- Affected component and version (`linkfyr --version` or the release tag)
- A minimal proof of concept or reproduction steps
- Impact assessment (what an attacker could do)
- Any suggested fix (optional)

You will get an acknowledgment within 72 hours, followed by status updates as
the fix progresses. We credit reporters in the release notes by default; tell
us if you prefer to stay anonymous.

## Scope

**In scope:**

- Local privilege escalation through `linkfyrd` or the desktop IPC surface
  (named pipe / unix socket authentication, command authorization)
- Rule bypass: an unprivileged process escaping per-app firewall, shaping, or
  routing policy in a way LinkFYR's capability matrix claims to prevent
- The Fusion tunnel protocol and Edge node (`linkfyr-protocol`,
  `linkfyr-edge`): authentication bypass, path confusion, replay, downgrade
- The update path: manifest forgery, signature verification bypass, downgrade
  attacks, artifact substitution
- Secret handling: tunnel keys or tokens leaking into logs, config files, or
  diagnostics exports
- Telemetry leakage: browsing/destination metadata leaving the device
  against the local-only promise

**Out of scope:**

- Weaknesses that require the operator to disable documented protections
  (fail-open policies the user chose, disabled kill switch)
- Vulnerabilities in the operating system or platform filtering layers
  themselves (report to the vendor)
- The simulator (`--simulated`, `LINKFYR_SIM=1`) — it is a test fixture, not
  a security boundary
- Denial of service by local administrators (they already control the machine)

## Built-in defenses

These are documented behavior, not claims to test around:

- The GUI/CLI never run elevated; the privileged service exposes a small,
  schema-checked IPC surface with deny-by-default authorization
- Config corruption falls back through backup rotation to safe mode; a bad
  config can disable LinkFYR but cannot disable your Internet
- Updates require a valid signature before swap (Ed25519-signed manifests)
- Logs are bounded and redacted; secrets are zero-on-drop types that never
  serialize; diagnostics export is explicit and previewable
- Per-feature fail-open/fail-closed policy is published in
  `docs/threat-model.md` and surfaced in the UI

## Data handling

LinkFYR is local-only by default: interface counters, latency probes, and
traffic history stay on device. Probe targets are configurable public
resolvers contacted with anonymous TCP connects. There is no telemetry
upload path, no account, and no cloud dependency in this repository.

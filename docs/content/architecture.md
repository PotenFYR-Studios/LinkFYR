# LinkFYR Architecture

Status: living document · Last updated: 2026-09-30

## 1. Product thesis

LinkFYR is *the operating system for your Internet connections*: a network
control layer that unifies what today requires Speedify + NetLimiter +
GlassWire + Little Snitch/Portmaster + a VPN client + diagnostics tools —
integrated, transparent, cross-platform, and privacy-first.

The competitive moat is **integration**: telemetry feeds automation,
automation feeds routing, routing respects reservations, reservations
respect link health, health feeds bonding, bonding integrates with VPN and
per-app policy, and history explains every automatic decision.

## 2. Process & trust topology

```
┌──────────────────────────────────────────────────────────────┐
│ Unprivileged                                                  │
│  linkfyr-gui (Tauri 2)   linkfyr (CLI)   linkfyr-tray         │
│        │                      │                               │
│        └────────┬─────────────┘                                │
│                 ▼                                              │
│  linkfyr-ipc  (typed, versioned API — no Tauri types here)    │
└─────────────────┬────────────────────────────────────────────┘
                  │ authenticated local IPC
                  │  • Windows: named pipe, ACL-restricted
                  │  • macOS:   unix domain socket (per-user)
                  │  • Linux:   unix domain socket (per-user)
                  │  • auth:    mutating-session token + origin check
┌─────────────────▼────────────────────────────────────────────┐
│ linkfyrd — privileged networking service (runs as SYSTEM/root)│
│  linkfyr-core      orchestration, config, profiles, policy    │
│  linkfyr-network   platform abstraction (see §4)              │
│  linkfyr-rules     Flow Rules engine + automations            │
│  linkfyr-telemetry ring buffer, storage, downsampling         │
│  linkfyr-protocol  Fusion multipath tunnel protocol           │
│  linkfyr-edge      Edge node (self-host or hosted)            │
└──────────────────────────────────────────────────────────────┘
```

Trust rules:

- The GUI/CLI **never** run elevated. They are clients of `linkfyrd`.
- `linkfyrd` exposes a deliberately small IPC surface; every message is
  schema-checked; unauthenticated or unversioned frames are dropped.
- Security-sensitive features define explicit **fail-open / fail-closed**
  policy (see docs/content/security.md) that the user can see and change.

## 3. Crate map (Cargo workspace)

| Crate | Role | Elevated? |
|---|---|---|
| `linkfyr-core` | Orchestrator: config store, profile state, scheduler harness, event bus, health engine | service |
| `linkfyr-network` | Platform abstraction: interfaces, per-app accounting, traffic control hooks | service |
| `linkfyr-rules` | Flow Rules AST, evaluation, automation triggers, Connection Composer lowering | service |
| `linkfyr-telemetry` | In-memory ring buffers + on-disk time-bucketed store, downsampling, retention | service |
| `linkfyr-optimize` | Optimization toolkit: DNS benchmark/apply, route scan, bufferbloat grading, speed test, MTU discovery, TCP audit, Wi-Fi analysis, route audit, repair actions, the 100+ tool registry. Pure-Rust, cross-platform, unprivileged measurement core; mutations (DNS apply) are explicit, reversible, elevation-honest | service |
| `linkfyr-bridge` | Network bridge manager: tiered Hyper-V SET / NetNat / Linux / macOS L2 with injection-validated specs and elevation-honest reports | service |
| `linkfyr-daemon` | `linkfyrd` background service: hosts the engine over an authenticated loopback transport (IPC envelope reuse); Windows SCM, systemd, launchd integration | service |
| `linkfyr-protocol` | Fusion tunnel protocol: framing, crypto, path scheduler, FEC/redundancy | service |
| `linkfyr-ipc` | The stable API: message schemas, request/response envelope, event stream | both |
| `linkfyr-cli` | `linkfyr` binary: status, interfaces, apps, traffic, rules, profile, diagnose | user |
| `linkfyr-edge` | `linkfyrd-edge` binary: terminates Fusion tunnels, reorders, forwards | server |
| `apps/desktop` | Tauri 2 GUI (the only place Tauri types exist) | user |

Forward-compat placeholders (crate reserved, module stubbed, roadmap-locked):
per-app platform modules inside `linkfyr-network` (`wfp`, `ne`, `nft`),
`linkfyr-protocol::fec`, `linkfyr-rules::dsl`, remote-management module in
`linkfyr-core`. **Nothing in early phases may make these harder to add.**

## 4. Platform abstraction (linkfyr-network)

One trait surface, N implementations:

```rust
pub trait InterfaceMonitor: Send {
    fn snapshot(&self) -> Vec<Interface>;           // discovery + state
    fn counters(&self) -> Vec<IfCounters>;          // rx/tx bytes, packets, errors
}

pub trait AppTrafficMonitor: Send {                 // Phase 2
    fn flows(&self) -> Vec<FlowRecord>;             // per-process connection table
}

pub trait TrafficControl: Send {                    // Phase 3+
    fn apply(&mut self, plan: &ControlPlan) -> Result<ApplyReport>;
    fn clear(&mut self) -> Result<()>;              // MUST be fail-safe
}
```

| Capability | Windows | macOS | Linux | Android | iOS |
|---|---|---|---|---|---|
| Interface discovery/counters | GetIfTable2/IP Helper | NetworkExtension/sysctl | netlink/`/sys/class/net` | VpnService+ConnectivityManager | NE |
| Per-app accounting | WFP + ETW | NE filter (needs approval) | nftables owner match (`cgroup`/uid) | VpnService | limited by OS |
| Per-app control | WFP (user-mode first) | NE content filter | nftables + tc | VpnService | NE (limited) |
| Shaping/priority | WFP + qWAVE | NE + dummynet-era APIs / pq | tc qdiscs (netem/fq/cake) | VpnService queues | NE |
| Bonding (Fusion) | userspace UDP tunnel | same | same (TUN) | VpnService TUN | NE PacketTunnel |
| eBPF observability | — | — | ✓ where kernel allows | — | — |

User-mode first on Windows (WFP via the `windows` crate / callout-free
redirects); a kernel callout driver is a **later, optional** component.
No parity faking: the UI renders a capability matrix per platform.

## 5. Data flow (Phase 1 vertical slice, shipped now)

```
InterfaceMonitor (1 Hz)          HealthEngine
   │ counters Δ                      │ per-interface score
   ▼                                 ▼
TelemetryEngine ──ring buffers──▶ SnapshotBus ──▶ IPC events ──▶ UI charts
   │                                                    │
   ▼                                                    ▼
Store (SQLite, bucketed)                         CLI / linkfyrd status
```

- Counters are delta-based; poll-period jitter is handled by timestamp
  differencing, never by assuming a fixed period.
- Ring buffers feed 1 s-resolution charts; the store downsamples
  (1 s → 1 min → 1 h) with configurable retention (docs/content/roadmap.md §storage).
- The scheduler (Phase 5/6) consumes the same snapshot stream — UI and
  engine share one truth.

## 6. Fusion (Phase 6) design sketch

- Client opens N authenticated paths (initially QUIC streams / UDP flows)
  to one Edge endpoint; Edge reorders, dedups (path-tagged sequence
  numbers), applies optional redundancy/FEC, and forwards via NAT.
- Adaptive scheduler computes per-path weights from RTT/jitter/loss/
  throughput/cost every ~200 ms; decisions are exposed to users verbatim
  (Explainability is a product feature, not a debug log).
- Redundancy modes: off / critical-only / adaptive / full.
- Selection between multipath-QUIC, MPTCP, and custom UDP transport is a
  **benchmark decision**, deferred to Phase 6 with recorded ADR. Protocol
  sketch: docs/content/protocol.md.

## 7. Storage

SQLite (via a maintained Rust binding) with time-bucketed tables:
`samples_1s` (recent), `samples_1m`, `samples_1h`; retention configurable;
WAL mode; bounded log ring; diagnostics export is opt-in and redacted.

## 8. Testing strategy

- Scheduler/health/rules: deterministic unit + property tests (no real NICs).
- Network scenarios simulated via a `SimMonitor` implementing the same
  traits: high latency, loss, flap, asymmetric bandwidth, cap exhaustion.
- UI: component tests + Playwright-style e2e against the dev server.
- Protocol: fuzz framing/parsers, interleaved-delivery simulation tests.
- CI matrix: Windows x64, macOS arm64/x86_64, Linux x86_64 (+ arm64 cross).

## 9. Performance budgets

| Metric | Budget |
|---|---|
| linkfyrd idle CPU | < 1% of one core |
| telemetry sampling overhead | < 5 MB/s allocation churn, zero unbounded growth |
| added latency (observe-only) | ≈ 0 (pure observer) |
| added latency (Fusion, healthy path) | < 5 ms p50 overhead |
| UI frame budget | 60 FPS; chart updates batched to rAF |
| IPC chatter | event coalescing ≥ 10 Hz for charts, on-demand for the rest |

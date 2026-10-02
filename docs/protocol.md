# LinkFYR Fusion Protocol (design, pre-implementation)

Status: draft — protocol selection requires benchmark ADR before Phase 6.

## Goal

Aggregate N access networks into one virtual interface with a single
stable egress IP at an Edge node, with seamless path migration and
optional redundancy — while exposing every scheduling decision to the
user.

## Candidates to benchmark (ADR-00X will record results)

1. **QUIC multipath (draft / mc-QUIC implementations)** — mature crypto,
   congestion control, streams; multipath still settling.
2. **MPTCP** — kernel support spotty off Linux; NAT/traversal issues.
3. **Custom UDP transport** — full control (path tags, scheduling, FEC
   hooks), cost: we own congestion control + crypto (use established
   primitives only). Likely hybrid: QUIC per-path + our session layer
   (sequencing/reordering/scheduling) on top.

## Session model (transport-agnostic)

```
Client                                    Edge
  │ N paths (Ethernet/Wi-Fi/5G…) │
  │──────── path handshake ─────▶│ authenticate (device key + token)
  │  each path: AEAD-encrypted   │ assign session id + egress
  │──────── data packets ───────▶│ per-packet: (session, path_id, seq, flags)
  │◀─── downlink distribution ───│ scheduler feedback (RTT/loss samples)
```

- Packet header (draft): version(1) · type(1) · session(4) · path(1) ·
  seq(6) · length(2) · flags(1) — authenticated via AEAD, replay window
  per path. Headers avoid smuggling; control frames separate stream.
- Edge responsibilities: authenticate, sequence per (session,flow),
  dedup by (path,seq), reorder within jitter window, optional redundant
  strip, forward via NAT, accounting, telemetry back-channel.
- Client scheduler (runs every tick): weight_i = f(RTT_i, loss_i,
  capacity_i, cost_i, battery_i); smoothed (EWMA) + hysteresis to avoid
  flapping; per-traffic-class overrides (gaming → lowest RTT; download →
  max capacity).
- Redundancy modes: off · critical-only (small protected flows) ·
  adaptive (duplicate when loss_i > threshold and spare capacity) · full.
- Failover: path death detected via keepalive loss; in-flight packets
  re-emitted on survivors (seq space shared per flow) → no TCP resets,
  no IP change (Edge anchor). This is the differentiator vs local-only.
- Migration/reconnect: session resumption tokens; Edge holds state for
  grace period (default 30 s).

## Security

- Only established primitives (TLS 1.3 / QUIC crypto or WireGuard-style
  Noise family) — no custom ciphers/modes. Keys from device identity +
  Edge enrollment; stored in OS keychain.
- Edge sees flow metadata (5-tuple) by necessity; client can pin self-
  hosted Edge to eliminate hosted trust.
- Anti-abuse: tokens per device, rate-limited handshake, QUIC-style
  address validation before data.

## Open items

FEC codec choice (RLNC vs XOR-based interleaving) · congestion control
per path (BBRv2-style vs Copa for interactive) · header compression ·
MTU/probing · IPv6 · benchmark harness (doc'd with ADR).

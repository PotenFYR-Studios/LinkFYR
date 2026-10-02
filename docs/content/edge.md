# LinkFYR Edge

Two deployment modes, one binary: `linkfyrd-edge`.

## Self-hosted (free, first-class)

```bash
docker run -d --network host \
  -v /etc/linkfyr-edge:/data \
  -e LINKFYR_EDGE_LISTEN=0.0.0.0:51820/udp \
  linkfyr/edge:latest
# or native: linkfyr-edge --config /etc/linkfyr-edge/config.toml
```

- Enrolls devices (device key + token); terminates Fusion sessions;
  NAT/forwards to Internet; exports Prometheus-style metrics + accounting.
- Runs on: any VPS/cloud VM, home server, colocated box. ARM64 images.
- Hosted-Edge parity is a hard requirement (same binary, same protocol).

## Community-hosted endpoints (optional, never gated)

- Regions/nodes appear only when real volunteer or community-run
  infrastructure exists; never advertised ahead of reality.
- Node selection: latency probe + capacity + user pin.
- There is no billing or entitlement layer anywhere in LinkFYR; trust for
  a hosted node is the operator's own trust decision (self-host by
  default; community nodes are opt-in and clearly labeled).

## Operations

- Config: TOML; secrets via env/file perms, never in repo.
- Telemetry: Edge keeps only forwarding/accounting metadata + session
  health; no DPI, no payload logging; retention short by default.
- Upgrades: blue-green via container tags; protocol version negotiation;
  old clients get grace window.

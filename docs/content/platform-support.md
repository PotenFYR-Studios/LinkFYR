# LinkFYR Platform Support & Capability Matrix

Status: living document · Last updated: 2026-09-30

Rule: **never fake parity.** The UI renders this matrix at runtime
(`capabilities()` API) and labels every feature: `available`,
`partial`, `platform-policy` (blocked by the OS, not by us), `elevated`
(needs an optional/privileged component), or `unavailable`.

## Build targets

| Platform | Arch | Tier | Shipment |
|---|---|---|---|
| Windows 10/11 | x86_64 | P0 | MSI + NSIS + portable |
| Windows 10/11 | ARM64 | P1 | MSI/NSIS (ARM64) |
| macOS 13+ | aarch64 | P0 | DMG (universal2 where practical) |
| macOS 13+ | x86_64 | P1 | via universal2 |
| Linux (glibc) | x86_64 | P0 | AppImage, .deb, .rpm, tarball |
| Linux (glibc) | aarch64 | P1 | .deb, tarball, AppImage |
| Android 10+ | arm64-v8a | P2 (Phase 8) | APK |
| Android | x86_64 | P3 | emulator builds |
| iOS/iPadOS 16+ | arm64 | P2 (Phase 8) | TestFlight/App Store (policy-limited) |
| Headless Linux | x86_64/aarch64 | P1 | `linkfyrd` + `linkfyr` tarballs, OCI image |
| OpenWrt | aarch64/mipsel | P3 (Phase 9, research) | ipk — not promised |

## Capability matrix

| Capability | Windows | macOS | Linux | Android | iOS | Headless |
|---|---|---|---|---|---|---|
| Interface discovery + counters | ✓ | ✓ | ✓ | partial¹ | partial¹ | ✓ |
| Live throughput/health charts | ✓ | ✓ | ✓ | ✓ | ✓ | via CLI/API |
| Per-app traffic visibility | ✓ (ETW/WFP) | partial² | ✓ (uid/cgroup) | partial¹ | ✗ policy | partial³ |
| Per-app allow/block firewall | ✓ (WFP) | partial² (NE, approval) | ✓ (nft) | ✓ (VpnService) | ✗ policy | ✓ |
| Ask-to-connect prompts | ✓ | ✓ (NE) | ✓ (OpenSnitch-style daemon) | partial | ✗ policy | ✓ (API event) |
| Per-app bandwidth limits | ✓ (WFP+qWAVE) | partial² | ✓ (tc/fq+cake) | ✓ | ✗ policy | ✓ |
| Interface selection per app | ✓ | partial² | ✓ (fwmark+policy routing) | partial | ✗ policy | ✓ |
| Multi-WAN failover (local) | ✓ | ✓ | ✓ | partial | ✗ policy | ✓ |
| Local load balancing (per-flow) | ✓ | ✓ | ✓ | partial | ✗ policy | ✓ |
| Fusion bonding (true multipath) | ✓ (tunnel) | ✓ (NE tunnel) | ✓ (TUN) | ✓ (VpnService) | ✓ (PacketTunnel) | ✓ (edge role) |
| Redundancy / FEC | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Per-app VPN routing | ✓ | partial² | ✓ | partial | ✗ policy | ✓ |
| WireGuard integration | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Encrypted DNS (DoH/DoT/DoQ) | ✓ | ✓ | ✓ | ✓ | partial | ✓ |
| DNS blocklists/filtering | ✓ | ✓ | ✓ | ✓ | partial | ✓ |
| Network Time Machine | ✓ | ✓ | ✓ | partial | partial | via API |
| Diagnose engine | ✓ | ✓ | ✓ | ✓ | partial | ✓ |
| Wi-Fi signal analysis | ✓ | partial (CoreWLAN limits) | partial (driver-dependent) | partial | ✗ policy | ✗ |
| Sysproxy (system proxy) | ✓ | ✓ | ✓ (DE-dependent) | ✗ | ✗ | ✓ |
| Remote management (Phase 8) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |

¹ Android/iOS expose only the app's own VPN-tunnel traffic plus coarse
  system stats; per-other-app detail is OS-policy-blocked. UI says so.
² macOS per-app features need a Network Extension (system extension
  approval); supported but gated by user approval + signing/notarization.
³ Headless sees process table where uid→process mapping exists.

## Legend / gating in UI

- `elevated`: feature requires the privileged service component; installer
  offers "core (no elevation)" and "full" installs.
- `platform-policy`: OS restriction; rendered with an explanation, never a
  grayed-out mystery.
- Optional components (e.g. kernel callout driver on Windows, NE system
  extension on macOS) are downloadable from within the app with clear
  scope; the core product must remain useful without them.

## Tier policy

P0 blocks releases. P1 should follow within one release cycle. P2/P3 are
roadmap-locked (docs/content/roadmap.md) with architecture reserved now.

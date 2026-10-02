# User-Demand Research (living, evidence-graded)

Grading: ★ widespread (many independent sources/longevity) · ☆ recurring
(several sources) · ˚ anecdotal (single threads). Sources searched:
Reddit (r/HomeNetworking, r/vpn, r/pcgaming, r/Twitch, r/homelab,
r/macapps, r/sysadmin), GitHub issues (OpenSnitch, Portmaster,
simplewall, GlassWire forums, Speedify), Hacker News, vendor forums.

## Recurring asks → LinkFYR requirements

| Ask (representative query) | Evidence | Maps to |
|---|---|---|
| "combine Wi-Fi and Ethernet for speed" / "bond multiple internet connections" | ★ | Fusion (P6) + honest local multi-WAN (P5) |
| "use multiple network adapters at the same time per app" | ★ | interface assignment per app (P3) |
| "NetLimiter alternative modern UI" / "GlassWire with limits" | ★ | Phase 3 shaping + Phase 1 visuals |
| "Little Snitch for Windows" / "Portmaster Windows" | ★ | app firewall (P3), ask-to-connect (P3) |
| "per-app VPN / different VPN per application" | ★ | multi-VPN routing (P7) |
| "prioritize Discord over Steam while downloading" | ★ | guarantees/priority/borrowing (P3) |
| "internet failover without disconnect" (calls dropping) | ★ | failover + Fusion seamless switch (P5/P6) |
| "Speedify too expensive / want self-host" | ☆ | self-hosted Edge (P6), fully free local core |
| "GlassWire on Linux" | ★ | Linux P0 |
| "ask-to-connect too noisy" | ☆ | fatigue-resistant prompts, learning suggestions |
| "why is my internet slow" tooling beyond speedtest | ☆ | Diagnose engine w/ evidence + actions |
| "block app from internet but allow LAN" | ★ | firewall scopes (P3) |
| "track which app used how much data this month (mobile caps)" | ★ | metered caps, budget alerts (P2/P3) |
| "OpenWrt multi-WAN easier" (mwan3 pain) | ☆ | desktop UX now; router later (P9) |
| "bandwidth reservation per app" | ☆ | smart reservation (P3) |
| "network history / who downloaded what at 3am" | ★ | Network Time Machine (P2) |
| "DNS per app / blocklists built-in" | ☆ | DNS engine (P7) |
| "monitor per-process on Linux like GlassWire" | ☆ | per-app monitor Linux (P2) |
| "Starlink + 5G bonding for rural" | ☆ | Fusion + scheduler (P6) — high-value niche |
| "explain what an app is connecting to" | ★ | drill-down monitor (P2) |

## Anti-patterns to avoid (complaint-derived)

- Subscription-gating *local* features users already paid hardware for ★
- Opaque "AI optimization" with no explanation ☆
- Notification spam from security tools ☆
- Ignoring Linux/ARM64 ☆
- UI rewrites that reset user rules/configs ˚

## Open questions for continued research

- Size of "bonding without cloud" demand vs hosted convenience.
- Willingness to pay for Teams/fleet features.
- Android as full client vs companion-only.

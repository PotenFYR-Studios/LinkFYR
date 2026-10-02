# Competitor Research Matrix (living)

Status: living document · Updated: 2026-09-30 · Method: public docs,
stores, forums, Reddit, GitHub issues. Evidence graded: ★ widespread ·
☆ recurring · ˚ anecdotal.

## Multi-WAN / bonding

### Speedify
Platforms: Win/macOS/Linux/Android/iOS. Pricing: freemium + subscription.
Strengths: true bonding (their cloud), channel bonding UX, failover.
Limitations: closed cloud required for bonding; per-app controls thin;
opaque "smart" decisions; history/analytics weak; no firewall.
Complaints: subscription cost for infrastructure users already pay for ★;
can't self-host bonding ☆; telemetry/privacy concerns ☆; no per-app VPN
split ☆. **Opportunities:** self-hosted Edge, explainable scheduler,
per-app bonding policies, capable local multi-WAN that costs nothing.

### Peplink/SpeedFusion, OpenMPTCProuter, mwan3
Router-space multi-WAN. Strengths: proven bonding/FEC (Peplink), open
(OMPTC: MPTCP+VPS), free (mwan3). Limitations: hardware/router-bound,
terrible UX for mortals, no per-app desktop control, no GUI history.
Complaints: config complexity ★; want desktop equivalent ☆.
**Opportunities:** bring router-grade multi-WAN to desktops with real UX.

## Per-app control / firewall

### NetLimiter (Win)
Strengths: per-app limits, priorities, mature. Complaints: dated UI ★,
pricing tiers ☆, no multi-WAN, Windows-only.
### NetBalancer (Win)
Similar space; complaints: UI/perf, licensing ☆.
### Little Snitch (macOS)
Gold standard ask-to-connect + rules. Complaints: price ★; no bandwidth
shaping ☆; no multi-WAN ☆; silent-mode fatigue ☆.
### Portmaster (macOS/Linux, Safing)
Open-core, privacy-centric, per-app DNS/blocklists. Complaints: Windows
gap ☆; no bandwidth control ☆; perf overhead reports ☆; paid SPN for
privacy routing.
### simplewall / OpenSnitch / LuLu
Free firewalls: Windows filter frontend / Linux ask-to-connect / macOS
blocker. Common gaps: no telemetry dashboards, no shaping, no multi-WAN,
single-purpose. Complaints about OpenSnitch: GTK UX ☆.
**Opportunities:** unified firewall + shaping + routing + visibility with
modern UX; ask-to-connect that learns (rule suggestions) without fatigue.

## Monitoring

### GlassWire
Strengths: beautiful graphs (the category benchmark), alerts, easy.
Complaints: no per-app shaping ★; subscription backlash ☆; no Linux ★;
shallow diagnostics ☆. **Opportunities:** GlassWire-grade visuals +
actual control; Linux support.

## VPN / identity

### Tailscale/ZeroTier
Mesh identity, great onboarding. Not traffic control/bonding.
Complaints: coordination-plane reliance ☆ (self-host headscale demand ★).
**Opportunity:** LinkFYR device mesh later (Phase 8) w/ self-host option.

### Commercial VPN clients
Per-app split tunneling usually crude; no multi-path; no per-app DNS.
**Opportunity:** per-app × per-destination × per-profile VPN matrices.

## Diagnostics
ping/traceroute/MTR/WiFiAnalyzer/etc.: fragmented CLI-era UX, no
integration with control. **Opportunity:** Diagnose that *acts* (creates
reversible actions / rules), not just reports.

## Synthesis — validated product requirements (evidence-graded)

1. Per-app bandwidth control with guarantees+burst, not just caps ★
2. Beautiful, fast traffic visuals ★
3. Multi-WAN failover without disconnects ★; true bonding without
   vendor cloud lock-in ☆
4. Ask-to-connect with fatigue-resistant UX ★
5. Cross-platform consistency (Linux gaps everywhere) ★
6. Explainable automation ("why did it do that?") ☆
7. Self-hosting for infrastructure features ☆
8. Free core that isn't crippled ☆
9. Per-app VPN split by domain AND app ☆
10. Modern DNS controls (DoH/DoT, blocklists) built-in ☆

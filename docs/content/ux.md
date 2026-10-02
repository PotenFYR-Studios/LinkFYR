# LinkFYR UX Standard & Design System

## Feel

Premium 2026+ network control center: technical but humane. Dark-first
(v1 targets network engineers/gamers at night), excellent light mode.
Information-dense when the user wants it, calm by default.

## Design tokens (implemented in `packages/ui/tokens.css`)

- Type: Inter (UI) + JetBrains Mono (numbers/telemetry). Tabular numerals
  for all metrics. Scale: 12/13/15/17/20/24/30/38/48.
- Spacing: 4-base scale (4,8,12,16,24,32,48,64).
- Radii: 6/10/14/20; cards 14; pills 999.
- Elevation: 3 surfaces (base/raised/overlay) via layered translucent
  fills + 1px inner strokes; shadows only for true overlays.
- Color (dark): bg `#0B0E14`, surface `#11151F`, raised `#161B28`,
  stroke `#232A3B`, text `#E8ECF4`, muted `#93A0B4`.
  Semantic: accent `#5B8CFF` (LinkFYR blue), ok `#3DDC97`, warn `#FFB454`,
  danger `#FF5C7A`, info `#9B8CFF`. Interfaces get stable hues
  (Ethernet/Wi-Fi/cellular/virtual) from a categorical palette.
- Motion: 120ms/180ms/240ms; spring for spatial (expand/collapse, cards),
  ease-out for feedback; ALL animation gated by `prefers-reduced-motion`
  and a user animation-intensity setting (off/reduced/full).
- Charts: 2px strokes, area fills ≤12% alpha, live mode scrolls smoothly,
  hover readouts snap to samples, gap-safe (data gaps render as gaps).

## Components (packages/ui primitives)

Button, IconButton, Card, Stat, Badge, Pill, Tabs, Segmented, Toggle,
Slider, Select, TextField, SearchInput, Kbd, Tooltip, Popover, Dialog,
Sheet, Toast, ProgressRing, Sparkline, LiveChart (area/line/stacked),
BarMeter, HealthGauge (radial, factor breakdown), FlowDiagram (animated
routing), Timeline (Time Machine), DataTable (virtualized), EmptyState,
Skeleton, CommandPalette, CapabilityTag (available/partial/policy/
elevated), ExplainCard ("why did LinkFYR do that" + one-click undo).

## Layout

App shell: left rail (nav) + content + optional right inspector (expert
mode). Command palette: Ctrl/Cmd+K. Views: Dashboard, Interfaces, Apps,
Flow Rules, Composer, VPN, DNS, Diagnose, Time Machine, Tools, Settings.

Dashboard default cards: Throughput (hero, combined ↓↑), Interfaces
(contribution bars), Health (per-if), Top talkers, Recent decisions
(explainability feed), Active profile, Alerts. Cards: hide/rearrange
(drag), density toggle; multiple named dashboards later.

## Accessibility

WCAG AA contrast on all token pairs; focus-visible rings; full keyboard
nav + palette-first flows; charts have accessible summaries + table
fallbacks; status never color-only (icon+text); reduced motion honored.

## Performance

rAF-batched chart updates; virtualized lists everywhere; skeletons for
>100ms loads; no layout-thrash animations (transform/opacity only).

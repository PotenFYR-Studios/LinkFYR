# Flow Rules — engine design

Status: living spec · Engine lives in `linkfyr-rules`.

## Model

```rust
Rule { id, name, enabled, priority,
  when:  Condition,          // AND of predicates; OR groups nested
  unless: Option<Condition>, // exception
  then: Vec<Action>,         // ordered
}
```

Predicates (Phase 4 targets; engine keeps the full enum from day one so
data model never migrates): app/executable/path · process user · interface
(id/type/health/SSID) · destination (ip/CIDR/domain/ASN/country/port/
protocol) · traffic category · VPN active · time-of-day/day-of-week ·
battery/charging · network cost/metered · live metrics (latency, jitter,
loss, throughput above/below threshold, sustained windows) · profile ·
connection state · monthly data usage thresholds.

Actions: allow · block · ask · throttle(down/up) · guarantee · priority ·
route_to(interface(s)) · use_vpn(profile) · use_dns(resolver) ·
bond(mode) · redundancy(mode) · activate_profile · notify · webhook ·
run_diagnostics · set_mode. Every action is reversible; every execution
writes an Explainability event (why, inputs snapshot, rule id, undo ref).

## Evaluation pipeline

Match phase builds a `RuleContext` from current state (app lookup,
metrics snapshot, time). Rules pre-indexed by coarse keys (app, dst
domain suffix) → candidate set; full eval only on candidates; first-match
by priority within action classes (firewall vs routing are separate
stages so a block rule can't silently cancel a route rule).

## Editors

- Beginner: card-based ("When [app] connects to [domain] → [do this]")
  with curated pickers; no free-form logic.
- Expert: full condition tree (AND/OR/NOT), metrics with windows
  ("loss > 5% for 10 s"), test-bench (simulate against last N minutes of
  telemetry), JSON/YAML import-export.

## Automation rules

Same engine; triggers are predicates over event streams (app launch,
interface up/down, thresholds crossing with debounce, schedule, profile
change). Dry-run mode logs what would happen; every automation has
explain-why + disable.

## Connection Composer

Visual graph (sources=interfaces/VPN/Edge, sinks=traffic classes/apps).
Lowering pass compiles the graph into Flow Rules + profile — the graph
is sugar, rules remain the source of truth (inspectable, exportable).

## Testing

Golden-case unit tests per predicate/action; property tests (rule set is
total function over context); simulation suite replays scripted network
scenarios (docs/architecture.md §8) asserting expected decisions.

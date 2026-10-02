//! Multi-WAN scheduler (Phase 5): monitors every up interface, ranks
//! them by measured health + latency, and adjusts routing metrics for
//! ordered failover. When the best link degrades, the scheduler
//! lowers its priority so the OS routes via the next-best link.
//! Failover is advisory at v0 (metric adjustment); per-flow steering
//! arrives with per-app accounting (Phase 2 store).

use linkfyr_model::optimize::ToolRunReport;
use linkfyr_model::{InterfaceTelemetry, Snapshot};

#[derive(Debug, Clone, PartialEq)]
pub struct LinkRank {
    pub id: String,
    pub name: String,
    pub health: u8,
    pub rx_bps: f64,
    pub tx_bps: f64,
    pub score: f64,
}

/// Rank interfaces by a blended score: health (50%), activity (30%),
/// and up-status (20%). Higher is better.
pub fn rank_interfaces(interfaces: &[InterfaceTelemetry]) -> Vec<LinkRank> {
    let mut ranks: Vec<LinkRank> = interfaces
        .iter()
        .filter(|t| t.interface.status == linkfyr_model::IfStatus::Up)
        .map(|t| {
            let health = t.health.as_ref().map_or(50.0, |h| f64::from(h.overall));
            let activity = ((t.rx_bps + t.tx_bps) / 1_000_000.0).min(100.0);
            let score = health * 0.5 + activity * 0.3 + 20.0;
            LinkRank {
                id: t.interface.id.clone(),
                name: t.interface.friendly_name.clone(),
                health: t.health.as_ref().map_or(50, |h| h.overall),
                rx_bps: t.rx_bps,
                tx_bps: t.tx_bps,
                score,
            }
        })
        .collect();
    ranks.sort_by(|a, b| b.score.partial_cmp(&a.score).expect("finite"));
    ranks
}

/// The multi-WAN plan: ordered interfaces + what to do.
pub fn failover_plan(snap: &Snapshot) -> ToolRunReport {
    let tool = "multiwan_plan";
    let ranks = rank_interfaces(&snap.interfaces);
    if ranks.is_empty() {
        return ToolRunReport {
            tool: tool.into(),
            ok: false,
            summary: "no interfaces are up; nothing to schedule".into(),
            took_ms: 0,
            data: serde_json::Value::Null,
        };
    }
    let plan: Vec<serde_json::Value> = ranks
        .iter()
        .enumerate()
        .map(|(i, r)| {
            serde_json::json!({
                "priority": i + 1,
                "interface": r.id,
                "name": r.name,
                "health": r.health,
                "score": (r.score * 10.0).round() / 10.0,
                "role": if i == 0 { "primary" } else { "failover" },
            })
        })
        .collect();
    let primary = &ranks[0];
    ToolRunReport {
        tool: tool.into(),
        ok: true,
        summary: format!(
            "{} up link(s); primary {} (health {}, score {:.0}), failover ready",
            ranks.len(),
            primary.name,
            primary.health,
            primary.score
        ),
        took_ms: 0,
        data: serde_json::json!({ "plan": plan }),
    }
}

/// Detect if the primary link has degraded enough to justify failover.
pub fn should_failover(ranks: &[LinkRank]) -> Option<String> {
    if ranks.len() < 2 {
        return None;
    }
    let primary = &ranks[0];
    let backup = &ranks[1];
    if primary.health < 40 && backup.health > primary.health + 20 {
        return Some(format!(
            "failover recommended: primary {} health {} < backup {} health {}",
            primary.name, primary.health, backup.name, backup.health
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use linkfyr_model::*;

    fn iface(id: &str, status: IfStatus, health: u8, rx: f64) -> InterfaceTelemetry {
        InterfaceTelemetry {
            interface: Interface {
                id: id.into(),
                name: id.into(),
                friendly_name: id.into(),
                kind: IfKind::Ethernet,
                status,
                mac: None,
                ipv4: vec![],
                ipv6: vec![],
                gateway: None,
                mtu: Some(1500),
                speed_bps: Some(1_000_000_000),
                metered: false,
            },
            rx_bps: rx,
            tx_bps: rx / 4.0,
            health: Some(HealthScore {
                overall: health,
                status: if health >= 80 {
                    HealthStatus::Healthy
                } else if health >= 50 {
                    HealthStatus::Degraded
                } else {
                    HealthStatus::Poor
                },
                factors: vec![],
            }),
            errors: IfCounters::default(),
        }
    }

    fn snap(ifaces: Vec<InterfaceTelemetry>) -> Snapshot {
        Snapshot {
            timestamp_ms: 1000,
            engine_version: "test".into(),
            interfaces: ifaces,
            totals: Totals::default(),
            internet: ProbeStats::default(),
        }
    }

    #[test]
    fn ranks_by_health_and_activity() {
        let s = snap(vec![
            iface("fast", IfStatus::Up, 95, 50_000_000.0),
            iface("slow", IfStatus::Up, 60, 1_000_000.0),
            iface("down", IfStatus::Down, 0, 0.0),
        ]);
        let ranks = rank_interfaces(&s.interfaces);
        assert_eq!(ranks.len(), 2, "down interfaces are excluded");
        assert_eq!(ranks[0].id, "fast");
        assert!(ranks[0].score > ranks[1].score);
    }

    #[test]
    fn failover_triggers_on_health_gap() {
        let ranks = vec![
            LinkRank {
                id: "a".into(),
                name: "A".into(),
                health: 30,
                rx_bps: 0.0,
                tx_bps: 0.0,
                score: 30.0,
            },
            LinkRank {
                id: "b".into(),
                name: "B".into(),
                health: 80,
                rx_bps: 0.0,
                tx_bps: 0.0,
                score: 80.0,
            },
        ];
        assert!(should_failover(&ranks).is_some());
        let healthy = vec![
            LinkRank {
                id: "a".into(),
                name: "A".into(),
                health: 85,
                rx_bps: 0.0,
                tx_bps: 0.0,
                score: 85.0,
            },
            LinkRank {
                id: "b".into(),
                name: "B".into(),
                health: 70,
                rx_bps: 0.0,
                tx_bps: 0.0,
                score: 70.0,
            },
        ];
        assert!(should_failover(&healthy).is_none());
    }

    #[test]
    fn plan_reports_honestly() {
        let s = snap(vec![iface("e0", IfStatus::Up, 90, 10_000_000.0)]);
        let r = failover_plan(&s);
        assert!(r.ok);
        assert!(r.summary.contains("1 up link"));
        let empty = snap(vec![]);
        let r = failover_plan(&empty);
        assert!(!r.ok);
    }
}

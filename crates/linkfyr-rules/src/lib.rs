//! LinkFYR Flow Rules engine: condition AST + evaluation.
//!
//! This crate is the deterministic heart of user policy. The visual
//! editor (Phase 4) compiles into these types; the automation engine
//! reuses the same predicate evaluation over event streams.
//! Every rule evaluation produces an auditable decision (rule id + why),
//! which the Explainability features surface verbatim to users.

use serde::{Deserialize, Serialize};
use std::net::IpAddr;

/// What kind of traffic a flow represents (Phase 2 classifier fills this;
/// rules can already match on it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrafficCategory {
    Gaming,
    Voice,
    VideoCall,
    Streaming,
    Browsing,
    Download,
    Upload,
    CloudSync,
    Updates,
    P2p,
    RemoteDesktop,
    Backup,
    Background,
    Vpn,
    Unknown,
}

/// One direction of firewall verdicts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    Tcp,
    Udp,
    Icmp,
    Other,
}

/// Facts about the flow + world state at evaluation time.
/// Cheap to build from a metrics snapshot; rules never touch live state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleContext {
    pub app: Option<String>,
    pub process: Option<String>,
    pub domain: Option<String>,
    pub ip: Option<IpAddr>,
    pub port: Option<u16>,
    pub protocol: Option<Protocol>,
    pub category: Option<TrafficCategory>,
    pub interface_id: Option<String>,
    pub ssid: Option<String>,
    /// Battery percent 0-100 when on battery.
    pub battery_percent: Option<u8>,
    pub charging: Option<bool>,
    pub metered: Option<bool>,
    /// Interface health 0-100 (None when unknown).
    pub interface_health: Option<u8>,
    pub latency_ms: Option<f64>,
    pub jitter_ms: Option<f64>,
    pub loss_pct: Option<f64>,
    /// Local time-of-day in minutes since midnight.
    pub minute_of_day: Option<u32>,
    /// 0 = Sunday … 6 = Saturday.
    pub day_of_week: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Predicate {
    AppEquals {
        value: String,
    },
    DomainSuffix {
        suffix: String,
    },
    PortEquals {
        port: u16,
    },
    ProtocolEquals {
        protocol: Protocol,
    },
    CategoryEquals {
        category: TrafficCategory,
    },
    InterfaceKind {
        is: String,
    },
    InterfaceHealthBelow {
        threshold: u8,
    },
    LossAbove {
        threshold_pct: f64,
    },
    LatencyAbove {
        threshold_ms: f64,
    },
    OnMetered {
        metered: bool,
    },
    OnBattery {
        battery: bool,
    },
    ChargingEquals {
        charging: bool,
    },
    TimeBetween {
        from_minute: u32,
        to_minute: u32,
    },
    /// Nested AND group.
    AllOf {
        of: Vec<Predicate>,
    },
    /// Nested OR group.
    AnyOf {
        of: Vec<Predicate>,
    },
    Not {
        of: Box<Predicate>,
    },
}

impl Predicate {
    pub fn matches(&self, ctx: &RuleContext) -> bool {
        match self {
            Predicate::AppEquals { value } => ctx
                .app
                .as_deref()
                .is_some_and(|a| a.eq_ignore_ascii_case(value)),
            Predicate::DomainSuffix { suffix } => ctx
                .domain
                .as_deref()
                .is_some_and(|d| domain_matches(d, suffix)),
            Predicate::PortEquals { port } => ctx.port.as_ref() == Some(port),
            Predicate::ProtocolEquals { protocol } => ctx.protocol.as_ref() == Some(protocol),
            Predicate::CategoryEquals { category } => ctx.category.as_ref() == Some(category),
            Predicate::InterfaceKind { is } => ctx.interface_id.as_deref() == Some(is.as_str()),
            Predicate::InterfaceHealthBelow { threshold } => {
                ctx.interface_health.is_some_and(|h| h < *threshold)
            }
            Predicate::LossAbove { threshold_pct } => {
                ctx.loss_pct.is_some_and(|l| l > *threshold_pct)
            }
            Predicate::LatencyAbove { threshold_ms } => {
                ctx.latency_ms.is_some_and(|l| l > *threshold_ms)
            }
            Predicate::OnMetered { metered } => ctx.metered.as_ref() == Some(metered),
            Predicate::OnBattery { battery } => {
                let on_battery =
                    matches!((ctx.charging, ctx.battery_percent), (Some(false), Some(_)));
                on_battery == *battery
            }
            Predicate::ChargingEquals { charging } => ctx.charging.as_ref() == Some(charging),
            Predicate::TimeBetween {
                from_minute,
                to_minute,
            } => {
                let m = ctx.minute_of_day.unwrap_or(0);
                if from_minute <= to_minute {
                    m >= *from_minute && m <= *to_minute
                } else {
                    // Window wraps midnight (e.g. 22:00 → 06:00).
                    m >= *from_minute || m <= *to_minute
                }
            }
            Predicate::AllOf { of } => of.iter().all(|p| p.matches(ctx)),
            Predicate::AnyOf { of } => of.iter().any(|p| p.matches(ctx)),
            Predicate::Not { of } => !of.matches(ctx),
        }
    }
}

/// Case-insensitive domain suffix match: "netflix.com" matches
/// "www.netflix.com" and "netflix.com", not "badnetflix.com".
fn domain_matches(domain: &str, suffix: &str) -> bool {
    let domain = domain.trim_end_matches('.');
    let suffix = suffix.trim_end_matches('.').to_ascii_lowercase();
    let domain = domain.to_ascii_lowercase();
    domain == suffix
        || domain
            .strip_suffix(&suffix)
            .is_some_and(|rest| rest.ends_with('.'))
}

/// What a matched rule asks the engine to do.
/// Firewall actions and routing actions are separate stages downstream:
/// a throttle can never silently cancel a block, and vice versa.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Action {
    Allow,
    Block,
    Ask,
    Throttle {
        down_bps: Option<u64>,
        up_bps: Option<u64>,
    },
    Guarantee {
        down_bps: Option<u64>,
        up_bps: Option<u64>,
    },
    SetPriority {
        priority: u8,
    },
    RouteTo {
        interfaces: Vec<String>,
    },
    UseVpn {
        profile: String,
    },
    UseDns {
        resolver: String,
    },
    ActivateProfile {
        profile: String,
    },
    Notify {
        message: String,
    },
}

/// The stage of the pipeline an action belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionClass {
    Firewall,
    Routing,
    Shaping,
    ProfileOrNotify,
}

impl Action {
    pub fn class(&self) -> ActionClass {
        match self {
            Action::Allow | Action::Block | Action::Ask => ActionClass::Firewall,
            Action::RouteTo { .. } | Action::UseVpn { .. } | Action::UseDns { .. } => {
                ActionClass::Routing
            }
            Action::Throttle { .. } | Action::Guarantee { .. } | Action::SetPriority { .. } => {
                ActionClass::Shaping
            }
            Action::ActivateProfile { .. } | Action::Notify { .. } => ActionClass::ProfileOrNotify,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    /// Lower number = evaluated first within a class.
    pub priority: u32,
    pub when: Predicate,
    #[serde(default)]
    pub unless: Option<Predicate>,
    pub then: Vec<Action>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Decision<'a> {
    pub rule: &'a Rule,
    pub actions: &'a [Action],
}

/// Evaluate rules in priority order. Within one action class, the first
/// matching rule wins (documented semantics); classes are independent so
/// a firewall rule and a routing rule can both fire from different rules.
pub fn evaluate<'a>(rules: &'a [Rule], ctx: &RuleContext) -> Vec<Decision<'a>> {
    let mut winners: Vec<(ActionClass, Decision)> = Vec::new();
    let mut ordered: Vec<&Rule> = rules.iter().filter(|r| r.enabled).collect();
    ordered.sort_by_key(|r| r.priority);

    for rule in ordered {
        if !rule.when.matches(ctx) {
            continue;
        }
        if rule.unless.as_ref().is_some_and(|u| u.matches(ctx)) {
            continue;
        }
        for action in &rule.then {
            let class = action.class();
            if !winners.iter().any(|(c, _)| *c == class) {
                winners.push((
                    class,
                    Decision {
                        rule,
                        actions: &rule.then,
                    },
                ));
            }
        }
    }

    winners.into_iter().map(|(_, d)| d).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(id: &str, priority: u32, when: Predicate, then: Vec<Action>) -> Rule {
        Rule {
            id: id.into(),
            name: id.into(),
            enabled: true,
            priority,
            when,
            unless: None,
            then,
        }
    }

    fn ctx_app(app: &str, domain: &str) -> RuleContext {
        RuleContext {
            app: Some(app.into()),
            domain: Some(domain.into()),
            port: Some(443),
            protocol: Some(Protocol::Tcp),
            ..Default::default()
        }
    }

    #[test]
    fn domain_suffix_matches_subdomains_but_not_lookalikes() {
        let p = Predicate::DomainSuffix {
            suffix: "netflix.com".into(),
        };
        assert!(p.matches(&ctx_app("chrome", "www.netflix.com")));
        assert!(p.matches(&ctx_app("chrome", "netflix.com")));
        assert!(!p.matches(&ctx_app("chrome", "badnetflix.com")));
        assert!(!p.matches(&ctx_app("chrome", "netflix.com.evil.io")));
        assert!(p.matches(&ctx_app("chrome", "NETFLIX.COM")));
    }

    #[test]
    fn all_of_requires_every_predicate() {
        let p = Predicate::AllOf {
            of: vec![
                Predicate::AppEquals {
                    value: "steam".into(),
                },
                Predicate::CategoryEquals {
                    category: TrafficCategory::Download,
                },
            ],
        };
        assert!(p.matches(&RuleContext {
            app: Some("steam".into()),
            category: Some(TrafficCategory::Download),
            ..Default::default()
        }));
        assert!(!p.matches(&RuleContext {
            app: Some("steam".into()),
            category: Some(TrafficCategory::Gaming),
            ..Default::default()
        }));
    }

    #[test]
    fn unless_exception_blocks_the_rule() {
        let rules = [rule(
            "r1",
            10,
            Predicate::AppEquals {
                value: "chrome".into(),
            },
            vec![Action::Allow],
        )];
        let mut with_exception = rules[0].clone();
        with_exception.unless = Some(Predicate::DomainSuffix {
            suffix: "bank.com".into(),
        });

        let all = [with_exception];
        let d = evaluate(&all, &ctx_app("chrome", "example.com"));
        assert_eq!(d.len(), 1, "matches without exception");

        let d2 = evaluate(&all, &ctx_app("chrome", "bank.com"));
        assert_eq!(d2.len(), 0, "exception suppresses");
    }

    #[test]
    fn disabled_rules_never_fire() {
        let mut r = rule(
            "r1",
            10,
            Predicate::AppEquals {
                value: "chrome".into(),
            },
            vec![Action::Block],
        );
        r.enabled = false;
        let rules = [r];
        let d = evaluate(&rules, &ctx_app("chrome", "x.com"));
        assert_eq!(d.len(), 0, "disabled rules must produce no decisions");
    }

    #[test]
    fn lower_priority_number_wins_within_class() {
        let rules = [
            rule(
                "late-block",
                100,
                Predicate::AppEquals {
                    value: "steam".into(),
                },
                vec![Action::Block],
            ),
            rule(
                "early-allow",
                1,
                Predicate::AppEquals {
                    value: "steam".into(),
                },
                vec![Action::Allow],
            ),
        ];
        let d = evaluate(&rules, &ctx_app("steam", "store.steampowered.com"));
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule.id, "early-allow");
        assert_eq!(d[0].actions[0], Action::Allow);
    }

    #[test]
    fn different_action_classes_both_fire() {
        let rules = vec![
            rule(
                "firewall",
                1,
                Predicate::AppEquals {
                    value: "discord".into(),
                },
                vec![Action::Allow],
            ),
            rule(
                "routing",
                1,
                Predicate::AppEquals {
                    value: "discord".into(),
                },
                vec![Action::RouteTo {
                    interfaces: vec!["eth0".into()],
                }],
            ),
        ];
        let d = evaluate(&rules, &ctx_app("discord", "gateway.discord.gg"));
        assert_eq!(d.len(), 2, "firewall + routing decisions are independent");
    }

    #[test]
    fn time_windows_can_wrap_midnight() {
        let night = Predicate::TimeBetween {
            from_minute: 22 * 60,
            to_minute: 6 * 60,
        };
        let at_23 = RuleContext {
            minute_of_day: Some(23 * 60),
            ..Default::default()
        };
        let at_noon = RuleContext {
            minute_of_day: Some(12 * 60),
            ..Default::default()
        };
        let at_5am = RuleContext {
            minute_of_day: Some(5 * 60),
            ..Default::default()
        };
        assert!(night.matches(&at_23));
        assert!(!night.matches(&at_noon));
        assert!(night.matches(&at_5am));
    }

    #[test]
    fn health_and_loss_predicates_need_data_to_fire() {
        // No metrics present → must NOT match (never act on missing data).
        let p = Predicate::LossAbove { threshold_pct: 5.0 };
        assert!(!p.matches(&RuleContext::default()));
        let p2 = Predicate::LossAbove { threshold_pct: 5.0 };
        assert!(p2.matches(&RuleContext {
            loss_pct: Some(7.8),
            ..Default::default()
        }));
    }

    #[test]
    fn rule_json_roundtrip() {
        let r = rule(
            "json",
            5,
            Predicate::AllOf {
                of: vec![
                    Predicate::AppEquals {
                        value: "chrome".into(),
                    },
                    Predicate::DomainSuffix {
                        suffix: "netflix.com".into(),
                    },
                ],
            },
            vec![
                Action::RouteTo {
                    interfaces: vec!["eth0".into(), "wifi0".into()],
                },
                Action::UseVpn {
                    profile: "us".into(),
                },
            ],
        );
        let json = serde_json::to_string(&r).unwrap();
        let back: Rule = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "json");
        assert_eq!(back.then.len(), 2);
        assert_eq!(
            back.then[0],
            Action::RouteTo {
                interfaces: vec!["eth0".into(), "wifi0".into()]
            }
        );
    }

    #[test]
    fn not_predicate_negates() {
        let p = Predicate::Not {
            of: Box::new(Predicate::AppEquals {
                value: "steam".into(),
            }),
        };
        assert!(p.matches(&ctx_app("chrome", "x.com")));
        assert!(!p.matches(&ctx_app("steam", "x.com")));
    }
}

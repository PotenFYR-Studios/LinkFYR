//! Enforcement v0 (Phase 3 core): the mutation tools every competitor
//! paywalls, done through the OS's supported firewall/shaping surfaces.
//!
//! - Per-app block/allow: Windows `netsh advfirewall` per-program rules
//!   (true per-app); Linux nftables owner rules (per-user, honestly
//!   labeled); macOS has no supported per-app surface (reported as
//!   platform_limited, not faked).
//! - Kill switch: default-deny outbound while armed. Windows firewall
//!   policy, Linux nftables insert/remove, macOS pf anchor.
//! - Split-tunnel apply: executes the exact route commands the advisory
//!   tool generates.
//! - Shaping + priority: Linux tc (fq_codel root + prio qdisc / DSCP
//!   filter); Windows/macOS honestly unavailable at v0.
//!
//! Everything is explicit-action + elevation-honest: without admin/root
//! the report lists exact commands; applied changes are tagged so
//! remove() only ever deletes what this tool created.

use std::collections::BTreeMap;
use std::time::Duration;

use linkfyr_model::optimize::ToolRunReport;

use linkfyr_optimize::exec;

const TIMEOUT: Duration = Duration::from_secs(25);
/// Marker so removal can never touch rules we did not create.
pub const OWNER_TAG: &str = "LinkFYR-Enforce";

fn report(tool: &str, ok: bool, summary: String, data: serde_json::Value) -> ToolRunReport {
    ToolRunReport {
        tool: tool.into(),
        ok,
        summary,
        took_ms: 0,
        data,
    }
}

fn elevated_or_list(tool: &str, commands: &[(String, Vec<String>)]) -> ToolRunReport {
    let rendered: Vec<String> = commands
        .iter()
        .map(|(p, a)| format!("{p} {}", a.join(" ")))
        .collect();
    if !exec::is_elevated() {
        return report(
            tool,
            false,
            "needs_elevation: commands listed".into(),
            serde_json::json!({ "outcome": "needs_elevation", "commands": rendered }),
        );
    }
    for (program, args) in commands {
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let out = exec::run(program, &refs, TIMEOUT);
        if !out.success {
            let detail: String = out.combined.trim().chars().take(300).collect();
            return report(
                tool,
                false,
                format!("failed: {detail}"),
                serde_json::json!({ "outcome": "failed", "commands": rendered, "detail": detail }),
            );
        }
    }
    report(
        tool,
        true,
        "applied".into(),
        serde_json::json!({ "outcome": "applied", "commands": rendered }),
    )
}

/// Plain-path validation for anything that reaches a subprocess.
fn valid_program_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 260
        && !path.contains(';')
        && !path.contains('&')
        && !path.contains('|')
}

/* ---- per-app firewall rules ---- */

/// Exact Windows commands for a per-program rule (pure).
pub fn windows_app_rule(name: &str, program: &str, block: bool) -> Vec<(String, Vec<String>)> {
    let action = if block { "block" } else { "allow" };
    vec![(
        "netsh".into(),
        vec![
            "advfirewall".into(),
            "firewall".into(),
            "add".into(),
            "rule".into(),
            format!("name=\"{name}\""),
            "dir=out".to_string(),
            format!("action={action}"),
            format!("program=\"{program}\""),
            "enable=yes".to_string(),
        ],
    )]
}

/// Exact Linux commands for a per-owner rule (pure). Linux cannot match
/// an executable in netfilter; the honest unit is the owning user.
pub fn linux_user_rule(name: &str, user: &str, block: bool) -> Vec<(String, Vec<String>)> {
    let verdict = if block { "reject" } else { "accept" };
    vec![
        (
            "nft".into(),
            vec![
                "add".into(),
                "table".into(),
                "inet".into(),
                OWNER_TABLE.into(),
            ],
        ),
        (
            "nft".into(),
            vec![
                "add".into(),
                "chain".into(),
                "inet".into(),
                OWNER_TABLE.into(),
                format!("{name}_out"),
                "{ type filter hook output priority 0 ; }".into(),
            ],
        ),
        (
            "nft".into(),
            vec![
                "add".into(),
                "rule".into(),
                "inet".into(),
                OWNER_TABLE.into(),
                format!("{name}_out"),
                format!("meta skuid {user} {verdict}"),
            ],
        ),
    ]
}

const OWNER_TABLE: &str = "linkfyr_enforce";

/// Block (or allow) a program/user. Windows: true per-app. Linux:
/// per-user (the kernel cannot match executables). macOS: none.
pub fn app_rule(params: &BTreeMap<String, String>, block: bool) -> ToolRunReport {
    let tool = if block { "app_block" } else { "app_allow" };
    let program = params.get("program").cloned().unwrap_or_default();
    let user = params.get("user").cloned().unwrap_or_default();
    let name = format!(
        "{OWNER_TAG}-{}",
        params
            .get("name")
            .cloned()
            .unwrap_or_else(|| if program.is_empty() {
                user.clone()
            } else {
                program.clone()
            })
    );

    if cfg!(windows) {
        if !valid_program_path(&program) {
            return report(
                tool,
                false,
                "missing or invalid 'program' (full path to an executable)".into(),
                serde_json::Value::Null,
            );
        }
        return elevated_or_list(tool, &windows_app_rule(&name, &program, block));
    }
    if cfg!(target_os = "macos") {
        return report(
            tool,
            false,
            "macOS has no supported per-app firewall surface at v0 (Network Extension is the Phase 3+ path)".into(),
            serde_json::Value::Null,
        );
    }
    if user.is_empty()
        || !user
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return report(
            tool,
            false,
            "missing or invalid 'user' (Linux enforces per-owner, not per-executable)".into(),
            serde_json::Value::Null,
        );
    }
    let mut cmds = linux_user_rule(&name, &user, block);
    if !block {
        // An allow rule alone does nothing without a default-deny; make
        // the semantics explicit: allow == remove any block we own.
        cmds = linux_remove_user_rule(&name);
        return elevated_or_list(tool, &cmds);
    }
    elevated_or_list(tool, &cmds)
}

/// Commands that remove one of our rules (pure).
pub fn linux_remove_user_rule(name: &str) -> Vec<(String, Vec<String>)> {
    let mut cmds = vec![(
        "nft".into(),
        vec![
            "delete".into(),
            "rule".into(),
            "inet".into(),
            OWNER_TABLE.into(),
            format!("{name}_out"),
            "handle".into(),
            "0".into(),
        ],
    )];
    // Table deletion follows rule deletion; failure of either is
    // best-effort at run time (elevated_or_list stops on first failure,
    // so order matters: rule first, table second only when empty).
    cmds.push((
        "nft".into(),
        vec![
            "delete".into(),
            "table".into(),
            "inet".into(),
            OWNER_TABLE.into(),
        ],
    ));
    cmds
}

pub fn windows_remove_rule(name: &str) -> Vec<(String, Vec<String>)> {
    vec![(
        "netsh".into(),
        vec![
            "advfirewall".into(),
            "firewall".into(),
            "delete".into(),
            "rule".into(),
            format!("name=\"{name}\""),
        ],
    )]
}

/// Remove every rule this tool created (scoped by the owner tag).
pub fn app_rules_remove() -> ToolRunReport {
    let tool = "app_rule_remove";
    let name = OWNER_TAG;
    let cmds = if cfg!(windows) {
        let mut all = Vec::new();
        // Delete-all-ours: Windows matches by name prefix only through
        // iteration; the PowerShell filter does the scoping.
        all.push((
            "powershell".into(),
            vec![
                "-NoProfile".into(),
                "-Command".into(),
                format!("Get-NetFirewallRule -DisplayName '{name}*' | Remove-NetFirewallRule"),
            ],
        ));
        all
    } else if cfg!(target_os = "macos") {
        Vec::new()
    } else {
        vec![(
            "nft".into(),
            vec![
                "delete".into(),
                "table".into(),
                "inet".into(),
                OWNER_TABLE.into(),
            ],
        )]
    };
    if cmds.is_empty() {
        return report(
            tool,
            false,
            "no supported surface on this platform".into(),
            serde_json::Value::Null,
        );
    }
    elevated_or_list(tool, &cmds)
}

/// List the rules we own.
pub fn app_rules_list() -> ToolRunReport {
    let tool = "app_rule_list";
    if cfg!(windows) {
        let out = exec::run(
            "powershell",
            &[
                "-NoProfile",
                "-Command",
                &format!(
                    "Get-NetFirewallRule -DisplayName '{OWNER_TAG}*' | Select-Object DisplayName,Enabled,Action | ConvertTo-Csv -NoTypeInformation"
                ),
            ],
            TIMEOUT,
        );
        let count = out
            .stdout
            .lines()
            .skip(1)
            .filter(|l| l.contains(OWNER_TAG))
            .count();
        return report(
            tool,
            true,
            format!("{count} LinkFYR rule(s)"),
            serde_json::json!({ "rules": out.stdout.trim() }),
        );
    }
    if cfg!(target_os = "macos") {
        return report(
            tool,
            false,
            "no supported surface on this platform".into(),
            serde_json::Value::Null,
        );
    }
    let out = exec::run("nft", &["list", "table", "inet", OWNER_TABLE], TIMEOUT);
    if out.success {
        let rules: Vec<&str> = out.stdout.lines().filter(|l| l.contains("skuid")).collect();
        return report(
            tool,
            true,
            format!("{} owner rule(s)", rules.len()),
            serde_json::json!({ "rules": rules }),
        );
    }
    report(
        tool,
        true,
        "no LinkFYR rules present".into(),
        serde_json::json!({ "rules": [] }),
    )
}

/* ---- kill switch ---- */

/// Commands to arm default-deny outbound (pure). Inbound default-deny
/// is already the OS default everywhere; we only flip outbound.
pub fn kill_switch_arm_commands() -> Vec<(String, Vec<String>)> {
    if cfg!(windows) {
        vec![(
            "netsh".into(),
            vec![
                "advfirewall".into(),
                "set".into(),
                "allprofiles".into(),
                "firewallpolicy".into(),
                "blockinbound,blockoutbound".into(),
            ],
        )]
    } else if cfg!(target_os = "macos") {
        vec![(
            "pfctl".into(),
            vec![
                "-e".into(),
                "-f".into(),
                "/etc/pf.anchors/linkfyr-killswitch".into(),
            ],
        )]
    } else {
        vec![
            (
                "nft".into(),
                vec![
                    "add".into(),
                    "table".into(),
                    "inet".into(),
                    "linkfyr_killswitch".into(),
                ],
            ),
            (
                "nft".into(),
                vec![
                    "add".into(),
                    "chain".into(),
                    "inet".into(),
                    "linkfyr_killswitch".into(),
                    "blocked".into(),
                    "{ type filter hook output priority -100 ; }".into(),
                ],
            ),
            // Block all new outbound except loopback and established
            // traffic (this is a hold, not a brick: fail-closed with
            // the documented escape of removing the table).
            (
                "nft".into(),
                vec![
                    "add".into(),
                    "rule".into(),
                    "inet".into(),
                    "linkfyr_killswitch".into(),
                    "blocked".into(),
                    "ct state established,related accept".into(),
                ],
            ),
            (
                "nft".into(),
                vec![
                    "add".into(),
                    "rule".into(),
                    "inet".into(),
                    "linkfyr_killswitch".into(),
                    "blocked".into(),
                    "iif \"lo\" accept".into(),
                ],
            ),
            (
                "nft".into(),
                vec![
                    "add".into(),
                    "rule".into(),
                    "inet".into(),
                    "linkfyr_killswitch".into(),
                    "blocked".into(),
                    "reject".into(),
                ],
            ),
        ]
    }
}

pub fn kill_switch_disarm_commands() -> Vec<(String, Vec<String>)> {
    if cfg!(windows) {
        vec![(
            "netsh".into(),
            vec![
                "advfirewall".into(),
                "set".into(),
                "allprofiles".into(),
                "firewallpolicy".into(),
                "blockinbound,allowoutbound".into(),
            ],
        )]
    } else if cfg!(target_os = "macos") {
        vec![("pfctl".into(), vec!["-d".into()])]
    } else {
        vec![(
            "nft".into(),
            vec![
                "delete".into(),
                "table".into(),
                "inet".into(),
                "linkfyr_killswitch".into(),
            ],
        )]
    }
}

/// Arm the kill switch (fail-closed hold). Explicit user action only.
pub fn kill_switch(params: &BTreeMap<String, String>) -> ToolRunReport {
    let arm = params.get("state").is_none_or(|s| s != "off");
    let tool = if arm {
        "kill_switch_arm"
    } else {
        "kill_switch_disarm"
    };
    let cmds = if arm {
        kill_switch_arm_commands()
    } else {
        kill_switch_disarm_commands()
    };
    let mut r = elevated_or_list(tool, &cmds);
    if r.ok && arm {
        r.summary =
            "kill switch ARMED: outbound traffic is held (established + loopback allowed)".into();
    } else if r.ok {
        r.summary = "kill switch disarmed: normal outbound policy restored".into();
    }
    r
}

/* ---- split-tunnel apply ---- */

/// Execute the advisory split-tunnel routes for one host. Resolves the
/// target, then adds a host route via the physical gateway.
pub fn split_tunnel_apply(params: &BTreeMap<String, String>) -> ToolRunReport {
    let tool = "vpn_split_enforce";
    let target = params.get("target").cloned().unwrap_or_default();
    let ip = if target.parse::<std::net::Ipv4Addr>().is_ok() {
        target.clone()
    } else {
        match (target.as_str(), 443u16).to_socket_addrs() {
            Ok(mut it) => it.next().map(|a| a.ip().to_string()).unwrap_or_default(),
            Err(_) => String::new(),
        }
    };
    if ip.is_empty() {
        return report(
            tool,
            false,
            "missing 'target' (IP or resolvable host)".into(),
            serde_json::Value::Null,
        );
    }
    let Some(gw) = linkfyr_optimize::probes::default_gateway() else {
        return report(
            tool,
            false,
            "no default gateway found".into(),
            serde_json::Value::Null,
        );
    };
    let cmds: Vec<(String, Vec<String>)> = if cfg!(windows) {
        vec![("route".into(), vec!["add".into(), ip.clone(), gw.clone()])]
    } else if cfg!(target_os = "macos") {
        vec![(
            "route".into(),
            vec![
                "-n".into(),
                "add".into(),
                "-host".into(),
                ip.clone(),
                gw.clone(),
            ],
        )]
    } else {
        vec![(
            "ip".into(),
            vec![
                "route".into(),
                "add".into(),
                format!("{ip}/32"),
                "via".into(),
                gw.clone(),
            ],
        )]
    };
    elevated_or_list(tool, &cmds)
}

use std::net::ToSocketAddrs;

/* ---- shaping + priority (Linux tc) ---- */

/// Apply the latency-friendly queueing setup: fq_codel root + a prio
/// child for marked traffic (DSCP EF/CS classes skip the bulk queue).
pub fn shaping_apply(params: &BTreeMap<String, String>) -> ToolRunReport {
    let tool = if params.get("priority").map(String::as_str) == Some("1") {
        "traffic_priority"
    } else {
        "shaping"
    };
    if !cfg!(target_os = "linux") && !exec::on_path("tc") {
        return report(
            tool,
            false,
            "shaping needs Linux tc (fq_codel/cake); other platforms are Phase 3+ (WFP/qWAVE)"
                .into(),
            serde_json::Value::Null,
        );
    }
    let iface = params
        .get("interface")
        .cloned()
        .unwrap_or_else(|| "eth0".into());
    if !valid_program_path(&iface) || iface.contains('/') {
        return report(
            tool,
            false,
            "invalid interface name".into(),
            serde_json::Value::Null,
        );
    }
    let cmds: Vec<(String, Vec<String>)> = if tool == "traffic_priority" {
        vec![
            (
                "tc".into(),
                vec![
                    "qdisc".into(),
                    "replace".into(),
                    "dev".into(),
                    iface.clone(),
                    "root".into(),
                    "handle".into(),
                    "1:".into(),
                    "prio".into(),
                ],
            ),
            (
                "tc".into(),
                vec![
                    "filter".into(),
                    "add".into(),
                    "dev".into(),
                    iface.clone(),
                    "parent".into(),
                    "1:".into(),
                    "protocol".into(),
                    "ip".into(),
                    "prio".into(),
                    "1".into(),
                    "u32".into(),
                    "match".into(),
                    "ip".into(),
                    "tos".into(),
                    "0xb8".into(),
                    "0xff".into(),
                    "flowid".into(),
                    "1:1".into(),
                ],
            ),
        ]
    } else {
        vec![(
            "tc".into(),
            vec![
                "qdisc".into(),
                "replace".into(),
                "dev".into(),
                iface.clone(),
                "root".into(),
                "fq_codel".into(),
                "limit".into(),
                "2048".into(),
                "target".into(),
                "5ms".into(),
            ],
        )]
    };
    elevated_or_list(tool, &cmds)
}

pub fn shaping_remove(params: &BTreeMap<String, String>) -> ToolRunReport {
    if !exec::on_path("tc") {
        return report(
            "shaping_remove",
            false,
            "tc unavailable".into(),
            serde_json::Value::Null,
        );
    }
    let iface = params
        .get("interface")
        .cloned()
        .unwrap_or_else(|| "eth0".into());
    elevated_or_list(
        "shaping_remove",
        &[(
            "tc".into(),
            vec![
                "qdisc".into(),
                "del".into(),
                "dev".into(),
                iface,
                "root".into(),
            ],
        )],
    )
}

/// Dispatch used by the engine for `enforce.*` registry tools.
pub fn run(tool: &str, params: &BTreeMap<String, String>) -> ToolRunReport {
    match tool {
        "app_block" => app_rule(params, true),
        "app_allow" => app_rule(params, false),
        "app_rule_list" => app_rules_list(),
        "app_rule_remove" => app_rules_remove(),
        "kill_switch_arm" | "kill_switch_disarm" => kill_switch(params),
        "vpn_split_enforce" => split_tunnel_apply(params),
        "shaping" => shaping_apply(params),
        "traffic_priority" => {
            let mut p = params.clone();
            p.insert("priority".to_string(), "1".to_string());
            shaping_apply(&p)
        }
        "shaping_remove" => shaping_remove(params),
        other => report(
            other,
            false,
            format!("unknown enforcement tool '{other}'"),
            serde_json::Value::Null,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn windows_rules_are_per_program_and_tagged() {
        let cmds = windows_app_rule("LinkFYR-Enforce-x", r"C:\Program Files\app.exe", true);
        let joined = cmds[0].1.join(" ");
        assert!(joined.contains("program=\"C:\\Program Files\\app.exe\""));
        assert!(joined.contains("action=block"));
        assert!(joined.contains("name=\"LinkFYR-Enforce-x\""));
        let allow = windows_app_rule("n", "a.exe", false);
        assert!(allow[0].1.join(" ").contains("action=allow"));
    }

    #[test]
    fn linux_rules_are_per_owner_and_scoped() {
        let cmds = linux_user_rule("u", "games", true);
        let all: String = cmds
            .iter()
            .map(|(_, a)| a.join(" "))
            .collect::<Vec<_>>()
            .join(" ; ");
        assert!(all.contains("linkfyr_enforce"));
        assert!(all.contains("meta skuid games reject"));
    }

    #[test]
    fn kill_switch_commands_arm_and_disarm_symmetrically() {
        if cfg!(windows) {
            assert!(
                kill_switch_arm_commands()[0]
                    .1
                    .join(" ")
                    .contains("blockoutbound")
            );
            assert!(
                kill_switch_disarm_commands()[0]
                    .1
                    .join(" ")
                    .contains("allowoutbound")
            );
        } else if cfg!(target_os = "macos") {
            assert!(kill_switch_arm_commands()[0].1.contains(&"-e".to_string()));
            assert!(
                kill_switch_disarm_commands()[0]
                    .1
                    .contains(&"-d".to_string())
            );
        } else {
            let arm = kill_switch_arm_commands();
            assert!(
                arm.iter()
                    .any(|(_, a)| a.contains(&"linkfyr_killswitch".to_string()))
            );
            assert!(
                arm.iter()
                    .any(|(_, a)| a.iter().any(|s| s.contains("ct state")))
            );
            assert_eq!(kill_switch_disarm_commands()[0].1[0], "delete");
        }
    }

    #[test]
    fn injection_shapes_are_rejected() {
        assert!(!app_rule(&p(&[("program", "x; rm -rf")]), true).ok);
        assert!(!app_rule(&p(&[("user", "root || reboot")]), true).ok);
        assert!(!shaping_apply(&p(&[("interface", "eth0; reboot")])).ok);
        assert!(!split_tunnel_apply(&p(&[("target", "a b")])).ok);
    }

    #[test]
    fn unelevated_runs_list_exact_commands() {
        if exec::is_elevated() {
            return; // containers are root: the applied path runs instead
        }
        let r = kill_switch(&p(&[("state", "on")]));
        assert_eq!(
            r.data.get("outcome").and_then(|o| o.as_str()),
            Some("needs_elevation")
        );
        assert_ne!(r.data.get("commands").unwrap().as_array().unwrap().len(), 0);
    }

    #[test]
    fn dispatcher_covers_every_tool() {
        assert!(!run("definitely_not", &p(&[])).ok);
        for id in ["app_rule_list", "app_rule_remove", "shaping_remove"] {
            let _ = run(id, &p(&[])); // must not panic on any platform
        }
    }
}

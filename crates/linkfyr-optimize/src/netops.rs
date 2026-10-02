//! Net-ops: elevation-honest control tools (DHCP, adapter reset,
//! winsock, TCP-tuning apply, hotspot/ICS) plus state-file backed
//! watch tools (route/DNS changes, ARP gateway watch, Wi-Fi history).
//! Control commands run only when elevated; otherwise the exact
//! command list is returned. Watch tools persist a small JSON state
//! file under the user data dir and diff against it.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use linkfyr_model::optimize::{TextReport, ToolRunReport};

use crate::exec;
use crate::{routeaudit, sysnet, wifiscan};

const TIMEOUT: Duration = Duration::from_secs(25);

fn report(tool: &str, ok: bool, summary: String, data: serde_json::Value) -> ToolRunReport {
    ToolRunReport {
        tool: tool.into(),
        ok,
        summary,
        took_ms: 0,
        data,
    }
}

fn text(tool: &str, ok: bool, text: &str) -> ToolRunReport {
    report(
        tool,
        ok,
        text.lines().next().unwrap_or_default().to_string(),
        serde_json::to_value(TextReport {
            text: text.to_string(),
            ms: None,
        })
        .unwrap_or_default(),
    )
}

fn valid_iface_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 64 && !name.contains(';') && !name.contains('&')
}

/// Run a control command when elevated, else list it (shared shape).
fn run_control(tool: &str, commands: &[(String, Vec<String>)]) -> ToolRunReport {
    let rendered: Vec<String> = commands
        .iter()
        .map(|(p, a)| format!("{p} {}", a.join(" ")))
        .collect();
    if commands.is_empty() {
        return report(
            tool,
            false,
            "unsupported on this platform".into(),
            serde_json::Value::Null,
        );
    }
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

pub fn dhcp_renew(params: &BTreeMap<String, String>) -> ToolRunReport {
    let iface = params.get("interface").map(String::as_str);
    let cmds = if cfg!(windows) {
        let mut v = vec![("ipconfig".to_string(), vec!["/renew".to_string()])];
        if let Some(i) = iface {
            v.push((
                "ipconfig".to_string(),
                vec!["/renew".to_string(), i.to_string()],
            ));
        }
        v
    } else if cfg!(target_os = "macos") {
        match iface {
            Some(i) if valid_iface_name(i) => {
                vec![(
                    "ipconfig".to_string(),
                    vec!["set".to_string(), i.to_string(), "DHCP".to_string()],
                )]
            }
            _ => vec![(
                "networksetup".to_string(),
                vec!["-setbootp".to_string(), "Wi-Fi".to_string()],
            )],
        }
    } else {
        match iface {
            Some(i) if valid_iface_name(i) => vec![(
                "nmcli".to_string(),
                vec!["con".into(), "up".into(), i.to_string()],
            )],
            _ => vec![("dhclient".to_string(), vec!["-1".to_string()])],
        }
    };
    run_control("dhcp_renew", &cmds)
}

pub fn dhcp_release(params: &BTreeMap<String, String>) -> ToolRunReport {
    let iface = params.get("interface").map(String::as_str);
    let cmds = if cfg!(windows) {
        let mut v = vec![("ipconfig".to_string(), vec!["/release".to_string()])];
        if let Some(i) = iface {
            v.push((
                "ipconfig".to_string(),
                vec!["/release".to_string(), i.to_string()],
            ));
        }
        v
    } else if cfg!(target_os = "macos") {
        match iface {
            Some(i) if valid_iface_name(i) => {
                vec![(
                    "ipconfig".to_string(),
                    vec!["set".to_string(), i.to_string(), "NONE".to_string()],
                )]
            }
            _ => {
                return report(
                    "dhcp_release",
                    false,
                    "macOS needs an interface name".into(),
                    serde_json::Value::Null,
                );
            }
        }
    } else {
        match iface {
            Some(i) if valid_iface_name(i) => vec![(
                "dhclient".to_string(),
                vec!["-r".to_string(), i.to_string()],
            )],
            _ => vec![("dhclient".to_string(), vec!["-r".to_string()])],
        }
    };
    run_control("dhcp_release", &cmds)
}

pub fn adapter_reset(params: &BTreeMap<String, String>) -> ToolRunReport {
    let Some(iface) = params.get("interface") else {
        return report(
            "adapter_reset",
            false,
            "missing 'interface' parameter".into(),
            serde_json::Value::Null,
        );
    };
    if !valid_iface_name(iface) {
        return report(
            "adapter_reset",
            false,
            "invalid interface name".into(),
            serde_json::Value::Null,
        );
    }
    let cmds = if cfg!(windows) {
        vec![
            (
                "netsh".to_string(),
                vec![
                    "interface".into(),
                    "set".into(),
                    "interface".into(),
                    format!("name=\"{iface}\""),
                    "disable".into(),
                ],
            ),
            (
                "netsh".to_string(),
                vec![
                    "interface".into(),
                    "set".into(),
                    "interface".into(),
                    format!("name=\"{iface}\""),
                    "enable".into(),
                ],
            ),
        ]
    } else if cfg!(target_os = "macos") {
        vec![
            ("ifconfig".to_string(), vec![iface.clone(), "down".into()]),
            ("ifconfig".to_string(), vec![iface.clone(), "up".into()]),
        ]
    } else {
        vec![
            (
                "ip".to_string(),
                vec!["link".into(), "set".into(), iface.clone(), "down".into()],
            ),
            (
                "ip".to_string(),
                vec!["link".into(), "set".into(), iface.clone(), "up".into()],
            ),
        ]
    };
    run_control("adapter_reset", &cmds)
}

pub fn winsock_reset() -> ToolRunReport {
    if !cfg!(windows) {
        return report(
            "winsock_reset",
            false,
            "Windows-only repair".into(),
            serde_json::Value::Null,
        );
    }
    let r = run_control(
        "winsock_reset",
        &[(
            "netsh".to_string(),
            vec!["winsock".to_string(), "reset".to_string()],
        )],
    );
    if r.ok {
        return text(
            "winsock_reset",
            true,
            "Winsock catalog reset applied. A reboot completes the repair.",
        );
    }
    r
}

/// Apply the exact fixes the TCP audit listed (explicit confirmation
/// is the user running this tool; nothing is ever silent).
pub fn tcp_tuning_apply() -> ToolRunReport {
    let audit = crate::tcpaudit::audit();
    let fixes: Vec<(String, Vec<String>)> = audit
        .checks
        .iter()
        .filter_map(|c| c.fix.as_ref().map(|f| (f.clone(), c.key.clone())))
        .filter_map(|(fix, _key)| parse_fix(&fix))
        .collect();
    if fixes.is_empty() {
        return text(
            "tcp_tuning_apply",
            true,
            "Nothing to apply: no suboptimal settings in the audit.",
        );
    }
    run_control("tcp_tuning_apply", &fixes)
}

/// netsh/sysctl fix strings -> argv. sysctl uses "key=value"; netsh is
/// plain argv. PowerShell-style fixes are not produced by the audit.
fn parse_fix(fix: &str) -> Option<(String, Vec<String>)> {
    let mut parts = fix.split_whitespace().map(ToString::to_string);
    let program = parts.next()?;
    let args: Vec<String> = parts.collect();
    if program == "sudo" {
        // macOS fixes carry sudo; the tool itself is elevated.
        let mut inner = fix.split_whitespace().skip(1);
        let prog = inner.next()?.to_string();
        return Some((prog, inner.map(ToString::to_string).collect()));
    }
    Some((program, args))
}

/// Windows ICS (hotspot) via the documented-by-use COM manager.
pub fn hotspot_share(params: &BTreeMap<String, String>) -> ToolRunReport {
    if !cfg!(windows) {
        return report(
            "hotspot_share",
            false,
            "Windows ICS only; Linux uses bridge + dnsmasq (see bridge tool)".into(),
            serde_json::Value::Null,
        );
    }
    let action = params.get("action").map_or("list", String::as_str);
    let ps = match action {
        "list" => {
            "Get-NetConnectionProfile | Select-Object InterfaceAlias,IPv4Connectivity | ConvertTo-Csv -NoTypeInformation".to_string()
        }
        "share" => {
            let public = params.get("public").cloned().unwrap_or_default();
            let private = params.get("private").cloned().unwrap_or_default();
            if public.is_empty() || private.is_empty() {
                return report(
                    "hotspot_share",
                    false,
                    "share needs 'public' and 'private' interface names".into(),
                    serde_json::Value::Null,
                );
            }
            format!(
                "$m = New-Object -ComObject HNetCfg.NetSharingManager; \
                 $m.EnumEveryConnection | ForEach-Object {{ \
                   $c = $m.INetSharingConfigurationForINetConnection($_); \
                   if ($_.Name -eq '{public}') {{ $c.EnableSharing(0) }} \
                   if ($_.Name -eq '{private}') {{ $c.EnableSharing(1) }} }}"
            )
        }
        other => {
            return report(
                "hotspot_share",
                false,
                format!("unknown action '{other}' (list|share)"),
                serde_json::Value::Null,
            )
        }
    };
    run_control(
        "hotspot_share",
        &[(
            "powershell".to_string(),
            vec!["-NoProfile".into(), "-Command".into(), ps],
        )],
    )
}

/* ---- state-file backed watches ---- */

fn state_dir() -> PathBuf {
    let base = directories::ProjectDirs::from("app", "linkfyr", "linkfyr").map_or_else(
        || std::env::temp_dir().join("linkfyr"),
        |d| d.data_local_dir().to_path_buf(),
    );
    let dir = base.join("toolstate");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Shared state directory for watch/snapshot tools (also used by svcprobe).
pub fn state_dir_public() -> PathBuf {
    state_dir()
}

/// Read a stored watch state file (used by the timeline tool).
pub fn load_state_public(name: &str) -> Option<serde_json::Value> {
    std::fs::read_to_string(state_dir().join(format!("{name}.json")))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
}

fn load_state(name: &str) -> serde_json::Value {
    std::fs::read_to_string(state_dir().join(format!("{name}.json")))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(serde_json::Value::Null)
}

fn store_state(name: &str, value: &serde_json::Value) {
    let _ = std::fs::write(
        state_dir().join(format!("{name}.json")),
        serde_json::to_string_pretty(value).unwrap_or_default(),
    );
}

fn diff_report(tool: &str, label: &str, current: &serde_json::Value) -> ToolRunReport {
    let previous = load_state(tool);
    store_state(tool, current);
    let changed = previous != *current && !previous.is_null();
    let summary = if previous.is_null() {
        format!("{label}: baseline stored; run again to detect changes")
    } else if changed {
        format!("{label}: CHANGED since last run")
    } else {
        format!("{label}: unchanged since last run")
    };
    report(
        tool,
        true,
        summary,
        serde_json::json!({ "previous": previous, "current": current, "changed": changed }),
    )
}

pub fn route_change_watch() -> ToolRunReport {
    let r = routeaudit::audit();
    let sig: Vec<String> = r
        .entries
        .iter()
        .map(|e| {
            format!(
                "{}|{}|{}",
                e.destination,
                e.gateway.as_deref().unwrap_or("-"),
                e.interface.as_deref().unwrap_or("-")
            )
        })
        .collect();
    diff_report(
        "route_change_watch",
        "Routing table",
        &serde_json::json!(sig),
    )
}

pub fn dns_change_watch() -> ToolRunReport {
    let resolvers = crate::dns::system_resolvers();
    diff_report(
        "dns_change_watch",
        "System resolvers",
        &serde_json::json!(resolvers),
    )
}

pub fn arp_spoof_check() -> ToolRunReport {
    let Some(gw) = crate::probes::default_gateway() else {
        return report(
            "arp_spoof_check",
            false,
            "no default gateway found".into(),
            serde_json::Value::Null,
        );
    };
    let mac = sysnet::arp_table()
        .into_iter()
        .find(|(ip, _, _)| *ip == gw)
        .map(|(_, mac, _)| mac);
    let current = serde_json::json!({ "gateway": gw, "mac": mac });
    let previous = load_state("arp_spoof_check");
    let changed =
        previous.get("mac").is_some() && previous.get("mac") != current.get("mac") && mac.is_some();
    store_state("arp_spoof_check", &current);
    report(
        "arp_spoof_check",
        true,
        if changed {
            format!(
                "WARNING: gateway {gw} MAC changed ({:?} -> {mac:?}); possible ARP spoofing",
                previous.get("mac")
            )
        } else {
            format!("gateway {gw} MAC stable ({mac:?})")
        },
        serde_json::json!({ "previous": previous, "current": current, "changed": changed }),
    )
}

pub fn channel_history() -> ToolRunReport {
    let scan = wifiscan::scan();
    let current: BTreeMap<String, u8> = scan
        .networks
        .iter()
        .map(|ap| {
            (
                format!(
                    "{}|{}",
                    ap.ssid.as_deref().unwrap_or("(hidden)"),
                    ap.channel
                ),
                ap.signal_pct,
            )
        })
        .collect();
    diff_report(
        "channel_history",
        "Wi-Fi environment",
        &serde_json::json!(current),
    )
}

pub fn pmtud_watch(params: &BTreeMap<String, String>) -> ToolRunReport {
    let target = params
        .get("target")
        .cloned()
        .unwrap_or_else(|| "1.1.1.1".into());
    let rounds: u32 = params
        .get("rounds")
        .and_then(|r| r.parse().ok())
        .unwrap_or(5)
        .clamp(1, 30);
    let mut mtus = Vec::new();
    for _ in 0..rounds {
        let r = crate::mtu::probe(&target, 1500);
        if r.path_mtu > 0 {
            mtus.push(r.path_mtu);
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    if mtus.is_empty() {
        return report(
            "pmtud_watch",
            false,
            format!("no DF probe to {target} answered"),
            serde_json::Value::Null,
        );
    }
    let min = *mtus.iter().min().unwrap();
    let max = *mtus.iter().max().unwrap();
    report(
        "pmtud_watch",
        true,
        format!(
            "path MTU to {target}: min {min}, max {max} over {} probes",
            mtus.len()
        ),
        serde_json::json!({ "samples": mtus, "min": min, "max": max, "stable": min == max }),
    )
}

pub fn mtu_monitor(params: &BTreeMap<String, String>) -> ToolRunReport {
    pmtud_watch(params)
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
    fn control_tools_are_elevation_honest() {
        if exec::is_elevated() {
            return; // containers run as root: the applied path is real
        }
        for r in [
            dhcp_renew(&p(&[])),
            dhcp_release(&p(&[])),
            adapter_reset(&p(&[("interface", "eth0")])),
            winsock_reset(),
        ] {
            if cfg!(unix) || r.tool != "winsock_reset" {
                assert_eq!(
                    r.data.get("outcome").and_then(|o| o.as_str()),
                    Some("needs_elevation"),
                    "{}",
                    r.summary
                );
            }
        }
    }

    #[test]
    fn adapter_reset_rejects_injection_shapes() {
        let r = adapter_reset(&p(&[("interface", "eth0; reboot")]));
        assert!(!r.ok);
        assert!(r.summary.contains("invalid"));
    }

    #[test]
    fn parse_fix_understands_sysctl_and_strips_sudo() {
        let f = parse_fix("sysctl -w net.ipv4.tcp_ecn=1").unwrap();
        assert_eq!(f.0, "sysctl");
        assert_eq!(
            f.1,
            vec!["-w".to_string(), "net.ipv4.tcp_ecn=1".to_string()]
        );
        let mac = parse_fix("sudo sysctl -w net.inet.tcp.sack=1").unwrap();
        assert_eq!(mac.0, "sysctl");
    }

    #[test]
    fn watches_store_and_diff() {
        let first = route_change_watch();
        assert!(first.summary.contains("baseline stored") || first.summary.contains("unchanged"));
        let second = route_change_watch();
        assert!(second.summary.contains("unchanged") || second.summary.contains("CHANGED"));
        let arp = arp_spoof_check();
        assert_ne!(arp.summary, "");
        let dns = dns_change_watch();
        assert!(dns.ok);
    }

    #[test]
    fn pmtud_watch_runs_real_probes() {
        if !exec::on_path("ping") {
            return;
        }
        let r = pmtud_watch(&p(&[("target", "127.0.0.1"), ("rounds", "2")]));
        assert!(r.ok, "{}", r.summary);
        assert!(r.summary.contains("1500"));
    }
}

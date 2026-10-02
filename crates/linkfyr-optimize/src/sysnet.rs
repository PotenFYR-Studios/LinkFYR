//! System network state: per-process connections (the GlassWire-style
//! "which app is talking" table), local listening ports, ARP/neighbor
//! tables, proxy configuration detection, and the hosts file. Each
//! platform is read through its supported command; all parsers are
//! fixture-tested.

use std::time::Duration;

use linkfyr_model::optimize::{AppConnection, AppGroup, ConnectionsReport};

use crate::exec;

const TIMEOUT: Duration = Duration::from_secs(15);

/// Parse Windows `netstat -ano` TCP/UDP tables (pure).
pub fn parse_netstat_ano(output: &str) -> Vec<AppConnection> {
    let mut out = Vec::new();
    for line in output.lines() {
        let t = line.trim();
        if t.starts_with("TCP") {
            let cols: Vec<&str> = t.split_whitespace().collect();
            // TCP  local  remote  state  pid
            if cols.len() >= 5 {
                out.push(AppConnection {
                    proto: "tcp".into(),
                    local: cols[1].into(),
                    remote: cols[2].into(),
                    state: cols[3].to_lowercase(),
                    pid: cols[4].parse().ok(),
                    process: None,
                });
            }
        } else if t.starts_with("UDP") {
            let cols: Vec<&str> = t.split_whitespace().collect();
            // UDP  local  *:*  pid
            if cols.len() >= 4 {
                out.push(AppConnection {
                    proto: "udp".into(),
                    local: cols[1].into(),
                    remote: cols[2].into(),
                    state: "unconn".into(),
                    pid: cols[3].parse().ok(),
                    process: None,
                });
            }
        }
    }
    out
}

/// Parse Linux `ss -tunpa` output (pure). The process column carries
/// `users:(("name",pid=123,fd=4))` when attribution is permitted.
pub fn parse_ss(output: &str) -> Vec<AppConnection> {
    let mut out = Vec::new();
    for line in output.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 5 {
            continue;
        }
        let proto = cols[0];
        if proto != "tcp" && proto != "udp" {
            continue;
        }
        // ss: proto state recvq sendq local peer [process...]
        let state = cols[1].to_lowercase();
        let (local, remote, rest_at) = (cols[4], cols[5], 6);
        let mut process = None;
        let mut pid = None;
        if let Some(process_col) = cols.get(rest_at) {
            let col = process_col.to_string();
            if let Some(name) = col.split('"').nth(1) {
                process = Some(name.to_string());
            }
            if let Some(pid_start) = col.find("pid=") {
                let digits: String = col[pid_start + 4..]
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                pid = digits.parse().ok();
            }
        }
        out.push(AppConnection {
            proto: proto.into(),
            local: local.into(),
            remote: if remote == "*" {
                "-".into()
            } else {
                remote.to_string()
            },
            state: if state == "unconn" && proto == "udp" {
                "unconn".into()
            } else {
                state
            },
            pid,
            process,
        });
    }
    out
}

/// Parse macOS `lsof -n -P -i -F pcnLP` machine-readable output (pure).
pub fn parse_lsof_f(output: &str) -> Vec<AppConnection> {
    #[allow(clippy::too_many_arguments)]
    fn flush(
        out: &mut Vec<AppConnection>,
        pid: &mut Option<u32>,
        process: &mut Option<String>,
        proto: &mut Option<String>,
        name: &mut Option<String>,
        state: &mut String,
    ) {
        if let (Some(p), Some(n)) = (proto.take(), name.take()) {
            let (local, remote) = n
                .split_once("->")
                .map(|(l, r)| (l.trim().to_string(), r.trim().to_string()))
                .unwrap_or((n.clone(), "-".into()));
            out.push(AppConnection {
                proto: p.to_lowercase(),
                local,
                remote,
                state: if state.is_empty() {
                    "established".into()
                } else {
                    state.clone()
                },
                pid: *pid,
                process: process.take(),
            });
        }
        *pid = None;
        *process = None;
        *state = String::new();
    }

    let mut out = Vec::new();
    let mut pid = None;
    let mut process = None;
    let mut proto = None;
    let mut name = None;
    let mut state = String::new();
    for line in output.lines() {
        let (tag, rest) = line.split_at(1.min(line.len()));
        match tag {
            "p" => {
                flush(
                    &mut out,
                    &mut pid,
                    &mut process,
                    &mut proto,
                    &mut name,
                    &mut state,
                );
                pid = rest.parse().ok();
            }
            "c" => process = Some(rest.to_string()),
            "P" => proto = Some(rest.to_string()),
            "n" => name = Some(rest.to_string()),
            "T" => {
                if let Some(s) = rest.strip_prefix("ST=") {
                    state = s.to_lowercase();
                }
            }
            _ => {}
        }
    }
    flush(
        &mut out,
        &mut pid,
        &mut process,
        &mut proto,
        &mut name,
        &mut state,
    );
    out
}

/// Capture + attribute connections for this OS.
pub fn connections() -> ConnectionsReport {
    let (raw, mut conns, detail) = if cfg!(windows) {
        let out = exec::run("netstat", &["-ano"], TIMEOUT);
        (
            out.stdout.clone(),
            parse_netstat_ano(&out.stdout),
            "netstat -ano (own table)".to_string(),
        )
    } else if cfg!(target_os = "macos") {
        let out = exec::run("lsof", &["-n", "-P", "-i", "-F", "pcnLPT"], TIMEOUT);
        (
            out.stdout.clone(),
            parse_lsof_f(&out.stdout),
            "lsof -F (own processes visible without elevation)".to_string(),
        )
    } else {
        let out = exec::run("ss", &["-tunpa"], TIMEOUT);
        let detail = if out.success {
            "ss -tunpa (other users' sockets hidden without elevation)".to_string()
        } else {
            "ss unavailable; no connection table".to_string()
        };
        (out.stdout.clone(), parse_ss(&out.stdout), detail)
    };
    let _ = raw;

    // Windows/macOS attribution: map pid → process name via the task list.
    if conns.iter().any(|c| c.process.is_none() && c.pid.is_some()) {
        let names = process_names();
        for c in &mut conns {
            if c.process.is_none() {
                if let Some(pid) = c.pid {
                    c.process = names.get(&pid).cloned();
                }
            }
        }
    }

    let groups = group_connections(&conns);
    ConnectionsReport {
        connections: conns,
        groups,
        detail,
    }
}

/// pid → name map from the OS task/process list.
pub fn process_names() -> std::collections::HashMap<u32, String> {
    let mut map = std::collections::HashMap::new();
    if cfg!(windows) {
        let out = exec::run("tasklist", &["/fo", "csv", "/nh"], TIMEOUT);
        for line in out.stdout.lines() {
            let fields: Vec<&str> = line.split("\",\"").collect();
            if fields.len() >= 3 {
                let name = fields[0].trim_matches('"');
                if let Ok(pid) = fields[1].parse::<u32>() {
                    map.insert(pid, name.to_string());
                }
            }
        }
    } else {
        let out = exec::run("ps", &["-eo", "pid,comm"], TIMEOUT);
        for line in out.stdout.lines().skip(1) {
            let mut it = line.split_whitespace();
            if let (Some(pid), Some(name)) = (it.next(), it.next()) {
                if let Ok(pid) = pid.parse::<u32>() {
                    map.insert(pid, name.to_string());
                }
            }
        }
    }
    map
}

/// Group connections per process (pure).
pub fn group_connections(conns: &[AppConnection]) -> Vec<AppGroup> {
    let mut groups: Vec<AppGroup> = Vec::new();
    for c in conns {
        let name = c.process.clone().unwrap_or_else(|| "(unknown)".into());
        if let Some(g) = groups
            .iter_mut()
            .find(|g| g.process == name && g.pid == c.pid)
        {
            g.connections += 1;
            if !g.remotes.contains(&c.remote) && c.remote != "-" {
                g.remotes.push(c.remote.clone());
            }
        } else {
            groups.push(AppGroup {
                process: name,
                pid: c.pid,
                connections: 1,
                remotes: if c.remote == "-" {
                    vec![]
                } else {
                    vec![c.remote.clone()]
                },
            });
        }
    }
    groups.sort_by_key(|g| std::cmp::Reverse(g.connections));
    for g in &mut groups {
        g.remotes.truncate(6);
    }
    groups
}

/// Listening sockets on this machine (derived from the same table).
pub fn listening_ports() -> Vec<AppConnection> {
    connections()
        .connections
        .into_iter()
        .filter(|c| {
            c.state.contains("listen")
                || (c.proto == "udp" && (c.remote == "-" || c.remote.ends_with(":*")))
        })
        .collect()
}

/// Parse `arp -a` / `ip neigh` output into (ip, mac, iface/state).
pub fn parse_arp(output: &str, windows_style: bool) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for line in output.lines() {
        if windows_style {
            // "  192.168.1.1        aa-bb-cc-dd-ee-ff     dynamic"
            let cols: Vec<&str> = line.split_whitespace().collect();
            if cols.len() >= 3
                && cols[0].parse::<std::net::Ipv4Addr>().is_ok()
                && cols[1].contains('-')
            {
                out.push((
                    cols[0].into(),
                    cols[1].replace('-', ":").to_lowercase(),
                    cols[2].into(),
                ));
            }
        } else {
            // ip neigh: "192.168.1.1 dev eth0 lladdr aa:bb:cc REACHABLE"
            let mut ip = None;
            let mut mac = None;
            let mut state = String::new();
            let mut dev = String::new();
            let mut toks = line.split_whitespace();
            if let Some(first) = toks.next() {
                if first.parse::<std::net::Ipv4Addr>().is_ok() {
                    ip = Some(first.to_string());
                    while let Some(t) = toks.next() {
                        match t {
                            "dev" => dev = toks.next().unwrap_or("").into(),
                            "lladdr" => mac = toks.next().map(ToString::to_string),
                            other => {
                                if state.is_empty() && other.chars().all(|c| c.is_ascii_uppercase())
                                {
                                    state = other.into();
                                }
                            }
                        }
                    }
                }
            }
            if let (Some(ip), Some(mac)) = (ip, mac) {
                let third = if state.is_empty() { dev } else { state };
                out.push((ip, mac.to_lowercase(), third));
            }
        }
    }
    out
}

/// The ARP/neighbor table for this OS.
pub fn arp_table() -> Vec<(String, String, String)> {
    if cfg!(windows) {
        let out = exec::run("arp", &["-a"], TIMEOUT);
        parse_arp(&out.stdout, true)
    } else if exec::on_path("ip") {
        let out = exec::run("ip", &["neigh", "show"], TIMEOUT);
        parse_arp(&out.stdout, false)
    } else {
        let out = exec::run("arp", &["-a"], TIMEOUT);
        parse_arp(&out.stdout, false)
    }
}

/// System proxy configuration as the OS reports it.
pub fn proxy_configuration() -> String {
    let mut parts = Vec::new();
    for (k, v) in std::env::vars() {
        let ku = k.to_uppercase();
        if ku.ends_with("_PROXY") || ku == "NO_PROXY" {
            parts.push(format!("{k}={v}"));
        }
    }
    if cfg!(windows) {
        let out = exec::run("netsh", &["winhttp", "show", "proxy"], TIMEOUT);
        for line in out.stdout.lines().skip(2) {
            let t = line.trim();
            if !t.is_empty() && !t.starts_with("Direct Access") {
                parts.push(t.to_string());
            }
        }
    }
    if parts.is_empty() {
        "no proxy configured".into()
    } else {
        parts.join(" | ")
    }
}

/// Parse a hosts file into (ip, hostname) pairs, comments skipped.
pub fn parse_hosts(content: &str) -> Vec<(String, String)> {
    content
        .lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .filter(|l| !l.is_empty())
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let ip = it.next()?;
            let host = it.next()?;
            Some((ip.to_string(), host.to_string()))
        })
        .collect()
}

/// The OS hosts file path.
pub fn hosts_path() -> std::path::PathBuf {
    if cfg!(windows) {
        std::path::PathBuf::from(r"C:\Windows\System32\drivers\etc\hosts")
    } else {
        std::path::PathBuf::from("/etc/hosts")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NETSTAT_FIXTURE: &str = "\nActive Connections\n\n  Proto  Local Address          Foreign Address        State           PID\n  TCP    127.0.0.1:5354         0.0.0.0:0              LISTENING       712\n  TCP    192.168.1.50:52114     93.184.216.34:443      ESTABLISHED     4684\n  UDP    127.0.0.1:1900          *:*                                  4098\n";

    const SS_FIXTURE: &str = "Netid State  Recv-Q Send-Q Local Address:Port  Peer Address:Port  Process\nudp   UNCONN 0      0         127.0.0.53%lo:53       0.0.0.0:*          users:((\"systemd-resolve\",pid=889,fd=13))\ntcp   ESTAB  0      0         192.168.1.50:52114      93.184.216.34:443  users:((\"firefox\",pid=4684,fd=87))\ntcp   LISTEN 0      5         127.0.0.1:5354           0.0.0.0:*          users:((\"mDNSResponder\",pid=712,fd=11))\n";

    const LSOF_FIXTURE: &str = "p4684\ncfirefox\nPTCP\nn192.168.1.50:52114->93.184.216.34:443\nTST=ESTABLISHED\np712\ncmDNSResponder\nPUDP\nn127.0.0.1:5354\n";

    #[test]
    fn parses_netstat_ano_fixture() {
        let conns = parse_netstat_ano(NETSTAT_FIXTURE);
        assert_eq!(conns.len(), 3);
        let tcp = conns
            .iter()
            .find(|c| c.proto == "tcp" && c.pid == Some(4684))
            .unwrap();
        assert_eq!(tcp.state, "established");
        assert_eq!(tcp.remote, "93.184.216.34:443");
        assert!(
            conns
                .iter()
                .any(|c| c.proto == "udp" && c.state == "unconn")
        );
        assert!(conns.iter().any(|c| c.state == "listening"));
    }

    #[test]
    fn parses_ss_fixture_with_process_attribution() {
        let conns = parse_ss(SS_FIXTURE);
        assert_eq!(conns.len(), 3);
        let ff = conns
            .iter()
            .find(|c| c.process.as_deref() == Some("firefox"))
            .unwrap();
        assert_eq!(ff.pid, Some(4684));
        assert_eq!(ff.state, "estab");
        let udp = conns.iter().find(|c| c.proto == "udp").unwrap();
        assert_eq!(udp.state, "unconn");
        assert_eq!(udp.remote, "0.0.0.0:*");
    }

    #[test]
    fn parses_lsof_f_fixture() {
        let conns = parse_lsof_f(LSOF_FIXTURE);
        assert_eq!(conns.len(), 2);
        let ff = conns
            .iter()
            .find(|c| c.process.as_deref() == Some("firefox"))
            .unwrap();
        assert_eq!(ff.pid, Some(4684));
        assert_eq!(ff.remote, "93.184.216.34:443");
        assert_eq!(ff.state, "established");
    }

    #[test]
    fn groups_per_process_sorted_by_count() {
        let conns = parse_ss(SS_FIXTURE);
        let groups = group_connections(&conns);
        let ff = groups.iter().find(|g| g.process == "firefox").unwrap();
        assert_eq!(ff.connections, 1);
        assert_eq!(groups[0].connections, 1); // all singles; order stable
    }

    #[test]
    fn listening_filter_works() {
        let conns = parse_ss(SS_FIXTURE);
        let listening: Vec<_> = conns
            .into_iter()
            .filter(|c| {
                c.state.contains("listen")
                    || (c.proto == "udp" && (c.remote == "-" || c.remote.ends_with(":*")))
            })
            .collect();
        assert_eq!(listening.len(), 2);
    }

    #[test]
    fn parses_arp_both_styles() {
        let win = parse_arp(
            "  192.168.1.1          AA-BB-CC-DD-EE-FF     dynamic\n  224.0.0.22            01-00-5e-00-00-16     static\n",
            true,
        );
        assert_eq!(win.len(), 2);
        assert_eq!(win[0].1, "aa:bb:cc:dd:ee:ff");
        assert_eq!(win[0].2, "dynamic");
        let linux = parse_arp(
            "192.168.1.1 dev eth0 lladdr aa:bb:cc:dd:ee:ff REACHABLE\n",
            false,
        );
        assert_eq!(linux.len(), 1);
        assert_eq!(linux[0].2, "REACHABLE");
    }

    #[test]
    fn parses_hosts_entries_and_comments() {
        let hosts =
            "# comment\n127.0.0.1 localhost\n::1 localhost\n10.0.0.5 buildserver # inline\n";
        let parsed = parse_hosts(hosts);
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[2], ("10.0.0.5".into(), "buildserver".into()));
    }

    #[test]
    fn live_connections_or_honest_detail() {
        let r = connections();
        if r.connections.is_empty() {
            assert_ne!(r.detail, "");
        } else {
            assert!(!r.groups.is_empty());
        }
    }
}

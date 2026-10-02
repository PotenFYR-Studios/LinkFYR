//! TCP stack audit: reads real OS settings, compares them against
//! latency-oriented recommendations, and lists the exact command that
//! fixes each suboptimal one (elevation required where the OS says so).
//! Nothing is applied silently; the fix strings are shown to the user.

use std::time::Duration;

use linkfyr_model::optimize::{CheckStatus, TcpAuditReport, TcpCheck};

use crate::exec;

const TIMEOUT: Duration = Duration::from_secs(10);

/// Linux kernel TCP state captured from /proc and `tc`.
#[derive(Debug, Clone, Default)]
pub struct LinuxTcpState {
    pub congestion_control: Option<String>,
    pub available_cc: Option<String>,
    pub slow_start_after_idle: Option<String>,
    pub mtu_probing: Option<String>,
    pub ecn: Option<String>,
    pub root_qdisc: Option<String>,
}

pub fn read_linux_state() -> LinuxTcpState {
    let read = |p: &str| {
        std::fs::read_to_string(p)
            .ok()
            .map(|s| s.trim().to_string())
    };
    let mut st = LinuxTcpState {
        congestion_control: read("/proc/sys/net/ipv4/tcp_congestion_control"),
        available_cc: read("/proc/sys/net/ipv4/tcp_available_congestion_control"),
        slow_start_after_idle: read("/proc/sys/net/ipv4/tcp_slow_start_after_idle"),
        mtu_probing: read("/proc/sys/net/ipv4/tcp_mtu_probing"),
        ecn: read("/proc/sys/net/ipv4/tcp_ecn"),
        root_qdisc: None,
    };
    if exec::on_path("tc") {
        let out = exec::run("tc", &["qdisc", "show"], TIMEOUT);
        // First non-empty line is the root qdisc of the default device.
        st.root_qdisc = out
            .stdout
            .lines()
            .next()
            .map(|l| l.split_whitespace().nth(1).unwrap_or("").to_string())
            .filter(|s| !s.is_empty());
    }
    st
}

fn check(key: &str, current: &str, recommended: &str, fix: Option<&str>) -> TcpCheck {
    let ok = current.eq_ignore_ascii_case(recommended);
    TcpCheck {
        key: key.into(),
        current: current.into(),
        recommended: recommended.into(),
        status: if ok {
            CheckStatus::Ok
        } else {
            CheckStatus::Suboptimal
        },
        fix: if ok {
            None
        } else {
            fix.map(ToString::to_string)
        },
    }
}

/// Linux analysis (pure).
pub fn analyze_linux(st: &LinuxTcpState) -> Vec<TcpCheck> {
    let mut checks = Vec::new();

    if let Some(cc) = &st.congestion_control {
        let bbr_ok = st
            .available_cc
            .as_deref()
            .is_some_and(|a| a.contains("bbr"));
        if bbr_ok {
            checks.push(check(
                "tcp_congestion_control",
                cc,
                "bbr",
                Some("sysctl -w net.ipv4.tcp_congestion_control=bbr"),
            ));
        } else if cc != "cubic" {
            checks.push(check(
                "tcp_congestion_control",
                cc,
                "cubic",
                Some("sysctl -w net.ipv4.tcp_congestion_control=cubic"),
            ));
        } else {
            checks.push(check("tcp_congestion_control", cc, "cubic", None));
        }
    }

    if let Some(v) = &st.slow_start_after_idle {
        checks.push(check(
            "tcp_slow_start_after_idle",
            v,
            "0",
            Some("sysctl -w net.ipv4.tcp_slow_start_after_idle=0"),
        ));
    }
    if let Some(v) = &st.mtu_probing {
        checks.push(check(
            "tcp_mtu_probing",
            v,
            "1",
            Some("sysctl -w net.ipv4.tcp_mtu_probing=1"),
        ));
    }
    if let Some(v) = &st.ecn {
        checks.push(check(
            "tcp_ecn",
            v,
            "1",
            Some("sysctl -w net.ipv4.tcp_ecn=1"),
        ));
    }
    if let Some(q) = &st.root_qdisc {
        let good = q == "fq" || q == "fq_codel" || q == "cake";
        checks.push(TcpCheck {
            key: "root_qdisc".into(),
            current: q.clone(),
            recommended: "fq_codel".into(),
            status: if good {
                CheckStatus::Ok
            } else {
                CheckStatus::Suboptimal
            },
            fix: if good {
                None
            } else {
                Some("sysctl -w net.core.default_qdisc=fq_codel".into())
            },
        });
    }
    checks
}

/// Parse `netsh int tcp show global` output into key/value pairs
/// (pure; tested with real Windows fixtures).
pub fn parse_netsh_global(output: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    for line in output.lines() {
        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim();
            let val = v.trim();
            if !key.is_empty()
                && !val.is_empty()
                && !key.eq_ignore_ascii_case("TCP Global Parameters")
                && !key.contains("Querying")
            {
                pairs.push((key.to_lowercase(), val.to_string()));
            }
        }
    }
    pairs
}

/// Windows analysis (pure).
pub fn analyze_windows(pairs: &[(String, String)]) -> Vec<TcpCheck> {
    let find = |needle: &str| {
        pairs
            .iter()
            .find(|(k, _)| k.contains(needle))
            .map(|(_, v)| v.clone())
    };

    let mut checks = Vec::new();
    if let Some(v) = find("auto-tuning") {
        // "normal" is the healthy default; "restricted" and
        // "disabled" throttle single-flow downloads badly.
        let ok = v.eq_ignore_ascii_case("normal");
        checks.push(TcpCheck {
            key: "receive_window_autotuning".into(),
            current: v.clone(),
            recommended: "normal".into(),
            status: if ok {
                CheckStatus::Ok
            } else {
                CheckStatus::Suboptimal
            },
            fix: if ok {
                None
            } else {
                Some("netsh int tcp set global autotuninglevel=normal".into())
            },
        });
    }
    if let Some(v) = find("ecn") {
        let ok = v.eq_ignore_ascii_case("enabled");
        checks.push(TcpCheck {
            key: "ecn_capability".into(),
            current: v.clone(),
            recommended: "enabled".into(),
            status: if ok {
                CheckStatus::Ok
            } else {
                CheckStatus::Suboptimal
            },
            fix: if ok {
                None
            } else {
                Some("netsh int tcp set global ecncapability=enabled".into())
            },
        });
    }
    if let Some(v) = find("timestamps") {
        // Default disabled; enabling helps some satellite/wireless paths
        // but adds header overhead. Reported, not nagged.
        checks.push(check("rfc1323_timestamps", &v, &v, None));
    }
    checks
}

/// macOS analysis over `sysctl -n` values (pure).
pub fn analyze_macos(vals: &[(String, String)]) -> Vec<TcpCheck> {
    let find = |needle: &str| {
        vals.iter()
            .find(|(k, _)| k == needle)
            .map(|(_, v)| v.clone())
    };
    let mut checks = Vec::new();
    for (key, rec, fix) in [
        (
            "net.inet.tcp.ecn_initiate_out",
            "1",
            "sudo sysctl -w net.inet.tcp.ecn_initiate_out=1",
        ),
        (
            "net.inet.tcp.ecn_negotiate_in",
            "1",
            "sudo sysctl -w net.inet.tcp.ecn_negotiate_in=1",
        ),
        (
            "net.inet.tcp.sack",
            "1",
            "sudo sysctl -w net.inet.tcp.sack=1",
        ),
    ] {
        if let Some(v) = find(key) {
            checks.push(check(key, &v, rec, Some(fix)));
        }
    }
    checks
}

/// Run the audit for this OS.
pub fn audit() -> TcpAuditReport {
    let platform = std::env::consts::OS.to_string();
    let elevated = exec::is_elevated();

    let (checks, summary) = if cfg!(windows) {
        let out = exec::run("netsh", &["int", "tcp", "show", "global"], TIMEOUT);
        let checks = analyze_windows(&parse_netsh_global(&out.stdout));
        let summary = summarize(&checks);
        (checks, summary)
    } else if cfg!(target_os = "macos") {
        let mut vals = Vec::new();
        for key in [
            "net.inet.tcp.ecn_initiate_out",
            "net.inet.tcp.ecn_negotiate_in",
            "net.inet.tcp.sack",
        ] {
            let out = exec::run("sysctl", &["-n", key], TIMEOUT);
            if out.success {
                vals.push((key.to_string(), out.stdout.trim().to_string()));
            }
        }
        let checks = analyze_macos(&vals);
        let summary = summarize(&checks);
        (checks, summary)
    } else {
        let checks = analyze_linux(&read_linux_state());
        let summary = summarize(&checks);
        (checks, summary)
    };

    TcpAuditReport {
        platform,
        elevated,
        checks,
        summary,
    }
}

fn summarize(checks: &[TcpCheck]) -> String {
    let suboptimal = checks
        .iter()
        .filter(|c| c.status == CheckStatus::Suboptimal)
        .count();
    if checks.is_empty() {
        "No TCP settings could be read on this platform".into()
    } else if suboptimal == 0 {
        format!("TCP stack looks healthy ({} checks)", checks.len())
    } else {
        format!(
            "{suboptimal} of {} settings below par; fixes need elevation and explicit confirmation",
            checks.len()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NETSH_FIXTURE: &str = "Querying active state...\n\nTCP Global Parameters\n----------------------------------------------\nReceive Segment Coalescing State : enabled\nReceive Window Auto-Tuning Level : restricted\nReceive Side Scaling             : enabled\nAdd-On Congestion Control Provider : none\nECN Capability                   : disabled\nRFC 1323 Timestamps              : disabled\nInitial RTO                      : 3000ms\n";

    #[test]
    fn parses_netsh_global_fixture() {
        let pairs = parse_netsh_global(NETSH_FIXTURE);
        assert!(pairs.contains(&(
            "receive window auto-tuning level".into(),
            "restricted".into()
        )));
        assert!(pairs.contains(&("ecn capability".into(), "disabled".into())));
        assert!(!pairs.iter().any(|(k, _)| k.contains("querying")));
    }

    #[test]
    fn windows_analysis_flags_restricted_autotuning() {
        let checks = analyze_windows(&parse_netsh_global(NETSH_FIXTURE));
        let auto = checks
            .iter()
            .find(|c| c.key == "receive_window_autotuning")
            .expect("autotuning check");
        assert_eq!(auto.status, CheckStatus::Suboptimal);
        assert!(
            auto.fix
                .as_deref()
                .unwrap()
                .contains("autotuninglevel=normal")
        );
        let ecn = checks
            .iter()
            .find(|c| c.key == "ecn_capability")
            .expect("ecn");
        assert_eq!(ecn.status, CheckStatus::Suboptimal);
    }

    #[test]
    fn windows_analysis_accepts_healthy_stack() {
        let healthy = "Receive Window Auto-Tuning Level : normal\nECN Capability : enabled\n";
        let checks = analyze_windows(&parse_netsh_global(healthy));
        assert!(checks.iter().all(|c| c.status != CheckStatus::Suboptimal));
    }

    #[test]
    fn linux_analysis_recommends_bbr_when_available() {
        let st = LinuxTcpState {
            congestion_control: Some("cubic".into()),
            available_cc: Some("reno cubic bbr".into()),
            slow_start_after_idle: Some("1".into()),
            mtu_probing: Some("0".into()),
            ecn: Some("2".into()),
            root_qdisc: Some("pfifo_fast".into()),
        };
        let checks = analyze_linux(&st);
        let cc = checks
            .iter()
            .find(|c| c.key == "tcp_congestion_control")
            .unwrap();
        assert_eq!(cc.recommended, "bbr");
        assert_eq!(cc.status, CheckStatus::Suboptimal);
        let idle = checks
            .iter()
            .find(|c| c.key == "tcp_slow_start_after_idle")
            .unwrap();
        assert_eq!(idle.status, CheckStatus::Suboptimal);
        let q = checks.iter().find(|c| c.key == "root_qdisc").unwrap();
        assert!(q.fix.as_deref().unwrap().contains("fq_codel"));
    }

    #[test]
    fn linux_analysis_ok_when_all_optimal() {
        let st = LinuxTcpState {
            congestion_control: Some("bbr".into()),
            available_cc: Some("reno cubic bbr".into()),
            slow_start_after_idle: Some("0".into()),
            mtu_probing: Some("1".into()),
            ecn: Some("1".into()),
            root_qdisc: Some("fq".into()),
        };
        assert!(
            analyze_linux(&st)
                .iter()
                .all(|c| c.status == CheckStatus::Ok)
        );
    }

    #[test]
    fn macos_analysis_reads_sysctl_values() {
        let vals = vec![
            ("net.inet.tcp.ecn_initiate_out".to_string(), "0".into()),
            ("net.inet.tcp.sack".to_string(), "1".into()),
        ];
        let checks = analyze_macos(&vals);
        assert_eq!(checks.len(), 2);
        let ecn = checks
            .iter()
            .find(|c| c.key == "net.inet.tcp.ecn_initiate_out")
            .unwrap();
        assert_eq!(ecn.status, CheckStatus::Suboptimal);
        let sack = checks
            .iter()
            .find(|c| c.key == "net.inet.tcp.sack")
            .unwrap();
        assert_eq!(sack.status, CheckStatus::Ok);
    }

    #[test]
    fn summary_counts_suboptimal() {
        let checks = vec![
            check("a", "1", "1", None),
            check("b", "0", "1", Some("fix b")),
        ];
        assert_eq!(
            summarize(&checks),
            "1 of 2 settings below par; fixes need elevation and explicit confirmation"
        );
        assert_eq!(
            summarize(&[]),
            "No TCP settings could be read on this platform"
        );
    }
}

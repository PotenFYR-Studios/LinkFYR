//! One-shot repair actions that genuinely fix things users complain
//! about (stale DNS after switching VPNs or routers is the classic).
//! Every action runs the OS's real command and reports the honest
//! outcome; nothing silently swallows failures.

use std::time::Duration;

use linkfyr_model::optimize::{RepairAction, RepairReport};

use crate::exec;

const TIMEOUT: Duration = Duration::from_secs(20);

/// Flush the OS DNS cache with the platform's supported command.
pub fn flush_dns() -> RepairReport {
    if cfg!(windows) {
        let out = exec::run("ipconfig", &["/flushdns"], TIMEOUT);
        outcome("ipconfig /flushdns", out.success, &out.combined)
    } else if cfg!(target_os = "macos") {
        let a = exec::run("dscacheutil", &["-flushcache"], TIMEOUT);
        let b = exec::run("killall", &["-HUP", "mDNSResponder"], TIMEOUT);
        let mut combined = a.combined;
        combined.push_str(&b.combined);
        outcome(
            "dscacheutil -flushcache + killall -HUP mDNSResponder",
            a.success && b.success,
            &combined,
        )
    } else if exec::on_path("resolvectl") {
        let out = exec::run("resolvectl", &["flush-caches"], TIMEOUT);
        outcome("resolvectl flush-caches", out.success, &out.combined)
    } else if exec::on_path("systemd-resolve") {
        let out = exec::run("systemd-resolve", &["--flush-caches"], TIMEOUT);
        outcome("systemd-resolve --flush-caches", out.success, &out.combined)
    } else {
        RepairReport {
            action: RepairAction::FlushDns,
            outcome: "unavailable".into(),
            detail: "no supported DNS cache tool found (resolvectl/systemd-resolve)".into(),
        }
    }
}

fn outcome(what: &str, success: bool, combined: &str) -> RepairReport {
    let not_run = combined.trim().is_empty() && !success;
    RepairReport {
        action: RepairAction::FlushDns,
        outcome: if not_run {
            "unavailable".into()
        } else if success {
            "applied".into()
        } else {
            "failed".into()
        },
        detail: if not_run {
            format!("{what}: command not available")
        } else if success {
            format!("{what} succeeded")
        } else {
            format!(
                "{what}: {}",
                combined.trim().chars().take(300).collect::<String>()
            )
        },
    }
}

/// Dispatch a repair action.
pub fn run(action: RepairAction) -> RepairReport {
    match action {
        RepairAction::FlushDns => flush_dns(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flush_dns_reports_an_honest_outcome() {
        let r = flush_dns();
        assert!(
            ["applied", "unavailable", "failed"].contains(&r.outcome.as_str()),
            "outcome: {}",
            r.outcome
        );
        assert_ne!(r.detail, "");
    }

    #[test]
    fn repair_report_round_trips_through_ipc_shape() {
        let r = run(RepairAction::FlushDns);
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"action\":\"flush_dns\""));
        let back: RepairReport = serde_json::from_str(&json).unwrap();
        assert_eq!(back.action, RepairAction::FlushDns);
    }
}

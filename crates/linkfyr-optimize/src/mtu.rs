//! Path MTU discovery using the OS `ping` binary with the DF
//! (don't-fragment) bit: unprivileged everywhere, real on the wire.
//! Binary-searches the largest unfragmented payload; a result below
//! 1500 exposes VPN/PPPoE/PPP overhead that silently breaks or slows
//! large transfers (the classic "some sites hang" black hole).

use std::time::Duration;

use linkfyr_model::optimize::MtuReport;

use crate::exec::{self, CmdOutput};

const PING_TIMEOUT: Duration = Duration::from_secs(3);

/// Build the per-OS ping argument vector for a DF probe (pure; tested
/// with fixtures). `payload` is the ICMP data size; on-wire total is
/// payload + 28 (IP + ICMP header).
pub fn ping_args(payload: u16, target: &str) -> Vec<String> {
    let payload = payload.to_string();
    let timeout_ms = "1000";
    if cfg!(windows) {
        [
            "ping", "-n", "1", "-l", &payload, "-f", "-w", timeout_ms, target,
        ]
        .iter()
        .map(ToString::to_string)
        .collect()
    } else if cfg!(target_os = "macos") {
        [
            "ping", "-c", "1", "-D", "-s", &payload, "-W", timeout_ms, target,
        ]
        .iter()
        .map(ToString::to_string)
        .collect()
    } else {
        [
            "ping", "-c", "1", "-M", "do", "-s", &payload, "-W", "1", target,
        ]
        .iter()
        .map(ToString::to_string)
        .collect()
    }
}

/// Interpret a ping DF probe result (pure). Success requires rc 0 AND
/// an echo line in the output (not a frag-needed / too-long error).
pub fn probe_ok(output: &CmdOutput) -> bool {
    if !output.success || output.timed_out {
        return false;
    }
    let text = output.combined.to_lowercase();
    text.contains("ttl=") || text.contains("time=")
}

fn run_probe(payload: u16, target: &str) -> CmdOutput {
    let args = ping_args(payload, target);
    // args[0] is the program name (kept in the vector for fixture tests).
    let refs: Vec<&str> = args[1..].iter().map(String::as_str).collect();
    exec::run("ping", &refs, PING_TIMEOUT)
}

/// Discover the path MTU to `target` capped at `max_mtu`.
pub fn probe(target: &str, max_mtu: u16) -> MtuReport {
    if !exec::on_path("ping") {
        return MtuReport {
            target: target.into(),
            path_mtu: 0,
            probes: 0,
            verdict: "ping is not available on this system".into(),
        };
    }

    let lo = 576u16; // IPv4 minimum
    let hi = max_mtu.clamp(lo, 1500);
    let mut probes = 0u16;
    let mut best = 0u16;
    let (mut low, mut high) = (lo, hi);
    while low <= high {
        let mid = low + (high - low) / 2;
        let payload = mid.saturating_sub(28);
        if payload == 0 {
            break;
        }
        probes += 1;
        if probe_ok(&run_probe(payload, target)) {
            best = best.max(mid);
            low = mid + 1;
        } else {
            high = mid - 1;
        }
    }

    if best == 0 {
        return MtuReport {
            target: target.into(),
            path_mtu: 0,
            probes,
            verdict: "target did not answer any DF probe (offline, blocked, or ICMP filtered)"
                .into(),
        };
    }

    MtuReport {
        target: target.into(),
        path_mtu: best,
        probes,
        verdict: mtu_verdict(best),
    }
}

/// Human verdict for a discovered MTU (pure).
pub fn mtu_verdict(mtu: u16) -> String {
    match mtu {
        1500 => "Standard 1500-byte path: no fragmentation black hole detected".into(),
        1492 => "PPPoE path (1492): normal for DSL; keep router MTU at 1492".into(),
        1480 | 1476 => "Tunnel overhead detected: VPN or mobile tethering; enable MTU/MSS clamping on the tunnel".into(),
        m if m >= 1400 => format!("Below-standard MTU ({m}): tunnel overhead; clamp MSS to {}", m.saturating_sub(40)),
        m => format!("Severely restricted MTU ({m}): check VPN, PPP, or carrier network; clamp MSS to {}", m.saturating_sub(40)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out(success: bool, combined: &str) -> CmdOutput {
        CmdOutput {
            success,
            code: if success { Some(0) } else { Some(1) },
            stdout: combined.into(),
            stderr: String::new(),
            combined: combined.into(),
            timed_out: false,
        }
    }

    #[test]
    fn ping_args_match_platform() {
        let args = ping_args(1472, "example.com");
        let joined = args.join(" ");
        if cfg!(windows) {
            assert!(joined.contains("-n 1 -l 1472 -f"));
        } else if cfg!(target_os = "macos") {
            assert!(joined.contains("-D -s 1472"));
        } else {
            assert!(joined.contains("-M do -s 1472"));
        }
        assert!(joined.ends_with("example.com"));
    }

    #[test]
    fn probe_ok_uses_rc_and_echo_line() {
        assert!(probe_ok(&out(
            true,
            "Reply from 93.184.216.34: bytes=1472 time=12ms TTL=56"
        )));
        assert!(probe_ok(&out(
            true,
            "64 bytes from 93.184.216.34: icmp_seq=1 ttl=56 time=12.1 ms"
        )));
        // Windows frag-needed and Linux "message too long" are failures.
        assert!(!probe_ok(&out(
            false,
            "Packet needs to be fragmented but DF set."
        )));
        assert!(!probe_ok(&out(
            false,
            "ping: local error: Message too long, mtu=1400"
        )));
        assert!(!probe_ok(&out(true, "Request timed out.")));
        let mut t = out(true, "time=1ms");
        t.timed_out = true;
        assert!(!probe_ok(&t));
    }

    #[test]
    fn verdicts_cover_common_paths() {
        assert!(mtu_verdict(1500).contains("Standard"));
        assert!(mtu_verdict(1492).contains("PPPoE"));
        assert!(mtu_verdict(1476).contains("Tunnel"));
        assert!(mtu_verdict(1300).contains("Severely"));
        assert!(mtu_verdict(1420).contains("1420"));
    }

    #[test]
    fn binary_search_finds_loopback_mtu_with_real_pings() {
        // Loopback accepts DF at any size; expect the 1500 cap.
        // Requires the `ping` binary (present in the Docker test image
        // and on every supported desktop OS).
        let report = probe("127.0.0.1", 1500);
        if report.path_mtu == 0 && report.verdict.contains("not available") {
            return; // host without ping: honestly skipped
        }
        assert_eq!(report.path_mtu, 1500, "report: {report:?}");
        assert!(report.probes >= 1 && report.probes <= 11);
        assert!(report.verdict.contains("Standard"));
    }
}

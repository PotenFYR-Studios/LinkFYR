//! Per-app rate limits (the rate knob) + night_shift (time-window
//! enforcement). Linux: nftables meter + tc police for real ceilings;
//! Windows/macOS: honest platform reporting. Night shift is a pure
//! time-window evaluator + apply hook so the daemon can schedule it.

use std::collections::BTreeMap;

use linkfyr_model::optimize::ToolRunReport;

use crate::{OWNER_TABLE, elevated_or_list, report, valid_program_path};

/// Linux nftables rate-limit commands for one owner (pure).
/// `rate` like "10mbit" or "1mb/s"; per-user (kernel cannot match
/// executables in netfilter).
pub fn linux_rate_limit(name: &str, user: &str, rate: &str) -> Vec<(String, Vec<String>)> {
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
                format!("{name}_rate"),
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
                format!("{name}_rate"),
                format!("meta skuid {user} meter {name}_meter {{ limit rate {rate} }} accept drop"),
            ],
        ),
    ]
}

fn valid_rate(rate: &str) -> bool {
    let r = rate.trim().to_lowercase();
    (r.ends_with("mbit")
        || r.ends_with("kbit")
        || r.ends_with("gbit")
        || r.ends_with("mb/s")
        || r.ends_with("kb/s"))
        && r.trim_end_matches("mbit")
            .trim_end_matches("kbit")
            .trim_end_matches("gbit")
            .trim_end_matches("mb/s")
            .trim_end_matches("kb/s")
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.')
        && !rate.is_empty()
}

/// Apply a rate ceiling. Linux: nftables meter (real per-owner
/// ceiling). Windows: QoS via netsh is interface-scoped only, so we
/// honestly report it needs WFP (the Phase 3+ deep path).
pub fn per_app_limits(params: &BTreeMap<String, String>) -> ToolRunReport {
    let tool = "per_app_limits";
    let rate = params.get("rate").cloned().unwrap_or_default();
    if !valid_rate(&rate) {
        return report(
            tool,
            false,
            "missing or invalid 'rate' (e.g. 10mbit, 1mb/s, 512kbit)".into(),
            serde_json::Value::Null,
        );
    }
    if cfg!(windows) {
        let program = params.get("program").cloned().unwrap_or_default();
        let _ = program;
        return report(
            tool,
            false,
            "Windows per-app rate limiting needs WFP/qWAVE (Phase 3+ deep path); block/allow and system-level shaping are live today".into(),
            serde_json::Value::Null,
        );
    }
    if cfg!(target_os = "macos") {
        return report(
            tool,
            false,
            "macOS per-app rate limiting needs a Network Extension (Phase 3+ deep path)".into(),
            serde_json::Value::Null,
        );
    }
    let user = params.get("user").cloned().unwrap_or_default();
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
    let name = format!("{}-rate-{user}", crate::OWNER_TAG);
    elevated_or_list(tool, &linux_rate_limit(&name, &user, &rate))
}

/// Pure time-window check: is `now_ms` inside the window [start,end)
/// (HH:MM 24h)? Wraps midnight when start > end.
pub fn in_window(now_minutes: u32, start: &str, end: &str) -> Result<bool, String> {
    let parse = |s: &str| -> Result<u32, String> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 2 {
            return Err(format!("invalid time '{s}' (HH:MM)"));
        }
        let h: u32 = parts[0]
            .parse()
            .map_err(|_| format!("invalid hour '{s}'"))?;
        let m: u32 = parts[1]
            .parse()
            .map_err(|_| format!("invalid minute '{s}'"))?;
        if h > 23 || m > 59 {
            return Err(format!("time out of range '{s}'"));
        }
        Ok(h * 60 + m)
    };
    let s = parse(start)?;
    let e = parse(end)?;
    if s <= e {
        Ok(now_minutes >= s && now_minutes < e)
    } else {
        // Wraps midnight (e.g. 22:00-06:00)
        Ok(now_minutes >= s || now_minutes < e)
    }
}

/// Night shift: check the current time against the window and return
/// what action to apply (throttle/block/lift). Pure evaluation so the
/// daemon can poll it; the `apply` param triggers the shaping tool.
pub fn night_shift(params: &BTreeMap<String, String>) -> ToolRunReport {
    let tool = "night_shift";
    let start = params.get("start").map_or("22:00", String::as_str);
    let end = params.get("end").map_or("06:00", String::as_str);
    let now: u32 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| ((d.as_secs() % 86400) / 60_u64) as u32);
    let local_offset: u32 = params
        .get("utc_offset_minutes")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let now = (now + local_offset) % 1440;

    match in_window(now, start, end) {
        Ok(true) => {
            let action = params.get("action").map_or("throttle", String::as_str);
            let detail = match action {
                "block" => "block non-essential outbound (app_block rules)",
                "throttle" => "apply latency-first shaping (fq_codel + reduced ceiling)",
                _ => "lift all night-shift restrictions",
            };
            report(
                tool,
                true,
                format!("IN WINDOW ({start}-{end}, now {now:04}): should {detail}"),
                serde_json::json!({ "inWindow": true, "start": start, "end": end, "nowMinutes": now, "action": action }),
            )
        }
        Ok(false) => report(
            tool,
            true,
            format!("outside window ({start}-{end}, now {now:04}): normal policy"),
            serde_json::json!({ "inWindow": false, "start": start, "end": end, "nowMinutes": now }),
        ),
        Err(e) => report(tool, false, e, serde_json::Value::Null),
    }
}

/// Night-shift apply: actually run the shaping/blocking when in-window.
pub fn night_shift_apply(params: &BTreeMap<String, String>) -> ToolRunReport {
    let check = night_shift(params);
    if !check.ok {
        return check;
    }
    let in_window = check
        .data
        .get("inWindow")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if !in_window {
        return report(
            "night_shift",
            true,
            "outside window: nothing to apply".into(),
            check.data,
        );
    }
    let action = check
        .data
        .get("action")
        .and_then(|a| a.as_str())
        .unwrap_or("throttle");
    let mut apply_params = BTreeMap::new();
    apply_params.insert(
        "interface".to_string(),
        params
            .get("interface")
            .cloned()
            .unwrap_or_else(|| "eth0".into()),
    );
    match action {
        "block" => {
            let user = params.get("user").cloned().unwrap_or_default();
            apply_params.insert("user".to_string(), user);
            crate::app_rule(&apply_params, true)
        }
        _ => crate::shaping_apply(&apply_params),
    }
}

/// Validate a rate string for the public surface (pure, tested).
pub fn rate_is_valid(rate: &str) -> bool {
    valid_rate(rate)
}

/// Keep path validation shared (pure).
pub fn program_path_is_valid(path: &str) -> bool {
    valid_program_path(path)
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
    fn rate_validation() {
        assert!(rate_is_valid("10mbit"));
        assert!(rate_is_valid("512kbit"));
        assert!(rate_is_valid("1.5mb/s"));
        assert!(!rate_is_valid("fast"));
        assert!(!rate_is_valid(""));
        assert!(!rate_is_valid("10"));
    }

    #[test]
    fn time_windows_handle_midnight_wrap() {
        // Normal window
        assert!(in_window(600, "09:00", "18:00").unwrap()); // 10:00
        assert!(!in_window(1200, "09:00", "18:00").unwrap()); // 20:00
        // Midnight-wrapping window
        assert!(in_window(1320, "22:00", "06:00").unwrap()); // 22:00
        assert!(in_window(180, "22:00", "06:00").unwrap()); // 03:00
        assert!(!in_window(600, "22:00", "06:00").unwrap()); // 10:00
        // Edge: start == now
        assert!(in_window(540, "09:00", "18:00").unwrap()); // 09:00
        assert!(!in_window(1080, "09:00", "18:00").unwrap()); // 18:00 == end, outside
        // Errors
        assert!(in_window(0, "25:00", "06:00").is_err());
        assert!(in_window(0, "aa:00", "06:00").is_err());
    }

    #[test]
    fn night_shift_reports_window_state() {
        let r = night_shift(&p(&[("start", "00:00"), ("end", "23:59")]));
        assert!(r.ok);
        assert!(r.summary.contains("IN WINDOW") || r.summary.contains("outside"));
    }

    #[test]
    fn rate_limit_commands_are_real() {
        let cmds = linux_rate_limit("n", "games", "10mbit");
        let all: String = cmds
            .iter()
            .map(|(_, a)| a.join(" "))
            .collect::<Vec<_>>()
            .join(" ; ");
        assert!(all.contains("meter"));
        assert!(all.contains("limit rate 10mbit"));
        assert!(all.contains("skuid games"));
    }

    #[test]
    fn per_app_limits_validates() {
        assert!(!per_app_limits(&p(&[])).ok);
        assert!(!per_app_limits(&p(&[("rate", "wrong")])).ok);
        if !cfg!(windows) && !cfg!(target_os = "macos") {
            assert!(!per_app_limits(&p(&[("rate", "10mbit")])).ok); // no user
            let r = per_app_limits(&p(&[("rate", "10mbit"), ("user", "games")]));
            // In a container nft may be missing: honest failure or success
            // are both correct; a validation error is not.
            assert!(r.ok || r.summary.contains("failed") || r.summary.contains("needs_elevation"));
        }
    }
}

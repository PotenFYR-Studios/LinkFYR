//! Network bridge manager.
//!
//! Windows removed the bridge UI; the capability is still reachable
//! through real, supported commands — this module drives them, tiered:
//!
//! 1. `L2Switch` on Windows = Hyper-V virtual switch with embedded
//!    teaming (`New-VMSwitch -EnableEmbeddedTeaming`) — closest to the
//!    removed feature, requires Hyper-V.
//! 2. `NatShare` on Windows = IP forwarding + `New-NetNat` — works on
//!    most editions, but is NAT, and every report says so.
//! 3. Linux = `ip link add … type bridge` (true L2).
//! 4. macOS = `ifconfig bridgeN create` + `addm` (true L2).
//!
//! Everything is elevation-honest: without admin/root the report lists
//! the exact commands instead of pretending success.

use std::time::Duration;

use linkfyr_model::optimize::{BridgeInfo, BridgeMode, BridgeReport, BridgeSpec};

use linkfyr_optimize::exec;

const TIMEOUT: Duration = Duration::from_secs(20);

/// Exact Windows commands for a spec (pure; fixture-tested shape).
pub fn windows_commands(spec: &BridgeSpec) -> Vec<String> {
    let members = spec.members.join(",");
    match spec.mode {
        BridgeMode::L2Switch => {
            let mut cmds = vec![format!(
                "New-VMSwitch -Name \"{}\" -NetAdapterName {members} -EnableEmbeddedTeaming -AllowManagementOS $true",
                spec.name
            )];
            for m in &spec.members {
                cmds.push(format!(
                    "Set-NetAdapterBinding -Name \"{m}\" -ComponentID vms_pp -Enabled $true"
                ));
            }
            cmds
        }
        BridgeMode::NatShare => {
            let prefix = spec
                .internal_prefix
                .clone()
                .unwrap_or_else(|| "192.168.137.0/24".into());
            let mut cmds = Vec::new();
            for m in &spec.members {
                cmds.push(format!(
                    "Set-NetIPInterface -InterfaceAlias \"{m}\" -Forwarding Enabled"
                ));
            }
            cmds.push(format!(
                "New-NetNat -Name \"{}\" -InternalIPInterfaceAddressPrefix {prefix}",
                spec.name
            ));
            cmds
        }
    }
}

/// Exact Linux commands for a spec (pure).
pub fn linux_commands(spec: &BridgeSpec) -> Vec<String> {
    let mut cmds = vec![format!("ip link add name {} type bridge", spec.name)];
    for m in &spec.members {
        cmds.push(format!("ip link set {m} master {}", spec.name));
    }
    cmds.push(format!("ip link set {} up", spec.name));
    cmds
}

/// Exact macOS commands for a spec (pure).
pub fn macos_commands(spec: &BridgeSpec) -> Vec<String> {
    let mut cmds = vec![format!("ifconfig {} create", spec.name)];
    for m in &spec.members {
        cmds.push(format!("ifconfig {} addm {m}", spec.name));
    }
    cmds.push(format!("ifconfig {} up", spec.name));
    cmds
}

fn commands_for(spec: &BridgeSpec) -> Vec<String> {
    if cfg!(windows) {
        windows_commands(spec)
    } else if cfg!(target_os = "macos") {
        macos_commands(spec)
    } else {
        linux_commands(spec)
    }
}

fn validate(spec: &BridgeSpec) -> Result<(), String> {
    if spec.name.is_empty()
        || !spec
            .name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("bridge name must be alphanumeric (letters, digits, - _)".into());
    }
    let min_members = if cfg!(windows) { 1 } else { 2 };
    if spec.members.len() < min_members {
        return Err(format!("at least {min_members} member interfaces required"));
    }
    if spec
        .members
        .iter()
        .any(|m| m.is_empty() || m.contains(' ') || m.contains(';'))
    {
        return Err("member names must be plain interface names".into());
    }
    Ok(())
}

/// Create a bridge. Runs the platform commands when elevated; otherwise
/// returns `needs_elevation` with the exact command list.
pub fn create(spec: &BridgeSpec) -> BridgeReport {
    let action = format!("create {} ({:?})", spec.name, spec.mode);
    if let Err(e) = validate(spec) {
        return BridgeReport {
            action,
            outcome: "failed".into(),
            detail: e,
            commands: vec![],
        };
    }
    let commands = commands_for(spec);
    if !exec::is_elevated() {
        return BridgeReport {
            action,
            outcome: "needs_elevation".into(),
            detail: "run as administrator/root; commands listed below".into(),
            commands,
        };
    }
    for cmd in &commands {
        let (program, args) = split_command(cmd);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let out = exec::run(&program, &refs, TIMEOUT);
        if !out.success {
            return BridgeReport {
                action,
                outcome: "failed".into(),
                detail: out.combined.trim().chars().take(300).collect(),
                commands,
            };
        }
    }
    BridgeReport {
        action,
        outcome: "applied".into(),
        detail: mode_note(spec.mode),
        commands,
    }
}

/// On Windows the command strings are PowerShell; on unix they map
/// 1:1 onto argv. This splitter keeps the elevated path real.
fn split_command(cmd: &str) -> (String, Vec<String>) {
    if cfg!(windows) {
        // PowerShell via -Command with the whole line.
        (
            "powershell".into(),
            vec!["-NoProfile".into(), "-Command".into(), cmd.into()],
        )
    } else {
        let mut parts = cmd.split_whitespace().map(ToString::to_string);
        let program = parts.next().unwrap_or_default();
        (program, parts.collect())
    }
}

fn mode_note(mode: BridgeMode) -> String {
    match mode {
        BridgeMode::L2Switch => "true layer-2 bridge created".into(),
        BridgeMode::NatShare => "NAT sharing enabled (NOT layer 2: segments stay separate subnets; this is the supported Windows fallback)".into(),
    }
}

/// Remove a bridge by name.
pub fn remove(name: &str) -> BridgeReport {
    let action = format!("remove {name}");
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return BridgeReport {
            action,
            outcome: "failed".into(),
            detail: "invalid bridge name".into(),
            commands: vec![],
        };
    }
    let commands = if cfg!(windows) {
        vec![
            format!(
                "Get-VMSwitch -Name \"{name}\" -ErrorAction SilentlyContinue | Remove-VMSwitch -Force"
            ),
            format!("Remove-NetNat -Name \"{name}\" -Confirm:$false -ErrorAction SilentlyContinue"),
        ]
    } else if cfg!(target_os = "macos") {
        (0..8)
            .map(|i| format!("ifconfig bridge{i} -m {name} 2>/dev/null"))
            .chain(std::iter::once(format!("ifconfig {name} destroy")))
            .collect()
    } else {
        vec![
            format!("ip link set {name} down"),
            format!("ip link delete {name} type bridge"),
        ]
    };
    if !exec::is_elevated() {
        return BridgeReport {
            action,
            outcome: "needs_elevation".into(),
            detail: "run as administrator/root; commands listed below".into(),
            commands,
        };
    }
    // Removal is best-effort: a missing bridge is success.
    let mut last_err = String::new();
    let mut any_fail = false;
    for cmd in &commands {
        let (program, args) = split_command(cmd);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let out = exec::run(&program, &refs, TIMEOUT);
        if !out.success
            && !out.combined.to_lowercase().contains("not found")
            && !out.combined.is_empty()
        {
            // PowerShell pipeline failures print errors; unix best-effort
            // commands are chained so only the final delete matters.
            if !cfg!(windows) && !cmd.contains("delete") {
                continue;
            }
            any_fail = true;
            last_err = out.combined.trim().chars().take(300).collect();
        }
    }
    BridgeReport {
        action,
        outcome: if any_fail {
            "failed".into()
        } else {
            "applied".into()
        },
        detail: if any_fail {
            last_err
        } else {
            "bridge removed (or already absent)".into()
        },
        commands,
    }
}

/// Parse Linux `ip link show type bridge` (pure).
pub fn parse_linux_bridges(output: &str) -> Vec<BridgeInfo> {
    let mut out = Vec::new();
    for line in output.lines() {
        // "8: br0: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 ..."
        let t = line.trim();
        let parts: Vec<&str> = t.splitn(3, ':').collect();
        if parts.len() < 3 {
            continue;
        }
        let name = parts[1].trim();
        if name.is_empty() {
            continue;
        }
        let state = if parts[2].contains("UP") {
            "up"
        } else {
            "down"
        };
        out.push(BridgeInfo {
            name: name.into(),
            mode: BridgeMode::L2Switch,
            members: vec![],
            state: state.into(),
        });
    }
    out
}

/// Parse Windows `Get-VMSwitch` CSV rows (pure).
pub fn parse_windows_switches(csv: &str) -> Vec<BridgeInfo> {
    let mut out = Vec::new();
    for line in csv.lines().skip(1) {
        let fields: Vec<&str> = line
            .split(',')
            .map(|f| f.trim().trim_matches('"'))
            .collect();
        if fields.len() >= 2 {
            out.push(BridgeInfo {
                name: fields[0].into(),
                mode: BridgeMode::L2Switch,
                members: vec![],
                state: fields[1].to_lowercase(),
            });
        }
    }
    out
}

/// Parse macOS `ifconfig` bridge blocks (pure): bridgeN with `member: X`.
pub fn parse_macos_bridges(output: &str) -> Vec<BridgeInfo> {
    let mut out: Vec<BridgeInfo> = Vec::new();
    for line in output.lines() {
        let t = line.trim();
        if let Some(name) = t.strip_prefix("bridge") {
            if let Some(name) = name.split(':').next() {
                if name.chars().all(|c| c.is_ascii_digit()) {
                    out.push(BridgeInfo {
                        name: format!("bridge{name}"),
                        mode: BridgeMode::L2Switch,
                        members: vec![],
                        state: "unknown".into(),
                    });
                }
            }
        } else if let Some((key, value)) = t.split_once(':') {
            if key.trim() == "member" {
                // "member: en0 flags=3<LEARNING,DISCOVER>" -> "en0"
                if let Some(member) = value.split_whitespace().next() {
                    if let Some(last) = out.last_mut() {
                        last.members.push(member.to_string());
                    }
                }
            } else if key.trim() == "status" {
                if let Some(last) = out.last_mut() {
                    last.state = value.trim().to_lowercase();
                }
            }
        }
    }
    out
}

/// List bridges on this machine.
pub fn list() -> Vec<BridgeInfo> {
    if cfg!(windows) {
        let out = exec::run(
            "powershell",
            &[
                "-NoProfile",
                "-Command",
                "Get-VMSwitch | Select-Object Name,SwitchType | ConvertTo-Csv -NoTypeInformation",
            ],
            TIMEOUT,
        );
        let mut bridges = parse_windows_switches(&out.stdout);
        let nat = exec::run(
            "powershell",
            &[
                "-NoProfile",
                "-Command",
                "Get-NetNat | Select-Object Name,InternalIPInterfaceAddressPrefix | ConvertTo-Csv -NoTypeInformation",
            ],
            TIMEOUT,
        );
        for line in nat.stdout.lines().skip(1) {
            let name = line.split(',').next().unwrap_or("").trim();
            if !name.is_empty() {
                bridges.push(BridgeInfo {
                    name: name.into(),
                    mode: BridgeMode::NatShare,
                    members: vec![],
                    state: "active".into(),
                });
            }
        }
        bridges
    } else if cfg!(target_os = "macos") {
        let out = exec::run("ifconfig", &["-a"], TIMEOUT);
        parse_macos_bridges(&out.stdout)
    } else {
        let out = exec::run("ip", &["link", "show", "type", "bridge"], TIMEOUT);
        parse_linux_bridges(&out.stdout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(mode: BridgeMode) -> BridgeSpec {
        BridgeSpec {
            name: "br0".into(),
            members: vec!["eth0".into(), "eth1".into()],
            mode,
            internal_prefix: Some("192.168.137.0/24".into()),
        }
    }

    #[test]
    fn windows_l2_uses_vmswitch_with_team() {
        let cmds = windows_commands(&spec(BridgeMode::L2Switch));
        assert!(cmds[0].contains("New-VMSwitch"));
        assert!(cmds[0].contains("-EnableEmbeddedTeaming"));
        assert!(cmds[0].contains("eth0,eth1"));
    }

    #[test]
    fn windows_nat_fallback_is_explicit() {
        let cmds = windows_commands(&spec(BridgeMode::NatShare));
        assert!(
            cmds.iter()
                .any(|c| c.contains("Set-NetIPInterface") && c.contains("Forwarding"))
        );
        assert!(
            cmds.iter()
                .any(|c| c.contains("New-NetNat") && c.contains("192.168.137.0/24"))
        );
    }

    #[test]
    fn unix_commands_are_plain_argv() {
        let l = linux_commands(&spec(BridgeMode::L2Switch));
        assert_eq!(l[0], "ip link add name br0 type bridge");
        assert_eq!(l[1], "ip link set eth0 master br0");
        let m = macos_commands(&spec(BridgeMode::L2Switch));
        assert_eq!(m[0], "ifconfig br0 create");
        assert_eq!(m[1], "ifconfig br0 addm eth0");
    }

    #[test]
    fn validation_rejects_command_injection_shapes() {
        let mut s = spec(BridgeMode::L2Switch);
        s.name = "br0; rm -rf /".into();
        let r = create(&s);
        assert_eq!(r.outcome, "failed");

        let mut s = spec(BridgeMode::L2Switch);
        s.members = vec!["eth0 $(reboot)".into()];
        let r = create(&s);
        assert_eq!(r.outcome, "failed");

        let mut s = spec(BridgeMode::L2Switch);
        s.members = vec!["eth0".into()];
        if cfg!(not(windows)) {
            let r = create(&s);
            assert_eq!(r.outcome, "failed");
            assert!(r.detail.contains("at least 2"));
        }
    }

    #[test]
    fn unelevated_create_lists_commands() {
        // CI containers run as root; only assert the branch when not.
        if exec::is_elevated() {
            return;
        }
        let r = create(&spec(BridgeMode::L2Switch));
        assert_eq!(r.outcome, "needs_elevation");
        assert!(!r.commands.is_empty(), "commands must be listed: {r:?}");
    }

    #[test]
    fn parses_linux_bridge_listing() {
        let out = "8: br0: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 qdisc noqueue state UP mode DEFAULT group default\n9: br-lan: <BROADCAST,MULTICAST> mtu 1500 qdisc noop state DOWN";
        let bridges = parse_linux_bridges(out);
        assert_eq!(bridges.len(), 2);
        assert_eq!(bridges[0].name, "br0");
        assert_eq!(bridges[0].state, "up");
        assert_eq!(bridges[1].state, "down");
    }

    #[test]
    fn parses_windows_switch_csv() {
        let csv =
            "\"Name\",\"SwitchType\"\n\"External\",\"External\"\n\"LinkFYR Bridge\",\"Internal\"";
        let b = parse_windows_switches(csv);
        assert_eq!(b.len(), 2);
        assert_eq!(b[1].name, "LinkFYR Bridge");
    }

    #[test]
    fn parses_macos_bridge_members() {
        let out = "lo0: flags=8049<UP,LOOPBACK,RUNNING,MULTICAST> mtu 16384\nbridge0: flags=8863<UP,BROADCAST,SMART,RUNNING,SIMPLEX,MULTICAST> mtu 1500\n\tmember: en0 flags=3<LEARNING,DISCOVER>\n\tmember: en1 flags=3<LEARNING,DISCOVER>\n\tstatus: active";
        let b = parse_macos_bridges(out);
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].members, vec!["en0".to_string(), "en1".to_string()]);
        assert_eq!(b[0].state, "active");
    }

    #[test]
    fn list_never_panics() {
        let _ = list();
    }
}

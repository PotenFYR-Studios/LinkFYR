//! Safe subprocess execution for OS network tools (ping, netsh, nmcli…).
//!
//! Commands are always run argument-vector style (no shell), with a hard
//! timeout, and never with elevated privileges unless the caller already
//! has them.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

/// Result of one command run.
#[derive(Debug, Clone)]
pub struct CmdOutput {
    pub success: bool,
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    /// stdout + stderr in one string, order not guaranteed.
    pub combined: String,
    pub timed_out: bool,
}

impl CmdOutput {
    /// A "command could not run at all" result (missing binary, etc.).
    pub fn not_run() -> Self {
        Self {
            success: false,
            code: None,
            stdout: String::new(),
            stderr: String::new(),
            combined: String::new(),
            timed_out: false,
        }
    }
}

#[cfg(windows)]
fn prepare(cmd: &mut Command) {
    // Keep console windows from flashing in the GUI.
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(unix)]
fn prepare(cmd: &mut Command) {
    cmd.stdin(Stdio::null());
}

/// Run `program` with `args`, killing it after `timeout`.
pub fn run(program: &str, args: &[&str], timeout: Duration) -> CmdOutput {
    let mut cmd = Command::new(program);
    cmd.args(args);
    prepare(&mut cmd);
    let Ok(mut child) = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() else {
        return CmdOutput::not_run();
    };

    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                fn read_pipe<R: std::io::Read>(pipe: Option<R>) -> String {
                    let mut buf = String::new();
                    if let Some(mut p) = pipe {
                        let _ = p.read_to_string(&mut buf);
                    }
                    buf
                }
                let out = read_pipe(child.stdout.take());
                let err = read_pipe(child.stderr.take());
                let success = status.success();
                let code = status.code();
                return CmdOutput {
                    combined: format!("{out}{err}"),
                    success,
                    code,
                    stdout: out,
                    stderr: err,
                    timed_out: false,
                };
            }
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return CmdOutput {
                        success: false,
                        code: None,
                        stdout: String::new(),
                        stderr: String::new(),
                        combined: String::new(),
                        timed_out: true,
                    };
                }
                std::thread::sleep(Duration::from_millis(15));
            }
            Err(_) => return CmdOutput::not_run(),
        }
    }
}

/// True when `program` exists on PATH (`which` semantics, no shell).
pub fn on_path(program: &str) -> bool {
    let exe = if cfg!(windows) && !program.to_ascii_lowercase().ends_with(".exe") {
        format!("{program}.exe")
    } else {
        program.to_string()
    };
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|dir| {
            let candidate: PathBuf = dir.join(&exe);
            candidate.is_file()
        })
    })
}

/// Whether the process currently runs elevated (admin on Windows, root
/// on unix). Determined with the OS's own check, not a guess.
pub fn is_elevated() -> bool {
    if cfg!(windows) {
        run("net", &["session"], Duration::from_secs(5)).success
    } else {
        run("id", &["-u"], Duration::from_secs(5)).stdout.trim() == "0"
    }
}

/// Extract all IPv4 addresses appearing in free-form tool output.
/// Locale-independent (numbers survive any codepage). Port suffixes
/// ("8.8.8.8:53") are stripped.
pub fn extract_ipv4(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for token in text.split_whitespace() {
        let candidate = token.split(':').next().unwrap_or(token);
        let cleaned = candidate.trim_matches(|c: char| !c.is_ascii_digit() && c != '.');
        if cleaned.parse::<std::net::Ipv4Addr>().is_ok() {
            found.push(cleaned.to_string());
        }
    }
    found
}

/// Extract all IPv6 addresses in free-form tool output.
pub fn extract_ipv6(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for token in text.split_whitespace() {
        let cleaned = token
            .trim_matches(|c: char| !(c.is_ascii_hexdigit() || c == ':'))
            .to_string();
        if cleaned.contains("::") || cleaned.matches(':').count() >= 3 {
            if let Ok(ip) = cleaned.parse::<std::net::Ipv6Addr>() {
                let s = ip.to_string();
                if !found.contains(&s) {
                    found.push(s);
                }
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_program_reports_not_run() {
        let out = run(
            "definitely-not-a-real-program-xyz",
            &["--v"],
            Duration::from_secs(2),
        );
        assert!(!out.success);
        assert!(!out.timed_out);
    }

    #[cfg(unix)]
    #[test]
    fn captures_stdout_and_respects_timeout() {
        let out = run("sh", &["-c", "echo hello"], Duration::from_secs(5));
        assert!(out.success);
        assert!(out.combined.contains("hello"));

        let killed = run("sleep", &["10"], Duration::from_millis(200));
        assert!(killed.timed_out);
        assert!(!killed.success);
    }

    #[cfg(windows)]
    #[test]
    fn captures_stdout_and_respects_timeout() {
        let out = run("cmd", &["/C", "echo hello"], Duration::from_secs(5));
        assert!(out.success);
        assert!(out.combined.contains("hello"));

        let killed = run(
            "ping",
            &["-n", "10", "127.0.0.1"],
            Duration::from_millis(300),
        );
        assert!(killed.timed_out);
    }

    #[test]
    fn extracts_ips_from_mixed_locale_noise() {
        let text = "nameserver 192.168.1.1 2a00::1a0b garbage 8.8.8.8:53 not.an.ip";
        let v4 = extract_ipv4(text);
        assert!(v4.contains(&"192.168.1.1".to_string()));
        assert!(v4.contains(&"8.8.8.8".to_string()));
        assert!(!v4.iter().any(|s| s.contains(':')));
        let v6 = extract_ipv6(text);
        assert!(v6.iter().any(|s| s.starts_with("2a00:")));
    }
}

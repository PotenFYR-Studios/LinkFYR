//! linkfyr — the LinkFYR command line interface.
//!
//! Phase 1 runs the engine in-process (same library the desktop app uses);
//! the privileged daemon (Phase 1.5) moves this behind authenticated IPC
//! without changing the command surface.

use clap::{Parser, Subcommand};
use linkfyr_core::{AppEngine, MonitorMode};
use linkfyr_model::Snapshot;
use linkfyr_optimize::registry::{
    catalog as registry_catalog, catalog_size as registry_catalog_size,
};

#[derive(Parser)]
#[command(
    name = "linkfyr",
    version,
    about = "LinkFYR: the operating system for your Internet connections",
    propagate_version = true
)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// Output machine-readable JSON instead of tables.
    #[arg(long, global = true)]
    json: bool,

    /// Use the deterministic simulator instead of real interfaces
    /// (useful for demos and testing).
    #[arg(long, global = true)]
    simulated: bool,
}

#[derive(Subcommand)]
enum Command {
    /// Engine status: version, interfaces, totals, Internet quality.
    Status,
    /// List discovered network interfaces.
    Interfaces,
    /// Sample live traffic for a few seconds and print rates.
    Traffic {
        /// Seconds to sample.
        #[arg(long, default_value_t = 5)]
        seconds: u64,
    },
    /// Real network optimization tools (DNS, routes, latency, MTU, Wi-Fi).
    Optimize {
        #[command(subcommand)]
        cmd: OptimizeCmd,
    },
    /// Network bridge management (tiered: Hyper-V SET, NetNat, L2).
    Bridge {
        #[command(subcommand)]
        cmd: BridgeCmd,
    },
    /// Recent engine alerts (interface up/down, health drops).
    Alerts,
    /// Live view: print new alerts and totals until Ctrl+C.
    Watch,
    /// Talk to a running linkfyrd service.
    Daemon {
        #[command(subcommand)]
        cmd: DaemonCmd,
    },
}

#[derive(Subcommand)]
enum DaemonCmd {
    /// Ping the service and print engine status.
    Status,
}

#[derive(Subcommand)]
enum BridgeCmd {
    /// List existing bridges.
    List,
    /// Create a bridge from interfaces.
    Create {
        /// Bridge name (letters, digits, - and _).
        name: String,
        /// Two or more member interface names.
        members: Vec<String>,
        /// Use the Windows NAT fallback instead of an L2 switch.
        #[arg(long)]
        nat: bool,
        /// NAT mode: internal subnet prefix (default 192.168.137.0/24).
        #[arg(long)]
        prefix: Option<String>,
    },
    /// Remove a bridge by name.
    Remove { name: String },
}

#[derive(Subcommand)]
enum OptimizeCmd {
    /// Show which optimization tools can run on this machine.
    Capabilities,
    /// Benchmark DNS resolvers with real queries and rank them.
    Dns,
    /// Apply a resolver list to the OS (may need elevation).
    ApplyDns {
        /// Resolver IPs, fastest first (see `optimize dns`).
        servers: Vec<String>,
        /// Target interface/service (default: auto-detect).
        #[arg(short, long)]
        interface: Option<String>,
    },
    /// Measure IPv4 vs IPv6 latency to a destination and detect penalties.
    RouteScan {
        /// Host name or IP to scan.
        target: String,
    },
    /// Grade latency under load (bufferbloat): the lag when the line
    /// saturates.
    Bloat {
        /// Seconds per phase.
        #[arg(long)]
        seconds: Option<u32>,
    },
    /// Real download/upload/latency measurement.
    Speedtest {
        /// Compatible endpoint base URL (default: Cloudflare).
        #[arg(long)]
        endpoint: Option<String>,
        /// Seconds per direction.
        #[arg(long)]
        seconds: Option<u32>,
    },
    /// Discover path MTU (finds VPN/PPPoE black holes).
    Mtu {
        /// Host to probe.
        target: String,
    },
    /// Scan Wi-Fi networks and recommend the best channel.
    Wifi,
    /// Audit TCP stack settings for latency-friendly values.
    Tcp,
    /// Audit the routing table (multi-WAN, VPN hijack patterns).
    Routes,
    /// Flush the OS DNS cache.
    FlushDns,
    /// List every registry tool (implemented + planned slots).
    List,
    /// Run any registry tool by id with string parameters.
    Run {
        /// Tool id (see `optimize list`).
        tool: String,
        /// Parameters as key=value pairs, e.g. target=example.com port=443.
        params: Vec<String>,
    },
}

fn config_dir() -> std::path::PathBuf {
    directories::ProjectDirs::from("app", "linkfyr", "linkfyr").map_or_else(
        || std::env::temp_dir().join("linkfyr"),
        |d| d.data_local_dir().to_path_buf(),
    )
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let mode = if cli.simulated {
        MonitorMode::Simulated
    } else {
        MonitorMode::Os
    };

    let engine = match AppEngine::open(&config_dir(), mode) {
        Ok(e) => e,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(1);
        }
    };

    let code = match cli.command {
        Command::Status => status(engine, cli.json).await,
        Command::Interfaces => interfaces(engine, cli.json).await,
        Command::Traffic { seconds } => traffic(engine, seconds, cli.json).await,
        Command::Optimize { cmd } => optimize(engine, cmd, cli.json).await,
        Command::Bridge { cmd } => bridge(engine, cmd, cli.json).await,
        Command::Alerts => alerts(engine, cli.json).await,
        Command::Watch => watch(engine).await,
        Command::Daemon { cmd } => daemon_cmd(cmd).await,
    };
    std::process::exit(code);
}

async fn daemon_cmd(cmd: DaemonCmd) -> i32 {
    match cmd {
        DaemonCmd::Status => {
            let dir = config_dir();
            let token_path = dir.join("daemon.token");
            let Ok(token) = std::fs::read_to_string(&token_path) else {
                eprintln!(
                    "no daemon token at {}; is linkfyrd running?",
                    token_path.display()
                );
                return 1;
            };
            let addr: std::net::SocketAddr = "127.0.0.1:58008".parse().expect("static addr");
            match linkfyr_daemon::client_request(addr, token.trim(), linkfyr_ipc::Request::Ping)
                .await
            {
                Ok(linkfyr_ipc::Response::Pong { version }) => {
                    println!("linkfyrd reachable, engine v{version}");
                    0
                }
                Ok(other) => {
                    eprintln!("unexpected response: {other:?}");
                    1
                }
                Err(e) => {
                    eprintln!("daemon unreachable: {e}");
                    1
                }
            }
        }
    }
}

async fn alerts(engine: std::sync::Arc<AppEngine>, json: bool) -> i32 {
    let resp = engine.handle_request(linkfyr_ipc::Request::GetAlerts).await;
    if let linkfyr_ipc::Response::Alerts { alerts } = resp {
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&alerts).unwrap_or_else(|_| "[]".into())
            );
            return 0;
        }
        if alerts.is_empty() {
            println!("No alerts yet (interface up/down and health drops appear here)");
            return 0;
        }
        for a in &alerts {
            let mark = match a.severity {
                linkfyr_ipc::Severity::Info => "info",
                linkfyr_ipc::Severity::Warning => "WARN",
                linkfyr_ipc::Severity::Critical => "CRIT",
            };
            println!("[{mark}] {}", a.title);
            println!("        {}", a.body);
        }
        return 0;
    }
    eprintln!("unexpected response");
    1
}

async fn watch(engine: std::sync::Arc<AppEngine>) -> i32 {
    println!("Watching (Ctrl+C to stop)...");
    let mut seen = usize::MAX;
    loop {
        if let linkfyr_ipc::Response::Alerts { alerts } =
            engine.handle_request(linkfyr_ipc::Request::GetAlerts).await
        {
            if seen == usize::MAX {
                seen = alerts.len();
                println!("{seen} existing alert(s) in the ring");
            } else if alerts.len() > seen {
                for a in &alerts[seen..] {
                    println!("{}: {}", a.title, a.body);
                }
                seen = alerts.len();
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

async fn bridge(engine: std::sync::Arc<AppEngine>, cmd: BridgeCmd, json: bool) -> i32 {
    use linkfyr_ipc::Request;
    let request = match &cmd {
        BridgeCmd::List => Request::BridgeList,
        BridgeCmd::Create {
            name,
            members,
            nat,
            prefix,
        } => Request::BridgeCreate {
            spec: linkfyr_model::optimize::BridgeSpec {
                name: name.clone(),
                members: members.clone(),
                mode: if *nat {
                    linkfyr_model::optimize::BridgeMode::NatShare
                } else {
                    linkfyr_model::optimize::BridgeMode::L2Switch
                },
                internal_prefix: prefix.clone(),
            },
        },
        BridgeCmd::Remove { name } => Request::BridgeRemove { name: name.clone() },
    };
    let resp = engine.handle_request(request).await;
    if json {
        match &resp {
            linkfyr_ipc::Response::Error(e) => {
                eprintln!("{}", serde_json::to_string_pretty(e).unwrap_or_default());
                1
            }
            other => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(other).unwrap_or_else(|_| "{}".into())
                );
                0
            }
        }
    } else {
        match resp {
            linkfyr_ipc::Response::Bridges { bridges } => {
                if bridges.is_empty() {
                    println!("No bridges found");
                }
                for b in bridges {
                    println!(
                        "{:<18} {:<10} members {} [{}]",
                        b.name,
                        format!("{:?}", b.mode).to_lowercase(),
                        if b.members.is_empty() {
                            "-".to_string()
                        } else {
                            b.members.join(",")
                        },
                        b.state
                    );
                }
                0
            }
            linkfyr_ipc::Response::BridgeReported(r) => {
                println!("{}: {}", r.outcome, r.detail);
                for c in &r.commands {
                    println!("  $ {c}");
                }
                i32::from(r.outcome != "applied")
            }
            other => {
                eprintln!("unexpected response: {other:?}");
                1
            }
        }
    }
}

use linkfyr_ipc::Response;
use linkfyr_model::optimize::RepairAction;

async fn optimize(engine: std::sync::Arc<AppEngine>, cmd: OptimizeCmd, json: bool) -> i32 {
    use linkfyr_ipc::Request;

    if matches!(cmd, OptimizeCmd::List) {
        let implemented = registry_catalog_size();
        println!("Registry: {implemented} implemented modules");
        println!("\nIMPLEMENTED (run with `optimize run <id> key=value …`):");
        for t in registry_catalog() {
            println!("  {:<22} {:<12} {}", t.id, t.group, t.blurb);
        }
        return 0;
    }

    if let OptimizeCmd::Run { tool, params } = &cmd {
        let mut map = std::collections::BTreeMap::new();
        for p in params {
            if let Some((k, v)) = p.split_once('=') {
                map.insert(k.to_string(), v.to_string());
            }
        }
        let request = Request::OptimizeRun {
            tool: tool.clone(),
            params: map,
        };
        let resp = engine.handle_request(request).await;
        match resp {
            linkfyr_ipc::Response::ToolRun(r) => {
                if json {
                    println!("{}", serde_json::to_string_pretty(&r).unwrap_or_default());
                } else {
                    println!("{} [{} ms]", r.summary, r.took_ms);
                    if !r.data.is_null() {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&r.data).unwrap_or_default()
                        );
                    }
                }
                return i32::from(!r.ok);
            }
            other => {
                eprintln!("unexpected response: {other:?}");
                return 1;
            }
        }
    }

    let request = match &cmd {
        OptimizeCmd::Capabilities => Request::OptimizeCapabilities,
        OptimizeCmd::Dns => Request::OptimizeDnsBenchmark,
        OptimizeCmd::ApplyDns { servers, interface } => Request::OptimizeDnsApply {
            servers: servers.clone(),
            interface: interface.clone(),
        },
        OptimizeCmd::RouteScan { target } => Request::OptimizeRouteScan {
            target: target.clone(),
        },
        OptimizeCmd::Bloat { seconds } => Request::OptimizeBloatTest {
            duration_s: *seconds,
        },
        OptimizeCmd::Speedtest { endpoint, seconds } => Request::OptimizeSpeedTest {
            endpoint: endpoint.clone(),
            duration_s: *seconds,
        },
        OptimizeCmd::Mtu { target } => Request::OptimizeMtu {
            target: target.clone(),
        },
        OptimizeCmd::Wifi => Request::OptimizeWifiScan,
        OptimizeCmd::Tcp => Request::OptimizeTcpAudit,
        OptimizeCmd::Routes => Request::OptimizeRouteAudit,
        OptimizeCmd::FlushDns => Request::OptimizeRepair {
            action: RepairAction::FlushDns,
        },
        OptimizeCmd::List | OptimizeCmd::Run { .. } => unreachable!("handled before dispatch"),
    };
    let resp = engine.handle_request(request).await;
    if json {
        match &resp {
            Response::Error(e) => {
                eprintln!(
                    "{}",
                    serde_json::to_string_pretty(e).unwrap_or_else(|_| "{}".into())
                );
                return 1;
            }
            other => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(other).unwrap_or_else(|_| "{}".into())
                );
                return 0;
            }
        }
    }
    match resp {
        Response::Capabilities(c) => {
            println!("Platform {}  elevated: {}", c.platform, c.elevated);
            for t in &c.tools {
                println!(
                    "  {:<18} {:<12} {}",
                    t.tool,
                    format!("{:?}", t.state).to_lowercase(),
                    t.detail
                );
            }
            0
        }
        Response::DnsBenchmark(r) => {
            println!(
                "System resolvers: {}",
                if r.system_resolvers.is_empty() {
                    "(not detected)".into()
                } else {
                    r.system_resolvers.join(", ")
                }
            );
            println!(
                "\n{:<18} {:<12} {:>10} {:>10} {:>8}",
                "RESOLVER", "LABEL", "CACHED", "UNCACHED", "SCORE"
            );
            for res in &r.results {
                println!(
                    "{:<18} {:<12} {:>10} {:>10} {:>8}",
                    res.server,
                    truncate(&res.label, 12),
                    res.cached_ms
                        .map_or_else(|| "-".into(), |v| format!("{v:.1} ms")),
                    res.uncached_ms
                        .map_or_else(|| "-".into(), |v| format!("{v:.1} ms")),
                    res.score
                        .map_or_else(|| "fail".into(), |v| format!("{v:.1}")),
                );
            }
            if let Some(rec) = &r.recommended {
                println!("\nRecommended: {rec}");
            } else {
                println!("\nNo resolver answered; check connectivity");
            }
            0
        }
        Response::DnsApplied(r) => {
            println!("DNS apply on {:?}: {}", r.interface, r.outcome);
            if !r.previous.is_empty() {
                println!("Previous: {}", r.previous.join(", "));
            }
            println!("{}", r.detail);
            i32::from(r.outcome != "applied")
        }
        Response::RouteScan(r) => {
            println!("Target: {}  → {}\n", r.target, r.verdict);
            println!(
                "{:<26} {:<24} {:>8} {:>8} {:>8} {:>8}",
                "PATH", "ADDR", "MED", "MIN", "P95", "JITTER"
            );
            for p in &r.paths {
                println!(
                    "{:<26} {:<24} {:>8} {:>8} {:>8} {:>8}",
                    truncate(&p.label, 26),
                    truncate(&p.addr, 24),
                    p.median_ms
                        .map_or_else(|| "-".into(), |v| format!("{v:.1}")),
                    p.min_ms.map_or_else(|| "-".into(), |v| format!("{v:.1}")),
                    p.p95_ms.map_or_else(|| "-".into(), |v| format!("{v:.1}")),
                    p.jitter_ms
                        .map_or_else(|| "-".into(), |v| format!("{v:.1}")),
                );
            }
            println!("\n{}", r.recommendation);
            0
        }
        Response::BloatTest(r) => {
            println!("Bufferbloat (probe target {})", r.probe_target);
            println!("  baseline latency   {:>8.1} ms", r.baseline_ms);
            println!(
                "  download  +{:.0} ms  (grade {})  load {:.1} Mbps",
                r.down_added_ms,
                r.down_grade.label(),
                r.down_mbps
            );
            println!(
                "  upload    +{:.0} ms  (grade {})  load {:.1} Mbps",
                r.up_added_ms,
                r.up_grade.label(),
                r.up_mbps
            );
            0
        }
        Response::SpeedTest(r) => {
            println!("Endpoint: {}", r.endpoint);
            println!(
                "  latency   {}",
                r.latency_ms
                    .map_or_else(|| "unreachable".into(), |v| format!("{v:.1} ms"))
            );
            println!("  download  {:.1} Mbps", r.download_mbps);
            println!("  upload    {:.1} Mbps", r.upload_mbps);
            i32::from(r.latency_ms.is_none())
        }
        Response::Mtu(r) => {
            println!(
                "Path MTU to {}: {} ({} probes)",
                r.target, r.path_mtu, r.probes
            );
            println!("{}", r.verdict);
            0
        }
        Response::WifiScan(r) => {
            if r.networks.is_empty() {
                println!("{}", r.explanation);
                return 0;
            }
            println!(
                "{:<24} {:>4} {:>6} {:>7}  SECURITY",
                "SSID", "CH", "BAND", "SIGNAL"
            );
            for ap in &r.networks {
                println!(
                    "{:<24} {:>4} {:>6} {:>6}%  {}",
                    truncate(ap.ssid.as_deref().unwrap_or("(hidden)"), 24),
                    ap.channel,
                    ap.band,
                    ap.signal_pct,
                    ap.security.as_deref().unwrap_or("open"),
                );
            }
            println!("\n{}", r.explanation);
            0
        }
        Response::TcpAudit(r) => {
            println!("TCP audit on {} (elevated: {})", r.platform, r.elevated);
            for c in &r.checks {
                let mark = match c.status {
                    linkfyr_model::optimize::CheckStatus::Ok => "ok",
                    linkfyr_model::optimize::CheckStatus::Suboptimal => "SUBOPTIMAL",
                    linkfyr_model::optimize::CheckStatus::Unknown => "?",
                };
                println!(
                    "  {:<32} {:<14} want {:<12} [{}]",
                    c.key, c.current, c.recommended, mark
                );
                if let Some(fix) = &c.fix {
                    println!("      fix: {fix}");
                }
            }
            println!("\n{}", r.summary);
            0
        }
        Response::RouteAudit(r) => {
            println!("Default routes: {}", r.default_count);
            println!(
                "\n{:<20} {:<16} {:<10} {:>8}",
                "DESTINATION", "GATEWAY", "IFACE", "METRIC"
            );
            for e in r.entries.iter().take(40) {
                println!(
                    "{:<20} {:<16} {:<10} {:>8}",
                    e.destination,
                    e.gateway.as_deref().unwrap_or("on-link"),
                    e.interface.as_deref().unwrap_or("-"),
                    e.metric.map_or_else(|| "-".into(), |m| m.to_string()),
                );
            }
            if !r.anomalies.is_empty() {
                println!("\nFindings:");
                for a in &r.anomalies {
                    println!("  - {a}");
                }
            }
            0
        }
        Response::Repaired(r) => {
            println!("{}: {}", r.outcome, r.detail);
            i32::from(r.outcome != "applied")
        }
        Response::Error(e) => {
            eprintln!("error: {} ({:?})", e.message, e.code);
            1
        }
        other => {
            eprintln!("error: unexpected response variant {other:?}");
            1
        }
    }
}

fn fmt_rate(bps: f64) -> String {
    let mbps = bps / 1_000_000.0;
    if mbps >= 100.0 {
        format!("{mbps:.0} Mbps")
    } else if mbps >= 10.0 {
        format!("{mbps:.1} Mbps")
    } else {
        format!("{mbps:.2} Mbps")
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!(
            "{}…",
            s.chars().take(max.saturating_sub(1)).collect::<String>()
        )
    }
}

async fn warm_snapshot(engine: &std::sync::Arc<AppEngine>) -> Snapshot {
    // Prime counters, pause briefly, take a second tick so rates exist.
    let _ = engine.tick_once().await;
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    (*engine.tick_once().await).clone()
}

async fn status(engine: std::sync::Arc<AppEngine>, json: bool) -> i32 {
    let snap = warm_snapshot(&engine).await;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&snap).unwrap_or_else(|_| "{}".into())
        );
        return 0;
    }

    println!("LinkFYR v{}", snap.engine_version);
    if let Some(note) = engine.degraded_note() {
        println!("config: {note}");
    }
    println!();
    println!(
        "Total  {}  up {}",
        fmt_rate(snap.totals.rx_bps),
        fmt_rate(snap.totals.tx_bps)
    );
    let inet = &snap.internet;
    match (&inet.rtt_avg_ms, &inet.loss_pct) {
        (Some(rtt), Some(loss)) => println!(
            "Internet  rtt {:.1} ms  jitter {:.1} ms  loss {:.1}%",
            rtt,
            inet.jitter_ms.unwrap_or(0.0),
            loss
        ),
        _ => println!("Internet  (probing...)"),
    }
    println!();
    println!(
        "{:<22} {:<10} {:>12} {:>12} {:>6}",
        "INTERFACE", "STATUS", "DOWN", "UP", "HEALTH"
    );
    for i in &snap.interfaces {
        println!(
            "{:<22} {:<10} {:>12} {:>12} {:>6}",
            truncate(&i.interface.friendly_name, 22),
            format!("{:?}", i.interface.status).to_lowercase(),
            fmt_rate(i.rx_bps),
            fmt_rate(i.tx_bps),
            i.health
                .as_ref()
                .map_or_else(|| "-".into(), |h| h.overall.to_string())
        );
    }
    0
}

async fn interfaces(engine: std::sync::Arc<AppEngine>, json: bool) -> i32 {
    if let linkfyr_ipc::Response::Interfaces { interfaces } = engine
        .handle_request(linkfyr_ipc::Request::GetInterfaces)
        .await
    {
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&interfaces).unwrap_or_else(|_| "[]".into())
            );
            return 0;
        }
        println!(
            "{:<22} {:<10} {:<10} {:<20} {:<16} {:>10}",
            "NAME", "STATUS", "KIND", "ADDRESSES", "GATEWAY", "SPEED"
        );
        for i in &interfaces {
            let mut addrs = i.ipv4.join(", ");
            if addrs.is_empty() {
                addrs = i.ipv6.join(", ");
            }
            println!(
                "{:<22} {:<10} {:<10} {:<20} {:<16} {:>10}",
                truncate(&i.friendly_name, 22),
                format!("{:?}", i.status).to_lowercase(),
                format!("{:?}", i.kind).to_lowercase(),
                truncate(&addrs, 20),
                truncate(i.gateway.as_deref().unwrap_or("-"), 16),
                i.speed_bps
                    .map_or_else(|| "-".into(), |s| format!("{} Mb", s / 1_000_000))
            );
        }
        0
    } else {
        eprintln!("error: unexpected response");
        1
    }
}

async fn traffic(engine: std::sync::Arc<AppEngine>, seconds: u64, json: bool) -> i32 {
    // Prime counters, then sample `seconds` live ticks.
    let _ = engine.tick_once().await;
    let mut samples = Vec::new();
    for _ in 0..seconds.max(1) {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        let snap = engine.tick_once().await;
        samples.push((*snap).clone());
    }

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&samples).unwrap_or_else(|_| "[]".into())
        );
        return 0;
    }

    println!("Sampling {} s (interface rates per second)", samples.len());
    for (n, s) in samples.iter().enumerate() {
        let top = s
            .interfaces
            .iter()
            .filter(|i| i.rx_bps > 0.0)
            .map(|i| format!("{} {}", i.interface.friendly_name, fmt_rate(i.rx_bps)))
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "  +{:>2}s  down {:>10}  up {:>10}  {}",
            n + 1,
            fmt_rate(s.totals.rx_bps),
            fmt_rate(s.totals.tx_bps),
            top
        );
    }
    0
}

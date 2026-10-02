//! LinkFYR Optimization Toolkit: real, measurable network tools.
//!
//! Every module here performs a genuine operation on the machine (DNS
//! queries, TCP connects, OS tool invocation, real throughput) and
//! reports measured values. Nothing is fabricated: when a tool cannot
//! run on this platform or without elevation, the report says so.
//!
//! Cross-platform surface: Windows, macOS, Linux. CPU-architecture
//! independent (pure Rust + rustls).

use linkfyr_model::now_ms;

/// Milliseconds since the Unix epoch (shared by tool modules).
pub fn clock_ms() -> u64 {
    now_ms()
}

pub mod bloat;
pub mod capability;
pub mod cert;
pub mod diag;
pub mod dns;
pub mod dnskit;
pub mod exec;
pub mod flows;
pub mod mtu;
pub mod multiwan;
pub mod netops;
pub mod probes;
pub mod registry;
pub mod repair;
pub mod routeaudit;
pub mod routescan;
pub mod speedtest;
pub mod stats;
pub mod svcprobe;
pub mod sysnet;
pub mod tcpaudit;
pub mod web;
pub mod wifiscan;

#[cfg(test)]
mod testutil;

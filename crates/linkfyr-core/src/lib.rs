//! LinkFYR core: the engine orchestrator.
//!
//! Owns configuration persistence (with backups + safe mode), the
//! telemetry engine lifecycle, and exposes the operations that both the
//! Tauri shell and the CLI call. No Tauri types here — this crate is the
//! shell-independent API implementation (ADR-0001).

pub mod config;
pub mod engine;

pub use config::{ConfigStore, StoreError};
pub use engine::{AppEngine, MonitorMode};

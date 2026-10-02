//! LinkFYR telemetry: sampling engine, ring buffers, latency probes,
//! and the multi-factor health engine.

pub mod engine;
pub mod health;
pub mod probe;
pub mod ring;

pub use engine::{EngineConfig, TelemetryEngine};
pub use health::{HealthInputs, score_interface};
pub use probe::{ProbeConfig, TcpProber, aggregate};
pub use ring::RingBuffer;

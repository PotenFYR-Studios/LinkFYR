//! LinkFYR Fusion tunnel protocol (Phase 6): multipath bonding over
//! multiple WAN links. This is the wire-format core  framing, path
//! tagging, reordering windows  designed for the self-hosted Edge
//! node (`linkfyr-edge`). The honest rule from the roadmap applies:
//! load balancing (multiple flows) is not single-flow bonding; Fusion
//! is the latter.

use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u8 = 1;
/// Maximum reordering window before a gap is declared lost.
pub const REORDER_WINDOW_MS: u32 = 120;
/// Heartbeat interval on idle paths.
pub const HEARTBEAT_INTERVAL_MS: u32 = 1000;

/// One frame on the wire. Path-tagged so the receiver can reorder
/// across links. Sequence numbers are per-session, monotonic.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Frame {
    /// Protocol version (1).
    pub version: u8,
    /// Monotonic per-session sequence number.
    pub seq: u64,
    /// Which link carried this frame (path id, assigned at handshake).
    pub path_id: u8,
    /// Payload type.
    pub frame_type: FrameType,
    /// Encrypted payload (the session layer encrypts before framing).
    pub payload: Vec<u8>,
    /// Send timestamp for RTT estimation (ms since session start).
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameType {
    /// Handshake / key exchange.
    Handshake,
    /// Encrypted data.
    Data,
    /// Keepalive on an idle path.
    Heartbeat,
    /// Path statistics report (for the adaptive scheduler).
    PathReport,
    /// Graceful close.
    Close,
}

/// Session handshake: version negotiation + path declaration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Handshake {
    pub protocol_version: u8,
    pub session_id: [u8; 16],
    /// The client's advertised paths (one per WAN link).
    pub paths: Vec<PathInfo>,
    /// Redundancy mode requested.
    pub redundancy: RedundancyMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathInfo {
    pub path_id: u8,
    /// Human-readable interface name for explainability.
    pub name: String,
    /// Estimated capacity (bits/s)  the scheduler refines this live.
    pub estimated_bps: u64,
    pub estimated_rtt_ms: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RedundancyMode {
    /// Single-path (no redundancy): all data on the best link.
    Off,
    /// Duplicate critical frames (handshakes, DNS) on all links.
    Critical,
    /// Adaptive: duplicate when the primary link is degraded.
    Adaptive,
    /// Full duplication: every frame on every link (highest cost).
    Full,
}

/// Reordering buffer: collects frames from multiple paths and emits
/// them in sequence order, within the reorder window.
pub struct ReorderBuffer {
    next_seq: u64,
    buffer: std::collections::BTreeMap<u64, Frame>,
    window_start_ms: u64,
}

impl ReorderBuffer {
    pub fn new(initial_seq: u64) -> Self {
        Self {
            next_seq: initial_seq,
            buffer: std::collections::BTreeMap::new(),
            window_start_ms: 0,
        }
    }

    /// Insert a frame; returns all frames now deliverable in order.
    /// Frames older than `next_seq` are duplicates (dropped silently).
    pub fn insert(&mut self, frame: Frame, now_ms: u64) -> Vec<Frame> {
        if frame.seq < self.next_seq {
            return vec![]; // duplicate
        }
        self.buffer.insert(frame.seq, frame);
        if self.buffer.is_empty() {
            return vec![];
        }
        // First insert starts the reorder window timer.
        if self.window_start_ms == 0 {
            self.window_start_ms = now_ms;
        }
        let mut deliverable = Vec::new();
        while let Some(f) = self.buffer.remove(&self.next_seq) {
            deliverable.push(f);
            self.next_seq += 1;
            self.window_start_ms = now_ms; // reset on progress
        }
        // Window expiry: skip the gap.
        if self.buffer.is_empty() {
            self.window_start_ms = 0;
        } else if now_ms.saturating_sub(self.window_start_ms) > u64::from(REORDER_WINDOW_MS) {
            let first = *self.buffer.keys().next().unwrap_or(&self.next_seq);
            self.next_seq = first;
            self.window_start_ms = 0;
            // Deliver what we have (gap declared lost).
            while let Some(f) = self.buffer.remove(&self.next_seq) {
                deliverable.push(f);
                self.next_seq += 1;
            }
        }
        deliverable
    }
}

/// Encode a frame to bytes (length-prefixed JSON at v0; a compact
/// binary format replaces this after benchmarking  the format is
/// version-tagged so the switch is backward-compatible).
pub fn encode_frame(frame: &Frame) -> Result<Vec<u8>, String> {
    serde_json::to_vec(frame).map_err(|e| e.to_string())
}

pub fn decode_frame(bytes: &[u8]) -> Result<Frame, String> {
    serde_json::from_slice(bytes).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data_frame(seq: u64, path: u8) -> Frame {
        Frame {
            version: 1,
            seq,
            path_id: path,
            frame_type: FrameType::Data,
            payload: vec![seq as u8; 16],
            timestamp_ms: seq * 10,
        }
    }

    #[test]
    fn reorder_delivers_in_sequence() {
        let mut buf = ReorderBuffer::new(0);
        // Arrive out of order (path 1 first, then path 0).
        let d1 = buf.insert(data_frame(1, 1), 10);
        assert!(d1.is_empty(), "seq 1 cannot deliver before 0");
        let d0 = buf.insert(data_frame(0, 0), 15);
        assert_eq!(d0.len(), 2, "0 then 1 deliver together");
        assert_eq!(d0[0].seq, 0);
        assert_eq!(d0[1].seq, 1);
    }

    #[test]
    fn reorder_skips_gap_on_window_expiry() {
        let mut buf = ReorderBuffer::new(0);
        // Frame 2 arrives; frame 0 and 1 are lost.
        buf.insert(data_frame(2, 0), 10);
        assert!(buf.buffer.contains_key(&2), "frame 2 buffered");
        // Window expires.
        let delivered = buf.insert(data_frame(3, 0), 10 + u64::from(REORDER_WINDOW_MS) + 1);
        assert_eq!(delivered.len(), 2, "gap skipped: 2 then 3");
        assert_eq!(delivered[0].seq, 2);
    }

    #[test]
    fn duplicates_are_dropped() {
        let mut buf = ReorderBuffer::new(0);
        buf.insert(data_frame(0, 0), 0);
        buf.insert(data_frame(1, 0), 0);
        let dup = buf.insert(data_frame(0, 1), 5);
        assert_eq!(dup.len(), 0);
    }

    #[test]
    fn frame_roundtrip() {
        let f = data_frame(42, 1);
        let bytes = encode_frame(&f).unwrap();
        let back = decode_frame(&bytes).unwrap();
        assert_eq!(back, f);
    }

    #[test]
    fn handshake_serializes_camel_case() {
        let h = Handshake {
            protocol_version: 1,
            session_id: [0u8; 16],
            paths: vec![PathInfo {
                path_id: 0,
                name: "eth0".into(),
                estimated_bps: 100_000_000,
                estimated_rtt_ms: 12.5,
            }],
            redundancy: RedundancyMode::Adaptive,
        };
        let json = serde_json::to_string(&h).unwrap();
        assert!(json.contains("\"protocolVersion\":1"));
        assert!(json.contains("\"estimatedRttMs\":12.5"));
        assert!(json.contains("\"adaptive\""));
    }
}

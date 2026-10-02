//! LinkFYR Edge node (Phase 6): terminates Fusion tunnels from one or
//! more clients, reorders frames, and forwards to the Internet. This
//! is the self-hostable relay (`docker run linkfyr/edge`). The hosted
//! Edge uses the same code; only the deployment differs.
//!
//! v0: accepts a single Fusion session on a TCP port, runs the
//! reordering buffer, and echoes path statistics back. Data forwarding
//! (the actual proxy) lands with the session crypto layer.

use std::net::SocketAddr;
use linkfyr_protocol::{Frame, FrameType, Handshake, ReorderBuffer};

pub const DEFAULT_LISTEN: &str = "0.0.0.0:7443";

pub struct EdgeConfig {
    pub listen: SocketAddr,
    /// Max sessions before refusing new handshakes.
    pub max_sessions: usize,
}

impl Default for EdgeConfig {
    fn default() -> Self {
        Self {
            listen: DEFAULT_LISTEN.parse().expect("valid listen"),
            max_sessions: 256,
        }
    }
}

/// A live Fusion session on the Edge.
pub struct EdgeSession {
    pub session_id: [u8; 16],
    pub reorder: ReorderBuffer,
    pub connected_at_ms: u64,
    pub bytes_forwarded: u64,
    pub frames_received: u64,
}

/// Process one received frame; returns frames to forward + stats.
pub fn process_frame(session: &mut EdgeSession, frame: Frame, now_ms: u64) -> Vec<Frame> {
    session.frames_received += 1;
    session.bytes_forwarded += frame.payload.len() as u64;
    match frame.frame_type {
        FrameType::Heartbeat => vec![Frame {
            version: frame.version,
            seq: frame.seq,
            path_id: frame.path_id,
            frame_type: FrameType::PathReport,
            payload: format!(
                "{{\"rttMs\":{}}}",
                now_ms.saturating_sub(frame.timestamp_ms)
            )
            .into_bytes(),
            timestamp_ms: now_ms,
        }],
        _ => session.reorder.insert(frame, now_ms),
    }
}

/// Accept + decode a handshake from raw bytes.
pub fn accept_handshake(bytes: &[u8]) -> Result<Handshake, String> {
    let h: Handshake = serde_json::from_slice(bytes).map_err(|e| format!("bad handshake: {e}"))?;
    if h.protocol_version != linkfyr_protocol::PROTOCOL_VERSION {
        return Err(format!(
            "version mismatch: client {} != edge {}",
            h.protocol_version,
            linkfyr_protocol::PROTOCOL_VERSION
        ));
    }
    if h.paths.is_empty() {
        return Err("handshake declared zero paths".into());
    }
    Ok(h)
}

#[cfg(test)]
mod tests {
    use super::*;
    use linkfyr_protocol::{PathInfo, RedundancyMode};

    fn handshake_frame(seq: u64, path: u8) -> Frame {
        Frame {
            version: 1,
            seq,
            path_id: path,
            frame_type: FrameType::Data,
            payload: vec![b'x'; 64],
            timestamp_ms: seq * 10,
        }
    }

    #[test]
    fn handshake_validates_version_and_paths() {
        let h = Handshake {
            protocol_version: 1,
            session_id: [1u8; 16],
            paths: vec![PathInfo {
                path_id: 0,
                name: "test".into(),
                estimated_bps: 1_000_000,
                estimated_rtt_ms: 10.0,
            }],
            redundancy: RedundancyMode::Adaptive,
        };
        let bytes = serde_json::to_vec(&h).unwrap();
        assert!(accept_handshake(&bytes).is_ok());

        let mut bad = h.clone();
        bad.protocol_version = 99;
        let bytes = serde_json::to_vec(&bad).unwrap();
        assert!(accept_handshake(&bytes).is_err());

        let mut empty = h;
        empty.paths.clear();
        let bytes = serde_json::to_vec(&empty).unwrap();
        assert!(accept_handshake(&bytes).is_err());
    }

    #[test]
    fn edge_session_reorders_and_counts() {
        let mut session = EdgeSession {
            session_id: [1u8; 16],
            reorder: ReorderBuffer::new(0),
            connected_at_ms: 0,
            bytes_forwarded: 0,
            frames_received: 0,
        };
        let delivered = process_frame(&mut session, handshake_frame(1, 0), 10);
        assert!(delivered.is_empty(), "seq 1 waits for 0");
        let delivered = process_frame(&mut session, handshake_frame(0, 0), 20);
        assert_eq!(delivered.len(), 2);
        assert_eq!(session.frames_received, 2);
        assert_eq!(session.bytes_forwarded, 128);
    }

    #[test]
    fn heartbeat_gets_a_path_report_back() {
        let mut session = EdgeSession {
            session_id: [1u8; 16],
            reorder: ReorderBuffer::new(0),
            connected_at_ms: 0,
            bytes_forwarded: 0,
            frames_received: 0,
        };
        let hb = Frame {
            version: 1,
            seq: 100,
            path_id: 0,
            frame_type: FrameType::Heartbeat,
            payload: vec![],
            timestamp_ms: 90,
        };
        let reply = process_frame(&mut session, hb, 100);
        assert_eq!(reply.len(), 1);
        assert!(matches!(reply[0].frame_type, FrameType::PathReport));
        let body = String::from_utf8_lossy(&reply[0].payload).to_string();
        assert!(body.contains("\"rttMs\":10"));
    }
}

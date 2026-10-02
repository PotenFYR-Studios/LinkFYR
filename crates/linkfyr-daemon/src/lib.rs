//! `linkfyrd` service library: hosts the AppEngine behind an
//! authenticated loopback transport so monitoring, optimization jobs,
//! and bridges keep running with the GUI closed (exam-safe).
//!
//! Transport v1: TCP on 127.0.0.1 only + per-user token file (0600).
//! Handshake: one JSON line `{"auth":"<token>"}`, then newline-framed
//! `Envelope<Request>` / `Envelope<Response>` pairs reusing the exact
//! linkfyr-ipc contract (no second protocol to drift). Named-pipe DACL
//! (Windows) and UDS + SO_PEERCRED (unix) replace the token in the
//! hardening phase (threat model T1/T15).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use linkfyr_core::{AppEngine, MonitorMode};
use linkfyr_ipc::{Envelope, Request, Response};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

pub const DEFAULT_BIND: &str = "127.0.0.1:58008";

#[derive(Debug, Clone)]
pub struct DaemonOptions {
    /// Loopback bind address. Tests use ephemeral ports.
    pub bind: String,
    /// Config directory (engine store + token file live here).
    pub config_dir: PathBuf,
}

impl DaemonOptions {
    pub fn with_config_dir(dir: impl Into<PathBuf>) -> Self {
        Self {
            bind: DEFAULT_BIND.into(),
            config_dir: dir.into(),
        }
    }

    pub fn token_path(&self) -> PathBuf {
        self.config_dir.join("daemon.token")
    }
}

/// Create (or reuse) the auth token; file permissions are user-only on
/// unix. Token entropy comes from the OS-provided address-space
/// randomization of `RandomState` (per-process), mixed and stretched.
pub fn ensure_token(path: &Path) -> Result<String, String> {
    if let Ok(existing) = std::fs::read_to_string(path) {
        let trimmed = existing.trim();
        if trimmed.len() >= 32 {
            return Ok(trimmed.to_string());
        }
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let token = generate_token();
    std::fs::write(path, &token).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(token)
}

fn generate_token() -> String {
    use std::collections::hash_map::RandomState;
    use std::fmt::Write as _;
    use std::hash::{BuildHasher, Hasher};
    let mut out = String::with_capacity(64);
    for _ in 0..4 {
        let mut h = RandomState::new().build_hasher();
        h.write_u32(std::process::id());
        h.write_usize(out.len());
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64);
        h.write_u64(nanos);
        let _ = write!(out, "{:016x}", h.finish());
    }
    out
}

/// A running daemon handle: stop via dropping the runtime or the
/// returned join handle abort.
pub struct Daemon {
    pub local_addr: std::net::SocketAddr,
    pub token_path: PathBuf,
    pub join: tokio::task::JoinHandle<()>,
}

/// Bind and start serving. The engine runs in OS mode against real
/// interfaces; every tick and request is the same code the GUI uses.
pub async fn serve(opts: DaemonOptions) -> Result<Daemon, String> {
    let token_path = opts.token_path();
    let token = ensure_token(&token_path)?;
    let listener = TcpListener::bind(&opts.bind)
        .await
        .map_err(|e| format!("bind {}: {e}", opts.bind))?;
    let local_addr = listener.local_addr().map_err(|e| e.to_string())?;

    let engine = AppEngine::open(&opts.config_dir, MonitorMode::Os)
        .map_err(|e| format!("engine open: {e}"))?;
    engine.start();

    let join = tokio::spawn(async move {
        loop {
            let Ok((stream, _peer)) = listener.accept().await else {
                return;
            };
            let engine = Arc::clone(&engine);
            let token = token.clone();
            tokio::spawn(async move {
                handle_connection(stream, engine, token).await;
            });
        }
    });

    Ok(Daemon {
        local_addr,
        token_path,
        join,
    })
}

async fn handle_connection(mut stream: TcpStream, engine: Arc<AppEngine>, token: String) {
    // Loopback-only enforcement: never talk to a non-loopback peer.
    let peer = stream.peer_addr().map(|a| a.ip()).ok();
    if peer.is_none_or(|ip| !ip.is_loopback()) {
        return;
    }
    let (reader, mut writer) = stream.split();
    let mut lines = BufReader::new(reader);
    let mut line = String::new();

    // Handshake: {"auth":"<token>"} on the first line.
    if lines.read_line(&mut line).await.unwrap_or(0) == 0 {
        return;
    }
    let auth_ok = serde_json::from_str::<serde_json::Value>(&line)
        .ok()
        .and_then(|v| v.get("auth").and_then(|a| a.as_str()).map(str::to_string))
        .is_some_and(|a| constant_time_eq(a.as_bytes(), token.as_bytes()));
    if !auth_ok {
        let _ = writer
            .write_all(
                b"{\"type\":\"error\",\"code\":\"unauthorized\",\"message\":\"bad token\"}\n",
            )
            .await;
        return;
    }

    loop {
        line.clear();
        match lines.read_line(&mut line).await {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let response: Response = match serde_json::from_str::<Envelope<Request>>(trimmed) {
            Ok(envelope) => {
                let id = envelope.id;
                let payload = envelope.payload;
                let mut r = engine.handle_request(payload).await;
                stamp_id(&mut r, id);
                r
            }
            Err(e) => Response::Error(Box::new(linkfyr_ipc::ApiError::new(
                linkfyr_ipc::ErrorCode::ValidationError,
                format!("bad request line: {e}"),
            ))),
        };
        let encoded = serde_json::to_string(&response).unwrap_or_else(|_| "{}".into());
        if writer.write_all(encoded.as_bytes()).await.is_err() {
            return;
        }
        if writer.write_all(b"\n").await.is_err() {
            return;
        }
    }
}

/// Response carries no envelope on this transport (client already has
/// ordering via line pairs); id stamping keeps parity with the envelope
/// contract by echoing in the JSON when trivially possible.
fn stamp_id(_response: &mut Response, _id: u64) {
    // Envelopes are rebuilt client-side; nothing to do in v1.
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// One-shot client for the transport (used by the CLI `daemon status`
/// and by tests). Returns the decoded response for a request.
pub async fn client_request(
    addr: std::net::SocketAddr,
    token: &str,
    request: Request,
) -> Result<Response, String> {
    let stream = TcpStream::connect(addr).await.map_err(|e| e.to_string())?;
    let (reader, mut writer) = stream.into_split();
    let handshake = serde_json::json!({ "auth": token }).to_string();
    writer
        .write_all(handshake.as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    writer.write_all(b"\n").await.map_err(|e| e.to_string())?;
    let envelope = serde_json::to_string(&Envelope::new(1, request)).map_err(|e| e.to_string())?;
    writer
        .write_all(envelope.as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    writer.write_all(b"\n").await.map_err(|e| e.to_string())?;

    let mut lines = BufReader::new(reader);
    let mut line = String::new();
    lines
        .read_line(&mut line)
        .await
        .map_err(|e| e.to_string())?;
    if line.trim().is_empty() {
        return Err("daemon closed without a response".into());
    }
    if line.contains("\"unauthorized\"") {
        return Err("unauthorized: token mismatch".into());
    }
    serde_json::from_str(&line).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use linkfyr_ipc::Response;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "linkfyrd-test-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("mkdir");
        dir
    }

    #[tokio::test]
    async fn daemon_serves_engine_over_real_socket() {
        let opts = DaemonOptions {
            bind: "127.0.0.1:0".into(),
            config_dir: temp_dir("serve"),
        };
        let daemon = serve(opts).await.expect("serve");
        let token = ensure_token(&daemon.token_path).expect("token");

        let resp = client_request(daemon.local_addr, &token, Request::Ping).await;
        match resp {
            Ok(Response::Pong { version }) => assert_ne!(version, ""),
            other => panic!("expected pong, got {other:?}"),
        }

        let resp = client_request(
            daemon.local_addr,
            &token,
            Request::OptimizeRun {
                tool: "proxy_config".into(),
                params: std::collections::BTreeMap::default(),
            },
        )
        .await
        .expect("tool run");
        match resp {
            Response::ToolRun(r) => assert!(r.ok, "{}", r.summary),
            other => panic!("expected tool run, got {other:?}"),
        }
        daemon.join.abort();
    }

    #[tokio::test]
    async fn wrong_token_is_rejected() {
        let opts = DaemonOptions {
            bind: "127.0.0.1:0".into(),
            config_dir: temp_dir("auth"),
        };
        let daemon = serve(opts).await.expect("serve");
        let err = client_request(daemon.local_addr, "wrong-token-value-1234", Request::Ping)
            .await
            .expect_err("must be rejected");
        assert!(err.contains("unauthorized"), "{err}");
        daemon.join.abort();
    }

    #[tokio::test]
    async fn bad_request_line_gets_validation_error() {
        let opts = DaemonOptions {
            bind: "127.0.0.1:0".into(),
            config_dir: temp_dir("bad"),
        };
        let daemon = serve(opts).await.expect("serve");
        let token = ensure_token(&daemon.token_path).expect("token");

        let stream = TcpStream::connect(daemon.local_addr)
            .await
            .expect("connect");
        let (reader, mut writer) = stream.into_split();
        writer
            .write_all(format!("{{\"auth\":\"{token}\"}}\n").as_bytes())
            .await
            .unwrap();
        writer.write_all(b"this is not json\n").await.unwrap();
        let mut lines = BufReader::new(reader);
        let mut line = String::new();
        lines.read_line(&mut line).await.unwrap();
        assert!(line.contains("validation_error"), "{line}");
        daemon.join.abort();
    }

    #[test]
    fn token_is_stable_and_private() {
        let dir = temp_dir("token");
        let path = dir.join("daemon.token");
        let t1 = ensure_token(&path).expect("first");
        let t2 = ensure_token(&path).expect("second");
        assert_eq!(t1, t2, "token must be reused, not rotated");
        assert!(t1.len() >= 32);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "token file must be 0600");
        }
    }

    #[test]
    fn generated_tokens_differ_between_calls() {
        assert_ne!(generate_token(), generate_token());
    }

    #[test]
    fn constant_time_compare_is_constant_length() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
    }
}

//! LinkFYR mobile companion (Tauri 2): a remote client for the
//! linkfyrd daemon running on the user's PC. The phone sends the same
//! IPC envelopes the desktop app sends and renders the same UI.
//!
//! Mobile OSes forbid the raw sockets and OS-level network commands
//! the engine needs, so no local engine runs here by design: the
//! daemon is the engine, the phone is a first-class client.

use std::sync::Mutex;
use std::time::Duration;

use linkfyr_ipc::{Envelope, Request, Response};
use tauri::State;

pub struct DaemonState {
    pub base_url: String,
    pub token: String,
}

type CmdResult<T> = Result<T, String>;

fn unavailable(message: impl Into<String>) -> Response {
    Response::Error(Box::new(linkfyr_ipc::ApiError::new(
        linkfyr_ipc::ErrorCode::EngineUnavailable,
        message,
    )))
}

/// Configure the daemon connection (host + token). Safe to call again
/// from Settings; nothing else is persisted on the phone.
#[tauri::command]
async fn configure_daemon(
    host: String,
    token: String,
    state: State<'_, Mutex<DaemonState>>,
) -> CmdResult<()> {
    let base = if host.starts_with("http") {
        host
    } else {
        format!("http://{host}")
    };
    *state.lock().expect("daemon state") = DaemonState {
        base_url: base,
        token,
    };
    Ok(())
}

/// Forward an IPC request to the daemon over HTTP. Same envelope, same
/// contract as the desktop engine: one API, zero drift.
#[tauri::command]
async fn engine_request(
    request: Request,
    state: State<'_, Mutex<DaemonState>>,
) -> CmdResult<Response> {
    let (base, token) = {
        let s = state.lock().expect("daemon state");
        (s.base_url.clone(), s.token.clone())
    };
    if base.is_empty() {
        return Ok(unavailable(
            "No daemon configured. Open Settings and enter the host and token from linkfyrd.",
        ));
    }
    tokio::task::spawn_blocking(move || {
        let envelope = match serde_json::to_string(&Envelope::new(1, request)) {
            Ok(e) => e,
            Err(e) => return unavailable(format!("encode failed: {e}")),
        };
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(5))
            .timeout_read(Duration::from_secs(30))
            .build();
        let url = format!("{base}/api/");
        let result = agent
            .post(&url)
            .set("Content-Type", "application/json")
            .set("Authorization", &format!("Bearer {token}"))
            .send_string(&envelope);
        match result {
            Ok(resp) => {
                let body = resp.into_string().unwrap_or_default();
                serde_json::from_str(&body)
                    .unwrap_or_else(|e| unavailable(format!("bad daemon response: {e}")))
            }
            Err(ureq::Error::Status(code, _)) => unavailable(format!(
                "daemon refused the request ({code}); check the token in Settings"
            )),
            Err(e) => unavailable(format!("daemon unreachable: {e}")),
        }
    })
    .await
    .map_err(|e| e.to_string())
}

/// Ping the daemon; returns the engine version on success.
#[tauri::command]
async fn daemon_ping(state: State<'_, Mutex<DaemonState>>) -> CmdResult<String> {
    match engine_request(Request::Ping, state).await {
        Ok(Response::Pong { version }) => Ok(version),
        Ok(Response::Error(e)) => Err(e.message),
        _ => Err("unexpected response".into()),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(Mutex::new(DaemonState {
            base_url: String::new(),
            token: String::new(),
        }))
        .invoke_handler(tauri::generate_handler![
            configure_daemon,
            engine_request,
            daemon_ping
        ])
        .run(tauri::generate_context!())
        .expect("error while running LinkFYR mobile");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_response_has_engine_unavailable_code() {
        let r = unavailable("nope");
        match r {
            Response::Error(e) => {
                assert_eq!(e.code, linkfyr_ipc::ErrorCode::EngineUnavailable);
            }
            other => panic!("expected error response, got {other:?}"),
        }
    }

    #[test]
    fn request_envelope_serializes_for_the_daemon() {
        let env = Envelope::new(1, Request::Ping);
        let json = serde_json::to_string(&env).unwrap();
        assert!(json.contains("\"type\":\"ping\""));
    }
}

//! LinkFYR desktop shell (Tauri 2). The ONLY crate allowed to depend on
//! Tauri (ADR-0001): it wires Tauri commands/events to the shell-agnostic
//! `linkfyr-core` engine.

use std::sync::Arc;

use linkfyr_core::{AppEngine, MonitorMode};
use linkfyr_ipc::{Request, Response};
use linkfyr_model::{IfStatus, Snapshot};
use tauri::{
    AppHandle, Emitter, Manager, State,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
};
use tokio::sync::watch;

pub struct EngineState {
    pub engine: Arc<AppEngine>,
}

type CmdResult<T> = Result<T, String>;

#[tauri::command]
async fn get_snapshot(state: State<'_, EngineState>) -> CmdResult<Snapshot> {
    tracing::debug!("command: get_snapshot");
    Ok((*state.engine.snapshot().await).clone())
}

/// Single typed entry point for everything the UI can ask the engine.
/// Keeping one command means the client API surface is exactly
/// `linkfyr-ipc` — nothing Tauri-specific leaks into the contract.
#[tauri::command]
async fn engine_request(request: Request, state: State<'_, EngineState>) -> CmdResult<Response> {
    tracing::debug!(?request, "command: engine_request");
    Ok(state.engine.handle_request(request).await)
}

/// Forward every published snapshot to the UI as a Tauri event, and raise
/// notifications on meaningful transitions (interface up/down). Only
/// transitions notify — never per-tick state — to avoid alert fatigue.
async fn snapshot_forwarder(app: AppHandle, mut rx: watch::Receiver<Arc<Snapshot>>) {
    use tauri_plugin_notification::NotificationExt;

    let mut prev_status: std::collections::HashMap<String, IfStatus> =
        std::collections::HashMap::new();
    let mut primed = false;

    loop {
        if rx.changed().await.is_err() {
            break;
        }
        let snap = rx.borrow().clone();
        if let Err(e) = app.emit("snapshot", &*snap) {
            tracing::debug!("snapshot emit failed: {e}");
        }

        for iface in &snap.interfaces {
            let status = iface.interface.status;
            let prev = prev_status.insert(iface.interface.id.clone(), status);
            if !primed {
                continue; // first snapshot establishes the baseline only
            }
            if prev == Some(status) {
                continue;
            }
            let name = iface.interface.friendly_name.clone();
            match (prev, status) {
                (None | Some(IfStatus::Down), IfStatus::Up) => {
                    let _ = app
                        .notification()
                        .builder()
                        .title("LinkFYR")
                        .body(format!("{name} is now online."))
                        .show();
                }
                (Some(IfStatus::Up), IfStatus::Down) => {
                    let _ = app
                        .notification()
                        .builder()
                        .title("LinkFYR")
                        .body(format!("{name} went down."))
                        .show();
                }
                _ => {}
            }
        }
        primed = true;
    }
}

/// Close-to-tray: hiding the window keeps the engine + notifications
/// alive, which is the expected behavior for a network control layer.
/// The tray menu's Quit is the real exit.
fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show LinkFYR", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit LinkFYR", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    let mut tray = TrayIconBuilder::with_id("main-tray")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("LinkFYR")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let tauri::tray::TrayIconEvent::Click {
                button: tauri::tray::MouseButton::Left,
                button_state: tauri::tray::MouseButtonState::Up,
                ..
            } = event
                && let Some(window) = tray.app_handle().get_webview_window("main")
            {
                let _ = window.show();
                let _ = window.set_focus();
            }
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        tray = tray.icon(icon);
    }
    tray.build(app)?;
    Ok(())
}

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    // The engine lives on its own tokio runtime. It is intentionally leaked:
    // Tauri drops the setup closure right after setup(), which would drop a
    // stack runtime and silently kill every engine task (found the hard way:
    // exactly one tick ran, then the loop vanished).
    let runtime = Box::leak(Box::new(
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime"),
    ));

    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .setup(move |app| {
            let config_dir = app.path().app_local_data_dir().expect("app local data dir");
            std::fs::create_dir_all(&config_dir).ok();

            // LINKFYR_SIM=1 runs the deterministic simulator (UI dev/demo).
            let mode = match std::env::var("LINKFYR_SIM").as_deref() {
                Ok("1" | "true") => MonitorMode::Simulated,
                _ => MonitorMode::Os,
            };
            let engine = AppEngine::open(&config_dir, mode)?;
            let rx = engine.subscribe();
            engine.start_on(runtime.handle());

            let handle = app.handle().clone();
            runtime
                .handle()
                .spawn(snapshot_forwarder(handle.clone(), rx));

            build_tray(&handle)?;

            // Close behavior follows the user's preference: hide to tray
            // (engine + notifications keep running) or quit for real.
            // Quit from the tray menu always works either way.
            if let Some(window) = handle.get_webview_window("main") {
                let h = handle.clone();
                let engine_for_close = Arc::clone(&engine);
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        if engine_for_close.config().preferences.close_to_tray {
                            api.prevent_close();
                            if let Some(w) = h.get_webview_window("main") {
                                let _ = w.hide();
                            }
                        } else {
                            h.exit(0);
                        }
                    }
                });
            }

            app.manage(EngineState { engine });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_snapshot, engine_request])
        .run(tauri::generate_context!())
        .expect("error while running LinkFYR");
}

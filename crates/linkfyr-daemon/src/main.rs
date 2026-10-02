//! linkfyrd — the LinkFYR background service binary.
//!
//! v1 runs foreground (systemd/launchd manage it on unix; on Windows
//! `sc create linkfyrd binPath= "…\linkfyrd.exe" --service` uses the
//! SCM wrapper below). This is what keeps telemetry, optimization jobs,
//! and bridges alive while the desktop app is closed — the exam-mode
//! requirement: students close the GUI, the service keeps the network
//! configuration they already set up.

use linkfyr_daemon::{DEFAULT_BIND, DaemonOptions, serve};

fn config_dir() -> std::path::PathBuf {
    directories::ProjectDirs::from("app", "linkfyr", "linkfyr").map_or_else(
        || std::env::temp_dir().join("linkfyr"),
        |d| d.data_local_dir().to_path_buf(),
    )
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--service") && cfg!(windows) {
        #[cfg(windows)]
        {
            if let Err(e) = run_windows_service() {
                eprintln!("service error: {e}");
                std::process::exit(1);
            }
        }
        return;
    }

    let bind = args
        .iter()
        .position(|a| a == "--bind")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| DEFAULT_BIND.to_string());

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let opts = DaemonOptions {
        bind,
        config_dir: config_dir(),
    };
    let token_path = opts.token_path();
    match serve(opts).await {
        Ok(daemon) => {
            println!(
                "linkfyrd listening on {} (token: {})",
                daemon.local_addr,
                token_path.display()
            );
            daemon.join.await.expect("daemon task");
        }
        Err(e) => {
            eprintln!("linkfyrd: {e}");
            std::process::exit(1);
        }
    }
}

#[cfg(windows)]
fn run_windows_service() -> Result<(), String> {
    use windows_service::service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    };
    use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
    use windows_service::{define_windows_service, service_dispatcher};

    define_windows_service!(ffi_service_main, windows_service_main);

    fn windows_service_main(_args: Vec<std::ffi::OsString>) {
        if let Err(e) = run_service_body() {
            eprintln!("linkfyrd service: {e}");
        }
    }

    fn run_service_body() -> Result<(), String> {
        let (shutdown_tx, shutdown_rx) = std::sync::mpsc::channel::<()>();
        let event_handler = move |control| match control {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                let _ = shutdown_tx.send(());
                ServiceControlHandlerResult::NoError
            }
            _ => ServiceControlHandlerResult::NotImplemented,
        };
        let status_handle = service_control_handler::register("linkfyrd", event_handler)
            .map_err(|e| e.to_string())?;
        status_handle
            .set_service_status(ServiceStatus {
                service_type: ServiceType::OWN_PROCESS,
                current_state: ServiceState::Running,
                controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
                exit_code: ServiceExitCode::Win32(0),
                checkpoint: 0,
                wait_hint: std::time::Duration::default(),
                process_id: None,
            })
            .map_err(|e| e.to_string())?;

        let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
        let result = rt.block_on(async {
            let opts = DaemonOptions {
                bind: DEFAULT_BIND.into(),
                config_dir: config_dir(),
            };
            match serve(opts).await {
                Ok(daemon) => {
                    // Park until SCM says stop.
                    let _ = shutdown_rx.recv();
                    daemon.join.abort();
                    Ok(())
                }
                Err(e) => Err(e),
            }
        });

        status_handle
            .set_service_status(ServiceStatus {
                service_type: ServiceType::OWN_PROCESS,
                current_state: ServiceState::Stopped,
                controls_accepted: ServiceControlAccept::empty(),
                exit_code: ServiceExitCode::Win32(0),
                checkpoint: 0,
                wait_hint: std::time::Duration::default(),
                process_id: None,
            })
            .map_err(|e| e.to_string())?;
        result
    }

    service_dispatcher::start("linkfyrd", ffi_service_main).map_err(|e| e.to_string())
}

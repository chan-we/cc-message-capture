mod cert;
mod proxy;

use std::sync::{Arc, Mutex};
use tauri::Manager;
use tracing_subscriber::EnvFilter;

struct ProxyState {
    running: bool,
    process: Option<proxy::MitmdumpProcess>,
    port: u16,
}

#[derive(Default)]
struct AppState {
    proxy: Arc<Mutex<ProxyState>>,
}

impl Default for ProxyState {
    fn default() -> Self {
        Self {
            running: false,
            process: None,
            port: 9898,
        }
    }
}

#[derive(serde::Serialize)]
struct ProxyStatus {
    running: bool,
    port: u16,
}

#[tauri::command]
async fn start_proxy(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    port: u16,
) -> Result<(), String> {
    {
        let proxy_state = state.proxy.lock().unwrap();
        if proxy_state.running {
            return Err("Proxy is already running".to_string());
        }
    }

    // Resolve mitmdump binary path inside mitmproxy.app bundle
    let mitmdump_path = app
        .path()
        .resolve(
            "resources/mitmproxy.app/Contents/MacOS/mitmdump",
            tauri::path::BaseDirectory::Resource,
        )
        .map_err(|e| format!("Cannot find mitmdump binary: {}", e))?;

    if !mitmdump_path.exists() {
        return Err(format!(
            "mitmdump binary not found at: {}",
            mitmdump_path.display()
        ));
    }

    // Resolve addon script path
    let addon_path = app
        .path()
        .resolve("resources/addon_capture.py", tauri::path::BaseDirectory::Resource)
        .map_err(|e| format!("Cannot find addon script: {}", e))?;

    if !addon_path.exists() {
        return Err(format!(
            "Addon script not found at: {}",
            addon_path.display()
        ));
    }

    // Ensure mitmdump is allowed to execute on macOS (strip quarantine, ad-hoc sign)
    cert::ensure_executable(&mitmdump_path)?;

    let process =
        proxy::MitmdumpProcess::start(app.clone(), port, mitmdump_path, addon_path).await?;

    {
        let mut proxy_state = state.proxy.lock().unwrap();
        proxy_state.running = true;
        proxy_state.process = Some(process);
        proxy_state.port = port;
    }

    Ok(())
}

#[tauri::command]
async fn stop_proxy(state: tauri::State<'_, AppState>) -> Result<(), String> {
    // Take the process out of state first, then release lock before awaiting
    let process = {
        let mut proxy_state = state.proxy.lock().unwrap();
        if !proxy_state.running {
            return Err("Proxy is not running".to_string());
        }
        proxy_state.running = false;
        proxy_state.process.take()
    };

    if let Some(mut process) = process {
        process.stop().await?;
    }

    Ok(())
}

#[tauri::command]
async fn get_proxy_status(state: tauri::State<'_, AppState>) -> Result<ProxyStatus, String> {
    let proxy_state = state.proxy.lock().unwrap();
    Ok(ProxyStatus {
        running: proxy_state.running,
        port: proxy_state.port,
    })
}

#[tauri::command]
async fn export_ca_cert(dest_path: String) -> Result<String, String> {
    let pem = cert::get_ca_cert_pem()?;
    std::fs::write(&dest_path, &pem).map_err(|e| e.to_string())?;
    Ok(dest_path)
}

#[tauri::command]
async fn get_ca_cert_path() -> Result<String, String> {
    let path = cert::get_ca_cert_path();
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
async fn install_ca_cert(app: tauri::AppHandle) -> Result<String, String> {
    let mitmdump_path = app
        .path()
        .resolve(
            "resources/mitmproxy.app/Contents/MacOS/mitmdump",
            tauri::path::BaseDirectory::Resource,
        )
        .map_err(|e| format!("Cannot find mitmdump binary: {}", e))?;

    cert::install_ca_to_keychain(&mitmdump_path)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            start_proxy,
            stop_proxy,
            get_proxy_status,
            export_ca_cert,
            get_ca_cert_path,
            install_ca_cert,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

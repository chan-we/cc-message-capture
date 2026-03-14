mod cert;
mod proxy;

use std::sync::{Arc, Mutex};
use tauri::Manager;
use tracing_subscriber::EnvFilter;

/// Helper function to find mitmdump binary path
#[allow(unused_variables)]
fn get_mitmdump_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    #[cfg(target_os = "macos")]
    {
        // In dev mode, use the original source path to preserve code signatures.
        // Tauri copies resources to target/debug/resources/ which breaks macOS
        // code signatures on the embedded Python framework.
        #[cfg(debug_assertions)]
        {
            let dev_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("resources/mitmproxy.app/Contents/MacOS/mitmdump");
            if dev_path.exists() {
                return Ok(dev_path);
            }
        }

        app.path()
            .resolve(
                "resources/mitmproxy.app/Contents/MacOS/mitmdump",
                tauri::path::BaseDirectory::Resource,
            )
            .map_err(|e| format!("无法找到 mitmdump 二进制文件: {}", e))
    }

    #[cfg(target_os = "linux")]
    {
        let path_in_env = std::env::var("PATH")
            .ok()
            .and_then(|path| {
                path.split(':')
                    .map(|p| std::path::PathBuf::from(p).join("mitmdump"))
                    .find(|p| p.exists())
            });

        path_in_env.or_else(|| {
            let paths = [
                std::path::PathBuf::from("/usr/bin/mitmdump"),
                std::path::PathBuf::from("/usr/local/bin/mitmdump"),
                std::path::PathBuf::from("/snap/bin/mitmdump"),
            ];
            paths.into_iter().find(|p| p.exists())
        })
            .ok_or_else(|| "mitmdump 未在 PATH 或常见位置中找到".to_string())
    }

    #[cfg(target_os = "windows")]
    {
        let path_in_env = std::env::var("PATH")
            .ok()
            .and_then(|path| {
                path.split(';')
                    .map(|p| std::path::PathBuf::from(p).join("mitmdump.exe"))
                    .find(|p| p.exists())
            });

        path_in_env.or_else(|| {
            std::env::var("APPDATA")
                .ok()
                .map(|appdata| {
                    std::path::PathBuf::from(appdata)
                        .join("Python")
                        .join("Scripts")
                        .join("mitmdump.exe")
                })
                .filter(|p| p.exists())
        })
            .or_else(|| {
                std::env::var("USERPROFILE")
                    .ok()
                    .map(|userprofile| {
                        std::path::PathBuf::from(userprofile)
                            .join("AppData")
                            .join("Roaming")
                            .join("Python")
                            .join("Python311")
                            .join("Scripts")
                            .join("mitmdump.exe")
                    })
                    .filter(|p| p.exists())
            })
            .ok_or_else(|| "mitmdump 未在 PATH 或 Python Scripts 目录中找到".to_string())
    }
}

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
            return Err("代理已在运行".to_string());
        }
    }

    // Resolve mitmdump binary path
    let mitmdump_path = get_mitmdump_path(&app)?;
    if !mitmdump_path.exists() {
        return Err(format!(
            "未找到 mitmdump 二进制文件: {}",
            mitmdump_path.display()
        ));
    }

    // Resolve addon script path
    let addon_path = app
        .path()
        .resolve("resources/addon_capture.py", tauri::path::BaseDirectory::Resource)
        .map_err(|e| format!("无法找到插件脚本: {}", e))?;

    if !addon_path.exists() {
        return Err(format!(
            "未找到插件脚本: {}",
            addon_path.display()
        ));
    }

    // Ensure mitmdump is allowed to execute on macOS (strip quarantine, ad-hoc sign)
    #[cfg(target_os = "macos")]
    cert::ensure_executable(&mitmdump_path)?;

    #[cfg(not(target_os = "macos"))]
    let _ = mitmdump_path; // Silence unused variable warning

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
            return Err("代理未运行".to_string());
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
    // Generate cert first if needed
    let mitmdump_path = get_mitmdump_path(&app)?;
    tracing::info!("install_ca_cert: mitmdump_path={}, exists={}", mitmdump_path.display(), mitmdump_path.exists());
    cert::ensure_ca_cert(&mitmdump_path)?;

    cert::install_ca_to_keychain(&mitmdump_path)
}

#[tauri::command]
async fn check_cert_status() -> Result<cert::CertStatus, String> {
    Ok(cert::check_cert_installed())
}

#[tauri::command]
async fn uninstall_ca_cert() -> Result<String, String> {
    cert::uninstall_ca_cert()
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
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            start_proxy,
            stop_proxy,
            get_proxy_status,
            export_ca_cert,
            get_ca_cert_path,
            install_ca_cert,
            check_cert_status,
            uninstall_ca_cert,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

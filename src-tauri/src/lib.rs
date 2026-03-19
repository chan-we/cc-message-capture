mod cert;
mod download;
mod proxy;

use std::sync::{Arc, Mutex};
use tauri::menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::{Emitter, Manager};
use tracing_subscriber::EnvFilter;

const GITHUB_REPO: &str = "chan-we/cc-message-capture";

#[derive(serde::Serialize)]
struct ReleaseAsset {
    name: String,
    download_url: String,
    size: u64,
}

#[derive(serde::Serialize)]
struct UpdateInfo {
    has_update: bool,
    current_version: String,
    latest_version: String,
    release_url: String,
    release_notes: String,
    assets: Vec<ReleaseAsset>,
}

/// Helper function to find mitmdump binary path.
/// Priority: app_data_dir (downloaded) > platform-specific fallbacks.
#[allow(unused_variables)]
fn get_mitmdump_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    // Priority 1: Check downloaded binary in app_data_dir
    if let Ok(Some(path)) = download::installed_mitmdump_path(app) {
        return Ok(path);
    }

    // Priority 2: Platform-specific fallbacks
    #[cfg(target_os = "macos")]
    {
        // In dev mode, check local resources for convenience
        #[cfg(debug_assertions)]
        {
            let dev_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("resources/mitmproxy.app/Contents/MacOS/mitmdump");
            if dev_path.exists() {
                return Ok(dev_path);
            }
        }

        Err("mitmdump 未安装，请点击启动代理以自动下载".to_string())
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
            .ok_or_else(|| "mitmdump 未安装，请点击启动代理以自动下载".to_string())
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
            .ok_or_else(|| "mitmdump 未安装，请点击启动代理以自动下载".to_string())
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

    // Kill any leftover mitmdump from a previous crash/restart
    proxy::kill_leftover_mitmdump(port);

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
async fn check_mitmdump(app: tauri::AppHandle) -> Result<bool, String> {
    let path = download::installed_mitmdump_path(&app)?;
    Ok(path.is_some())
}

#[tauri::command]
async fn download_mitmdump(app: tauri::AppHandle) -> Result<String, String> {
    let path = download::download_and_extract(app).await?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
async fn uninstall_mitmdump(app: tauri::AppHandle) -> Result<(), String> {
    download::uninstall(&app)
}

#[tauri::command]
async fn cancel_download() -> Result<(), String> {
    download::cancel_download();
    Ok(())
}

#[tauri::command]
async fn get_app_version(app: tauri::AppHandle) -> Result<String, String> {
    let version = app.config().version.clone().unwrap_or_default();
    Ok(version)
}

#[tauri::command]
async fn check_update(app: tauri::AppHandle) -> Result<UpdateInfo, String> {
    let current_version = app.config().version.clone().unwrap_or_default();

    let client = reqwest::Client::builder()
        .user_agent("cc-message-capture")
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

    let url = format!("https://api.github.com/repos/{}/releases/latest", GITHUB_REPO);
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("请求 GitHub API 失败: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("GitHub API 返回错误: {}", resp.status()));
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("解析响应失败: {}", e))?;

    let latest_version = json["tag_name"]
        .as_str()
        .unwrap_or("")
        .trim_start_matches('v')
        .to_string();

    let release_url = json["html_url"].as_str().unwrap_or("").to_string();
    let release_notes = json["body"].as_str().unwrap_or("").to_string();

    let assets = json["assets"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|a| {
                    let name = a["name"].as_str()?.to_string();
                    // Only include installable assets
                    if name.ends_with(".dmg")
                        || name.ends_with(".msi")
                        || name.ends_with(".exe")
                        || name.ends_with(".deb")
                        || name.ends_with(".AppImage")
                    {
                        Some(ReleaseAsset {
                            name,
                            download_url: a["browser_download_url"].as_str()?.to_string(),
                            size: a["size"].as_u64().unwrap_or(0),
                        })
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    let has_update = !latest_version.is_empty() && latest_version != current_version;

    Ok(UpdateInfo {
        has_update,
        current_version,
        latest_version,
        release_url,
        release_notes,
        assets,
    })
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
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let about_item = MenuItemBuilder::with_id("about", "关于 CC Message Capture")
                .build(app)?;
            let app_submenu = SubmenuBuilder::new(app, "CC Message Capture")
                .item(&about_item)
                .separator()
                .quit()
                .build()?;
            let edit_submenu = SubmenuBuilder::new(app, "编辑")
                .undo()
                .redo()
                .separator()
                .cut()
                .copy()
                .paste()
                .select_all()
                .build()?;
            let menu = MenuBuilder::new(app)
                .item(&app_submenu)
                .item(&edit_submenu)
                .build()?;
            app.set_menu(menu)?;

            let app_handle = app.handle().clone();
            app.on_menu_event(move |_window, event| {
                if event.id() == "about" {
                    let _ = app_handle.emit("menu-about", ());
                }
            });
            Ok(())
        })
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
            check_mitmdump,
            download_mitmdump,
            uninstall_mitmdump,
            cancel_download,
            get_app_version,
            check_update,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

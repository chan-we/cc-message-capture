use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager};

const MITMPROXY_VERSION: &str = "12.2.1";
const MITMDUMP_DIR_NAME: &str = "mitmdump";

#[derive(Clone, serde::Serialize)]
pub struct DownloadProgress {
    pub downloaded: u64,
    pub total: u64,
    pub stage: String,
}

/// Returns the install directory: `app_data_dir/mitmdump/`
pub fn mitmdump_install_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("无法获取 app_data_dir: {}", e))?
        .join(MITMDUMP_DIR_NAME);
    Ok(dir)
}

/// Returns the path to the installed mitmdump binary if it exists and version matches.
pub fn installed_mitmdump_path(app: &AppHandle) -> Result<Option<PathBuf>, String> {
    let dir = mitmdump_install_dir(app)?;
    let binary_path = mitmdump_binary_path(&dir);
    let version_path = dir.join(".version");

    if !binary_path.exists() {
        return Ok(None);
    }

    // Check version match
    match std::fs::read_to_string(&version_path) {
        Ok(ver) if ver.trim() == MITMPROXY_VERSION => Ok(Some(binary_path)),
        _ => {
            // Version mismatch or no version file — treat as not installed
            Ok(None)
        }
    }
}

/// Returns the expected mitmdump binary path within the install directory.
fn mitmdump_binary_path(dir: &PathBuf) -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        dir.join("mitmproxy.app/Contents/MacOS/mitmdump")
    }
    #[cfg(target_os = "linux")]
    {
        dir.join("mitmdump")
    }
    #[cfg(target_os = "windows")]
    {
        dir.join("mitmdump.exe")
    }
}

/// Constructs the download URL from snapshots.mitmproxy.org for the current platform.
fn download_url() -> Result<String, String> {
    let archive_name = platform_archive_name()?;
    Ok(format!(
        "https://snapshots.mitmproxy.org/{}/{}",
        MITMPROXY_VERSION, archive_name
    ))
}

fn platform_archive_name() -> Result<String, String> {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        Ok(format!("mitmproxy-{}-macos-arm64.tar.gz", MITMPROXY_VERSION))
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        Ok(format!("mitmproxy-{}-macos-x86_64.tar.gz", MITMPROXY_VERSION))
    }
    #[cfg(target_os = "linux")]
    {
        Ok(format!(
            "mitmproxy-{}-linux-x86_64.tar.gz",
            MITMPROXY_VERSION
        ))
    }
    #[cfg(target_os = "windows")]
    {
        Ok(format!(
            "mitmproxy-{}-windows-x86_64.zip",
            MITMPROXY_VERSION
        ))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        Err("不支持的操作系统".to_string())
    }
}

/// Uninstalls mitmdump by removing the install directory.
pub fn uninstall(app: &AppHandle) -> Result<(), String> {
    let dir = mitmdump_install_dir(app)?;
    if dir.exists() {
        std::fs::remove_dir_all(&dir)
            .map_err(|e| format!("删除 mitmdump 目录失败: {}", e))?;
        tracing::info!("mitmdump 已卸载: {}", dir.display());
    }
    Ok(())
}

/// Downloads and extracts mitmdump, emitting progress events.
pub async fn download_and_extract(app: AppHandle) -> Result<PathBuf, String> {
    let url = download_url()?;
    let dir = mitmdump_install_dir(&app)?;

    // Create install directory
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("创建目录失败: {}", e))?;

    let temp_file = dir.join(".downloading");

    tracing::info!("开始下载 mitmdump: {}", url);

    // Stream download
    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("下载请求失败: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("下载失败，HTTP 状态: {}", response.status()));
    }

    let total = response.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;

    // Download to temp file
    use futures_util::StreamExt;
    let mut stream = response.bytes_stream();
    let mut file_data = Vec::with_capacity(total as usize);

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("下载中断: {}", e))?;
        file_data.extend_from_slice(&chunk);
        downloaded += chunk.len() as u64;

        let _ = app.emit(
            "mitmdump-download-progress",
            DownloadProgress {
                downloaded,
                total,
                stage: "downloading".to_string(),
            },
        );
    }

    // Write to temp file
    std::fs::write(&temp_file, &file_data)
        .map_err(|e| format!("写入临时文件失败: {}", e))?;

    tracing::info!("下载完成，开始解压");

    let _ = app.emit(
        "mitmdump-download-progress",
        DownloadProgress {
            downloaded: total,
            total,
            stage: "extracting".to_string(),
        },
    );

    // Extract archive contents
    extract_archive(&dir, &file_data)?;
    let binary_path = mitmdump_binary_path(&dir);

    // Write version file
    std::fs::write(dir.join(".version"), MITMPROXY_VERSION)
        .map_err(|e| format!("写入版本文件失败: {}", e))?;

    // Clean up temp file
    let _ = std::fs::remove_file(&temp_file);

    tracing::info!("mitmdump 安装完成: {}", binary_path.display());

    Ok(binary_path)
}

/// Extracts the archive contents to the install directory.
/// macOS: extracts the full mitmproxy.app bundle (contains Python framework).
/// Linux: extracts only the mitmdump binary.
#[cfg(not(target_os = "windows"))]
fn extract_archive(dir: &PathBuf, data: &[u8]) -> Result<(), String> {
    use flate2::read::GzDecoder;
    use tar::Archive;

    let decoder = GzDecoder::new(data);
    let mut archive = Archive::new(decoder);

    #[cfg(target_os = "macos")]
    {
        // macOS: extract the entire mitmproxy.app bundle
        archive
            .unpack(dir)
            .map_err(|e| format!("解压归档失败: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        // Linux: extract only the mitmdump binary
        use std::io::Read;
        let binary_name = "mitmdump";

        for entry in archive
            .entries()
            .map_err(|e| format!("读取 tar 归档失败: {}", e))?
        {
            let mut entry = entry.map_err(|e| format!("读取 tar 条目失败: {}", e))?;
            let path = entry
                .path()
                .map_err(|e| format!("读取条目路径失败: {}", e))?;

            if path.file_name().map_or(false, |n| n == binary_name) {
                let mut content = Vec::new();
                entry
                    .read_to_end(&mut content)
                    .map_err(|e| format!("读取 mitmdump 内容失败: {}", e))?;
                let dest = dir.join(binary_name);
                std::fs::write(&dest, &content)
                    .map_err(|e| format!("写入 mitmdump 失败: {}", e))?;

                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755))
                    .map_err(|e| format!("设置权限失败: {}", e))?;

                return Ok(());
            }
        }

        return Err("归档中未找到 mitmdump".to_string());
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn extract_archive(dir: &PathBuf, data: &[u8]) -> Result<(), String> {
    use std::io::{Cursor, Read};

    let reader = Cursor::new(data);
    let mut archive =
        zip::ZipArchive::new(reader).map_err(|e| format!("读取 zip 归档失败: {}", e))?;

    let binary_name = "mitmdump.exe";

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| format!("读取 zip 条目失败: {}", e))?;

        if file
            .enclosed_name()
            .and_then(|p| p.file_name())
            .map_or(false, |n| n == binary_name)
        {
            let mut content = Vec::new();
            file.read_to_end(&mut content)
                .map_err(|e| format!("读取 mitmdump.exe 内容失败: {}", e))?;
            let dest = dir.join(binary_name);
            std::fs::write(&dest, &content)
                .map_err(|e| format!("写入 mitmdump.exe 失败: {}", e))?;

            return Ok(());
        }
    }

    Err("归档中未找到 mitmdump.exe".to_string())
}

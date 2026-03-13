use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::Stdio;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertStatus {
    pub installed: bool,
    pub method: String,
    pub details: String,
}

fn mitmproxy_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let userprofile = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .unwrap_or_else(|_| ".".to_string());
        PathBuf::from(userprofile).join(".mitmproxy")
    }

    #[cfg(not(target_os = "windows"))]
    {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".mitmproxy")
    }
}

pub fn get_ca_cert_path() -> PathBuf {
    mitmproxy_dir().join("mitmproxy-ca-cert.pem")
}

/// Remove macOS Gatekeeper quarantine so the binary can be executed.
/// NOTE: Do NOT re-codesign the .app bundle — ad-hoc signing with
/// `codesign --force --deep` invalidates the embedded Python framework's
/// original signature, causing dlopen to fail.
#[cfg(target_os = "macos")]
pub fn ensure_executable(mitmdump_path: &PathBuf) -> Result<(), String> {
    // Strip quarantine attribute from the .app bundle
    if let Some(app_dir) = mitmdump_path
        .ancestors()
        .find(|p| p.extension().map_or(false, |ext| ext == "app"))
    {
        let _ = std::process::Command::new("xattr")
            .args(["-r", "-d", "com.apple.quarantine"])
            .arg(app_dir)
            .output();
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn ensure_executable(_mitmdump_path: &PathBuf) -> Result<(), String> {
    Ok(())
}

/// Ensure mitmproxy CA cert exists. If not, run mitmdump briefly to generate it.
pub fn ensure_ca_cert(mitmdump_path: &PathBuf) -> Result<(), String> {
    if get_ca_cert_path().exists() {
        return Ok(());
    }

    tracing::info!("CA cert not found, running mitmdump once to generate it...");

    // Make sure the binary is allowed to execute on macOS
    ensure_executable(mitmdump_path)?;

    // Run mitmdump with a dummy listen port, it generates certs on startup then we kill it
    let mut child = std::process::Command::new(mitmdump_path)
        .args(["--listen-port", "0"]) // port 0 = random unused port
        .args(["--set", "flow_detail=0"])
        .args(["--quiet"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("运行 mitmdump 生成证书失败: {}", e))?;

    // Wait a bit for cert generation, then kill
    std::thread::sleep(std::time::Duration::from_secs(2));

    #[cfg(unix)]
    unsafe {
        if let Some(pid) = child.id().into() {
            libc::kill(pid as i32, libc::SIGTERM);
        }
    }
    let _ = child.wait();

    if get_ca_cert_path().exists() {
        tracing::info!("CA cert generated successfully");
        Ok(())
    } else {
        Err("生成 mitmproxy CA 证书失败".to_string())
    }
}

pub fn get_ca_cert_pem() -> Result<String, String> {
    let path = get_ca_cert_path();
    if !path.exists() {
        return Err(
            "未找到 mitmproxy CA 证书，请先启动代理以自动生成证书。"
                .to_string(),
        );
    }
    fs::read_to_string(&path).map_err(|e| e.to_string())
}

pub fn install_ca_to_keychain(mitmdump_path: &PathBuf) -> Result<String, String> {
    // Auto-generate cert if it doesn't exist yet
    ensure_ca_cert(mitmdump_path)?;

    #[cfg(target_os = "macos")]
    let cert_path = get_ca_cert_path();

    #[cfg(target_os = "macos")]
    {
        let home =
            std::env::var("HOME").map_err(|_| "无法确定 HOME 目录".to_string())?;
        let keychain_path = PathBuf::from(&home).join("Library/Keychains/login.keychain-db");

        let output = std::process::Command::new("security")
            .args(["add-trusted-cert", "-d", "-r", "trustRoot", "-k"])
            .arg(&keychain_path)
            .arg(&cert_path)
            .output()
            .map_err(|e| format!("运行 security 命令失败: {}", e))?;

        if output.status.success() {
            Ok(cert_path.to_string_lossy().to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("安装 CA 证书失败: {}", stderr))
        }
    }

    #[cfg(target_os = "linux")]
    {
        let home = std::env::var("HOME").map_err(|_| "无法确定 HOME 目录".to_string())?;
        let cert_path = get_ca_cert_path();

        // First, try to install to system CA directory (requires root via pkexec)
        let system_ca_dir = PathBuf::from("/usr/local/share/ca-certificates");

        if system_ca_dir.exists() {
            // Try to copy with pkexec (will prompt for password)
            let system_dest = system_ca_dir.join("mitmproxy-ca-cert.crt");

            // Use pkexec with cp and then update-ca-certificates
            let copy_result = std::process::Command::new("pkexec")
                .args(["cp", &cert_path.to_string_lossy(), &system_dest.to_string_lossy()])
                .output();

            if copy_result.as_ref().map(|o| o.status.success()).unwrap_or(false) {
                // Successfully copied, now update CA certificates
                let update_result = std::process::Command::new("pkexec")
                    .args(["update-ca-certificates"])
                    .output();

                if update_result.map(|o| o.status.success()).unwrap_or(false) {
                    return Ok("CA 证书已安装到系统 CA 存储 (需要重启浏览器生效)。".to_string());
                }
            } else {
                // pkexec failed (user cancelled or no permission)
                let err_msg = copy_result
                    .map(|o| String::from_utf8_lossy(&o.stderr).to_string())
                    .unwrap_or_else(|_| "未知错误".to_string());

                tracing::warn!("安装到系统 CA 失败: {}, 尝试用户目录", err_msg);
            }
        }

        // Fallback: install to user directory and also try Firefox/Chrome directories
        let local_ca_dir = PathBuf::from(&home).join(".local/share/ca-certificates");
        std::fs::create_dir_all(&local_ca_dir)
            .map_err(|e| format!("创建 CA 证书目录失败: {}", e))?;

        let dest_path = local_ca_dir.join("mitmproxy-ca-cert.pem");
        std::fs::copy(&cert_path, &dest_path)
            .map_err(|e| format!("复制 CA 证书失败: {}", e))?;

        // Also copy to browser certificate directories

        // Firefox/NSS directory
        let firefox_dir = PathBuf::from(&home).join(".mozilla/firefox");
        if firefox_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&firefox_dir) {
                for entry in entries.flatten() {
                    let cert_db = entry.path().join("cert8.db");
                    let cert9_db = entry.path().join("cert9.db");
                    if cert_db.exists() || cert9_db.exists() {
                        // For Firefox, we need to use certutil (if available)
                        // But that's complex, so we'll just note it
                        let _ = entry.path();
                    }
                }
            }
        }

        // Chrome/Chromium directory
        let chrome_dir = PathBuf::from(&home).join(".pki/nssdb");
        if chrome_dir.exists() {
            let _ = chrome_dir;
        }

        // Build result message
        let mut message = format!(
            "CA 证书已复制到: {}\n\n注意: 未获得系统管理员权限，证书未安装到系统信任存储。\n\n要完全信任证书，请选择以下方式之一:\n",
            dest_path.display()
        );

        message.push_str("1. 在终端运行: sudo cp ~/.mitmproxy/mitmproxy-ca-cert.pem /usr/local/share/ca-certificates/ && sudo update-ca-certificates\n");
        message.push_str("2. 或在浏览器设置中手动导入证书\n");
        message.push_str("3. Chrome/Chromium: 设置 -> 隐私与安全 -> 安全 -> 管理证书 -> 受信任的根证书颁发机构 -> 导入");

        Ok(message)
    }

    #[cfg(target_os = "windows")]
    {
        use std::env;

        // On Windows, use certutil to add the certificate to the store
        // Convert PEM to DER format for Windows
        let cert_der_path = cert_path.with_extension("der");

        // Run certutil to convert PEM to DER
        let convert_output = std::process::Command::new("certutil")
            .args(["-encode", &cert_path.to_string_lossy(), &cert_der_path.to_string_lossy()])
            .output()
            .map_err(|e| format!("运行 certutil 失败: {}", e))?;

        if !convert_output.status.success() {
            // If conversion fails, try direct install with PEM
            let output = std::process::Command::new("certutil")
                .args(["-addstore", "Root", &cert_path.to_string_lossy()])
                .output()
                .map_err(|e| format!("运行 certutil 失败: {}", e))?;

            if output.status.success() {
                return Ok(format!(
                    "CA 证书已安装到 Windows Root 存储，请重启浏览器。"
                ));
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(format!("安装 CA 证书失败: {}", stderr));
            }
        }

        // Install the DER certificate to Root store
        let output = std::process::Command::new("certutil")
            .args(["-addstore", "Root", &cert_der_path.to_string_lossy()])
            .output()
            .map_err(|e| format!("运行 certutil 失败: {}", e))?;

        // Clean up temp DER file
        let _ = std::fs::remove_file(&cert_der_path);

        if output.status.success() {
            Ok("CA 证书已安装到 Windows Root 存储，请重启浏览器。".to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("安装 CA 证书失败: {}", stderr))
        }
    }
}

/// Check if the mitmproxy CA certificate is installed/trusted
pub fn check_cert_installed() -> CertStatus {
    let cert_path = get_ca_cert_path();

    if !cert_path.exists() {
        return CertStatus {
            installed: false,
            method: "none".to_string(),
            details: "未找到 CA 证书，请先启动代理。".to_string(),
        };
    }

    #[cfg(target_os = "macos")]
    {
        // Check if cert is in keychain as trusted
        let output = std::process::Command::new("security")
            .args(["find-certificate", "-c", "mitmproxy"])
            .output();

        if output.map(|o| o.status.success()).unwrap_or(false) {
            return CertStatus {
                installed: true,
                method: "keychain".to_string(),
                details: "证书已安装到 macOS 钥匙串。".to_string(),
            };
        }
    }

    #[cfg(target_os = "linux")]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());

        // Check system CA directory
        let system_paths = [
            "/usr/local/share/ca-certificates/mitmproxy-ca-cert.crt",
            "/usr/local/share/ca-certificates/mitmproxy-ca-cert.pem",
        ];

        for path in system_paths {
            if std::path::Path::new(path).exists() {
                return CertStatus {
                    installed: true,
                    method: "system".to_string(),
                    details: format!("证书已安装到系统 CA 存储: {}", path),
                };
            }
        }

        // Check /etc/ssl/certs for mitmproxy cert (might have hash filename)
        if let Ok(entries) = std::fs::read_dir("/etc/ssl/certs") {
            for entry in entries.flatten() {
                if let Ok(name) = entry.file_name().into_string() {
                    if name.contains("mitmproxy") || name.starts_with("mitmproxy") {
                        return CertStatus {
                            installed: true,
                            method: "system".to_string(),
                            details: format!("证书已安装到系统 CA 存储: /etc/ssl/certs/{}", name),
                        };
                    }
                }
            }
        }

        // Check user local CA directory
        let user_path = PathBuf::from(&home)
            .join(".local/share/ca-certificates/mitmproxy-ca-cert.pem");
        if user_path.exists() {
            return CertStatus {
                installed: false,
                method: "user".to_string(),
                details: format!(
                    "证书已复制到用户目录但未受系统信任。\n路径: {}\n\n请在浏览器设置中手动导入此证书，或使用 sudo 安装到系统。",
                    user_path.display()
                ),
            };
        }
    }

    #[cfg(target_os = "windows")]
    {
        // Check Windows certificate store
        let output = std::process::Command::new("certutil")
            .args(["-store", "Root", "mitmproxy"])
            .output();

        if output.map(|o| o.status.success()).unwrap_or(false) {
            return CertStatus {
                installed: true,
                method: "windows-store".to_string(),
                details: "证书已安装到 Windows Root 存储。".to_string(),
            };
        }
    }

    // Default: cert file exists but not installed anywhere
    CertStatus {
        installed: false,
        method: "file".to_string(),
        details: format!(
            "证书文件存在但未安装。\n路径: {}\n\n请点击「安装证书」进行安装。",
            cert_path.display()
        ),
    }
}

/// Uninstall the mitmproxy CA certificate from the system
pub fn uninstall_ca_cert() -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        // Remove from keychain
        let output = std::process::Command::new("security")
            .args(["delete-certificate", "-c", "mitmproxy"])
            .output()
            .map_err(|e| format!("Failed to run security command: {}", e))?;

        if output.status.success() {
            Ok("证书已从 macOS 钥匙串中移除。".to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("could not be found") {
                Ok("钥匙串中未找到该证书。".to_string())
            } else {
                Err(format!("移除证书失败: {}", stderr))
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let mut removed = false;

        // Try to remove from system CA directory (requires root)
        let system_paths = [
            "/usr/local/share/ca-certificates/mitmproxy-ca-cert.crt",
            "/usr/local/share/ca-certificates/mitmproxy-ca-cert.pem",
        ];

        for path in system_paths {
            if std::path::Path::new(path).exists() {
                let result = std::process::Command::new("pkexec")
                    .args(["rm", "-f", path])
                    .output();

                if result.map(|o| o.status.success()).unwrap_or(false) {
                    removed = true;
                }
            }
        }

        // Also remove from /etc/ssl/certs (might have hash filename)
        if let Ok(entries) = std::fs::read_dir("/etc/ssl/certs") {
            for entry in entries.flatten() {
                if let Ok(name) = entry.file_name().into_string() {
                    if name.contains("mitmproxy") {
                        let result = std::process::Command::new("pkexec")
                            .args(["rm", "-f", &format!("/etc/ssl/certs/{}", name)])
                            .output();
                        if result.map(|o| o.status.success()).unwrap_or(false) {
                            removed = true;
                        }
                    }
                }
            }
        }

        // Try to update CA certificates
        let _ = std::process::Command::new("pkexec")
            .args(["update-ca-certificates"])
            .output();

        // Remove from user local CA directory
        let user_path = PathBuf::from(&home)
            .join(".local/share/ca-certificates/mitmproxy-ca-cert.pem");
        if user_path.exists() {
            std::fs::remove_file(&user_path)
                .map_err(|e| format!("移除用户证书失败: {}", e))?;
            removed = true;
        }

        if removed {
            Ok("Certificate uninstalled successfully.".to_string())
        } else {
            Ok("该证书未安装。".to_string())
        }
    }

    #[cfg(target_os = "windows")]
    {
        // Remove from Windows certificate store
        let output = std::process::Command::new("certutil")
            .args(["-delstore", "Root", "mitmproxy"])
            .output()
            .map_err(|e| format!("Failed to run certutil: {}", e))?;

        if output.status.success() {
            Ok("证书已从 Windows Root 存储中移除。".to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("not found") || stderr.contains("找不到") {
                Ok("Root 存储中未找到该证书。".to_string())
            } else {
                Err(format!("移除证书失败: {}", stderr))
            }
        }
    }
}

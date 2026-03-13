use std::fs;
use std::path::PathBuf;
use std::process::Stdio;

fn mitmproxy_dir() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".mitmproxy")
}

pub fn get_ca_cert_path() -> PathBuf {
    mitmproxy_dir().join("mitmproxy-ca-cert.pem")
}

/// Remove macOS Gatekeeper quarantine so the binary can be executed.
/// NOTE: Do NOT re-codesign the .app bundle — ad-hoc signing with
/// `codesign --force --deep` invalidates the embedded Python framework's
/// original signature, causing dlopen to fail.
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
        .map_err(|e| format!("Failed to run mitmdump for cert generation: {}", e))?;

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
        Err("Failed to generate mitmproxy CA certificate".to_string())
    }
}

pub fn get_ca_cert_pem() -> Result<String, String> {
    let path = get_ca_cert_path();
    if !path.exists() {
        return Err(
            "mitmproxy CA certificate not found. Please start the proxy once to auto-generate it."
                .to_string(),
        );
    }
    fs::read_to_string(&path).map_err(|e| e.to_string())
}

pub fn install_ca_to_keychain(mitmdump_path: &PathBuf) -> Result<String, String> {
    // Auto-generate cert if it doesn't exist yet
    ensure_ca_cert(mitmdump_path)?;

    let cert_path = get_ca_cert_path();

    let home =
        std::env::var("HOME").map_err(|_| "Cannot determine HOME directory".to_string())?;
    let keychain_path = PathBuf::from(&home).join("Library/Keychains/login.keychain-db");

    let output = std::process::Command::new("security")
        .args(["add-trusted-cert", "-d", "-r", "trustRoot", "-k"])
        .arg(&keychain_path)
        .arg(&cert_path)
        .output()
        .map_err(|e| format!("Failed to run security command: {}", e))?;

    if output.status.success() {
        Ok(cert_path.to_string_lossy().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("Failed to install CA cert: {}", stderr))
    }
}

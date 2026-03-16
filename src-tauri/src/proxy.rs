use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use tauri::Emitter;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapturedMessage {
    pub id: String,
    pub timestamp: String,
    pub method: String,
    pub url: String,
    pub request_headers: HashMap<String, String>,
    pub request_body: String,
    pub status: u16,
    pub response_headers: HashMap<String, String>,
    pub response_body: String,
    pub duration_ms: i64,
}

pub struct MitmdumpProcess {
    child: Child,
}

/// Kill any leftover mitmdump processes listening on the given port.
/// This handles the case where the app was killed/crashed but mitmdump survived.
pub fn kill_leftover_mitmdump(port: u16) {
    #[cfg(unix)]
    {
        // Use lsof to find processes listening on the port
        let output = std::process::Command::new("lsof")
            .args(["-ti", &format!(":{}", port)])
            .output();

        if let Ok(output) = output {
            let pids = String::from_utf8_lossy(&output.stdout);
            for pid_str in pids.split_whitespace() {
                if let Ok(pid) = pid_str.trim().parse::<i32>() {
                    // Verify it's actually a mitmdump process before killing
                    let ps_output = std::process::Command::new("ps")
                        .args(["-p", &pid.to_string(), "-o", "comm="])
                        .output();
                    if let Ok(ps_out) = ps_output {
                        let comm = String::from_utf8_lossy(&ps_out.stdout);
                        if comm.contains("mitmdump") {
                            tracing::warn!(
                                "Killing leftover mitmdump process (pid={}) on port {}",
                                pid, port
                            );
                            unsafe { libc::kill(pid, libc::SIGTERM); }
                            // Give it a moment to exit
                            std::thread::sleep(std::time::Duration::from_millis(500));
                        }
                    }
                }
            }
        }
    }

    #[cfg(windows)]
    {
        // Use netstat to find PIDs on the port, then taskkill if it's mitmdump
        let output = std::process::Command::new("netstat")
            .args(["-ano"])
            .output();

        if let Ok(output) = output {
            let text = String::from_utf8_lossy(&output.stdout);
            let port_str = format!(":{}", port);
            for line in text.lines() {
                if line.contains(&port_str) && line.contains("LISTENING") {
                    if let Some(pid_str) = line.split_whitespace().last() {
                        if let Ok(pid) = pid_str.parse::<u32>() {
                            // Check if it's mitmdump
                            let tasklist = std::process::Command::new("tasklist")
                                .args(["/FI", &format!("PID eq {}", pid), "/FO", "CSV", "/NH"])
                                .output();
                            if let Ok(tl) = tasklist {
                                let name = String::from_utf8_lossy(&tl.stdout);
                                if name.contains("mitmdump") {
                                    tracing::warn!(
                                        "Killing leftover mitmdump process (pid={}) on port {}",
                                        pid, port
                                    );
                                    let _ = std::process::Command::new("taskkill")
                                        .args(["/PID", &pid.to_string(), "/F"])
                                        .output();
                                    std::thread::sleep(std::time::Duration::from_millis(500));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

impl MitmdumpProcess {
    pub async fn start(
        app_handle: tauri::AppHandle,
        port: u16,
        mitmdump_path: PathBuf,
        addon_path: PathBuf,
    ) -> Result<Self, String> {
        tracing::info!("mitmdump_path: {}", mitmdump_path.display());
        tracing::info!("addon_path: {}", addon_path.display());

        let mut cmd = Command::new(&mitmdump_path);
        cmd.args(["--listen-port", &port.to_string()])
            .args(["--set", "flow_detail=0"])
            .args(["--quiet"])
            .args(["-s", addon_path.to_str().ok_or("无效的插件路径")?])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("启动 mitmdump 失败: {}", e))?;

        // Wait briefly and check if mitmdump is still running
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        match child.try_wait() {
            Ok(Some(status)) => {
                // Read stderr for error details
                let mut stderr_output = String::new();
                if let Some(mut stderr) = child.stderr.take() {
                    use tokio::io::AsyncReadExt;
                    let _ = stderr.read_to_string(&mut stderr_output).await;
                }
                let mut stdout_output = String::new();
                if let Some(mut stdout) = child.stdout.take() {
                    use tokio::io::AsyncReadExt;
                    let _ = stdout.read_to_string(&mut stdout_output).await;
                }
                return Err(format!(
                    "mitmdump exited immediately with status: {}\nstderr: {}\nstdout: {}",
                    status, stderr_output, stdout_output
                ));
            }
            Ok(None) => {
                tracing::info!("mitmdump process started successfully (pid: {:?})", child.id());
            }
            Err(e) => {
                return Err(format!("检查 mitmdump 状态失败: {}", e));
            }
        }

        let stdout = child
            .stdout
            .take()
            .ok_or("无法捕获 mitmdump 标准输出")?;

        let stderr = child
            .stderr
            .take()
            .ok_or("无法捕获 mitmdump 标准错误")?;

        // Spawn task to read stdout (JSON Lines from addon)
        let app_for_stdout = app_handle.clone();
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();

            while let Ok(Some(line)) = lines.next_line().await {
                if !line.starts_with('{') {
                    tracing::warn!("mitmdump stdout: {}", line);
                    continue;
                }

                match serde_json::from_str::<CapturedMessage>(&line) {
                    Ok(msg) => {
                        let _ = app_for_stdout.emit("captured-message", &msg);
                    }
                    Err(e) => {
                        tracing::warn!("解析 mitmdump 输出失败: {} | {}", e, &line[..line.len().min(200)]);
                    }
                }
            }

            tracing::info!("mitmdump stdout reader finished");
        });

        // Spawn task to read stderr (log/error messages)
        tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();

            while let Ok(Some(line)) = lines.next_line().await {
                tracing::warn!("mitmdump stderr: {}", line);
            }
        });

        Ok(Self { child })
    }

    pub async fn stop(&mut self) -> Result<(), String> {
        // Try graceful shutdown first via SIGTERM
        #[cfg(unix)]
        {
            if let Some(pid) = self.child.id() {
                unsafe {
                    libc::kill(pid as i32, libc::SIGTERM);
                }
                // Wait briefly for graceful shutdown
                match tokio::time::timeout(
                    std::time::Duration::from_secs(3),
                    self.child.wait(),
                )
                .await
                {
                    Ok(Ok(_)) => return Ok(()),
                    _ => {
                        tracing::warn!("mitmdump did not exit gracefully, force killing");
                    }
                }
            }
        }

        // Fallback: force kill
        self.child
            .kill()
            .await
            .map_err(|e| format!("终止 mitmdump 失败: {}", e))?;

        Ok(())
    }
}

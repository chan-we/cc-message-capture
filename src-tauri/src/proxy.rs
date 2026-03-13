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

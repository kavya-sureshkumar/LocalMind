use crate::{binaries, hardware, models};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LlamaSettings {
    pub model_id: String,
    pub context_size: Option<u32>,
    pub n_gpu_layers: Option<i32>,
    pub threads: Option<u32>,
    pub port: Option<u16>,
    pub mmproj_id: Option<String>,
    pub flash_attn: Option<bool>,
    /// Comma-separated `host:port` Synapse workers (Phase 1: manual entry).
    /// When set, llama-server is launched with `--rpc <list>` so layers are
    /// pipeline-sharded across this host plus each worker.
    pub synapse_workers: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlamaStatus {
    pub running: bool,
    pub port: u16,
    pub model_id: Option<String>,
    pub mmproj_id: Option<String>,
    pub pid: Option<u32>,
    pub embedding_running: bool,
    pub embedding_port: u16,
    pub embedding_model_id: Option<String>,
}

struct ServerHandle {
    child: Option<Child>,
    port: u16,
    model_id: Option<String>,
    mmproj_id: Option<String>,
}

impl ServerHandle {
    fn new(default_port: u16) -> Self {
        Self {
            child: None,
            port: default_port,
            model_id: None,
            mmproj_id: None,
        }
    }
}

pub struct LlamaState {
    chat: Mutex<ServerHandle>,
    embed: Mutex<ServerHandle>,
}

impl LlamaState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            chat: Mutex::new(ServerHandle::new(8181)),
            embed: Mutex::new(ServerHandle::new(8182)),
        })
    }

    pub async fn status(&self) -> LlamaStatus {
        let chat = self.chat.lock().await;
        let embed = self.embed.lock().await;
        LlamaStatus {
            running: chat.child.is_some(),
            port: chat.port,
            model_id: chat.model_id.clone(),
            mmproj_id: chat.mmproj_id.clone(),
            pid: chat.child.as_ref().and_then(|c| c.id()),
            embedding_running: embed.child.is_some(),
            embedding_port: embed.port,
            embedding_model_id: embed.model_id.clone(),
        }
    }

    pub async fn embedding_port(&self) -> u16 {
        self.embed.lock().await.port
    }

    pub async fn embedding_running(&self) -> bool {
        self.embed.lock().await.child.is_some()
    }

    pub async fn stop(&self) -> Result<()> {
        let mut chat = self.chat.lock().await;
        if let Some(mut c) = chat.child.take() {
            let _ = c.kill().await;
        }
        chat.model_id = None;
        chat.mmproj_id = None;
        Ok(())
    }

    pub async fn stop_embedding(&self) -> Result<()> {
        let mut embed = self.embed.lock().await;
        if let Some(mut c) = embed.child.take() {
            let _ = c.kill().await;
        }
        embed.model_id = None;
        Ok(())
    }

    pub async fn start(&self, app: &AppHandle, settings: LlamaSettings) -> Result<LlamaStatus> {
        self.stop().await?;
        let port = settings.port.unwrap_or(8181);
        kill_orphan_on_port(port).await;

        let binary = binaries::ensure_llama_server(app).await?;
        let model = models::model_path(&settings.model_id)?;
        let hw = hardware::detect();

        let ctx = settings.context_size.unwrap_or(4096);
        let n_gpu = settings.n_gpu_layers.unwrap_or(hw.recommended_n_gpu_layers);
        let threads = settings
            .threads
            .unwrap_or((hw.cpu_cores as u32).saturating_sub(1).max(1));

        let mut cmd = Command::new(&binary);
        cmd.arg("-m").arg(&model);
        cmd.arg("--host").arg("127.0.0.1");
        cmd.arg("--port").arg(port.to_string());
        cmd.arg("-c").arg(ctx.to_string());
        cmd.arg("-t").arg(threads.to_string());
        cmd.arg("-ngl").arg(n_gpu.to_string());
        cmd.arg("--jinja");
        cmd.arg("-fa").arg(if settings.flash_attn.unwrap_or(true) {
            "on"
        } else {
            "off"
        });
        // Synapse: pipeline-shard layers across remote rpc-server workers.
        // Validated upstream (frontend trims/filters), but we also drop empties
        // here so a stray comma can't turn into "--rpc ,host:port".
        if let Some(workers) = &settings.synapse_workers {
            let csv = workers
                .iter()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(",");
            if !csv.is_empty() {
                cmd.arg("--rpc").arg(&csv);
            }
        }

        let mut loaded_mmproj: Option<String> = None;
        if let Some(mmproj_id) = &settings.mmproj_id {
            if let Ok(p) = models::model_path(mmproj_id) {
                cmd.arg("--mmproj").arg(p);
                loaded_mmproj = Some(mmproj_id.clone());
            }
        }
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| anyhow!("failed to spawn llama-server: {e}"))?;
        pipe_output(app, &mut child, "chat");

        {
            let mut chat = self.chat.lock().await;
            chat.child = Some(child);
            chat.port = port;
            chat.model_id = Some(settings.model_id.clone());
            chat.mmproj_id = loaded_mmproj;
        }

        wait_ready(port).await?;

        let _ = app.emit(
            "llama:ready",
            serde_json::json!({ "port": port, "modelId": settings.model_id, "stream": "chat" }),
        );

        Ok(self.status().await)
    }

    pub async fn start_embedding(&self, app: &AppHandle, model_id: String) -> Result<LlamaStatus> {
        self.stop_embedding().await?;
        let port = 8182;
        kill_orphan_on_port(port).await;

        let binary = binaries::ensure_llama_server(app).await?;
        let model = models::model_path(&model_id)?;
        let hw = hardware::detect();
        let threads = (hw.cpu_cores as u32).saturating_sub(1).max(1);
        let n_gpu = hw.recommended_n_gpu_layers;

        let mut cmd = Command::new(&binary);
        cmd.arg("-m").arg(&model);
        cmd.arg("--host").arg("127.0.0.1");
        cmd.arg("--port").arg(port.to_string());
        cmd.arg("-t").arg(threads.to_string());
        cmd.arg("-ngl").arg(n_gpu.to_string());
        cmd.arg("--embeddings");
        cmd.arg("--pooling").arg("mean");
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| anyhow!("failed to spawn embedding server: {e}"))?;
        pipe_output(app, &mut child, "embed");

        {
            let mut embed = self.embed.lock().await;
            embed.child = Some(child);
            embed.port = port;
            embed.model_id = Some(model_id.clone());
        }

        wait_ready(port).await?;

        let _ = app.emit(
            "llama:ready",
            serde_json::json!({ "port": port, "modelId": model_id, "stream": "embed" }),
        );

        Ok(self.status().await)
    }
}

fn pipe_output(app: &AppHandle, child: &mut Child, tag: &str) {
    if let Some(stdout) = child.stdout.take() {
        let app = app.clone();
        let tag = tag.to_string();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = app.emit(
                    "llama:log",
                    serde_json::json!({ "stream": "stdout", "line": line, "tag": tag }),
                );
            }
        });
    }
    if let Some(stderr) = child.stderr.take() {
        let app = app.clone();
        let tag = tag.to_string();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = app.emit(
                    "llama:log",
                    serde_json::json!({ "stream": "stderr", "line": line, "tag": tag }),
                );
            }
        });
    }
}

pub(crate) async fn kill_orphan_on_port(port: u16) {
    #[cfg(target_family = "unix")]
    {
        let out = tokio::process::Command::new("lsof")
            .arg("-t")
            .arg(format!("-iTCP:{}", port))
            .arg("-sTCP:LISTEN")
            .output()
            .await;
        if let Ok(o) = out {
            for pid in String::from_utf8_lossy(&o.stdout).lines() {
                let pid = pid.trim();
                if pid.is_empty() {
                    continue;
                }
                let _ = tokio::process::Command::new("kill")
                    .arg("-9")
                    .arg(pid)
                    .output()
                    .await;
            }
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(out) = tokio::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "Get-NetTCPConnection -LocalPort {} -ErrorAction SilentlyContinue | Select-Object -Expand OwningProcess",
                    port
                ),
            ])
            .output()
            .await
        {
            for pid in String::from_utf8_lossy(&out.stdout).lines() {
                let pid = pid.trim();
                if pid.is_empty() { continue; }
                let _ = tokio::process::Command::new("taskkill")
                    .args(["/F", "/PID", pid])
                    .output()
                    .await;
            }
        }
    }
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
}

async fn wait_ready(port: u16) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/health", port);
    // Allow up to ~3 minutes: first-time Metal shader compilation can take 10s+,
    // and large models may take additional time to mmap and warm up.
    for _ in 0..360 {
        if let Ok(r) = client.get(&url).send().await {
            if r.status().is_success() {
                return Ok(());
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    Err(anyhow!(
        "llama-server did not become ready on port {port} within 3 min"
    ))
}

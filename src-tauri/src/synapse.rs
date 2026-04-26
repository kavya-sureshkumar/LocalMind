// Phase 1 Synapse: each machine can run an `rpc-server` child process that
// llama.cpp on a remote host reaches via `--rpc host:port`. Together the
// machines form a pipeline-parallel inference cluster. Discovery, pairing, and
// auto-split come in later phases — for now we just expose start/stop/status
// for the worker, and let the host paste worker addresses manually.
use crate::{binaries, config};
use anyhow::{anyhow, Result};
use serde::Serialize;
use std::process::Stdio;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

pub const DEFAULT_WORKER_PORT: u16 = 50052;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SynapseWorkerStatus {
    pub running: bool,
    pub port: u16,
    pub pid: Option<u32>,
}

struct WorkerHandle {
    child: Option<Child>,
    port: u16,
}

pub struct SynapseState {
    worker: Mutex<WorkerHandle>,
}

impl SynapseState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            worker: Mutex::new(WorkerHandle {
                child: None,
                port: DEFAULT_WORKER_PORT,
            }),
        })
    }

    pub async fn status(&self) -> SynapseWorkerStatus {
        let w = self.worker.lock().await;
        SynapseWorkerStatus {
            running: w.child.is_some(),
            port: w.port,
            pid: w.child.as_ref().and_then(|c| c.id()),
        }
    }

    pub async fn stop_worker(&self) -> Result<()> {
        let mut w = self.worker.lock().await;
        if let Some(mut c) = w.child.take() {
            let _ = c.kill().await;
        }
        Ok(())
    }

    pub async fn start_worker(
        &self,
        app: &AppHandle,
        port: Option<u16>,
    ) -> Result<SynapseWorkerStatus> {
        // Make sure the llama.cpp bundle is unpacked — `rpc-server` ships in
        // the same archive as `llama-server`, so we trigger that download here
        // if the user toggles worker mode before they've ever loaded a model.
        binaries::ensure_llama_server(app).await?;

        self.stop_worker().await?;
        let port = port.unwrap_or(DEFAULT_WORKER_PORT);
        crate::llama::kill_orphan_on_port(port).await;

        let binary = config::rpc_server_path();
        if !binary.exists() {
            return Err(anyhow!(
                "rpc-server binary not found at {} — is the bundled llama.cpp build missing RPC support?",
                binary.display()
            ));
        }

        // Bind to 0.0.0.0 so other machines on the LAN can connect; the host
        // typed our address into its workers field. There's no auth on the RPC
        // wire (Phase 2 will fix that with an auth shim) — only run worker
        // mode on networks you control.
        let mut cmd = Command::new(&binary);
        cmd.arg("-H").arg("0.0.0.0");
        cmd.arg("-p").arg(port.to_string());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| anyhow!("failed to spawn rpc-server: {e}"))?;
        pipe_output(app, &mut child);

        {
            let mut w = self.worker.lock().await;
            w.child = Some(child);
            w.port = port;
        }

        let _ = app.emit("synapse:ready", serde_json::json!({ "port": port }));

        Ok(self.status().await)
    }
}

fn pipe_output(app: &AppHandle, child: &mut Child) {
    if let Some(stdout) = child.stdout.take() {
        let app = app.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = app.emit(
                    "synapse:log",
                    serde_json::json!({ "stream": "stdout", "line": line }),
                );
            }
        });
    }
    if let Some(stderr) = child.stderr.take() {
        let app = app.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = app.emit(
                    "synapse:log",
                    serde_json::json!({ "stream": "stderr", "line": line }),
                );
            }
        });
    }
}

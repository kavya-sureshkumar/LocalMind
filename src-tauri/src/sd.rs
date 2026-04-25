use crate::{binaries, config, models};
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::Mutex;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SdRequest {
    pub model_id: String,
    pub prompt: String,
    pub negative_prompt: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub steps: Option<u32>,
    pub cfg_scale: Option<f32>,
    pub seed: Option<i64>,
    pub sampler: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SdImage {
    pub id: String,
    pub path: String,
    pub prompt: String,
    pub model_id: String,
    pub width: u32,
    pub height: u32,
    pub seed: i64,
    pub created_at: i64,
}

pub struct SdState {
    busy: Mutex<bool>,
}

impl SdState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { busy: Mutex::new(false) })
    }

    pub async fn is_busy(&self) -> bool {
        *self.busy.lock().await
    }

    pub async fn generate(&self, app: &AppHandle, req: SdRequest) -> Result<SdImage> {
        {
            let mut busy = self.busy.lock().await;
            if *busy {
                return Err(anyhow!("another image is already being generated"));
            }
            *busy = true;
        }

        let result = self.run(app, req).await;

        let mut busy = self.busy.lock().await;
        *busy = false;

        result
    }

    async fn run(&self, app: &AppHandle, req: SdRequest) -> Result<SdImage> {
        let bin = binaries::ensure_sd(app).await.context("stable-diffusion.cpp not available")?;

        let installed = models::list_installed().context("listing installed models")?;
        let model = installed
            .into_iter()
            .find(|m| m.id == req.model_id)
            .ok_or_else(|| anyhow!("model not found: {}", req.model_id))?;

        let model_path = PathBuf::from(&model.path);
        if !model_path.exists() {
            return Err(anyhow!("model file missing: {}", model_path.display()));
        }

        let id = uuid::Uuid::new_v4().to_string();
        let out_path = config::sd_output_dir().join(format!("{}.png", id));

        let width = req.width.unwrap_or(512).clamp(64, 2048);
        let height = req.height.unwrap_or(512).clamp(64, 2048);
        let steps = req.steps.unwrap_or(20).clamp(1, 150);
        let cfg = req.cfg_scale.unwrap_or(7.0);
        let seed = req.seed.unwrap_or(-1);
        let sampler = req.sampler.as_deref().unwrap_or("euler_a");

        emit_progress(app, &id, "starting", 0, steps, "Loading model");

        let mut cmd = tokio::process::Command::new(&bin);
        cmd.arg("-m").arg(&model_path)
            .arg("-p").arg(&req.prompt)
            .arg("-o").arg(&out_path)
            .arg("-W").arg(width.to_string())
            .arg("-H").arg(height.to_string())
            .arg("--steps").arg(steps.to_string())
            .arg("--cfg-scale").arg(format!("{:.2}", cfg))
            .arg("-s").arg(seed.to_string())
            .arg("--sampling-method").arg(sampler);

        if let Some(neg) = req.negative_prompt.as_deref().filter(|s| !s.is_empty()) {
            cmd.arg("-n").arg(neg);
        }

        cmd.stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let mut child = cmd.spawn().context("spawning sd binary")?;
        let stdout = child.stdout.take().ok_or_else(|| anyhow!("no stdout from sd"))?;
        let stderr = child.stderr.take().ok_or_else(|| anyhow!("no stderr from sd"))?;

        let app_stdout = app.clone();
        let app_stderr = app.clone();
        let id_stdout = id.clone();
        let id_stderr = id.clone();

        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                parse_and_emit(&app_stdout, &id_stdout, &line, steps);
            }
        });

        tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                parse_and_emit(&app_stderr, &id_stderr, &line, steps);
            }
        });

        let status = child.wait().await.context("waiting for sd")?;
        if !status.success() {
            return Err(anyhow!("stable-diffusion.cpp exited with {}", status));
        }

        if !out_path.exists() {
            return Err(anyhow!("sd did not produce output image at {}", out_path.display()));
        }

        emit_progress(app, &id, "done", steps, steps, "Complete");

        Ok(SdImage {
            id,
            path: out_path.to_string_lossy().into_owned(),
            prompt: req.prompt,
            model_id: req.model_id,
            width,
            height,
            seed,
            created_at: chrono::Utc::now().timestamp(),
        })
    }
}

fn parse_and_emit(app: &AppHandle, id: &str, line: &str, total_steps: u32) {
    // stable-diffusion.cpp prints lines like "  5/20 - 2.34s/it"
    let trimmed = line.trim_start();
    if let Some((a, _)) = trimmed.split_once('/') {
        if let Ok(cur) = a.parse::<u32>() {
            if cur <= total_steps && cur > 0 {
                emit_progress(app, id, "sampling", cur, total_steps, trimmed);
                return;
            }
        }
    }
    if line.contains("decode") || line.to_lowercase().contains("vae") {
        emit_progress(app, id, "decoding", total_steps, total_steps, "Decoding image");
    }
}

fn emit_progress(app: &AppHandle, id: &str, stage: &str, step: u32, total: u32, message: &str) {
    #[derive(Serialize, Clone)]
    #[serde(rename_all = "camelCase")]
    struct Progress<'a> {
        id: &'a str,
        stage: &'a str,
        step: u32,
        total: u32,
        message: &'a str,
    }
    let _ = app.emit(
        "sd:progress",
        Progress { id, stage, step, total, message },
    );
}

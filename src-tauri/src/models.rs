use crate::config;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelListing {
    pub id: String,
    pub name: String,
    pub author: String,
    pub downloads: u64,
    pub likes: u64,
    pub tags: Vec<String>,
    pub updated: Option<String>,
    pub description: Option<String>,
    pub files: Vec<ModelFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelFile {
    pub filename: String,
    pub size_bytes: u64,
    pub quantization: String,
    pub download_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledModel {
    pub id: String,
    pub filename: String,
    pub repo: String,
    pub size_bytes: u64,
    pub path: String,
    pub kind: ModelKind,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ModelKind {
    Llm,
    Vision,
    Mmproj,
    Embedding,
    Whisper,
    Sd,
}

#[derive(Clone, Serialize)]
pub struct ModelDownloadProgress {
    pub id: String,
    pub downloaded: u64,
    pub total: u64,
    pub percent: f64,
    pub stage: String,
}

pub async fn search_huggingface(query: &str, limit: u32) -> Result<Vec<ModelListing>> {
    let client = reqwest::Client::builder()
        .user_agent("LocalMind/0.1")
        .build()?;

    let url = format!(
        "https://huggingface.co/api/models?search={}&filter=gguf&limit={}&sort=downloads&direction=-1",
        urlencode(query),
        limit
    );

    let raw: Vec<serde_json::Value> = client.get(&url).send().await?.error_for_status()?.json().await?;

    let ids: Vec<String> = raw
        .iter()
        .filter_map(|m| m["id"].as_str().map(String::from))
        .take(limit as usize)
        .collect();

    let trees = fetch_trees(&client, &ids).await;

    let mut out = Vec::new();
    for m in raw.iter().take(limit as usize) {
        let id = m["id"].as_str().unwrap_or("").to_string();
        if id.is_empty() { continue; }
        let (author, name) = id.split_once('/').unwrap_or(("unknown", id.as_str()));
        let tree = trees.get(&id).cloned().unwrap_or_default();
        let files: Vec<ModelFile> = tree
            .iter()
            .filter_map(|s| {
                let fname = s["path"].as_str()?;
                if !fname.to_lowercase().ends_with(".gguf") { return None; }
                let quant = infer_quant(fname);
                let size = s["lfs"]["size"].as_u64().or_else(|| s["size"].as_u64()).unwrap_or(0);
                Some(ModelFile {
                    filename: fname.to_string(),
                    size_bytes: size,
                    quantization: quant,
                    download_url: format!("https://huggingface.co/{}/resolve/main/{}", id, fname),
                })
            })
            .collect();

        if files.is_empty() { continue; }

        out.push(ModelListing {
            id: id.clone(),
            name: name.to_string(),
            author: author.to_string(),
            downloads: m["downloads"].as_u64().unwrap_or(0),
            likes: m["likes"].as_u64().unwrap_or(0),
            tags: m["tags"]
                .as_array()
                .map(|a| a.iter().filter_map(|t| t.as_str().map(String::from)).collect())
                .unwrap_or_default(),
            updated: m["lastModified"].as_str().map(String::from),
            description: m["description"].as_str().map(String::from),
            files,
        });
    }

    Ok(out)
}

async fn fetch_trees(
    client: &reqwest::Client,
    ids: &[String],
) -> std::collections::HashMap<String, Vec<serde_json::Value>> {
    use futures_util::future::join_all;
    let futs = ids.iter().map(|id| {
        let client = client.clone();
        let id = id.clone();
        async move {
            let url = format!("https://huggingface.co/api/models/{}/tree/main?recursive=false", id);
            let res: Vec<serde_json::Value> = match client.get(&url).send().await {
                Ok(r) => r.json().await.unwrap_or_default(),
                Err(_) => Vec::new(),
            };
            (id, res)
        }
    });
    join_all(futs).await.into_iter().collect()
}

pub async fn download_model(
    app: &AppHandle,
    repo: &str,
    filename: &str,
    kind: ModelKind,
) -> Result<InstalledModel> {
    let url = format!("https://huggingface.co/{}/resolve/main/{}", repo, filename);
    let dest = config::models_dir().join(safe_name(repo, filename));

    if dest.exists() {
        let size = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
        return Ok(InstalledModel {
            id: safe_name(repo, filename),
            filename: filename.to_string(),
            repo: repo.to_string(),
            size_bytes: size,
            path: dest.to_string_lossy().into_owned(),
            kind,
        });
    }

    let client = reqwest::Client::builder()
        .user_agent("LocalMind/0.1")
        .build()?;

    let mut response = client.get(&url).send().await?.error_for_status()?;
    let total = response.content_length().unwrap_or(0);

    let tmp = dest.with_extension("part");
    let mut file = tokio::fs::File::create(&tmp).await?;
    let mut downloaded: u64 = 0;
    let id = safe_name(repo, filename);

    emit_progress(app, &id, 0, total, "downloading");

    while let Some(chunk) = response.chunk().await? {
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        if downloaded % (4 * 1024 * 1024) < chunk.len() as u64 {
            emit_progress(app, &id, downloaded, total, "downloading");
        }
    }
    file.flush().await?;
    drop(file);
    tokio::fs::rename(&tmp, &dest).await?;

    emit_progress(app, &id, downloaded, total, "ready");

    Ok(InstalledModel {
        id,
        filename: filename.to_string(),
        repo: repo.to_string(),
        size_bytes: downloaded,
        path: dest.to_string_lossy().into_owned(),
        kind,
    })
}

pub fn list_installed() -> Result<Vec<InstalledModel>> {
    let dir = config::models_dir();
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let disk_name = path.file_name().unwrap().to_string_lossy().to_string();
        if !disk_name.to_lowercase().ends_with(".gguf") {
            continue;
        }
        let size = entry.metadata()?.len();
        let (repo, filename) = parse_safe_name(&disk_name);
        let kind = infer_kind_with_repo(&filename, &repo);
        out.push(InstalledModel {
            id: disk_name.clone(),
            filename,
            repo,
            size_bytes: size,
            path: path.to_string_lossy().into_owned(),
            kind,
        });
    }
    Ok(out)
}

fn parse_safe_name(disk_name: &str) -> (String, String) {
    let mut parts = disk_name.splitn(3, "__");
    let p1 = parts.next().unwrap_or("");
    match (parts.next(), parts.next()) {
        (Some(name), Some(filename)) if !p1.is_empty() && !name.is_empty() => {
            (format!("{}/{}", p1, name), filename.to_string())
        }
        _ => ("local".to_string(), disk_name.to_string()),
    }
}

pub fn delete_model(id: &str) -> Result<()> {
    let dir = config::models_dir();
    let path = dir.join(id);
    if !path.starts_with(&dir) {
        return Err(anyhow!("invalid path"));
    }
    std::fs::remove_file(path)?;
    Ok(())
}

pub fn model_path(id: &str) -> Result<PathBuf> {
    let path = config::models_dir().join(id);
    if !path.exists() {
        return Err(anyhow!("model not found: {}", id));
    }
    Ok(path)
}

fn infer_quant(fname: &str) -> String {
    let lower = fname.to_lowercase();
    for q in [
        "q2_k", "q3_k_s", "q3_k_m", "q3_k_l", "q4_0", "q4_1", "q4_k_s", "q4_k_m",
        "q5_0", "q5_1", "q5_k_s", "q5_k_m", "q6_k", "q8_0", "f16", "bf16", "f32",
        "iq1_s", "iq1_m", "iq2_xxs", "iq2_xs", "iq2_s", "iq2_m", "iq3_xxs", "iq3_s",
        "iq3_m", "iq3_xs", "iq4_nl", "iq4_xs",
    ] {
        if lower.contains(q) {
            return q.to_uppercase();
        }
    }
    "UNKNOWN".to_string()
}

fn infer_kind_with_repo(fname: &str, repo: &str) -> ModelKind {
    let l = fname.to_lowercase();
    let r = repo.to_lowercase();
    if l.contains("mmproj") {
        ModelKind::Mmproj
    } else if l.contains("llava") || l.contains("vision") || l.contains("-vl-")
        || r.contains("llava") || r.contains("vision") || r.contains("-vl-")
    {
        ModelKind::Vision
    } else if l.contains("embed") || l.contains("bge-") || l.contains("nomic") {
        ModelKind::Embedding
    } else if l.contains("whisper") {
        ModelKind::Whisper
    } else if l.contains("stable-diffusion") || l.contains("sdxl") || l.contains("sd1.5") {
        ModelKind::Sd
    } else {
        ModelKind::Llm
    }
}

fn safe_name(repo: &str, filename: &str) -> String {
    let r = repo.replace('/', "__");
    format!("{}__{}", r, filename)
}

fn emit_progress(app: &AppHandle, id: &str, downloaded: u64, total: u64, stage: &str) {
    let percent = if total > 0 { downloaded as f64 / total as f64 * 100.0 } else { 0.0 };
    let _ = app.emit(
        "model:progress",
        ModelDownloadProgress {
            id: id.to_string(),
            downloaded,
            total,
            percent,
            stage: stage.to_string(),
        },
    );
}

fn urlencode(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            ' ' => "+".to_string(),
            _ => format!("%{:02X}", c as u32),
        })
        .collect()
}

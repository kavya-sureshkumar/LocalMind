use crate::config;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub id: String,
    pub name: String,
    pub source_path: Option<String>,
    pub created_at: i64,
    pub chunk_count: usize,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chunk {
    pub id: String,
    pub doc_id: String,
    pub doc_name: String,
    pub content: String,
    pub ordinal: u32,
    #[serde(default)]
    pub embedding: Vec<f32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrievedChunk {
    pub chunk: Chunk,
    pub score: f32,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct RagStore {
    documents: Vec<Document>,
    chunks: Vec<Chunk>,
}

pub struct RagState {
    store: RwLock<RagStore>,
    path: PathBuf,
}

impl RagState {
    pub fn new() -> Arc<Self> {
        let path = config::data_dir().join("rag.json");
        let store = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Arc::new(Self {
            store: RwLock::new(store),
            path,
        })
    }

    async fn save(&self) -> Result<()> {
        let store = self.store.read().await;
        let tmp = self.path.with_extension("tmp");
        let json = serde_json::to_vec_pretty(&*store)?;
        tokio::fs::write(&tmp, json).await?;
        tokio::fs::rename(&tmp, &self.path).await?;
        Ok(())
    }

    pub async fn list(&self) -> Vec<Document> {
        self.store.read().await.documents.clone()
    }

    pub async fn delete(&self, doc_id: &str) -> Result<()> {
        {
            let mut store = self.store.write().await;
            store.documents.retain(|d| d.id != doc_id);
            store.chunks.retain(|c| c.doc_id != doc_id);
        }
        self.save().await
    }

    pub async fn ingest(&self, path: &Path, embedder: &Embedder) -> Result<Document> {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "document".into());
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        let bytes_metadata = std::fs::metadata(path)?.len();
        let text = match ext.as_str() {
            "txt" | "md" | "markdown" | "text" | "log" | "json" | "csv" | "xml" | "html"
            | "htm" => std::fs::read_to_string(path)?,
            _ => {
                return Err(anyhow!(
                    "unsupported file type: {}. Use .txt, .md, .csv, or .json",
                    ext
                ))
            }
        };

        let chunks = chunk_text(&text, 700, 80);
        if chunks.is_empty() {
            return Err(anyhow!("no content in file"));
        }

        let embeddings = embedder.embed_batch(&chunks).await?;

        let doc_id = Uuid::new_v4().to_string();
        let created_at = chrono::Utc::now().timestamp();
        let doc = Document {
            id: doc_id.clone(),
            name: name.clone(),
            source_path: Some(path.to_string_lossy().into_owned()),
            created_at,
            chunk_count: chunks.len(),
            bytes: bytes_metadata,
        };

        let chunk_records: Vec<Chunk> = chunks
            .iter()
            .zip(embeddings.iter())
            .enumerate()
            .map(|(i, (text, embedding))| Chunk {
                id: Uuid::new_v4().to_string(),
                doc_id: doc_id.clone(),
                doc_name: name.clone(),
                content: text.clone(),
                ordinal: i as u32,
                embedding: embedding.clone(),
            })
            .collect();

        {
            let mut store = self.store.write().await;
            store.documents.push(doc.clone());
            store.chunks.extend(chunk_records);
        }
        self.save().await?;
        Ok(doc)
    }

    pub async fn retrieve(
        &self,
        query_embedding: &[f32],
        top_k: usize,
        doc_ids: Option<&[String]>,
    ) -> Vec<RetrievedChunk> {
        let store = self.store.read().await;
        let mut scored: Vec<(f32, &Chunk)> = store
            .chunks
            .iter()
            .filter(|c| match doc_ids {
                Some(ids) if !ids.is_empty() => ids.contains(&c.doc_id),
                _ => true,
            })
            .filter(|c| !c.embedding.is_empty())
            .map(|c| (cosine(query_embedding, &c.embedding), c))
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored
            .into_iter()
            .take(top_k)
            .map(|(score, chunk)| RetrievedChunk {
                chunk: chunk.clone(),
                score,
            })
            .collect()
    }
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0f32;
    let mut na = 0f32;
    let mut nb = 0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    let denom = (na.sqrt() * nb.sqrt()).max(1e-8);
    dot / denom
}

fn chunk_text(text: &str, target: usize, overlap: usize) -> Vec<String> {
    let clean = text.replace("\r\n", "\n");
    let paragraphs: Vec<&str> = clean
        .split("\n\n")
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect();

    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    for p in paragraphs {
        if current.chars().count() + p.chars().count() + 2 <= target {
            if !current.is_empty() {
                current.push('\n');
                current.push('\n');
            }
            current.push_str(p);
        } else {
            if !current.is_empty() {
                out.push(current.clone());
            }
            if p.chars().count() > target {
                out.extend(split_by_chars(p, target, overlap));
                current = String::new();
            } else {
                current = p.to_string();
            }
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

fn split_by_chars(text: &str, target: usize, overlap: usize) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let end = (i + target).min(chars.len());
        out.push(chars[i..end].iter().collect::<String>());
        if end == chars.len() {
            break;
        }
        i = end.saturating_sub(overlap);
    }
    out
}

pub struct Embedder {
    pub port: u16,
    pub client: reqwest::Client,
}

impl Embedder {
    pub fn new(port: u16) -> Self {
        Self {
            port,
            client: reqwest::Client::new(),
        }
    }

    pub async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let mut out = Vec::with_capacity(texts.len());
        for t in texts {
            out.push(self.embed_one(t).await?);
        }
        Ok(out)
    }

    pub async fn embed_one(&self, text: &str) -> Result<Vec<f32>> {
        let url = format!("http://127.0.0.1:{}/v1/embeddings", self.port);
        let body = serde_json::json!({
            "input": text,
            "model": "embedding",
        });
        let resp = self.client.post(&url).json(&body).send().await?;
        if !resp.status().is_success() {
            return Err(anyhow!("embedding server error: {}", resp.status()));
        }
        let json: serde_json::Value = resp.json().await?;
        let emb = json["data"][0]["embedding"]
            .as_array()
            .ok_or_else(|| anyhow!("bad embedding response"))?;
        Ok(emb
            .iter()
            .filter_map(|v| v.as_f64().map(|f| f as f32))
            .collect())
    }
}

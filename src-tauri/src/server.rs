use crate::llama::LlamaState;
use anyhow::Result;
use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{any, get},
    Json, Router,
};
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::{cors::CorsLayer, services::ServeDir};

#[derive(Clone)]
pub struct AppState {
    pub llama: Arc<LlamaState>,
    pub http: reqwest::Client,
}

pub async fn start_lan_server(
    llama: Arc<LlamaState>,
    static_dir: Option<PathBuf>,
    port: u16,
) -> Result<String> {
    let state = AppState {
        llama,
        http: reqwest::Client::new(),
    };

    let mut app = Router::new()
        .route("/api/health", get(health))
        .route("/api/status", get(status))
        .route("/v1/*rest", any(proxy_v1))
        .route("/health", get(proxy_health))
        .nest_service("/sd-images", ServeDir::new(crate::config::sd_output_dir()));

    if let Some(dir) = static_dir {
        app = app.fallback_service(ServeDir::new(dir).append_index_html_on_directories(true));
    } else {
        app = app.fallback(get(|| async { "LocalMind API" }));
    }

    let app = app.layer(CorsLayer::permissive()).with_state(state);

    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    let bound = listener.local_addr()?;

    let ip = local_ip_address::local_ip()
        .map(|i| i.to_string())
        .unwrap_or_else(|_| "127.0.0.1".into());

    let url = format!("http://{}:{}", ip, bound.port());

    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            eprintln!("LAN server error: {e}");
        }
    });

    Ok(url)
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "ok": true, "service": "LocalMind" }))
}

async fn status(State(s): State<AppState>) -> Json<serde_json::Value> {
    let st = s.llama.status().await;
    Json(serde_json::to_value(&st).unwrap())
}

async fn proxy_health(State(s): State<AppState>) -> Response {
    let port = s.llama.status().await.port;
    let url = format!("http://127.0.0.1:{}/health", port);
    forward(&s.http, &url, Request::builder().uri("/").body(Body::empty()).unwrap()).await
}

async fn proxy_v1(State(s): State<AppState>, req: Request<Body>) -> Response {
    let port = s.llama.status().await.port;
    if !s.llama.status().await.running {
        return (StatusCode::SERVICE_UNAVAILABLE, "no model loaded").into_response();
    }
    let path = req.uri().path().to_string();
    let qs = req.uri().query().map(|q| format!("?{q}")).unwrap_or_default();
    let url = format!("http://127.0.0.1:{}{}{}", port, path, qs);
    forward(&s.http, &url, req).await
}

async fn forward(client: &reqwest::Client, url: &str, req: Request<Body>) -> Response {
    let method = req.method().clone();
    let headers = req.headers().clone();
    let body_bytes = match axum::body::to_bytes(req.into_body(), 64 * 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => return (StatusCode::BAD_REQUEST, format!("body error: {e}")).into_response(),
    };

    let mut builder = client.request(method, url);
    for (k, v) in headers.iter() {
        if k == "host" || k == "content-length" { continue; }
        builder = builder.header(k, v);
    }
    builder = builder.body(body_bytes.to_vec());

    let upstream = match builder.send().await {
        Ok(r) => r,
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("upstream error: {e}")).into_response(),
    };

    let status = upstream.status();
    let mut rheaders = HeaderMap::new();
    for (k, v) in upstream.headers().iter() {
        rheaders.insert(k.clone(), v.clone());
    }
    let stream = upstream.bytes_stream();
    let body = Body::from_stream(stream);

    let mut resp = Response::builder().status(status);
    if let Some(h) = resp.headers_mut() {
        h.extend(rheaders);
    }
    resp.body(body).unwrap()
}

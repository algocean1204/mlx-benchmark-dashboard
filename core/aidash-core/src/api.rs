//! axum 디버그 SSE + HTML 미리보기 (로컬 전용)

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{Html, IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use futures_util::stream::Stream;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::db::Database;
use crate::env_detect;
use crate::events::CoreEvent;
use crate::stats::DEFAULT_OVERVIEW_CONTEXT;

#[derive(Debug, Clone)]
pub struct ApiServerConfig {
    pub bind_addr: String,
    pub port: u16,
}

impl Default for ApiServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1".into(),
            port: 8787,
        }
    }
}

#[derive(Clone)]
pub struct ApiState {
    pub db: Arc<Database>,
    pub event_tx: broadcast::Sender<CoreEvent>,
    pub project_root: std::path::PathBuf,
}

pub fn create_event_bus() -> broadcast::Sender<CoreEvent> {
    let (tx, _) = broadcast::channel(512);
    tx
}

pub fn build_router(state: ApiState) -> Router {
    Router::new()
        .route("/", get(index_handler))
        .route("/api/runs", get(runs_handler))
        .route("/api/stats/overview", get(stats_overview_handler))
        .route("/api/doctor", get(doctor_handler))
        .route("/events", get(events_sse_handler))
        .with_state(state)
}

async fn runs_handler(State(state): State<ApiState>) -> impl IntoResponse {
    match state.db.list_runs(None) {
        Ok(rows) => Json(rows).into_response(),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn stats_overview_handler(State(state): State<ApiState>) -> impl IntoResponse {
    match state.db.stats_overview(DEFAULT_OVERVIEW_CONTEXT) {
        Ok(rows) => Json(rows).into_response(),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn doctor_handler(State(state): State<ApiState>) -> impl IntoResponse {
    let report = env_detect::scan_environment(&state.project_root);
    Json(report).into_response()
}

async fn events_sse_handler(
    State(state): State<ApiState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.event_tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|msg| match msg {
        Ok(event) => {
            let json = serde_json::to_string(&event).unwrap_or_else(|_| "{}".into());
            Some(Ok(Event::default().data(json)))
        }
        Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(_)) => None,
    });

    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}

async fn index_handler(State(state): State<ApiState>) -> impl IntoResponse {
    let overview = state
        .db
        .stats_overview(DEFAULT_OVERVIEW_CONTEXT)
        .unwrap_or_default();
    let mut rows_html = String::new();
    for row in &overview {
        let tps = row
            .decode_tps
            .map(|t| format!("{t:.1}"))
            .unwrap_or_else(|| "-".into());
        let tier = row
            .tier
            .map(|t| t.info().badge)
            .unwrap_or("-");
        let measured = row.measured_at.as_deref().unwrap_or("-");
        rows_html.push_str(&format!(
            "<tr><td>{id}</td><td>{tier} {tps}</td><td>{measured}</td></tr>\n",
            id = row.profile_id,
            tier = tier,
            tps = tps,
            measured = measured,
        ));
    }

    let html = format!(
        r#"<!DOCTYPE html>
<html lang="ko"><head><meta charset="utf-8"><title>AI Dashboard</title>
<style>body{{font-family:system-ui,sans-serif;background:#111;color:#eee;padding:2rem}}
table{{border-collapse:collapse;width:100%}}td,th{{border:1px solid #333;padding:.5rem}}</style>
</head><body>
<h1>AI Dashboard — Stats Overview</h1>
<table><thead><tr><th>Model</th><th>Decode TPS</th><th>Measured</th></tr></thead>
<tbody>{rows_html}</tbody></table>
</body></html>"#
    );
    Html(html)
}

pub async fn serve(config: ApiServerConfig, state: ApiState) -> Result<(), String> {
    let addr: SocketAddr = format!("{}:{}", config.bind_addr, config.port)
        .parse()
        .map_err(|e: std::net::AddrParseError| e.to_string())?;
    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| e.to_string())?;
    eprintln!("api server listening on http://{addr}");
    axum::serve(listener, app)
        .await
        .map_err(|e| e.to_string())
}
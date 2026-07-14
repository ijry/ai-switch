use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};

use crate::app_state::AppState;
use crate::web::auth::is_authorized;
use crate::web::handlers::dispatch_command;
use crate::web::static_assets::resolve_static_file;
use crate::web::ws::events_socket;

#[derive(Clone)]
pub struct WebServerContext {
    pub state: Arc<AppState>,
    pub token: Arc<String>,
    pub static_dir: PathBuf,
}

pub fn build_router(state: Arc<AppState>, token: String, static_dir: PathBuf) -> Router {
    let context = WebServerContext {
        state,
        token: Arc::new(token),
        static_dir,
    };

    Router::new()
        .route("/health", get(health))
        .route("/ws/events", get(events_socket))
        .route("/api/:command", post(api_command))
        .fallback(static_fallback)
        .with_state(context)
}

async fn health() -> Json<Value> {
    Json(json!({ "ok": true }))
}

async fn api_command(
    State(context): State<WebServerContext>,
    Path(command): Path<String>,
    headers: HeaderMap,
    Json(args): Json<Value>,
) -> Response {
    if !is_authorized(&headers, &context.token) {
        return error_response(StatusCode::UNAUTHORIZED, "Unauthorized");
    }

    match dispatch_command(context.state, &command, args).await {
        Ok(value) => Json(value).into_response(),
        Err(error) => error_response(StatusCode::BAD_REQUEST, &error),
    }
}

async fn static_fallback(State(context): State<WebServerContext>, uri: Uri) -> Response {
    let Some(file_path) = resolve_static_file(&context.static_dir, uri.path()) else {
        return error_response(StatusCode::NOT_FOUND, "AI Switch web assets not found");
    };

    match tokio::fs::read(&file_path).await {
        Ok(bytes) => {
            let content_type = content_type_for(&file_path);
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(bytes))
                .unwrap_or_else(|_| {
                    error_response(StatusCode::INTERNAL_SERVER_ERROR, "Could not build response")
                })
        }
        Err(_) => error_response(StatusCode::NOT_FOUND, "AI Switch web assets not found"),
    }
}

fn content_type_for(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "application/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "map" => "application/json; charset=utf-8",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        _ => "application/octet-stream",
    }
}

fn error_response(status: StatusCode, message: &str) -> Response {
    (
        status,
        Json(json!({
            "code": "web.error",
            "message": message,
            "details": null,
            "recoverable": status != StatusCode::UNAUTHORIZED
        })),
    )
        .into_response()
}

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};

use crate::app_state::AppState;
use crate::web::auth::is_authorized;
use crate::web::handlers::dispatch_command;
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

async fn static_fallback(State(context): State<WebServerContext>) -> Response {
    let index = context.static_dir.join("index.html");
    match tokio::fs::read_to_string(index).await {
        Ok(contents) => (
            StatusCode::OK,
            [("content-type", "text/html; charset=utf-8")],
            contents,
        )
            .into_response(),
        Err(_) => error_response(StatusCode::NOT_FOUND, "AI Switch web assets not found"),
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

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::IntoResponse;
use axum::extract::State;

use crate::app_state::AppState;
use crate::web::auth::{is_authorized, is_query_token_authorized};
use crate::web::router::WebServerContext;

const READY_CHANNEL: &str = "__ready__";

pub async fn events_socket(
    State(context): State<WebServerContext>,
    headers: HeaderMap,
    uri: Uri,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    if !is_authorized(&headers, &context.token)
        && !is_query_token_authorized(uri.query(), &context.token)
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    ws.on_upgrade(move |socket| handle_socket(socket, context.state))
        .into_response()
}

async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>) {
    let _ = socket
        .send(Message::Text(
            serde_json::json!({
                "channel": READY_CHANNEL,
                "payload": {}
            })
            .to_string(),
        ))
        .await;

    let mut receiver = state.event_broadcaster.subscribe();
    while let Ok(event) = receiver.recv().await {
        let Ok(message) = serde_json::to_string(&event) else {
            continue;
        };
        if socket.send(Message::Text(message)).await.is_err() {
            break;
        }
    }
}

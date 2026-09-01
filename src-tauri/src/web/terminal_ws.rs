use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;

use crate::web::auth::{
    is_authorized, is_mobile_token_authorized, is_mobile_token_query_authorized,
    is_query_token_authorized,
};
use crate::web::router::WebServerContext;
use crate::web::terminal_hub::{TerminalFrame, TerminalHub, TerminalMessage};

/// Read a non-negative `since` cursor without depending on a URL parser.
pub(crate) fn parse_since(query: Option<&str>) -> Option<u64> {
    query?
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(key, _)| *key == "since")
        .and_then(|(_, value)| value.parse::<u64>().ok())
}

pub async fn terminal_socket(
    State(context): State<WebServerContext>,
    Path(session_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    ws: WebSocketUpgrade,
) -> Response {
    let query = uri.query();
    let authorized = is_authorized(&headers, &context.token)
        || is_query_token_authorized(query, &context.token)
        || is_mobile_token_authorized(&headers, &context.mobile_tokens).await
        || is_mobile_token_query_authorized(query, &context.mobile_tokens).await;
    if !authorized {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    let since = parse_since(query);
    let hub = Arc::clone(&context.state.terminal_hub);
    ws.on_upgrade(move |socket| handle_terminal_socket(socket, hub, session_id, since))
        .into_response()
}

async fn send_terminal_message(
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    message: &TerminalMessage,
) -> bool {
    let Ok(text) = serde_json::to_string(message) else {
        return false;
    };
    sender.send(Message::Text(text.into())).await.is_ok()
}

async fn send_replay(
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    messages: &[TerminalMessage],
) -> bool {
    for message in messages {
        if !send_terminal_message(sender, message).await {
            return false;
        }
    }
    true
}

async fn handle_terminal_socket(
    socket: WebSocket,
    hub: Arc<TerminalHub>,
    session_id: String,
    since: Option<u64>,
) {
    // Keep the guard alive for the complete socket lifetime. It is also used
    // by write/resize/kill command authorization to prove an active stream.
    let _guard = TerminalHub::register_subscriber_arc(&hub, &session_id);
    let replay = hub.subscribe(&session_id, since);
    let (mut sender, mut receiver) = socket.split();

    if !send_replay(&mut sender, &replay.messages).await {
        return;
    }

    let mut stream = replay.receiver;
    loop {
        tokio::select! {
            frame = stream.recv() => match frame {
                Ok(TerminalFrame { seq, data }) => {
                    if !send_terminal_message(
                        &mut sender,
                        &TerminalMessage::Chunk { seq, data },
                    ).await {
                        return;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    // A slow client missed broadcast frames. Re-subscribe and
                    // send a reset snapshot; the client will clear and redraw.
                    let replay = hub.subscribe(&session_id, None);
                    if !send_replay(&mut sender, &replay.messages).await {
                        return;
                    }
                    stream = replay.receiver;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
            },
            incoming = receiver.next() => match incoming {
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return,
                // Consume client frames so ping/close handshakes work. Input
                // is sent through the authenticated API command for now.
                Some(Ok(_)) => {}
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_since;

    #[test]
    fn parse_since_reads_numeric_query_parameter() {
        assert_eq!(parse_since(Some("since=42")), Some(42));
        assert_eq!(parse_since(Some("token=abc&since=7")), Some(7));
    }

    #[test]
    fn parse_since_ignores_missing_or_invalid_values() {
        assert_eq!(parse_since(None), None);
        assert_eq!(parse_since(Some("token=abc")), None);
        assert_eq!(parse_since(Some("since=")), None);
        assert_eq!(parse_since(Some("since=abc")), None);
        assert_eq!(parse_since(Some("since=-1")), None);
    }
}

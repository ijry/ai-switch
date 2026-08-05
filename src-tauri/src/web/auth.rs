use axum::extract::{RawPathParams, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use std::sync::Arc;

use crate::web::handlers::is_sensitive_command;

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    for index in 0..left.len().max(right.len()) {
        let left_byte = left.get(index).copied().unwrap_or_default();
        let right_byte = right.get(index).copied().unwrap_or_default();
        difference |= usize::from(left_byte ^ right_byte);
    }
    difference == 0
}

pub fn is_authorized(headers: &HeaderMap, token: &str) -> bool {
    if token.is_empty() {
        return true;
    }

    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|value| constant_time_eq(value.as_bytes(), token.as_bytes()))
}

pub fn is_query_token_authorized(query: Option<&str>, token: &str) -> bool {
    if token.is_empty() {
        return true;
    }

    query
        .unwrap_or_default()
        .split('&')
        .filter_map(|part| part.split_once('='))
        .any(|(key, value)| {
            key == "token" && constant_time_eq(value.as_bytes(), token.as_bytes())
        })
}

pub async fn authorize_api_request(
    State(token): State<Arc<String>>,
    path_params: RawPathParams,
    request: Request,
    next: Next,
) -> Response {
    let sensitive = path_params
        .iter()
        .find_map(|(key, value)| (key == "command").then_some(value))
        .is_some_and(is_sensitive_command);
    if (!sensitive || !token.is_empty()) && is_authorized(request.headers(), &token) {
        return next.run(request).await;
    }

    (
        StatusCode::UNAUTHORIZED,
        Json(json!({
            "code": "web.unauthorized",
            "message": "Unauthorized",
            "details": null,
            "recoverable": false,
            "operation_id": null
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderMap, HeaderValue};

    #[test]
    fn authorizes_matching_bearer_token() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            HeaderValue::from_static("Bearer secret"),
        );

        assert!(is_authorized(&headers, "secret"));
        assert!(!is_authorized(&headers, "other"));
    }

    #[test]
    fn authorizes_matching_query_token() {
        assert!(is_query_token_authorized(Some("token=secret"), "secret"));
        assert!(!is_query_token_authorized(Some("token=other"), "secret"));
    }

    #[test]
    fn constant_time_comparison_handles_content_and_length_mismatches() {
        assert!(constant_time_eq(b"secret", b"secret"));
        assert!(!constant_time_eq(b"secret", b"secreu"));
        assert!(!constant_time_eq(b"secret", b"secret-longer"));
        assert!(!constant_time_eq(b"secret-longer", b"secret"));
    }
}

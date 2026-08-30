use axum::extract::{RawPathParams, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use crate::services::mobile_pairing::MobileTokenRegistry;
use std::time::SystemTime;
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

#[derive(Clone)]
pub struct ApiAuthState {
    pub primary_token: Arc<String>,
    pub mobile_tokens: MobileTokenRegistry,
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
        .any(|(key, value)| key == "token" && constant_time_eq(value.as_bytes(), token.as_bytes()))
}

pub async fn is_mobile_token_authorized(
    headers: &HeaderMap,
    mobile_tokens: &MobileTokenRegistry,
) -> bool {
    let Some(value) = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
    else {
        return false;
    };
    let token = value.trim();
    if token.is_empty() {
        return false;
    }
    let digest = mobile_token_digest(token);
    let mut registry = mobile_tokens.lock().await;
    let now = SystemTime::now();
    registry.retain(|_, expires_at| *expires_at > now);
    registry.contains_key(&digest)
}

fn mobile_token_digest(token: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub async fn authorize_api_request(
    State(auth): State<Arc<ApiAuthState>>,
    path_params: RawPathParams,
    request: Request,
    next: Next,
) -> Response {
    let sensitive = path_params
        .iter()
        .find_map(|(key, value)| (key == "command").then_some(value))
        .is_some_and(is_sensitive_command);
    let primary_authorized = !auth.primary_token.is_empty()
        && is_authorized(request.headers(), &auth.primary_token);
    let mobile_authorized = is_mobile_token_authorized(request.headers(), &auth.mobile_tokens).await;
    if primary_authorized || (!sensitive && (auth.primary_token.is_empty() || mobile_authorized)) {
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

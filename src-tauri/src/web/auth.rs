use crate::services::mobile_pairing::MobileTokenRegistry;
use axum::extract::{RawPathParams, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use std::sync::Arc;
use std::time::SystemTime;

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

/// Which credential let a request through.
///
/// The mobile pairing token is deliberately the lower-privilege one: it is valid
/// for 30 days, handed to a phone browser, and reachable over a Tailscale Funnel
/// URL when `tailscale_exposure_mode` is `public`. Sensitive commands already
/// reject it, but ordinary ones include `list_route_credentials`, which returns
/// `secret_payload_json` verbatim — so the dispatcher needs to know which token
/// asked in order to keep plaintext api_keys out of that response.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebAuthLevel {
    Primary,
    Mobile,
}

pub fn is_authorized(headers: &HeaderMap, token: &str) -> bool {
    // No configured token means no access control, not permission to enter.
    // Ordinary commands include ones that return stored credentials verbatim
    // (list_route_credentials), so this has to fail closed.
    if token.is_empty() {
        return false;
    }

    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|value| constant_time_eq(value.as_bytes(), token.as_bytes()))
}

pub fn is_query_token_authorized(query: Option<&str>, token: &str) -> bool {
    if token.is_empty() {
        return false;
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
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    is_mobile_token_candidate_authorized(token, mobile_tokens).await
}

/// Browser WebSocket clients cannot set an Authorization header, so the
/// terminal stream also accepts the short-lived mobile token in its query
/// string. This helper deliberately remains separate from the API middleware:
/// mobile tokens are still denied on `/ws/events` and on sensitive commands.
pub async fn is_mobile_token_query_authorized(
    query: Option<&str>,
    mobile_tokens: &MobileTokenRegistry,
) -> bool {
    let token = query
        .unwrap_or_default()
        .split('&')
        .filter_map(|part| part.split_once('='))
        .find_map(|(key, value)| (key == "token").then_some(value));
    is_mobile_token_candidate_authorized(token, mobile_tokens).await
}

async fn is_mobile_token_candidate_authorized(
    token: Option<&str>,
    mobile_tokens: &MobileTokenRegistry,
) -> bool {
    let Some(token) = token.map(str::trim).filter(|token| !token.is_empty()) else {
        return false;
    };
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
    mut request: Request,
    next: Next,
) -> Response {
    let sensitive = path_params
        .iter()
        .find_map(|(key, value)| (key == "command").then_some(value))
        .is_some_and(is_sensitive_command);
    let primary_authorized = is_authorized(request.headers(), &auth.primary_token);
    let mobile_authorized =
        is_mobile_token_authorized(request.headers(), &auth.mobile_tokens).await;
    if primary_authorized || (!sensitive && mobile_authorized) {
        // The handler projects secrets out of ordinary responses for anything
        // short of the primary token, so it has to be told which one this was.
        request.extensions_mut().insert(if primary_authorized {
            WebAuthLevel::Primary
        } else {
            WebAuthLevel::Mobile
        });
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
    fn an_empty_token_authorizes_nobody() {
        // An empty token used to mean "let everyone through", so an
        // unauthenticated list_route_credentials handed out plaintext api_keys.
        // Missing configuration has to fail closed.
        assert!(!is_authorized(&HeaderMap::new(), ""));
        assert!(!is_query_token_authorized(Some("token=anything"), ""));
        assert!(!is_query_token_authorized(None, ""));

        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            HeaderValue::from_static("Bearer anything"),
        );
        assert!(!is_authorized(&headers, ""));
    }

    #[test]
    fn constant_time_comparison_handles_content_and_length_mismatches() {
        assert!(constant_time_eq(b"secret", b"secret"));
        assert!(!constant_time_eq(b"secret", b"secreu"));
        assert!(!constant_time_eq(b"secret", b"secret-longer"));
        assert!(!constant_time_eq(b"secret-longer", b"secret"));
    }
}

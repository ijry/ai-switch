use axum::http::HeaderMap;

pub fn is_authorized(headers: &HeaderMap, token: &str) -> bool {
    if token.is_empty() {
        return true;
    }

    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|value| value == token)
}

pub fn is_query_token_authorized(query: Option<&str>, token: &str) -> bool {
    if token.is_empty() {
        return true;
    }

    query
        .unwrap_or_default()
        .split('&')
        .filter_map(|part| part.split_once('='))
        .any(|(key, value)| key == "token" && value == token)
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
}

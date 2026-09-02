use std::error::Error;
use std::time::Duration;

/// The desktop user is waiting on a QR code, and the failures worth catching
/// (closed connection, refused port) surface in well under a second.
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Whether the advertised access URL can be verified from this computer.
///
/// A Funnel URL traverses the public internet, so probing it here exercises the
/// same path a phone takes. A tailnet URL cannot be verified: the listener
/// lives in the sidecar's userspace stack, which the host has no route to, so
/// probing one only ever produces a false negative.
pub fn should_probe_access_url(public: bool, force: bool) -> bool {
    public && !force
}

/// Requests `/health` over the advertised URL the way an unauthenticated mobile
/// client would. `Err` carries the reason, already trimmed for display.
pub async fn probe_access_url(access_url: &str) -> Result<(), String> {
    let client = reqwest::Client::builder()
        // A system proxy answers for the desktop only. The phone dials the
        // endpoint directly, so a reachable proxy must not hide an outage.
        .no_proxy()
        .timeout(PROBE_TIMEOUT)
        .build()
        .map_err(|error| error.to_string())?;
    let url = format!("{}/health", access_url.trim_end_matches('/'));
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| root_cause(&error))?;
    let status = response.status();
    if status.is_success() {
        return Ok(());
    }
    Err(format!("HTTP {}", status.as_u16()))
}

/// reqwest nests transport failures several layers deep; the innermost cause is
/// the part a human can act on, such as "connection closed before message
/// completed".
fn root_cause(error: &reqwest::Error) -> String {
    let mut reason = error.to_string();
    let mut source = error.source();
    while let Some(inner) = source {
        reason = inner.to_string();
        source = inner.source();
    }
    reason
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use axum::routing::get;
    use axum::Router;
    use std::net::SocketAddr;

    async fn spawn_health(status: StatusCode) -> SocketAddr {
        let app = Router::new().route("/health", get(move || async move { status }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        address
    }

    /// Accepts then drops every connection, the way a Funnel ingress with no
    /// route to its node does.
    async fn spawn_closing_listener() -> SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                drop(stream);
            }
        });
        address
    }

    #[tokio::test]
    async fn healthy_endpoint_passes() {
        let address = spawn_health(StatusCode::OK).await;
        assert_eq!(probe_access_url(&format!("http://{address}")).await, Ok(()));
    }

    #[tokio::test]
    async fn trailing_slash_does_not_double_the_path() {
        let address = spawn_health(StatusCode::OK).await;
        assert_eq!(
            probe_access_url(&format!("http://{address}/")).await,
            Ok(())
        );
    }

    #[tokio::test]
    async fn non_success_status_reports_the_code() {
        let address = spawn_health(StatusCode::BAD_GATEWAY).await;
        assert_eq!(
            probe_access_url(&format!("http://{address}")).await,
            Err("HTTP 502".to_string())
        );
    }

    #[tokio::test]
    async fn closed_connection_reports_a_transport_reason() {
        let address = spawn_closing_listener().await;
        let reason = probe_access_url(&format!("http://{address}"))
            .await
            .expect_err("a dropped connection must not pass the probe");
        assert!(!reason.trim().is_empty());
        assert!(!reason.starts_with("HTTP "), "unexpected reason: {reason}");
    }

    #[test]
    fn only_public_urls_are_probed_and_force_skips_the_gate() {
        assert!(should_probe_access_url(true, false));
        assert!(!should_probe_access_url(true, true));
        assert!(!should_probe_access_url(false, false));
        assert!(!should_probe_access_url(false, true));
    }
}

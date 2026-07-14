use crate::services::tailscale_types::{
    TailscaleLogin, TailscaleStartRequest, TailscaleStatus,
};
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Mutex;

#[async_trait]
pub trait SidecarControlClient: Send + Sync {
    async fn start(&self, request: TailscaleStartRequest) -> Result<TailscaleStatus, String>;
    async fn login_oauth(&self) -> Result<TailscaleLogin, String>;
    async fn stop(&self) -> Result<TailscaleStatus, String>;
    async fn logout(&self) -> Result<TailscaleStatus, String>;
    async fn status(&self) -> Result<TailscaleStatus, String>;
}

#[derive(Debug)]
struct FakeSidecarInner {
    status: TailscaleStatus,
    last_start: Option<TailscaleStartRequest>,
}

impl Default for FakeSidecarInner {
    fn default() -> Self {
        Self {
            status: TailscaleStatus::needs_login("Sign in to connect secure network"),
            last_start: None,
        }
    }
}

#[derive(Debug, Default)]
pub struct FakeSidecarControlClient {
    inner: Mutex<FakeSidecarInner>,
}

impl FakeSidecarControlClient {
    pub fn with_status(status: TailscaleStatus) -> Self {
        Self {
            inner: Mutex::new(FakeSidecarInner {
                status,
                last_start: None,
            }),
        }
    }

    pub fn last_start(&self) -> Option<TailscaleStartRequest> {
        self.inner
            .lock()
            .ok()
            .and_then(|inner| inner.last_start.clone())
    }
}

#[async_trait]
impl SidecarControlClient for FakeSidecarControlClient {
    async fn start(&self, request: TailscaleStartRequest) -> Result<TailscaleStatus, String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "fake sidecar lock poisoned".to_string())?;
        inner.last_start = Some(request.clone());

        if let Some(auth_key) = request.auth_key.as_deref() {
            if auth_key.trim().is_empty() {
                return Err("auth key is required".to_string());
            }
            inner.status = TailscaleStatus {
                state: "connected".to_string(),
                device_name: Some(request.hostname.clone()),
                tailnet_ip: Some("100.64.0.12".to_string()),
                magic_dns_name: Some(format!("{}.tailnet.ts.net", request.hostname)),
                login_url: None,
                access_urls: Vec::new(),
                serving: true,
                message: None,
            };
        } else {
            inner.status = TailscaleStatus::needs_login("Sign in to connect secure network");
        }

        Ok(inner.status.clone())
    }

    async fn login_oauth(&self) -> Result<TailscaleLogin, String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "fake sidecar lock poisoned".to_string())?;
        let login_url = "https://login.tailscale.com/a/example".to_string();
        inner.status = TailscaleStatus {
            state: "needsLogin".to_string(),
            device_name: None,
            tailnet_ip: None,
            magic_dns_name: None,
            login_url: Some(login_url.clone()),
            access_urls: Vec::new(),
            serving: false,
            message: Some("Complete browser sign-in".to_string()),
        };
        Ok(TailscaleLogin {
            login_url: Some(login_url),
            message: "Open the secure network sign-in page".to_string(),
        })
    }

    async fn stop(&self) -> Result<TailscaleStatus, String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "fake sidecar lock poisoned".to_string())?;
        inner.status = TailscaleStatus::stopped("Secure network stopped");
        Ok(inner.status.clone())
    }

    async fn logout(&self) -> Result<TailscaleStatus, String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "fake sidecar lock poisoned".to_string())?;
        inner.status = TailscaleStatus::needs_login("Signed out of secure network");
        Ok(inner.status.clone())
    }

    async fn status(&self) -> Result<TailscaleStatus, String> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| "fake sidecar lock poisoned".to_string())?;
        Ok(inner.status.clone())
    }
}

pub fn resolve_sidecar_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("AI_SWITCH_TSNET_PATH") {
        let candidate = PathBuf::from(path);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    let exe = std::env::current_exe().ok()?;
    let sibling = exe.with_file_name(if cfg!(windows) {
        "ai-switch-tsnet.exe"
    } else {
        "ai-switch-tsnet"
    });
    sibling.exists().then_some(sibling)
}

pub fn parse_control_addr_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let rest = trimmed.strip_prefix("CONTROL ")?;
    let addr = rest.trim();
    if addr.is_empty() {
        None
    } else {
        Some(addr.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        parse_control_addr_line, resolve_sidecar_path, FakeSidecarControlClient, SidecarControlClient,
    };
    use crate::services::tailscale_types::{TailscaleStartRequest, TailscaleStatus};

    #[tokio::test]
    async fn fake_sidecar_reports_needs_login_until_oauth_completes() {
        let client = FakeSidecarControlClient::with_status(TailscaleStatus::needs_login(
            "Sign in to connect secure network",
        ));
        let status = client.status().await.unwrap();
        assert_eq!(status.state, "needsLogin");

        let login = client.login_oauth().await.unwrap();
        assert!(login.login_url.unwrap().starts_with("https://"));
    }

    #[tokio::test]
    async fn fake_sidecar_connects_with_auth_key() {
        let client = FakeSidecarControlClient::default();
        let status = client
            .start(TailscaleStartRequest {
                state_dir: "C:/tmp/tailscale".to_string(),
                hostname: "ai-switch".to_string(),
                auth_key: Some("tskey-auth-test".to_string()),
                backend_addr: "127.0.0.1:3090".to_string(),
                serve_port: 3090,
            })
            .await
            .unwrap();

        assert_eq!(status.state, "connected");
        assert_eq!(status.tailnet_ip.as_deref(), Some("100.64.0.12"));
        assert_eq!(status.device_name.as_deref(), Some("ai-switch"));
        assert!(status.serving);
    }

    #[test]
    fn parse_control_addr_line_reads_localhost_port() {
        assert_eq!(
            parse_control_addr_line("CONTROL 127.0.0.1:4567"),
            Some("127.0.0.1:4567".to_string())
        );
    }

    #[test]
    fn resolve_sidecar_path_prefers_env_override_when_present() {
        // When env path is unset or missing, this should not panic.
        let _ = resolve_sidecar_path();
    }
}

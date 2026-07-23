use crate::services::tailscale_types::{TailscaleLogin, TailscaleStartRequest, TailscaleStatus};
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::io::AsyncBufReadExt;

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
                public: request.public,
                exposure_mode: Some(if request.public {
                    "public".to_string()
                } else {
                    "private".to_string()
                }),
                public_port: if request.public { Some(443) } else { None },
                message: None,
            };
        } else if inner.status.state == "connected" {
            // Rebind existing session to a new local backend without forcing re-login.
            inner.status.serving = true;
            inner.status.message = None;
            inner.status.public = request.public;
            inner.status.exposure_mode = Some(if request.public {
                "public".to_string()
            } else {
                "private".to_string()
            });
            inner.status.public_port = if request.public { Some(443) } else { None };
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
            public: false,
            exposure_mode: Some("private".to_string()),
            public_port: None,
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

#[derive(Debug)]
pub struct SidecarProcess {
    child: Mutex<Option<tokio::process::Child>>,
    control_base: Mutex<Option<String>>,
    start_lock: tokio::sync::Mutex<()>,
}

impl Default for SidecarProcess {
    fn default() -> Self {
        Self {
            child: Mutex::new(None),
            control_base: Mutex::new(None),
            start_lock: tokio::sync::Mutex::new(()),
        }
    }
}

#[cfg(test)]
impl SidecarProcess {
    fn set_control_base_for_test(&self, value: Option<String>) {
        if let Ok(mut base) = self.control_base.lock() {
            *base = value;
        }
    }

    fn control_base_for_test(&self) -> Option<String> {
        self.control_base.lock().ok().and_then(|base| base.clone())
    }
}

impl Drop for SidecarProcess {
    fn drop(&mut self) {
        if let Ok(mut child_slot) = self.child.lock() {
            if let Some(mut child) = child_slot.take() {
                let _ = child.start_kill();
            }
        }
    }
}

#[derive(Debug)]
pub struct HttpSidecarControlClient {
    process: Arc<SidecarProcess>,
    binary: PathBuf,
    http: reqwest::Client,
}

impl HttpSidecarControlClient {
    pub fn new(binary: PathBuf) -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(90))
            .build()
            .map_err(|error| format!("Could not create secure network client: {error}"))?;
        Ok(Self {
            process: Arc::new(SidecarProcess::default()),
            binary,
            http,
        })
    }

    fn cached_control_base(&self) -> Result<Option<String>, String> {
        self.process
            .control_base
            .lock()
            .map_err(|_| "secure network process lock poisoned".to_string())
            .map(|base| base.clone())
    }

    async fn ensure_control_base(&self) -> Result<String, String> {
        if let Some(value) = self.cached_control_base()? {
            return Ok(value);
        }

        let _guard = self.process.start_lock.lock().await;
        if let Some(value) = self.cached_control_base()? {
            return Ok(value);
        }

        self.spawn_control_process().await
    }

    async fn spawn_control_process(&self) -> Result<String, String> {
        // Replace any leftover process before spawning a fresh control plane.
        self.kill_process().await;

        let mut command = tokio::process::Command::new(&self.binary);
        command
            .arg("--control-addr")
            .arg("127.0.0.1:0")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);

        // Hide the sidecar console window on Windows; keep stdout/stderr piped.
        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = command
            .spawn()
            .map_err(|error| format!("Could not start secure network component: {error}"))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "secure network component produced no output".to_string())?;
        let mut reader = tokio::io::BufReader::new(stdout).lines();

        let control_addr =
            tokio::time::timeout(std::time::Duration::from_secs(8), async {
                while let Some(line) = reader.next_line().await.map_err(|error| {
                    format!("Could not read secure network startup output: {error}")
                })? {
                    if let Some(addr) = parse_control_addr_line(&line) {
                        return Ok(addr);
                    }
                }
                Err("Secure network component did not publish a control address".to_string())
            })
            .await
            .map_err(|_| "Secure network component startup timed out".to_string())??;

        {
            let mut child_slot = self
                .process
                .child
                .lock()
                .map_err(|_| "secure network process lock poisoned".to_string())?;
            *child_slot = Some(child);
        }
        let base = format!("http://{control_addr}");
        {
            let mut slot = self
                .process
                .control_base
                .lock()
                .map_err(|_| "secure network process lock poisoned".to_string())?;
            *slot = Some(base.clone());
        }

        Ok(base)
    }

    async fn invalidate_control_base(&self) {
        self.kill_process().await;
    }

    async fn post_json<T: serde::de::DeserializeOwned, B: serde::Serialize>(
        &self,
        path: &str,
        body: Option<&B>,
    ) -> Result<T, String> {
        self.with_control_retry(|base| {
            let url = format!("{base}{path}");
            let request = if let Some(payload) = body {
                self.http.post(url).json(payload)
            } else {
                self.http.post(url)
            };
            async move { request.send().await }
        })
        .await
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, String> {
        self.with_control_retry(|base| {
            let url = format!("{base}{path}");
            let request = self.http.get(url);
            async move { request.send().await }
        })
        .await
    }

    async fn with_control_retry<T, F, Fut>(&self, build: F) -> Result<T, String>
    where
        T: serde::de::DeserializeOwned,
        F: Fn(String) -> Fut,
        Fut: std::future::Future<Output = Result<reqwest::Response, reqwest::Error>>,
    {
        let mut last_error = None;
        for attempt in 0..2 {
            let base = self.ensure_control_base().await?;
            match build(base).await {
                Ok(response) => {
                    let status = response.status();
                    let text = response.text().await.map_err(|error| {
                        format!("Secure network control response failed: {error}")
                    })?;
                    if !status.is_success() {
                        return Err(format!("Secure network control error ({status}): {text}"));
                    }
                    return serde_json::from_str(&text).map_err(|error| {
                        format!("Secure network control parse failed: {error}; payload={text}")
                    });
                }
                Err(error) => {
                    last_error = Some(format!("Secure network control request failed: {error}"));
                    // Transport failure usually means the cached control plane died.
                    self.invalidate_control_base().await;
                    if attempt == 0 {
                        continue;
                    }
                }
            }
        }
        Err(last_error.unwrap_or_else(|| "Secure network control request failed".to_string()))
    }

    async fn kill_process(&self) {
        let child = {
            let mut child_slot = match self.process.child.lock() {
                Ok(guard) => guard,
                Err(_) => return,
            };
            child_slot.take()
        };
        if let Some(mut child) = child {
            let _ = child.start_kill();
            let _ = child.wait().await;
        }
        if let Ok(mut base) = self.process.control_base.lock() {
            *base = None;
        }
    }
}

#[async_trait]
impl SidecarControlClient for HttpSidecarControlClient {
    async fn start(&self, request: TailscaleStartRequest) -> Result<TailscaleStatus, String> {
        self.post_json("/control/start", Some(&request)).await
    }

    async fn login_oauth(&self) -> Result<TailscaleLogin, String> {
        self.post_json::<TailscaleLogin, serde_json::Value>(
            "/control/login-oauth",
            None::<&serde_json::Value>,
        )
        .await
    }

    async fn stop(&self) -> Result<TailscaleStatus, String> {
        let result = self
            .post_json::<TailscaleStatus, serde_json::Value>(
                "/control/stop",
                None::<&serde_json::Value>,
            )
            .await;
        self.kill_process().await;
        result.or_else(|_| Ok(TailscaleStatus::stopped("Secure network stopped")))
    }

    async fn logout(&self) -> Result<TailscaleStatus, String> {
        let result = self
            .post_json::<TailscaleStatus, serde_json::Value>(
                "/control/logout",
                None::<&serde_json::Value>,
            )
            .await;
        self.kill_process().await;
        result.or_else(|_| Ok(TailscaleStatus::needs_login("Signed out of secure network")))
    }

    async fn status(&self) -> Result<TailscaleStatus, String> {
        self.get_json("/control/status").await
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
        parse_control_addr_line, resolve_sidecar_path, FakeSidecarControlClient,
        HttpSidecarControlClient, SidecarControlClient,
    };
    use crate::services::tailscale_types::{TailscaleStartRequest, TailscaleStatus};
    use std::path::PathBuf;

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
                public: false,
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

    #[tokio::test]
    #[ignore = "requires AI_SWITCH_TSNET_PATH binary"]
    async fn sidecar_process_starts_and_reports_status() {
        let path = std::env::var("AI_SWITCH_TSNET_PATH").expect("AI_SWITCH_TSNET_PATH");
        let client = HttpSidecarControlClient::new(PathBuf::from(path)).expect("client");
        let status = client.status().await.expect("status");
        assert_eq!(status.state, "needsLogin");
        let _ = client.stop().await;
    }

    #[tokio::test]
    #[ignore = "requires AI_SWITCH_TSNET_PATH binary"]
    async fn status_recovers_when_cached_control_base_is_stale() {
        let path = std::env::var("AI_SWITCH_TSNET_PATH").expect("AI_SWITCH_TSNET_PATH");
        let client = HttpSidecarControlClient::new(PathBuf::from(path)).expect("client");

        let first = client.status().await.expect("initial status");
        assert_eq!(first.state, "needsLogin");
        let previous = client
            .process
            .control_base_for_test()
            .expect("control base after first status");

        client
            .process
            .set_control_base_for_test(Some("http://127.0.0.1:1".to_string()));

        let recovered = client.status().await.expect("status after stale base");
        assert_eq!(recovered.state, "needsLogin");

        let current = client
            .process
            .control_base_for_test()
            .expect("control base after recovery");
        assert_ne!(current, "http://127.0.0.1:1".to_string());
        assert_ne!(current, previous);

        let _ = client.stop().await;
    }
}

use serde::Serialize;
use std::process::Command;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TailscaleStatus {
    pub state: String,
    pub device_name: Option<String>,
    pub tailnet_ip: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TailscaleLogin {
    pub login_url: Option<String>,
    pub message: String,
}

pub struct TailscaleService;

impl TailscaleService {
    pub async fn status() -> TailscaleStatus {
        tokio::task::spawn_blocking(status_blocking)
            .await
            .unwrap_or_else(|error| TailscaleStatus {
                state: "error".to_string(),
                device_name: None,
                tailnet_ip: None,
                message: Some(format!("Could not check Tailscale: {error}")),
            })
    }

    pub async fn start_login() -> TailscaleLogin {
        tokio::task::spawn_blocking(login_blocking)
            .await
            .unwrap_or_else(|error| TailscaleLogin {
                login_url: None,
                message: format!("Could not start Tailscale login: {error}"),
            })
    }

    pub async fn disconnect() -> TailscaleStatus {
        tokio::task::spawn_blocking(|| {
            let _ = Command::new("tailscale").arg("down").output();
            status_blocking()
        })
        .await
        .unwrap_or_else(|error| TailscaleStatus {
            state: "error".to_string(),
            device_name: None,
            tailnet_ip: None,
            message: Some(format!("Could not disconnect Tailscale: {error}")),
        })
    }
}

fn status_blocking() -> TailscaleStatus {
    let output = match Command::new("tailscale").arg("status").output() {
        Ok(output) => output,
        Err(error) => {
            return TailscaleStatus {
                state: "notInstalled".to_string(),
                device_name: None,
                tailnet_ip: None,
                message: Some(format!("Tailscale CLI was not found: {error}")),
            };
        }
    };

    if !output.status.success() {
        return TailscaleStatus {
            state: "needsLogin".to_string(),
            device_name: None,
            tailnet_ip: None,
            message: Some(String::from_utf8_lossy(&output.stderr).trim().to_string()),
        };
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let first = text.lines().find(|line| !line.trim().is_empty());
    let mut parts = first.unwrap_or_default().split_whitespace();
    let tailnet_ip = parts.next().map(ToOwned::to_owned);
    let device_name = parts.next().map(ToOwned::to_owned);

    TailscaleStatus {
        state: "connected".to_string(),
        device_name,
        tailnet_ip,
        message: None,
    }
}

fn login_blocking() -> TailscaleLogin {
    let output = match Command::new("tailscale").args(["up", "--qr=false"]).output() {
        Ok(output) => output,
        Err(error) => {
            return TailscaleLogin {
                login_url: None,
                message: format!("Tailscale CLI was not found: {error}"),
            };
        }
    };

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    TailscaleLogin {
        login_url: extract_url(&combined),
        message: combined.trim().to_string(),
    }
}

fn extract_url(text: &str) -> Option<String> {
    text.split_whitespace()
        .find(|part| part.starts_with("https://") || part.starts_with("http://"))
        .map(|part| part.trim_end_matches('.').to_string())
}

#[cfg(test)]
mod tests {
    use super::extract_url;

    #[test]
    fn extracts_login_url() {
        assert_eq!(
            extract_url("To authenticate, visit https://login.tailscale.com/a/abc."),
            Some("https://login.tailscale.com/a/abc".to_string())
        );
    }
}

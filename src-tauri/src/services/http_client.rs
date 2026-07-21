use reqwest::{Client, Proxy};
use std::time::Duration;

/// Build an outbound HTTP client that respects local proxy settings.
///
/// On Windows, many users configure Clash/V2Ray via WinINET
/// (`Internet Settings` ProxyEnable/ProxyServer) without setting
/// `HTTP(S)_PROXY` env vars. reqwest's system-proxy path typically
/// reads WinHTTP, which may still be "direct". Detect WinINET here.
pub fn build_outbound_http_client(timeout: Option<Duration>) -> Result<Client, String> {
    let mut builder = Client::builder().use_rustls_tls();
    if let Some(timeout) = timeout {
        builder = builder.timeout(timeout);
    }

    if let Some((proxy_url, no_proxy)) = detect_windows_wininet_proxy() {
        match Proxy::all(proxy_url) {
            Ok(mut proxy) => {
                if let Some(no_proxy) = no_proxy {
                    proxy = proxy.no_proxy(Some(no_proxy));
                }
                builder = builder.proxy(proxy);
            }
            Err(_) => {
                // Fall through to reqwest defaults / env proxy if configured.
            }
        }
    }

    builder
        .build()
        .map_err(|err| format!("Could not create HTTP client: {err}"))
}

fn detect_windows_wininet_proxy() -> Option<(String, Option<reqwest::NoProxy>)> {
    #[cfg(windows)]
    {
        return windows_wininet_proxy();
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(windows)]
fn windows_wininet_proxy() -> Option<(String, Option<reqwest::NoProxy>)> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    // Prefer explicit process env if already set; reqwest handles those.
    if std::env::var_os("HTTPS_PROXY").is_some()
        || std::env::var_os("https_proxy").is_some()
        || std::env::var_os("HTTP_PROXY").is_some()
        || std::env::var_os("http_proxy").is_some()
        || std::env::var_os("ALL_PROXY").is_some()
        || std::env::var_os("all_proxy").is_some()
    {
        return None;
    }

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let settings = hkcu
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Internet Settings")
        .ok()?;
    let enabled: u32 = settings.get_value("ProxyEnable").ok()?;
    if enabled == 0 {
        return None;
    }
    let server: String = settings.get_value("ProxyServer").ok()?;
    let server = server.trim();
    if server.is_empty() {
        return None;
    }

    // Formats:
    // - "127.0.0.1:7897"
    // - "http=127.0.0.1:7897;https=127.0.0.1:7897"
    let candidate = if server.contains('=') {
        let mut http = None;
        let mut https = None;
        for part in server.split(';') {
            let mut kv = part.splitn(2, '=');
            let key = kv.next().unwrap_or("").trim().to_ascii_lowercase();
            let value = kv.next().unwrap_or("").trim();
            if value.is_empty() {
                continue;
            }
            match key.as_str() {
                "https" => https = Some(value.to_string()),
                "http" => http = Some(value.to_string()),
                _ => {}
            }
        }
        https.or(http)?
    } else {
        server.to_string()
    };

    let proxy_url = if candidate.starts_with("http://")
        || candidate.starts_with("https://")
        || candidate.starts_with("socks5://")
    {
        candidate
    } else {
        format!("http://{candidate}")
    };

    let override_list: Option<String> = settings.get_value("ProxyOverride").ok();
    let no_proxy = override_list
        .as_deref()
        .map(normalize_proxy_override)
        .and_then(|value| reqwest::NoProxy::from_string(&value));

    Some((proxy_url, no_proxy))
}

/// Convert WinINET ProxyOverride into a NO_PROXY-like list.
fn normalize_proxy_override(raw: &str) -> String {
    raw.split(';')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| {
            // WinINET uses "<local>" for intranet hosts; treat as localhost set.
            if item.eq_ignore_ascii_case("<local>") {
                "localhost,127.0.0.1,::1".to_string()
            } else {
                // "127.*" / "192.168.*" are common; reqwest NoProxy supports '*' suffixes.
                item.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_client_without_panic() {
        let client = build_outbound_http_client(Some(Duration::from_secs(5))).expect("client");
        let _ = client;
    }

    #[test]
    fn normalizes_wininet_proxy_override() {
        let normalized = normalize_proxy_override("localhost;127.*;192.168.*;<local>");
        assert!(normalized.contains("localhost"));
        assert!(normalized.contains("127.0.0.1"));
        assert!(normalized.contains("127.*"));
        assert!(normalized.contains("192.168.*"));
    }
}

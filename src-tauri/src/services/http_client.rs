use reqwest::cookie::{CookieStore, Jar};
use reqwest::header::HeaderValue;
use reqwest::{Client, Proxy};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

static SHARED_CHATGPT_CLOUDFLARE_COOKIE_STORE: OnceLock<Arc<ChatGptCloudflareCookieStore>> =
    OnceLock::new();

#[derive(Debug, Default)]
struct ChatGptCloudflareCookieStore {
    jar: Jar,
}

impl CookieStore for ChatGptCloudflareCookieStore {
    fn set_cookies(
        &self,
        cookie_headers: &mut dyn Iterator<Item = &HeaderValue>,
        url: &reqwest::Url,
    ) {
        if !is_chatgpt_cookie_url(url) {
            return;
        }

        let mut cloudflare_cookie_headers =
            cookie_headers.filter(|header| is_allowed_cloudflare_set_cookie_header(header));
        self.jar.set_cookies(&mut cloudflare_cookie_headers, url);
    }

    fn cookies(&self, url: &reqwest::Url) -> Option<HeaderValue> {
        if is_chatgpt_cookie_url(url) {
            self.jar.cookies(url).and_then(only_cloudflare_cookies)
        } else {
            None
        }
    }
}

/// Build an outbound HTTP client that respects local proxy settings.
///
/// On Windows, many users configure Clash/V2Ray via WinINET
/// (`Internet Settings` ProxyEnable/ProxyServer) without setting
/// `HTTP(S)_PROXY` env vars. reqwest's system-proxy path typically
/// reads WinHTTP, which may still be "direct". Detect WinINET here.
pub fn build_outbound_http_client(timeout: Option<Duration>) -> Result<Client, String> {
    let mut builder = Client::builder()
        .use_rustls_tls()
        .cookie_provider(chatgpt_cloudflare_cookie_store());
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

fn chatgpt_cloudflare_cookie_store() -> Arc<ChatGptCloudflareCookieStore> {
    Arc::clone(SHARED_CHATGPT_CLOUDFLARE_COOKIE_STORE.get_or_init(|| {
        Arc::new(ChatGptCloudflareCookieStore::default())
    }))
}

fn is_chatgpt_cookie_url(url: &reqwest::Url) -> bool {
    if url.scheme() != "https" {
        return false;
    }

    let Some(host) = url.host_str() else {
        return false;
    };

    is_allowed_chatgpt_host(host)
}

fn is_allowed_chatgpt_host(host: &str) -> bool {
    matches!(host, "chatgpt.com" | "chat.openai.com" | "chatgpt-staging.com")
        || host.ends_with(".chatgpt.com")
        || host.ends_with(".chatgpt-staging.com")
}

fn is_allowed_cloudflare_set_cookie_header(header: &HeaderValue) -> bool {
    header
        .to_str()
        .ok()
        .and_then(set_cookie_name)
        .is_some_and(is_allowed_cloudflare_cookie_name)
}

fn set_cookie_name(header: &str) -> Option<&str> {
    let (name, _) = header.split_once('=')?;
    let name = name.trim();
    (!name.is_empty()).then_some(name)
}

fn only_cloudflare_cookies(header: HeaderValue) -> Option<HeaderValue> {
    let header = header.to_str().ok()?;
    let cookies = header
        .split(';')
        .filter_map(|cookie| {
            let cookie = cookie.trim();
            let name = cookie.split_once('=')?.0.trim();
            is_allowed_cloudflare_cookie_name(name).then_some(cookie)
        })
        .collect::<Vec<_>>()
        .join("; ");

    if cookies.is_empty() {
        None
    } else {
        HeaderValue::from_str(&cookies).ok()
    }
}

fn is_allowed_cloudflare_cookie_name(name: &str) -> bool {
    matches!(
        name,
        "__cf_bm"
            | "__cflb"
            | "__cfruid"
            | "__cfseq"
            | "__cfwaitingroom"
            | "_cfuvid"
            | "cf_clearance"
            | "cf_ob_info"
            | "cf_use_ob"
    ) || name.starts_with("cf_chl_")
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
    use reqwest::cookie::CookieStore;

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

    #[test]
    fn stores_only_cloudflare_cookies_for_chatgpt_hosts() {
        let store = ChatGptCloudflareCookieStore::default();
        let url = reqwest::Url::parse("https://chatgpt.com/backend-api/codex/usage").unwrap();
        let load_balancer = HeaderValue::from_static("__cflb=west; Path=/; Secure; HttpOnly");
        let cfuvid = HeaderValue::from_static("_cfuvid=visitor; Path=/; Secure; HttpOnly");
        let account_cookie =
            HeaderValue::from_static("chatgpt_session=secret; Path=/; Secure; HttpOnly");
        let mut set_cookies = [&load_balancer, &cfuvid, &account_cookie].into_iter();

        store.set_cookies(&mut set_cookies, &url);

        let cookies = store
            .cookies(&url)
            .and_then(|value| value.to_str().ok().map(str::to_string))
            .unwrap_or_default();
        assert!(cookies.contains("__cflb=west"));
        assert!(cookies.contains("_cfuvid=visitor"));
        assert!(!cookies.contains("chatgpt_session"));
    }

    #[test]
    fn does_not_return_chatgpt_cookies_for_other_hosts() {
        let store = ChatGptCloudflareCookieStore::default();
        let chatgpt_url = reqwest::Url::parse("https://chatgpt.com/backend-api/codex/usage").unwrap();
        let api_url = reqwest::Url::parse("https://api.openai.com/v1/responses").unwrap();
        let cfuvid = HeaderValue::from_static("_cfuvid=visitor; Path=/; Secure; HttpOnly");
        let mut set_cookies = [&cfuvid].into_iter();

        store.set_cookies(&mut set_cookies, &chatgpt_url);

        assert_eq!(store.cookies(&api_url), None);
    }
}

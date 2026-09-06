use std::time::Duration;

use crate::models::notification::{
    NotificationChannel, NotificationChannelKind, NotificationConfig, NotificationEvent,
};

/// Build the reqwest client for outbound notification delivery.
fn make_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .build()
        .unwrap_or_default()
}

/// Dispatch a notification event to all matching enabled channels.
/// Delivery is fire-and-forget: slow or unreachable endpoints never block the caller.
pub fn dispatch_notification(config: &NotificationConfig, event: &NotificationEvent) {
    if !config.enabled {
        return;
    }

    let client = make_client();

    for channel_config in &config.channels {
        if !channel_config.enabled {
            continue;
        }
        let client = client.clone();
        let event = event.clone();
        let kind = channel_config.kind.clone();
        tokio::spawn(async move {
            if let Err(e) = send_one(&client, &kind, &event).await {
                eprintln!("[Notification] delivery failed on {}: {e}", channel_type_name(&kind));
            }
        });
    }
}

fn channel_type_name(kind: &NotificationChannelKind) -> &'static str {
    match kind {
        NotificationChannelKind::Feishu { .. } => "feishu",
        NotificationChannelKind::Bark { .. } => "bark",
        NotificationChannelKind::Webhook { .. } => "webhook",
    }
}

/// Send to a single channel.
async fn send_one(
    client: &reqwest::Client,
    kind: &NotificationChannelKind,
    event: &NotificationEvent,
) -> Result<(), String> {
    match kind {
        NotificationChannelKind::Feishu { webhook_url } => {
            send_feishu(client, webhook_url, event).await
        }
        NotificationChannelKind::Bark { server_url, device_key } => {
            send_bark(client, server_url, device_key, event).await
        }
        NotificationChannelKind::Webhook { url } => {
            send_webhook(client, url, event).await
        }
    }
}

/// Feishu bot webhook: POST rich-text card.
async fn send_feishu(
    client: &reqwest::Client,
    webhook_url: &str,
    event: &NotificationEvent,
) -> Result<(), String> {
    // Build a rich-text body with fields
    let mut content_lines: Vec<String> = Vec::new();
    content_lines.push(format!("{{\"tag\":\"text\",\"text\":\"{}\"}}", escape_feishu(&event.body)));
    for (key, value) in &event.fields {
        content_lines.push(format!(
            "{{\"tag\":\"text\",\"text\":\"\\n{}: {}\"}}",
            escape_feishu(key),
            escape_feishu(value),
        ));
    }
    let content = format!("[{}]", content_lines.join(","));

    let payload = serde_json::json!({
        "msg_type": "post",
        "content": {
            "post": {
                "zh_cn": {
                    "title": &event.title,
                    "content": [[
                        { "tag": "text", "text": &event.body }
                    ]]
                }
            }
        }
    });

    // Simpler approach: use the "text" message type for maximum compatibility
    let text = format!("{}\n{}", event.title, event.body);
    let payload = serde_json::json!({
        "msg_type": "text",
        "content": { "text": text }
    });

    let resp = client
        .post(webhook_url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| e.without_url().to_string())?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {status}: {body}"));
    }
    Ok(())
}

/// Bark: GET to server_url/device_key/title/body
async fn send_bark(
    client: &reqwest::Client,
    server_url: &str,
    device_key: &str,
    event: &NotificationEvent,
) -> Result<(), String> {
    let url = format!(
        "{}/{}/{}/{}",
        server_url.trim_end_matches('/'),
        device_key,
        percent_encode(&event.title),
        percent_encode(&event.body),
    );

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| e.without_url().to_string())?;

    let status = resp.status();
    if !status.is_success() {
        return Err(format!("HTTP {status}"));
    }
    Ok(())
}

/// Generic webhook: POST JSON payload.
async fn send_webhook(
    client: &reqwest::Client,
    url: &str,
    event: &NotificationEvent,
) -> Result<(), String> {
    let fields: Vec<serde_json::Value> = event
        .fields
        .iter()
        .map(|(k, v)| serde_json::json!({ "label": k, "value": v }))
        .collect();

    let payload = serde_json::json!({
        "title": &event.title,
        "body": &event.body,
        "fields": fields,
        "source": "ai-switch",
    });

    let resp = client
        .post(url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| e.without_url().to_string())?;

    let status = resp.status();
    if !status.is_success() {
        return Err(format!("HTTP {status}"));
    }
    Ok(())
}

/// Test delivery: send a test notification to a single channel.
pub async fn test_channel(kind: &NotificationChannelKind) -> Result<(), String> {
    let client = make_client();
    let event = NotificationEvent {
        title: "AI Switch 测试通知".to_string(),
        body: "如果您收到这条消息，说明通知渠道配置正确。".to_string(),
        fields: vec![("时间".to_string(), chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string())],
    };
    send_one(&client, kind, &event).await
}

/// Simple percent-encoding for URL path segments.
fn percent_encode(s: &str) -> String {
    let mut encoded = String::with_capacity(s.len() * 3);
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => {
                encoded.push('%');
                encoded.push_str(&format!("{:02X}", byte));
            }
        }
    }
    encoded
}

fn escape_feishu(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::notification::NotificationChannelKind;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn sample_event() -> NotificationEvent {
        NotificationEvent {
            title: "账号报错".to_string(),
            body: "Codex 账号 token-exchange-abc 已连续失败 3 次".to_string(),
            fields: vec![("平台".to_string(), "Codex".to_string())],
        }
    }

    #[tokio::test]
    async fn send_webhook_posts_json() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = Vec::new();
            let mut chunk = [0u8; 1024];
            loop {
                let n = stream.read(&mut chunk).await.unwrap_or(0);
                buf.extend_from_slice(&chunk[..n]);
                if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    let len: usize = String::from_utf8_lossy(&buf[..pos])
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length:").map(|v| v.trim().parse().unwrap_or(0)))
                        .unwrap_or(0);
                    if buf.len() >= pos + 4 + len {
                        break;
                    }
                }
            }
            let _ = stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .await;
            String::from_utf8_lossy(&buf).into_owned()
        });

        let kind = NotificationChannelKind::Webhook {
            url: format!("http://{addr}/hook"),
        };
        let result = test_channel(&kind).await;
        assert!(result.is_ok(), "webhook test should succeed: {:?}", result.err());

        let request = server.await.unwrap();
        assert!(request.starts_with("POST /hook"));
    }

    #[test]
    fn escape_feishu_handles_special_chars() {
        assert_eq!(escape_feishu("a\"b\nc\\d"), "a\\\"b\\nc\\\\d");
    }
}

use super::{LogConfig, LogQueue, LogRecord, QueueBatch, QueueStats};
use async_trait::async_trait;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use tokio::io::{AsyncBufRead, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::time::timeout;
use url::Url;

const GROUP: &str = "saas-log-writers";
const CONSUMER: &str = "writer";
const IO_TIMEOUT: Duration = Duration::from_secs(3);
const RESPONSE_LIMIT: usize = 16 * 1024 * 1024;
const ENQUEUE: &str = r#"
local count = redis.call('XLEN', KEYS[1])
local bytes = tonumber(redis.call('GET', KEYS[2]) or '0')
if count ~= redis.call('HLEN', KEYS[3]) or (count > 0 and bytes <= 0) then
  return redis.error_reply('ERR inconsistent log queue metadata')
end
if count >= tonumber(ARGV[1]) or bytes + string.len(ARGV[3]) > tonumber(ARGV[2]) then return 0 end
local receipt = redis.call('XADD', KEYS[1], '*', 'record', ARGV[3])
redis.call('HSET', KEYS[3], receipt, string.len(ARGV[3]))
redis.call('INCRBY', KEYS[2], string.len(ARGV[3]))
return 1
"#;
const ACK: &str = r#"
for index = 2, #ARGV do
  local bytes = redis.call('HGET', KEYS[3], ARGV[index])
  if bytes then
    redis.call('XACK', KEYS[1], ARGV[1], ARGV[index])
    redis.call('XDEL', KEYS[1], ARGV[index])
    redis.call('HDEL', KEYS[3], ARGV[index])
    redis.call('DECRBY', KEYS[2], bytes)
  end
end
return 1
"#;
const STATS: &str = r#"
local count = redis.call('XLEN', KEYS[1])
local bytes = tonumber(redis.call('GET', KEYS[2]) or '0')
if count ~= redis.call('HLEN', KEYS[3]) or (count > 0 and bytes <= 0) or bytes < 0 then
  return redis.error_reply('ERR inconsistent log queue metadata')
end
return {count, bytes}
"#;

pub struct RedisLogQueue {
    endpoint: Endpoint,
    stream: String,
    bytes_key: String,
    sizes_key: String,
    max_records: usize,
    max_bytes: usize,
    connection: Mutex<Option<BufReader<TcpStream>>>,
}

struct Endpoint {
    host: String,
    port: u16,
    username: Option<String>,
    password: Option<String>,
    database: u32,
}

#[derive(Debug, PartialEq, Eq)]
enum Resp {
    Text(Vec<u8>),
    Integer(i64),
    Array(Vec<Resp>),
    Nil,
}

impl RedisLogQueue {
    pub async fn connect(config: &LogConfig) -> Result<Self, String> {
        config.validate()?;
        if config.instance_id == "default" {
            return Err("Redis requires a stable, unique instance ID".into());
        }
        let endpoint =
            Endpoint::parse(config.redis_url.as_deref().ok_or("Redis URL is required")?)?;
        let stream = format!("ai-switch:saas:{{{}}}:request-logs", config.instance_id);
        let queue = Self {
            endpoint,
            bytes_key: format!("{stream}:bytes"),
            sizes_key: format!("{stream}:sizes"),
            stream,
            max_records: config.max_records,
            max_bytes: config.max_bytes,
            connection: Mutex::new(None),
        };
        match queue
            .command(&["XGROUP", "CREATE", &queue.stream, GROUP, "0", "MKSTREAM"])
            .await
        {
            Ok(_) => {}
            Err(error) if error == "Redis consumer group already exists" => {}
            Err(error) => return Err(error),
        }
        queue.stats().await?;
        Ok(queue)
    }

    async fn command(&self, arguments: &[&str]) -> Result<Resp, String> {
        let mut slot = self.connection.lock().await;
        let result = timeout(IO_TIMEOUT, async {
            let mut connection = match slot.take() {
                Some(connection) => connection,
                None => self.endpoint.connect().await?,
            };
            let response = send(&mut connection, arguments).await?;
            *slot = Some(connection);
            Ok(response)
        })
        .await;
        result.map_err(|_| "Redis log operation timed out".to_string())?
    }

    async fn read(&self, maximum: usize, position: &str) -> Result<QueueBatch, String> {
        let response = self
            .command(&[
                "XREADGROUP",
                "GROUP",
                GROUP,
                CONSUMER,
                "COUNT",
                &maximum.to_string(),
                "STREAMS",
                &self.stream,
                position,
            ])
            .await?;
        let mut batch = QueueBatch::default();
        if response == Resp::Nil {
            return Ok(batch);
        }
        let Resp::Array(streams) = response else {
            return Err("invalid Redis log stream response".into());
        };
        for stream in streams {
            let Resp::Array(mut parts) = stream else {
                return Err("invalid Redis log stream response".into());
            };
            if parts.len() != 2 {
                return Err("invalid Redis log stream response".into());
            }
            let Resp::Array(entries) = parts.pop().unwrap() else {
                return Err("invalid Redis log entries".into());
            };
            for entry in entries {
                let Resp::Array(mut parts) = entry else {
                    return Err("invalid Redis log entry".into());
                };
                if parts.len() != 2 {
                    return Err("invalid Redis log entry".into());
                }
                let fields = parts.pop().unwrap();
                let Resp::Text(receipt) = parts.pop().unwrap() else {
                    return Err("invalid Redis log receipt".into());
                };
                let Resp::Array(fields) = fields else {
                    return Err("Redis log payload was removed before acknowledgement".into());
                };
                if fields.len() != 2 || fields[0] != Resp::Text(b"record".to_vec()) {
                    return Err("invalid Redis log payload fields".into());
                }
                let Resp::Text(payload) = &fields[1] else {
                    return Err("invalid Redis log payload".into());
                };
                if payload.len() > super::types::MAX_RECORD_BYTES {
                    return Err("Redis log payload exceeds record limit".into());
                }
                let record: LogRecord =
                    serde_json::from_slice(payload).map_err(|_| "invalid Redis log payload")?;
                batch.records.push(record.sanitized()?);
                let receipt =
                    String::from_utf8(receipt).map_err(|_| "invalid Redis log receipt")?;
                if receipt.len() > 64
                    || !receipt
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || byte == b'-')
                {
                    return Err("invalid Redis log receipt".into());
                }
                batch.receipts.push(receipt);
            }
        }
        Ok(batch)
    }
}

#[async_trait]
impl LogQueue for RedisLogQueue {
    async fn try_enqueue(&self, record: LogRecord) -> Result<bool, String> {
        let payload =
            serde_json::to_string(&record.sanitized()?).map_err(|_| "log serialization failed")?;
        if payload.len() > self.max_bytes {
            return Err("request log exceeds queue byte capacity".into());
        }
        match self
            .command(&[
                "EVAL",
                ENQUEUE,
                "3",
                &self.stream,
                &self.bytes_key,
                &self.sizes_key,
                &self.max_records.to_string(),
                &self.max_bytes.to_string(),
                &payload,
            ])
            .await?
        {
            Resp::Integer(1) => Ok(true),
            Resp::Integer(0) => Ok(false),
            _ => Err("invalid Redis log enqueue response".into()),
        }
    }
    async fn receive(&self, maximum: usize) -> Result<QueueBatch, String> {
        if maximum == 0 || maximum > 500 {
            return Err("invalid log batch size".into());
        }
        let pending = self.read(maximum, "0").await?;
        if !pending.records.is_empty() {
            return Ok(pending);
        }
        self.read(maximum, ">").await
    }
    async fn ack(&self, batch: &QueueBatch) -> Result<(), String> {
        if batch.receipts.is_empty() {
            return Ok(());
        }
        let mut arguments = vec![
            "EVAL",
            ACK,
            "3",
            &self.stream,
            &self.bytes_key,
            &self.sizes_key,
            GROUP,
        ];
        arguments.extend(batch.receipts.iter().map(String::as_str));
        match self.command(&arguments).await? {
            Resp::Integer(1) => Ok(()),
            _ => Err("invalid Redis log acknowledgement response".into()),
        }
    }
    async fn stats(&self) -> Result<QueueStats, String> {
        let result = self
            .command(&[
                "EVAL",
                STATS,
                "3",
                &self.stream,
                &self.bytes_key,
                &self.sizes_key,
            ])
            .await?;
        if let Resp::Array(values) = result {
            if let [Resp::Integer(pending), Resp::Integer(bytes)] = values.as_slice() {
                return Ok(QueueStats {
                    pending: usize::try_from(*pending).map_err(|_| "invalid Redis queue count")?,
                    pending_bytes: usize::try_from(*bytes)
                        .map_err(|_| "invalid Redis queue size")?,
                });
            }
        }
        Err("invalid Redis log status response".into())
    }
}

impl Endpoint {
    fn parse(value: &str) -> Result<Self, String> {
        let url = Url::parse(value).map_err(|_| "invalid Redis URL")?;
        if url.scheme() != "redis" || url.query().is_some() || url.fragment().is_some() {
            return Err(
                "Redis requires redis:// TCP; use a TLS tunnel for encrypted transport".into(),
            );
        }
        let host = url
            .host_str()
            .ok_or("Redis host is required")?
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_owned();
        let database = url.path().trim_start_matches('/');
        let database = if database.is_empty() {
            0
        } else {
            database
                .parse::<u32>()
                .map_err(|_| "invalid Redis database")?
        };
        let username = if url.username().is_empty() {
            None
        } else {
            Some(decode_component(url.username())?)
        };
        let password = url.password().map(decode_component).transpose()?;
        if username.is_some() && password.is_none() {
            return Err("Redis username requires a password".into());
        }
        Ok(Self {
            host,
            port: url.port().unwrap_or(6379),
            username,
            password,
            database,
        })
    }

    async fn connect(&self) -> Result<BufReader<TcpStream>, String> {
        let stream = TcpStream::connect((self.host.as_str(), self.port))
            .await
            .map_err(|_| "Redis log connection failed")?;
        stream
            .set_nodelay(true)
            .map_err(|_| "Redis socket configuration failed")?;
        let mut connection = BufReader::new(stream);
        if let Some(password) = &self.password {
            match &self.username {
                Some(username) => {
                    send(&mut connection, &["AUTH", username, password]).await?;
                }
                None => {
                    send(&mut connection, &["AUTH", password]).await?;
                }
            }
        }
        if self.database != 0 {
            send(&mut connection, &["SELECT", &self.database.to_string()]).await?;
        }
        send(&mut connection, &["PING"]).await?;
        Ok(connection)
    }
}

fn decode_component(value: &str) -> Result<String, String> {
    let mut decoded = Vec::new();
    let mut bytes = value.bytes();
    while let Some(byte) = bytes.next() {
        if byte != b'%' {
            decoded.push(byte);
            continue;
        }
        let high = bytes
            .next()
            .and_then(|byte| (byte as char).to_digit(16))
            .ok_or("invalid Redis credential encoding")?;
        let low = bytes
            .next()
            .and_then(|byte| (byte as char).to_digit(16))
            .ok_or("invalid Redis credential encoding")?;
        decoded.push((high * 16 + low) as u8);
    }
    String::from_utf8(decoded).map_err(|_| "invalid Redis credential encoding".into())
}

async fn send(connection: &mut BufReader<TcpStream>, arguments: &[&str]) -> Result<Resp, String> {
    let mut request = format!("*{}\r\n", arguments.len()).into_bytes();
    for argument in arguments {
        request.extend_from_slice(format!("${}\r\n", argument.len()).as_bytes());
        request.extend_from_slice(argument.as_bytes());
        request.extend_from_slice(b"\r\n");
    }
    connection
        .get_mut()
        .write_all(&request)
        .await
        .map_err(|_| "Redis log send failed")?;
    let mut budget = RESPONSE_LIMIT;
    read_resp(connection, &mut budget, 0).await
}

fn read_resp<'reader, Reader>(
    reader: &'reader mut Reader,
    budget: &'reader mut usize,
    depth: usize,
) -> Pin<Box<dyn Future<Output = Result<Resp, String>> + Send + 'reader>>
where
    Reader: AsyncBufRead + Unpin + Send + 'reader,
{
    Box::pin(async move {
        if depth > 8 || *budget == 0 {
            return Err("Redis response limit exceeded".into());
        }
        let prefix = reader
            .read_u8()
            .await
            .map_err(|_| "Redis log connection interrupted")?;
        *budget -= 1;
        let mut line = Vec::new();
        loop {
            if line.len() >= 1024 || *budget == 0 {
                return Err("Redis response header limit exceeded".into());
            }
            let byte = reader
                .read_u8()
                .await
                .map_err(|_| "Redis log connection interrupted")?;
            *budget -= 1;
            line.push(byte);
            if byte == b'\n' {
                break;
            }
        }
        if !line.ends_with(b"\r\n") {
            return Err("invalid Redis response framing".into());
        }
        line.truncate(line.len() - 2);
        if prefix == b'+' {
            return Ok(Resp::Text(line));
        }
        if prefix == b'-' {
            let code = line.split(|byte| *byte == b' ').next().unwrap_or_default();
            return Err(match code {
                b"BUSYGROUP" => "Redis consumer group already exists",
                b"NOAUTH" | b"WRONGPASS" => "Redis authentication failed",
                b"NOPERM" => "Redis log permission denied",
                _ => "Redis log command failed",
            }
            .into());
        }
        let length = std::str::from_utf8(&line)
            .ok()
            .and_then(|value| value.parse::<i64>().ok())
            .ok_or("invalid Redis response number")?;
        match prefix {
            b':' => Ok(Resp::Integer(length)),
            b'$' | b'*' if length == -1 => Ok(Resp::Nil),
            b'$' => {
                let length = usize::try_from(length).map_err(|_| "invalid Redis bulk length")?;
                if length.saturating_add(2) > *budget {
                    return Err("Redis response size limit exceeded".into());
                }
                *budget -= length + 2;
                let mut bytes = vec![0; length];
                reader
                    .read_exact(&mut bytes)
                    .await
                    .map_err(|_| "Redis log connection interrupted")?;
                let mut tail = [0; 2];
                reader
                    .read_exact(&mut tail)
                    .await
                    .map_err(|_| "Redis log connection interrupted")?;
                if tail != *b"\r\n" {
                    return Err("invalid Redis response framing".into());
                }
                Ok(Resp::Text(bytes))
            }
            b'*' => {
                let length = usize::try_from(length).map_err(|_| "invalid Redis array length")?;
                if length > 5000 || length > *budget {
                    return Err("Redis response array limit exceeded".into());
                }
                let mut values = Vec::with_capacity(length);
                for _ in 0..length {
                    values.push(read_resp(reader, budget, depth + 1).await?);
                }
                Ok(Resp::Array(values))
            }
            _ => Err("unsupported Redis response type".into()),
        }
    })
}

#[cfg(test)]
pub(super) async fn cleanup_test_queue(queue: &RedisLogQueue) {
    assert_eq!(queue.stats().await.unwrap().pending, 0);
    queue
        .command(&["DEL", &queue.stream, &queue.bytes_key, &queue.sizes_key])
        .await
        .unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn invalid_redis_configuration_fails_without_echoing_credentials() {
        for url in [
            "redis://:SECRET@localhost/not-a-db",
            "rediss://:SECRET@localhost",
            "http://:SECRET@localhost",
            "redis://:SECRET@localhost/0?ignored=true",
        ] {
            let result = RedisLogQueue::connect(&LogConfig {
                queue: super::super::LogQueueKind::Redis,
                redis_url: Some(url.into()),
                instance_id: "test-config".into(),
                ..LogConfig::default()
            })
            .await;
            assert!(result.is_err());
            assert!(!result.err().unwrap().contains("SECRET"));
        }
    }

    #[tokio::test]
    async fn unavailable_redis_is_an_error_not_an_in_memory_fallback() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let result = RedisLogQueue::connect(&LogConfig {
            queue: super::super::LogQueueKind::Redis,
            redis_url: Some(format!("redis://:SECRET@127.0.0.1:{port}/0")),
            instance_id: "test-connect".into(),
            ..LogConfig::default()
        })
        .await;
        assert!(result.is_err());
        assert!(!result.err().unwrap().contains("SECRET"));
    }

    #[tokio::test]
    async fn resp_parser_handles_binary_payloads_and_rejects_unbounded_or_malformed_frames() {
        let mut reader = BufReader::new(&b"*3\r\n:2\r\n$3\r\na\0b\r\n$-1\r\n"[..]);
        let mut budget = 1024;
        assert_eq!(
            read_resp(&mut reader, &mut budget, 0).await.unwrap(),
            Resp::Array(vec![
                Resp::Integer(2),
                Resp::Text(b"a\0b".to_vec()),
                Resp::Nil
            ])
        );
        for frame in [
            &b"$9999999999\r\n"[..],
            &b"*999999\r\n"[..],
            &b"$1\r\naXX"[..],
            &b"+bad\n"[..],
            &b"-NOPERM SECRET\r\n"[..],
        ] {
            let mut reader = BufReader::new(frame);
            let mut budget = 1024;
            let error = read_resp(&mut reader, &mut budget, 0).await.unwrap_err();
            assert!(!error.contains("SECRET"));
        }
    }

    #[test]
    fn redis_url_decodes_credentials_without_decoding_them_into_other_fields() {
        let endpoint =
            Endpoint::parse("redis://name%3Auser:p%40ss%2Fword@127.0.0.1:6380/2").unwrap();
        assert_eq!(endpoint.username.as_deref(), Some("name:user"));
        assert_eq!(endpoint.password.as_deref(), Some("p@ss/word"));
        assert_eq!(endpoint.port, 6380);
        assert_eq!(endpoint.database, 2);
        assert_eq!(endpoint.host, "127.0.0.1");
    }
}

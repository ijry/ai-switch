use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use url::Url;
use uuid::Uuid;

pub type MobileTokenRegistry = Arc<Mutex<HashMap<String, SystemTime>>>;

const MOBILE_TOKEN_TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60);
const PERSISTED_TOKEN_VERSION: u8 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MobilePairingPayload {
    #[serde(rename = "v")]
    pub version: u8,
    pub public_url: Option<String>,
    pub private_url: Option<String>,
    pub pairing_code: String,
    pub expires_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MobilePairingRedeemResponse {
    pub token: String,
    pub expires_at: u64,
}

#[derive(Debug, Clone)]
struct PairingGrant {
    token: String,
    expires_at: SystemTime,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedMobileTokens {
    version: u8,
    tokens: HashMap<String, u64>,
}

#[derive(Clone, Default)]
pub struct MobilePairingStore {
    grants: Arc<Mutex<HashMap<String, PairingGrant>>>,
    mobile_tokens: MobileTokenRegistry,
    persistence_lock: Arc<Mutex<()>>,
}

impl MobilePairingStore {
    pub async fn create(
        &self,
        public_url: Option<String>,
        private_url: Option<String>,
        now: SystemTime,
        ttl: Duration,
    ) -> Result<MobilePairingPayload, String> {
        let public_url = normalize_optional_url(public_url)?;
        let private_url = normalize_optional_url(private_url)?;
        if public_url.is_none() && private_url.is_none() {
            return Err("at least one remote URL is required".to_string());
        }

        let pairing_code = format!("pair_{}", Uuid::new_v4().simple());
        let token = format!("ms_{}", Uuid::new_v4().simple());
        let expires_at = now
            .checked_add(ttl)
            .ok_or_else(|| "pairing expiration is out of range".to_string())?;
        let grant = PairingGrant { token, expires_at };
        self.grants
            .lock()
            .await
            .insert(digest(&pairing_code), grant);

        Ok(MobilePairingPayload {
            version: 1,
            public_url,
            private_url,
            pairing_code,
            expires_at: epoch_millis(expires_at),
        })
    }

    pub async fn redeem(
        &self,
        pairing_code: &str,
        now: SystemTime,
    ) -> Result<MobilePairingRedeemResponse, String> {
        let code = pairing_code.trim();
        if code.is_empty() {
            return Err("pairing code is required".to_string());
        }
        let grant = self.grants.lock().await.remove(&digest(code));
        let Some(grant) = grant else {
            return Err("pairing code is invalid or already used".to_string());
        };
        if grant.expires_at <= now {
            return Err("pairing code has expired".to_string());
        }

        let token = grant.token;
        let token_expires_at = now
            .checked_add(MOBILE_TOKEN_TTL)
            .ok_or_else(|| "mobile token expiration is out of range".to_string())?;
        self.mobile_tokens
            .lock()
            .await
            .insert(digest(&token), token_expires_at);
        Ok(MobilePairingRedeemResponse {
            token,
            expires_at: epoch_millis(token_expires_at),
        })
    }

    pub async fn is_mobile_token_valid(&self, token: &str, now: SystemTime) -> bool {
        let key = digest(token.trim());
        let mut tokens = self.mobile_tokens.lock().await;
        tokens.retain(|_, expires_at| *expires_at > now);
        tokens.contains_key(&key)
    }

    /// Restores the non-secret token registry from disk. Only SHA-256
    /// digests and expiry timestamps are stored; the usable token is never
    /// written to this file.
    pub async fn load_tokens(&self, path: &Path, now: SystemTime) -> Result<(), String> {
        let bytes = match tokio::fs::read(path).await {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(format!("could not read mobile token registry: {error}")),
        };
        let persisted: PersistedMobileTokens = serde_json::from_slice(&bytes)
            .map_err(|error| format!("could not parse mobile token registry: {error}"))?;
        if persisted.version != PERSISTED_TOKEN_VERSION {
            return Err("unsupported mobile token registry version".to_string());
        }

        let mut restored = HashMap::new();
        for (digest, expires_at) in persisted.tokens {
            if !is_digest(&digest) {
                continue;
            }
            let Some(expires_at) = UNIX_EPOCH.checked_add(Duration::from_millis(expires_at)) else {
                continue;
            };
            if expires_at > now {
                restored.insert(digest, expires_at);
            }
        }

        let _persist_guard = self.persistence_lock.lock().await;
        let mut tokens = self.mobile_tokens.lock().await;
        *tokens = restored;
        Ok(())
    }

    /// Persists only token digests and expiry timestamps using a temporary
    /// file replacement so a crash cannot leave a half-written registry.
    pub async fn persist_tokens(&self, path: &Path, now: SystemTime) -> Result<(), String> {
        let _persist_guard = self.persistence_lock.lock().await;
        let tokens = {
            let mut registry = self.mobile_tokens.lock().await;
            registry.retain(|_, expires_at| *expires_at > now);
            registry
                .iter()
                .map(|(digest, expires_at)| (digest.clone(), epoch_millis(*expires_at)))
                .collect::<HashMap<_, _>>()
        };
        let payload = PersistedMobileTokens {
            version: PERSISTED_TOKEN_VERSION,
            tokens,
        };
        let serialized = serde_json::to_vec_pretty(&payload)
            .map_err(|error| format!("could not serialize mobile token registry: {error}"))?;
        let parent = path
            .parent()
            .ok_or_else(|| "mobile token registry path has no parent".to_string())?;
        tokio::fs::create_dir_all(parent).await.map_err(|error| {
            format!("could not create mobile token registry directory: {error}")
        })?;

        let temp_path = parent.join(format!(".mobile-tokens-{}.tmp", Uuid::new_v4().simple()));
        if let Err(error) = tokio::fs::write(&temp_path, serialized).await {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(format!("could not write mobile token registry: {error}"));
        }
        if let Err(error) = set_private_file_permissions(&temp_path).await {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(format!("could not protect mobile token registry: {error}"));
        }
        if cfg!(windows) {
            let _ = tokio::fs::remove_file(path).await;
        }
        if let Err(error) = tokio::fs::rename(&temp_path, path).await {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(format!("could not replace mobile token registry: {error}"));
        }
        Ok(())
    }

    pub fn mobile_token_registry(&self) -> MobileTokenRegistry {
        Arc::clone(&self.mobile_tokens)
    }

    #[cfg(test)]
    pub async fn debug_contains_plaintext_code(&self, code: &str) -> bool {
        self.grants.lock().await.contains_key(code)
    }
}

fn digest(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn is_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(unix)]
async fn set_private_file_permissions(path: &Path) -> Result<(), std::io::Error> {
    use std::os::unix::fs::PermissionsExt;
    tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).await
}

#[cfg(not(unix))]
async fn set_private_file_permissions(_path: &Path) -> Result<(), std::io::Error> {
    Ok(())
}

fn normalize_optional_url(value: Option<String>) -> Result<Option<String>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim().trim_end_matches('/').to_string();
    if value.is_empty() {
        return Ok(None);
    }
    let parsed =
        Url::parse(&value).map_err(|_| "pairing URL must be a valid HTTPS URL".to_string())?;
    if parsed.scheme() != "https"
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err("pairing URL must be a valid HTTPS URL".to_string());
    }
    Ok(Some(value))
}

fn epoch_millis(value: SystemTime) -> u64 {
    value
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::{digest, MobilePairingStore};
    use std::time::{Duration, UNIX_EPOCH};

    #[tokio::test]
    async fn persists_only_token_hashes_and_restores_unexpired_tokens() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("mobile-tokens.json");
        let now = UNIX_EPOCH + Duration::from_secs(10_000);
        let store = MobilePairingStore::default();
        let payload = store
            .create(
                Some("https://public.example".to_string()),
                None,
                now,
                Duration::from_secs(60),
            )
            .await
            .unwrap();
        let redeemed = store
            .redeem(&payload.pairing_code, now + Duration::from_secs(1))
            .await
            .unwrap();

        store
            .persist_tokens(&path, now + Duration::from_secs(1))
            .await
            .unwrap();
        let contents = tokio::fs::read_to_string(&path).await.unwrap();
        assert!(!contents.contains(&redeemed.token));
        assert!(contents.contains(&digest(&redeemed.token)));

        let restored = MobilePairingStore::default();
        restored
            .load_tokens(&path, now + Duration::from_secs(2))
            .await
            .unwrap();
        assert!(
            restored
                .is_mobile_token_valid(&redeemed.token, now + Duration::from_secs(2))
                .await
        );
    }

    #[tokio::test]
    async fn rejects_non_https_pairing_urls() {
        let store = MobilePairingStore::default();
        let now = UNIX_EPOCH + Duration::from_secs(10_000);

        assert!(store
            .create(
                Some("http://public.example".to_string()),
                None,
                now,
                Duration::from_secs(60),
            )
            .await
            .is_err());
        assert!(store
            .create(
                Some("https://public.example/".to_string()),
                None,
                now,
                Duration::from_secs(60),
            )
            .await
            .is_ok());
    }
}

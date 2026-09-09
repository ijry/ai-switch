use crate::error::AppError;
use crate::saas::repository::{self, db_error, invalid};
use crate::security::{KeyringSecretStore, SecretStore};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{SqliteConnection, SqlitePool};

const SAAS_BETA_CODES: [&str; 2] = ["ai-switch-ok", "ai-switch-nb"];
const ACTIVATION_KEY: &str = "activation_unlocked";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SaasConfig {
    pub enabled: bool,
    pub registration_enabled: bool,
    pub password_login_enabled: bool,
    pub site_name: String,
    pub public_base_url: String,
    pub github_client_id: String,
    pub github_client_secret_configured: bool,
    pub exchange_rate_micros: Option<i64>,
    pub checkin_enabled: bool,
    pub checkin_reward_micros: i64,
    pub invite_enabled: bool,
    pub invite_registration_required: bool,
    pub invite_signup_reward_micros: i64,
    pub invite_recharge_rate_micros: i64,
    pub logs: Value,
}

impl Default for SaasConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            registration_enabled: true,
            password_login_enabled: true,
            site_name: "AI Switch".into(),
            public_base_url: String::new(),
            github_client_id: String::new(),
            github_client_secret_configured: false,
            exchange_rate_micros: None,
            checkin_enabled: false,
            checkin_reward_micros: 0,
            invite_enabled: false,
            invite_registration_required: false,
            invite_signup_reward_micros: 0,
            invite_recharge_rate_micros: 0,
            logs: json!({}),
        }
    }
}

pub fn validate_public_base_url(value: &str) -> Result<url::Url, AppError> {
    let parsed = url::Url::parse(value)
        .map_err(|_| invalid("saas.config_url", "A valid public origin is required"))?;
    let loopback = match parsed.host() {
        Some(url::Host::Domain(host)) => host == "localhost",
        Some(url::Host::Ipv4(address)) => address.is_loopback(),
        Some(url::Host::Ipv6(address)) => address.is_loopback(),
        None => false,
    };
    if parsed.host().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || parsed.path() != "/"
        || !(parsed.scheme() == "https" || (parsed.scheme() == "http" && loopback))
    {
        return Err(invalid(
            "saas.config_url",
            "Public URL must be an HTTPS origin (HTTP is allowed only on loopback)",
        ));
    }
    Ok(parsed)
}

pub(crate) async fn load_connection(
    connection: &mut SqliteConnection,
) -> Result<SaasConfig, AppError> {
    let raw: Option<String> =
        sqlx::query_scalar("SELECT value_json FROM saas_settings WHERE key='config'")
            .fetch_optional(connection)
            .await
            .map_err(db_error)?;
    let mut config = match raw {
        Some(raw) => serde_json::from_str(&raw).map_err(|_| {
            invalid(
                "saas.config_invalid",
                "Stored SaaS configuration is invalid",
            )
        })?,
        None => SaasConfig::default(),
    };
    if env_secret().is_some() {
        config.github_client_secret_configured = true;
    }
    Ok(config)
}

pub async fn load(pool: &SqlitePool) -> Result<SaasConfig, AppError> {
    load_connection(&mut *pool.acquire().await.map_err(db_error)?).await
}

pub async fn load_with_secret_store(
    pool: &SqlitePool,
    _: &dyn SecretStore,
) -> Result<SaasConfig, AppError> {
    load(pool).await
}

async fn is_unlocked_connection(connection: &mut SqliteConnection) -> Result<bool, AppError> {
    let value: Option<String> =
        sqlx::query_scalar("SELECT value_json FROM saas_settings WHERE key=?")
            .bind(ACTIVATION_KEY)
            .fetch_optional(&mut *connection)
            .await
            .map_err(db_error)?;
    if value.as_deref() == Some("true") {
        return Ok(true);
    }
    Ok(load_connection(connection).await?.enabled)
}

pub async fn is_unlocked(pool: &SqlitePool) -> Result<bool, AppError> {
    is_unlocked_connection(&mut *pool.acquire().await.map_err(db_error)?).await
}

pub async fn unlock(pool: &SqlitePool, activation_code: &str) -> Result<bool, AppError> {
    if !SAAS_BETA_CODES.contains(&activation_code.trim()) {
        return Err(invalid(
            "saas.activation_code_invalid",
            "The SaaS beta access code is invalid",
        ));
    }
    sqlx::query("INSERT INTO saas_settings(key,value_json,updated_at) VALUES(?, 'true', ?) ON CONFLICT(key) DO UPDATE SET value_json='true',updated_at=excluded.updated_at")
        .bind(ACTIVATION_KEY)
        .bind(repository::now())
        .execute(pool)
        .await
        .map_err(db_error)?;
    Ok(true)
}

pub(crate) async fn require_enabled(
    connection: &mut SqliteConnection,
) -> Result<SaasConfig, AppError> {
    let config = load_connection(connection).await?;
    if !config.enabled {
        return Err(invalid("saas.disabled", "SaaS is disabled"));
    }
    Ok(config)
}

pub async fn public_config(pool: &SqlitePool) -> Result<Value, AppError> {
    let config = load(pool).await?;
    Ok(
        json!({"enabled":config.enabled,"registrationEnabled":config.registration_enabled,"passwordLoginEnabled":config.password_login_enabled,"siteName":config.site_name,"publicBaseUrl":config.public_base_url,"exchangeRateMicros":config.exchange_rate_micros,
        "checkinEnabled":config.checkin_enabled,"checkinRewardMicros":config.checkin_reward_micros,"inviteEnabled":config.invite_enabled,"inviteRegistrationRequired":config.invite_registration_required,
        "githubLoginAvailable":config.enabled && config.github_client_secret_configured && !config.github_client_id.is_empty()}),
    )
}

fn env_secret() -> Option<String> {
    std::env::var("AI_SWITCH_SAAS_GITHUB_CLIENT_SECRET")
        .ok()
        .filter(|value| !value.trim().is_empty())
}
pub async fn apply_env_config(pool: &SqlitePool) -> Result<(), AppError> {
    let enabled = match env_text("AI_SWITCH_SAAS_ENABLE") {
        None => return Ok(()),
        Some(value) if value.trim() == "1" || value.eq_ignore_ascii_case("true") => true,
        Some(value) if value.trim() == "0" || value.eq_ignore_ascii_case("false") => false,
        Some(_) => {
            return Err(invalid(
                "saas.config_enable",
                "AI_SWITCH_SAAS_ENABLE must be 1, 0, true, or false",
            ))
        }
    };

    if !enabled {
        let config = load(pool).await?;
        if config.enabled {
            save(pool, json!({"enabled": false})).await?;
        }
        return Ok(());
    }

    let activation_code =
        env_text("AI_SWITCH_SAAS_ACTIVATION_CODE").unwrap_or_else(|| "ai-switch-ok".to_string());
    unlock(pool, &activation_code).await?;

    let logs = json!({
        "queue": env_text("AI_SWITCH_SAAS_LOGS_QUEUE").unwrap_or_else(|| "redis".to_string()),
        "store": env_text("AI_SWITCH_SAAS_LOGS_STORE").unwrap_or_else(|| "postgres".to_string()),
        "redisUrlEnv": env_text("AI_SWITCH_SAAS_LOGS_REDIS_URL_ENV").unwrap_or_else(|| "SAAS_LOGS_REDIS_URL".to_string()),
        "postgresUrlEnv": env_text("AI_SWITCH_SAAS_LOGS_POSTGRES_URL_ENV").unwrap_or_else(|| "SAAS_LOGS_POSTGRES_URL".to_string()),
    });

    save_instance_id(pool).await?;

    let payload = json!({
        "enabled": true,
        "siteName": env_text("AI_SWITCH_SAAS_SITE_NAME").unwrap_or_else(|| "AI Switch".to_string()),
        "publicBaseUrl": env_text("AI_SWITCH_SAAS_PUBLIC_BASE_URL").unwrap_or_default(),
        "githubClientId": env_text("AI_SWITCH_SAAS_GITHUB_CLIENT_ID").unwrap_or_default(),
        "registrationEnabled": env_flag("AI_SWITCH_SAAS_REGISTRATION_ENABLED").unwrap_or(true),
        "passwordLoginEnabled": env_flag("AI_SWITCH_SAAS_PASSWORD_LOGIN_ENABLED").unwrap_or(true),
        "exchangeRateMicros": parse_env_i64("AI_SWITCH_SAAS_EXCHANGE_RATE_MICROS"),
        "checkinEnabled": env_flag("AI_SWITCH_SAAS_CHECKIN_ENABLED").unwrap_or(false),
        "checkinRewardMicros": env_i64("AI_SWITCH_SAAS_CHECKIN_REWARD_MICROS").unwrap_or(0),
        "inviteEnabled": env_flag("AI_SWITCH_SAAS_INVITE_ENABLED").unwrap_or(false),
        "inviteRegistrationRequired": env_flag("AI_SWITCH_SAAS_INVITE_REGISTRATION_REQUIRED").unwrap_or(false),
        "inviteSignupRewardMicros": env_i64("AI_SWITCH_SAAS_INVITE_SIGNUP_REWARD_MICROS").unwrap_or(0),
        "inviteRechargeRateMicros": env_i64("AI_SWITCH_SAAS_INVITE_RECHARGE_RATE_MICROS").unwrap_or(0),
        "logs": logs,
    });
    save(pool, payload).await?;
    Ok(())
}

fn env_text(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn env_flag(name: &str) -> Option<bool> {
    env_text(name).map(|value| value.trim() == "1" || value.eq_ignore_ascii_case("true"))
}

fn env_i64(name: &str) -> Option<i64> {
    env_text(name).and_then(|value| value.trim().parse().ok())
}

fn parse_env_i64(name: &str) -> Value {
    env_i64(name).map(Value::from).unwrap_or(Value::Null)
}

async fn save_instance_id(pool: &SqlitePool) -> Result<(), AppError> {
    let instance_id =
        env_text("AI_SWITCH_SAAS_INSTANCE_ID").unwrap_or_else(|| "docker".to_string());
    if instance_id.trim().is_empty() {
        return Err(invalid("saas.config_logs", "SaaS instance ID is required"));
    }
    sqlx::query("INSERT INTO saas_settings(key,value_json,updated_at) VALUES('instance_id',?,?) ON CONFLICT(key) DO NOTHING")
        .bind(serde_json::to_string(&instance_id).map_err(|_| invalid("saas.config_invalid", "Invalid SaaS instance identifier"))?)
        .bind(repository::now())
        .execute(pool)
        .await
        .map_err(db_error)?;
    Ok(())
}

async fn secret_key(connection: &mut SqliteConnection) -> Result<String, AppError> {
    let raw: String =
        sqlx::query_scalar("SELECT value_json FROM saas_settings WHERE key='instance_id'")
            .fetch_one(connection)
            .await
            .map_err(db_error)?;
    let instance: String = serde_json::from_str(&raw)
        .map_err(|_| invalid("saas.config_invalid", "Invalid SaaS instance identifier"))?;
    Ok(format!("saas.{instance}.github"))
}

pub async fn github_secret_with_store(
    pool: &SqlitePool,
    store: &dyn SecretStore,
) -> Result<String, AppError> {
    if let Some(secret) = env_secret() {
        return Ok(secret);
    }
    let key = secret_key(&mut *pool.acquire().await.map_err(db_error)?).await?;
    store
        .get_secret(&key)
        .map_err(|_| invalid("saas.oauth_secret", "GitHub client secret is unavailable"))
}

pub async fn github_secret(pool: &SqlitePool) -> Result<String, AppError> {
    github_secret_with_store(pool, &KeyringSecretStore::new("ai-switch.saas")).await
}

pub async fn save(pool: &SqlitePool, payload: Value) -> Result<SaasConfig, AppError> {
    save_with_secret_store(pool, payload, &KeyringSecretStore::new("ai-switch.saas")).await
}

pub async fn save_with_secret_store(
    pool: &SqlitePool,
    payload: Value,
    store: &dyn SecretStore,
) -> Result<SaasConfig, AppError> {
    let object = payload
        .as_object()
        .ok_or_else(|| invalid("saas.validation", "Configuration must be an object"))?;
    let allowed = [
        "enabled",
        "registrationEnabled",
        "passwordLoginEnabled",
        "siteName",
        "publicBaseUrl",
        "githubClientId",
        "githubClientSecret",
        "githubClientSecretConfigured",
        "exchangeRateMicros",
        "checkinEnabled",
        "checkinRewardMicros",
        "inviteEnabled",
        "inviteRegistrationRequired",
        "inviteSignupRewardMicros",
        "inviteRechargeRateMicros",
        "logs",
    ];
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(invalid("saas.validation", "Unknown configuration field"));
    }
    let mut transaction = repository::begin(pool).await?;
    let old = load_connection(&mut transaction).await?;
    let mut merged = serde_json::to_value(&old)
        .map_err(|_| invalid("saas.config_invalid", "Invalid SaaS configuration"))?;
    for (key, value) in object {
        if key != "githubClientSecret" && key != "githubClientSecretConfigured" {
            merged[key] = value.clone();
        }
    }
    let mut config: SaasConfig = serde_json::from_value(merged)
        .map_err(|_| invalid("saas.validation", "Invalid configuration fields"))?;
    let new_secret = match object.get("githubClientSecret") {
        None | Some(Value::Null) => None,
        Some(Value::String(secret)) if secret.is_empty() => None,
        Some(Value::String(secret)) if secret.len() <= 4096 => Some(secret.as_str()),
        _ => return Err(invalid("saas.validation", "Invalid GitHub client secret")),
    };
    config.github_client_secret_configured =
        old.github_client_secret_configured || new_secret.is_some() || env_secret().is_some();
    config.site_name = config.site_name.trim().into();
    if config.site_name.is_empty() || config.site_name.len() > 120 {
        return Err(invalid(
            "saas.validation",
            "Site name must contain 1–120 bytes",
        ));
    }
    if let Some(rate) = config.exchange_rate_micros {
        if !(1..=repository::MAX_MONEY).contains(&rate) {
            return Err(invalid(
                "saas.exchange_rate",
                "Exchange rate must be a positive integer",
            ));
        }
    }
    if !(0..=repository::MAX_MONEY).contains(&config.checkin_reward_micros)
        || !(0..=repository::MAX_MONEY).contains(&config.invite_signup_reward_micros)
        || !(0..=1_000_000).contains(&config.invite_recharge_rate_micros)
    {
        return Err(invalid(
            "saas.validation",
            "Invalid growth reward configuration",
        ));
    }
    if !config.public_base_url.is_empty() {
        config.public_base_url = validate_public_base_url(&config.public_base_url)?
            .origin()
            .ascii_serialization();
    }
    if !config.logs.is_object() || config.logs.to_string().len() > 32_768 {
        return Err(invalid("saas.config_logs", "Invalid log configuration"));
    }
    reject_inline_log_secrets(&config.logs)?;
    if config.enabled && !old.enabled {
        if !is_unlocked_connection(&mut transaction).await? {
            return Err(invalid(
                "saas.activation_required",
                "Unlock the SaaS beta before enabling the public site",
            ));
        }
    }
    if let Some(secret) = new_secret {
        let key = secret_key(&mut transaction).await?;
        store.set_secret(&key, secret).map_err(|_| {
            invalid(
                "saas.secret_save",
                "Could not securely save GitHub client secret",
            )
        })?;
    }
    if old.enabled
        && (!config.enabled
            || old.public_base_url != config.public_base_url
            || old.github_client_id != config.github_client_id)
    {
        sqlx::query("UPDATE saas_sessions SET revoked_at=? WHERE revoked_at IS NULL")
            .bind(repository::now())
            .execute(&mut *transaction)
            .await
            .map_err(db_error)?;
        sqlx::query("DELETE FROM saas_oauth_states")
            .execute(&mut *transaction)
            .await
            .map_err(db_error)?;
    }
    sqlx::query("INSERT INTO saas_settings(key,value_json,updated_at) VALUES('config',?,?) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json,updated_at=excluded.updated_at")
        .bind(serde_json::to_string(&config).map_err(|_| invalid("saas.config_invalid", "Invalid configuration"))?).bind(repository::now()).execute(&mut *transaction).await.map_err(db_error)?;
    repository::audit(
        &mut transaction,
        "config.save",
        None,
        json!({"enabled":config.enabled,"changedFields":object.keys().collect::<Vec<_>>()}),
    )
    .await?;
    transaction.commit().await.map_err(db_error)?;
    Ok(config)
}

fn reject_inline_log_secrets(value: &Value) -> Result<(), AppError> {
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                let lower = key.to_ascii_lowercase();
                if ["password", "secret", "token"]
                    .iter()
                    .any(|needle| lower.contains(needle))
                    && !lower.ends_with("env")
                    && !lower.ends_with("configured")
                    && !lower.ends_with("ref")
                    && !child.is_null()
                {
                    return Err(invalid(
                        "saas.config_logs",
                        "Log secrets must use environment references",
                    ));
                }
                reject_inline_log_secrets(child)?;
            }
        }
        Value::Array(items) => {
            for child in items {
                reject_inline_log_secrets(child)?;
            }
        }
        Value::String(text) => {
            if let Ok(url) = url::Url::parse(text) {
                if url.password().is_some() || !url.username().is_empty() || url.query().is_some() {
                    return Err(invalid(
                        "saas.config_logs",
                        "Log connection credentials must use environment references",
                    ));
                }
            }
        }
        _ => (),
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests;

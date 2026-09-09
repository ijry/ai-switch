pub mod auth;
pub mod billing;
pub mod config;
pub mod domain;
pub mod logs;
pub mod proxy;
pub mod repository;
pub mod transport;

use crate::app_state::AppState;
use crate::error::AppError;
use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, OnceCell};

#[derive(Clone, Default)]
pub struct SaasRuntime {
    pub logs: logs::LogRuntime,
    initialized: Arc<OnceCell<()>>,
    transition: Arc<Mutex<()>>,
    limits: Arc<Mutex<HashMap<String, (Instant, u32)>>>,
    log_startup_error: Arc<Mutex<Option<String>>>,
}

impl SaasRuntime {
    pub async fn initialize(&self, pool: &SqlitePool) -> Result<(), AppError> {
        self.initialized
            .get_or_try_init(|| async {
                repository::migrate(pool).await?;
                billing::recover_pending(pool).await?;
                let config = config::load(pool).await?;
                if config.enabled {
                    let result = async {
                        self.logs
                            .configure(resolve_log_config(pool, &config.logs).await?)
                            .await
                            .map_err(log_error)
                    }
                    .await;
                    if let Err(error) = result {
                        *self.log_startup_error.lock().await = Some(error.code().to_string());
                    }
                }
                Ok::<(), AppError>(())
            })
            .await?;
        Ok(())
    }

    pub async fn allow(&self, bucket: String, limit: u32) -> Result<(), AppError> {
        let mut limits = self.limits.lock().await;
        limits.retain(|_, (started, _)| started.elapsed() < Duration::from_secs(60));
        if limits.len() >= 10_000 && !limits.contains_key(&bucket) {
            return Err(repository::invalid(
                "saas.rate_limited",
                "Too many active requests",
            ));
        }
        let entry = limits.entry(bucket).or_insert((Instant::now(), 0));
        if entry.1 >= limit {
            return Err(repository::invalid(
                "saas.rate_limited",
                "Too many requests; try again shortly",
            ));
        }
        entry.1 += 1;
        Ok(())
    }
}

pub fn log_error(_: String) -> AppError {
    repository::invalid(
        "saas.logs_unavailable",
        "Request log storage is unavailable; check the configured driver",
    )
}

pub async fn resolve_log_config(
    pool: &SqlitePool,
    value: &Value,
) -> Result<logs::LogConfig, AppError> {
    let mut value = value.clone();
    let object = value.as_object_mut().ok_or_else(|| {
        repository::invalid("saas.config_logs", "Log configuration must be an object")
    })?;
    for (reference, destination) in [
        ("redisUrlEnv", "redisUrl"),
        ("postgresUrlEnv", "postgresUrl"),
    ] {
        if let Some(reference) = object.remove(reference) {
            let needed = match destination {
                "redisUrl" => object.get("queue").and_then(Value::as_str) == Some("redis"),
                "postgresUrl" => object.get("store").and_then(Value::as_str) == Some("postgres"),
                _ => false,
            };
            if !needed {
                continue;
            }
            let name = reference
                .as_str()
                .filter(|name| !name.is_empty())
                .ok_or_else(|| {
                    repository::invalid(
                        "saas.config_logs",
                        "A log connection environment variable name is required",
                    )
                })?;
            let secret = std::env::var(name).map_err(|_| {
                repository::invalid(
                    "saas.config_logs",
                    "A configured log connection environment variable is missing",
                )
            })?;
            object.insert(destination.into(), Value::String(secret));
        }
    }
    let instance: String =
        sqlx::query_scalar("SELECT value_json FROM saas_settings WHERE key='instance_id'")
            .fetch_one(pool)
            .await
            .map_err(repository::db_error)?;
    object.insert(
        "instanceId".into(),
        serde_json::from_str(&instance).map_err(|_| {
            repository::invalid("saas.config_logs", "Invalid SaaS instance identifier")
        })?,
    );
    let config: logs::LogConfig = serde_json::from_value(value)
        .map_err(|_| repository::invalid("saas.config_logs", "Invalid log configuration fields"))?;
    config.validate().map_err(log_error)?;
    Ok(config)
}

pub async fn admin_command(
    state: &AppState,
    operation: &str,
    payload: Value,
) -> Result<Value, AppError> {
    state.saas.initialize(&state.pool).await?;
    match operation {
        "activation.status" => Ok(json!({"unlocked": config::is_unlocked(&state.pool).await?})),
        "activation.unlock" => {
            let activation_code = payload
                .get("activationCode")
                .and_then(Value::as_str)
                .unwrap_or_default();
            Ok(json!({"unlocked": config::unlock(&state.pool, activation_code).await?}))
        }
        "config.get" => serde_json::to_value(config::load(&state.pool).await?).map_err(|_| {
            repository::invalid("saas.config_invalid", "Could not read configuration")
        }),
        "config.save" => {
            let _transition = state.saas.transition.lock().await;
            let previous = config::load(&state.pool).await?;
            let enabled = payload
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(previous.enabled);
            let proposed_logs = payload.get("logs").unwrap_or(&previous.logs);
            if enabled {
                let next_log_config = resolve_log_config(&state.pool, proposed_logs).await?;
                state
                    .saas
                    .logs
                    .configure(next_log_config)
                    .await
                    .map_err(log_error)?;
            }
            match config::save(&state.pool, payload).await {
                Ok(config) => {
                    *state.saas.log_startup_error.lock().await = None;
                    if !config.enabled {
                        state.saas.logs.shutdown().await.map_err(log_error)?;
                    }
                    Ok(json!(config))
                }
                Err(error) => {
                    if previous.enabled {
                        if let Ok(config) = resolve_log_config(&state.pool, &previous.logs).await {
                            let _ = state.saas.logs.configure(config).await;
                        }
                    } else {
                        let _ = state.saas.logs.shutdown().await;
                    }
                    Err(error)
                }
            }
        }
        "logs.query" => {
            let query = serde_json::from_value(payload)
                .map_err(|_| repository::invalid("saas.validation", "Invalid log query"))?;
            Ok(json!(state
                .saas
                .logs
                .query(query)
                .await
                .map_err(log_error)?))
        }
        "logs.status" => {
            let mut status = json!(state.saas.logs.status().await.map_err(log_error)?);
            status["startupError"] = json!(*state.saas.log_startup_error.lock().await);
            Ok(status)
        }
        _ => domain::admin(&state.pool, operation, payload).await,
    }
}

pub async fn user_command(
    state: &AppState,
    user_id: &str,
    operation: &str,
    payload: Value,
) -> Result<Value, AppError> {
    state.saas.initialize(&state.pool).await?;
    state.saas.allow(format!("user:{user_id}"), 120).await?;
    if operation == "logs.query" {
        let mut query: logs::LogQuery = serde_json::from_value(payload)
            .map_err(|_| repository::invalid("saas.validation", "Invalid log query"))?;
        query.owner_user_id = Some(user_id.to_string());
        return Ok(json!(state
            .saas
            .logs
            .query(query)
            .await
            .map_err(log_error)?));
    }
    domain::user(&state.pool, user_id, operation, payload).await
}

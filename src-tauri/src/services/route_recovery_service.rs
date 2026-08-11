use crate::database::repositories::route_credential_repository::RouteCredentialRepository;
use crate::error::AppError;
use crate::models::route_credential::RouteCredential;
use crate::models::route_pool::RoutePoolModelTestRequest;
use crate::services::route_credential_activity::RouteCredentialActivityRegistry;
use crate::services::route_model_test_service::RouteModelTestService;
use chrono::{DateTime, Local, TimeZone, Timelike, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use tokio::sync::Mutex;

pub const RECOVERY_TICK_SECONDS: u64 = 30;
const DEFAULT_PROBE_INTERVAL_MINUTES: u32 = 30;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryMode {
    #[default]
    Off,
    Scheduled,
    Healthcheck,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct RecoveryRule {
    #[serde(default)]
    pub mode: RecoveryMode,
    /// Daily local trigger times ("HH:MM") for Scheduled mode.
    #[serde(default)]
    pub times: Vec<String>,
    /// Probe cadence (minutes) for Healthcheck mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probe_interval_minutes: Option<u32>,
}

pub struct RouteRecoveryService;

impl RouteRecoveryService {
    /// Persist a recovery rule into the account's config_json under `recovery`.
    /// An `Off` rule removes the key entirely.
    pub async fn set_rule(
        pool: &SqlitePool,
        id: String,
        rule: RecoveryRule,
    ) -> Result<RouteCredential, AppError> {
        let rule = normalize_rule(rule)?;
        let credential = RouteCredentialRepository::get(pool, &id).await?;
        let mut config = serde_json::from_str::<Value>(&credential.config_json).map_err(|err| {
            AppError::Validation {
                code: "validation.recovery_config_json",
                message: "Account config JSON must be a valid object".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            }
        })?;
        if !config.is_object() {
            return Err(AppError::Validation {
                code: "validation.recovery_config_json",
                message: "Account config JSON must be a valid object".to_string(),
                details: None,
                recoverable: true,
            });
        }
        let object = config.as_object_mut().expect("config json is an object");
        if matches!(rule.mode, RecoveryMode::Off) {
            object.remove("recovery");
        } else {
            let value = serde_json::to_value(&rule).map_err(|err| AppError::Validation {
                code: "validation.recovery_rule",
                message: "Could not serialize recovery rule".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?;
            object.insert("recovery".to_string(), value);
        }
        RouteCredentialRepository::update_config_json(pool, &id, &config.to_string()).await?;
        RouteCredentialRepository::get(pool, &id).await
    }

    /// Run the recovery loop forever; each caller spawns this on its runtime.
    pub async fn run_loop(pool: SqlitePool, activity: RouteCredentialActivityRegistry) {
        let probe_state: Mutex<HashMap<String, DateTime<Utc>>> = Mutex::new(HashMap::new());
        let mut ticker = tokio::time::interval(Duration::from_secs(RECOVERY_TICK_SECONDS));
        let mut previous = Local::now();
        ticker.tick().await; // consume the immediate first tick as the baseline
        loop {
            ticker.tick().await;
            let now = Local::now();
            Self::run_tick(&pool, &activity, previous, now, &probe_state).await;
            previous = now;
        }
    }
}

impl RouteRecoveryService {
    pub async fn run_tick(
        pool: &SqlitePool,
        activity: &RouteCredentialActivityRegistry,
        previous: DateTime<Local>,
        now: DateTime<Local>,
        probe_state: &Mutex<HashMap<String, DateTime<Utc>>>,
    ) {
        let candidates = match RouteCredentialRepository::list_recovery_candidates(pool).await {
            Ok(candidates) => candidates,
            Err(_) => return,
        };
        let now_utc = now.with_timezone(&Utc);
        for candidate in candidates {
            let rule = parse_recovery_rule(&candidate.config_json);
            let down = needs_recovery(
                &candidate.status,
                candidate.next_retry_at.as_deref(),
                candidate.cooldown_until.as_deref(),
            );
            match rule.mode {
                RecoveryMode::Off => {}
                RecoveryMode::Scheduled => {
                    if down && scheduled_fires(&rule.times, previous, now) {
                        if RouteCredentialRepository::reactivate_credential(pool, &candidate.id)
                            .await
                            .is_ok()
                        {
                            activity.notify_status_change(&candidate.platform, &candidate.id);
                        }
                    }
                }
                RecoveryMode::Healthcheck => {
                    let interval = rule
                        .probe_interval_minutes
                        .unwrap_or(DEFAULT_PROBE_INTERVAL_MINUTES)
                        .max(1);
                    if down && probe_is_due(probe_state, &candidate.id, interval, now_utc).await {
                        // A successful explicit test auto-recovers the account via
                        // recover_after_explicit_test inside test_model.
                        let _ = RouteModelTestService::test_model_with_activity(
                            pool,
                            activity,
                            RoutePoolModelTestRequest {
                                platform: candidate.platform.clone(),
                                account_id: Some(candidate.id.clone()),
                                model: None,
                                interface_format: None,
                            },
                        )
                        .await;
                    }
                }
            }
        }
    }
}

pub fn parse_recovery_rule(config_json: &str) -> RecoveryRule {
    serde_json::from_str::<Value>(config_json)
        .ok()
        .and_then(|value| value.get("recovery").cloned())
        .and_then(|value| serde_json::from_value::<RecoveryRule>(value).ok())
        .unwrap_or_default()
}

/// True when a non-revoked account is not fully healthy: status is not "ok"
/// (paused/error/warning) or it still carries a retry/cooldown window.
fn needs_recovery(status: &str, next_retry_at: Option<&str>, cooldown_until: Option<&str>) -> bool {
    if status == "revoked" {
        return false;
    }
    status != "ok" || next_retry_at.is_some() || cooldown_until.is_some()
}

fn parse_hhmm(value: &str) -> Option<chrono::NaiveTime> {
    let (hour, minute) = value.trim().split_once(':')?;
    let hour: u32 = hour.trim().parse().ok()?;
    let minute: u32 = minute.trim().parse().ok()?;
    chrono::NaiveTime::from_hms_opt(hour, minute, 0)
}

fn normalize_rule(mut rule: RecoveryRule) -> Result<RecoveryRule, AppError> {
    match rule.mode {
        RecoveryMode::Off => {
            rule.times.clear();
            rule.probe_interval_minutes = None;
        }
        RecoveryMode::Scheduled => {
            let mut seen = HashSet::new();
            let mut normalized = Vec::with_capacity(rule.times.len());
            for raw in rule.times {
                let time = parse_hhmm(&raw).ok_or_else(|| AppError::Validation {
                    code: "validation.recovery_times",
                    message: "Recovery times must use valid HH:MM values".to_string(),
                    details: Some(raw),
                    recoverable: true,
                })?;
                let value = format!("{:02}:{:02}", time.hour(), time.minute());
                if seen.insert(value.clone()) {
                    normalized.push(value);
                }
            }
            if normalized.is_empty() {
                return Err(AppError::Validation {
                    code: "validation.recovery_times_required",
                    message: "At least one scheduled recovery time is required".to_string(),
                    details: None,
                    recoverable: true,
                });
            }
            normalized.sort();
            rule.times = normalized;
            rule.probe_interval_minutes = None;
        }
        RecoveryMode::Healthcheck => {
            let interval = rule
                .probe_interval_minutes
                .unwrap_or(DEFAULT_PROBE_INTERVAL_MINUTES);
            if !(1..=1440).contains(&interval) {
                return Err(AppError::Validation {
                    code: "validation.recovery_probe_interval",
                    message: "Recovery probe interval must be between 1 and 1440 minutes"
                        .to_string(),
                    details: Some(interval.to_string()),
                    recoverable: true,
                });
            }
            rule.times.clear();
            rule.probe_interval_minutes = Some(interval);
        }
    }
    Ok(rule)
}

/// True when any daily "HH:MM" boundary falls within the (previous, now] window.
/// Checks both dates so it survives a tick that straddles midnight.
fn scheduled_fires(times: &[String], previous: DateTime<Local>, now: DateTime<Local>) -> bool {
    if now <= previous || now.date_naive() < previous.date_naive() {
        return false;
    }
    for raw in times {
        let Some(time) = parse_hhmm(raw) else {
            continue;
        };
        let mut date = previous.date_naive();
        loop {
            let occurrence = match Local.from_local_datetime(&date.and_time(time)) {
                chrono::LocalResult::Single(value) => Some(value),
                chrono::LocalResult::Ambiguous(first, _) => Some(first),
                chrono::LocalResult::None => None,
            };
            if occurrence.is_some_and(|value| value > previous && value <= now) {
                return true;
            }
            if date >= now.date_naive() {
                break;
            }
            let Some(next) = date.succ_opt() else {
                break;
            };
            date = next;
        }
    }
    false
}

async fn probe_is_due(
    probe_state: &Mutex<HashMap<String, DateTime<Utc>>>,
    id: &str,
    interval_minutes: u32,
    now: DateTime<Utc>,
) -> bool {
    let mut guard = probe_state.lock().await;
    let due = match guard.get(id) {
        Some(last) => now.signed_duration_since(*last).num_minutes() >= i64::from(interval_minutes),
        None => true,
    };
    if due {
        guard.insert(id.to_string(), now);
    }
    due
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(hour: u32, minute: u32, second: u32) -> DateTime<Local> {
        Local
            .with_ymd_and_hms(2026, 8, 11, hour, minute, second)
            .single()
            .expect("local datetime")
    }

    #[test]
    fn scheduled_fires_only_when_boundary_crossed() {
        let times = vec!["15:00".to_string()];
        assert!(scheduled_fires(&times, at(14, 59, 50), at(15, 0, 20)));
        assert!(!scheduled_fires(&times, at(15, 0, 20), at(15, 0, 40)));
        assert!(!scheduled_fires(
            &vec!["12:00".to_string()],
            at(14, 59, 50),
            at(15, 0, 20)
        ));
    }

    #[test]
    fn scheduled_accepts_unpadded_times() {
        assert!(scheduled_fires(
            &vec!["0:10".to_string()],
            at(0, 9, 50),
            at(0, 10, 10)
        ));
    }

    #[test]
    fn scheduled_survives_midnight_rollover() {
        let previous = Local
            .with_ymd_and_hms(2026, 8, 11, 23, 59, 50)
            .single()
            .expect("prev");
        let now = Local
            .with_ymd_and_hms(2026, 8, 12, 0, 0, 20)
            .single()
            .expect("now");
        assert!(scheduled_fires(&vec!["00:00".to_string()], previous, now));
    }

    #[test]
    fn scheduled_survives_multi_day_sleep() {
        let previous = Local
            .with_ymd_and_hms(2026, 8, 11, 16, 0, 0)
            .single()
            .expect("prev");
        let now = Local
            .with_ymd_and_hms(2026, 8, 13, 10, 0, 0)
            .single()
            .expect("now");
        assert!(scheduled_fires(&vec!["15:00".to_string()], previous, now));
    }

    #[test]
    fn needs_recovery_matrix() {
        assert!(needs_recovery("paused", None, None));
        assert!(needs_recovery("error", None, None));
        assert!(needs_recovery("warning", None, None));
        assert!(needs_recovery("ok", None, Some("2026-08-11T15:00:00Z")));
        assert!(!needs_recovery("ok", None, None));
        assert!(!needs_recovery(
            "revoked",
            None,
            Some("2026-08-11T15:00:00Z")
        ));
    }

    #[test]
    fn parse_recovery_rule_reads_and_defaults() {
        let cfg = r#"{"base_url":"x","recovery":{"mode":"scheduled","times":["15:00"]}}"#;
        let rule = parse_recovery_rule(cfg);
        assert_eq!(rule.mode, RecoveryMode::Scheduled);
        assert_eq!(rule.times, vec!["15:00".to_string()]);
        assert_eq!(parse_recovery_rule("{}").mode, RecoveryMode::Off);
        assert_eq!(parse_recovery_rule("not json").mode, RecoveryMode::Off);
    }

    #[test]
    fn normalize_rule_canonicalizes_scheduled_times() {
        let rule = normalize_rule(RecoveryRule {
            mode: RecoveryMode::Scheduled,
            times: vec!["3:00".to_string(), "03:00".to_string(), "15:00".to_string()],
            probe_interval_minutes: Some(5),
        })
        .expect("normalized rule");

        assert_eq!(rule.times, vec!["03:00".to_string(), "15:00".to_string()]);
        assert_eq!(rule.probe_interval_minutes, None);
    }

    #[test]
    fn normalize_rule_rejects_invalid_settings() {
        let no_times = normalize_rule(RecoveryRule {
            mode: RecoveryMode::Scheduled,
            times: Vec::new(),
            probe_interval_minutes: None,
        })
        .expect_err("scheduled mode requires a time");
        assert!(matches!(
            no_times,
            AppError::Validation {
                code: "validation.recovery_times_required",
                ..
            }
        ));

        let bad_interval = normalize_rule(RecoveryRule {
            mode: RecoveryMode::Healthcheck,
            times: Vec::new(),
            probe_interval_minutes: Some(0),
        })
        .expect_err("probe interval must be positive");
        assert!(matches!(
            bad_interval,
            AppError::Validation {
                code: "validation.recovery_probe_interval",
                ..
            }
        ));
    }

    #[tokio::test]
    async fn set_rule_preserves_config_and_off_removes_recovery_key() {
        use crate::models::route_credential::CreateApiRouteCredentialInput;
        use crate::services::route_credential_service::RouteCredentialService;

        let pool = crate::database::create_memory_pool().await.expect("pool");
        crate::database::run_migrations(&pool)
            .await
            .expect("migrations");
        let credential = RouteCredentialService::create_api(
            &pool,
            CreateApiRouteCredentialInput {
                platform: "codex".to_string(),
                display_name: "Scheduled".to_string(),
                api_key: "sk-test".to_string(),
                base_url: "https://api.example.com/v1".to_string(),
                interface_format: "openai".to_string(),
                model_mappings_json: "[]".to_string(),
                api_key_field: None,
                preview_json: None,
                batch_id: None,
                responses_custom_tool_compat: None,
                user_agent: None,
            },
        )
        .await
        .expect("credential");

        let updated = RouteRecoveryService::set_rule(
            &pool,
            credential.id.clone(),
            RecoveryRule {
                mode: RecoveryMode::Scheduled,
                times: vec!["3:00".to_string(), "15:00".to_string()],
                probe_interval_minutes: None,
            },
        )
        .await
        .expect("set rule");
        let config: Value = serde_json::from_str(&updated.config_json).expect("config");
        assert_eq!(config["base_url"], "https://api.example.com/v1");
        assert_eq!(
            config["recovery"]["times"],
            serde_json::json!(["03:00", "15:00"])
        );

        let disabled =
            RouteRecoveryService::set_rule(&pool, credential.id, RecoveryRule::default())
                .await
                .expect("disable rule");
        let config: Value = serde_json::from_str(&disabled.config_json).expect("config");
        assert!(config.get("recovery").is_none());
        assert_eq!(config["base_url"], "https://api.example.com/v1");
    }

    #[tokio::test]
    async fn set_rule_rejects_invalid_config_without_overwriting_it() {
        let pool = crate::database::create_memory_pool().await.expect("pool");
        crate::database::run_migrations(&pool)
            .await
            .expect("migrations");
        let credential = RouteCredentialRepository::create(
            &pool,
            "codex",
            "api",
            "Invalid config",
            None,
            "ok",
            None,
            r#"{"api_key":"sk-test"}"#,
            "not json",
            "{}",
        )
        .await
        .expect("credential");

        let error = RouteRecoveryService::set_rule(
            &pool,
            credential.id.clone(),
            RecoveryRule {
                mode: RecoveryMode::Healthcheck,
                times: Vec::new(),
                probe_interval_minutes: Some(30),
            },
        )
        .await
        .expect_err("invalid config should be rejected");

        assert!(matches!(
            error,
            AppError::Validation {
                code: "validation.recovery_config_json",
                ..
            }
        ));
        let current = RouteCredentialRepository::get(&pool, &credential.id)
            .await
            .expect("current credential");
        assert_eq!(current.config_json, "not json");
    }
}

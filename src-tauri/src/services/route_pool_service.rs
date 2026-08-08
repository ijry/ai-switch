use crate::database::repositories::route_credential_repository::RouteCredentialRepository;
use crate::database::repositories::route_pool_repository::RoutePoolRepository;
use crate::error::AppError;
use crate::models::platform::{PlatformId, PlatformOperation};
use crate::models::route_pool::{
    RoutePoolRouteOutcome, RoutePoolRouteRequest, RoutePoolState, SetRoutePoolMembersInput,
};
use crate::services::platform_capability_service::PlatformCapabilityService;
use crate::services::route_credential_activity::RouteCredentialActivityRegistry;
use chrono::DateTime;
use sqlx::SqlitePool;
use std::collections::{BTreeMap, HashSet};

pub struct RoutePoolService;

const DEFAULT_REQUEST_PAGE: i64 = 1;
const DEFAULT_REQUEST_PAGE_SIZE: i64 = 20;
const MAX_REQUEST_PAGE_SIZE: i64 = 100;

impl RoutePoolService {
    pub async fn get(
        pool: &SqlitePool,
        platform: String,
        since: Option<String>,
        request_page: Option<i64>,
        request_page_size: Option<i64>,
    ) -> Result<RoutePoolState, AppError> {
        let platform = PlatformId::parse(&platform)?;
        PlatformCapabilityService::require(platform, PlatformOperation::RouteCredentials)?;
        let since = normalize_since(since)?;
        let pagination = normalize_request_pagination(request_page, request_page_size);
        Self::state(
            pool,
            platform.as_str(),
            since.as_deref(),
            pagination.page,
            pagination.page_size,
        )
        .await
    }

    pub async fn set_members(
        pool: &SqlitePool,
        input: SetRoutePoolMembersInput,
    ) -> Result<RoutePoolState, AppError> {
        let platform = PlatformId::parse(&input.platform)?;
        PlatformCapabilityService::require(platform, PlatformOperation::RouteCredentials)?;
        let mut seen = HashSet::new();
        let account_ids: Vec<String> = input
            .account_ids
            .into_iter()
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty())
            .filter(|id| seen.insert(id.clone()))
            .collect();

        for account_id in &account_ids {
            let account_platform = RouteCredentialRepository::platform_of(pool, account_id).await?;
            let account_platform = PlatformId::parse(&account_platform)?;
            if account_platform != platform {
                return Err(AppError::Validation {
                    code: "validation.route_pool_platform_mismatch",
                    message: "Route pool account belongs to another platform".to_string(),
                    details: Some(format!("{account_id}:{}", account_platform.as_str())),
                    recoverable: true,
                });
            }
        }

        RoutePoolRepository::replace_members(pool, platform.as_str(), &account_ids).await?;
        Self::state(
            pool,
            platform.as_str(),
            None,
            DEFAULT_REQUEST_PAGE,
            DEFAULT_REQUEST_PAGE_SIZE,
        )
        .await
    }

    pub async fn route_once(
        pool: &SqlitePool,
        request: RoutePoolRouteRequest,
    ) -> Result<RoutePoolRouteOutcome, AppError> {
        Self::route_once_with_activity(
            pool,
            &RouteCredentialActivityRegistry::default(),
            request,
        )
        .await
    }

    pub async fn route_once_with_activity(
        pool: &SqlitePool,
        activity: &RouteCredentialActivityRegistry,
        request: RoutePoolRouteRequest,
    ) -> Result<RoutePoolRouteOutcome, AppError> {
        let platform = PlatformId::parse(&request.platform)?;
        PlatformCapabilityService::require(platform, PlatformOperation::GenericApiRouting)?;
        let platform_key = platform.as_str();
        let metadata_json = normalize_metadata_json(request.metadata_json)?;
        let token_count = non_negative(request.token_count.unwrap_or(0), "token_count")?;
        let cost_micros = non_negative(request.cost_micros.unwrap_or(0), "cost_micros")?;
        let members = RoutePoolRepository::member_accounts(pool, platform_key).await?;

        if members.is_empty() {
            return Err(AppError::Validation {
                code: "validation.route_pool_empty",
                message: "Route pool has no enabled accounts".to_string(),
                details: Some(platform_key.to_string()),
                recoverable: true,
            });
        }

        let cursor = RoutePoolRepository::next_cursor_index(pool, platform_key).await?;
        let candidate_indexes = Self::member_indexes_by_priority(&members, cursor);
        let mut selected = None;
        let mut selected_index = 0;
        let mut selected_lease = None;
        for index in candidate_indexes {
            let member = &members[index];
            if member.status != "ok" {
                continue;
            }
            let Some(lease) = activity
                .try_acquire(platform_key, &member.id, member.max_concurrency)
                .await
            else {
                continue;
            };
            selected = Some(member.clone());
            selected_index = index;
            selected_lease = Some(lease);
            break;
        }
        let Some(selected) = selected else {
            let has_routeable_member = members.iter().any(|member| member.status == "ok");
            return Err(AppError::Validation {
                code: if has_routeable_member {
                    "route_pool.concurrency_exhausted"
                } else {
                    "validation.route_pool_empty"
                },
                message: if has_routeable_member {
                    "All route pool accounts are at their concurrency limit".to_string()
                } else {
                    "Route pool has no available accounts".to_string()
                },
                details: Some(platform_key.to_string()),
                recoverable: true,
            });
        };
        let _selected_lease = selected_lease;
        let next_index = (selected_index + 1) as i64 % members.len() as i64;

        RoutePoolRepository::insert_usage_event(
            pool,
            &selected.id,
            "route_pool",
            "request",
            1,
            "count",
            &metadata_json,
        )
        .await?;
        if token_count > 0 {
            RoutePoolRepository::insert_usage_event(
                pool,
                &selected.id,
                "route_pool",
                "token",
                token_count,
                "token",
                &metadata_json,
            )
            .await?;
        }
        if cost_micros > 0 {
            RoutePoolRepository::insert_usage_event(
                pool,
                &selected.id,
                "route_pool",
                "cost",
                cost_micros,
                "usd_micros",
                &metadata_json,
            )
            .await?;
        }

        RoutePoolRepository::save_cursor_index(pool, platform_key, next_index).await?;

        Ok(RoutePoolRouteOutcome {
            platform: platform_key.to_string(),
            selected_account_id: selected.id,
            selected_account_name: selected.display_name,
            stats: RoutePoolRepository::stats(
                pool,
                platform_key,
                None,
                DEFAULT_REQUEST_PAGE,
                DEFAULT_REQUEST_PAGE_SIZE,
            )
            .await?,
        })
    }

fn member_indexes_by_priority(
    members: &[crate::models::route_pool::RoutePoolMemberAccount],
    cursor: i64,
) -> Vec<usize> {
    let mut groups = BTreeMap::<i64, Vec<usize>>::new();
    for (index, member) in members.iter().enumerate() {
        groups.entry(member.route_priority).or_default().push(index);
    }
    groups
        .into_values()
        .flat_map(|indexes| {
            let first = cursor.rem_euclid(indexes.len() as i64) as usize;
            (0..indexes.len()).map(move |offset| indexes[(first + offset) % indexes.len()])
        })
        .collect()
}

    async fn state(
        pool: &SqlitePool,
        platform: &str,
        since: Option<&str>,
        request_page: i64,
        request_page_size: i64,
    ) -> Result<RoutePoolState, AppError> {
        Ok(RoutePoolState {
            platform: platform.to_string(),
            account_ids: RoutePoolRepository::list_member_ids(pool, platform).await?,
            stats: RoutePoolRepository::stats(
                pool,
                platform,
                since,
                request_page,
                request_page_size,
            )
            .await?,
        })
    }
}

fn normalize_metadata_json(metadata_json: Option<String>) -> Result<String, AppError> {
    let raw = metadata_json.unwrap_or_else(|| "{}".to_string());
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok("{}".to_string());
    }

    let value: serde_json::Value =
        serde_json::from_str(trimmed).map_err(|err| AppError::Validation {
            code: "validation.route_pool_metadata_json",
            message: "Route metadata JSON is invalid".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

    if !value.is_object() {
        return Err(AppError::Validation {
            code: "validation.route_pool_metadata_json",
            message: "Route metadata JSON must be an object".to_string(),
            details: Some(trimmed.to_string()),
            recoverable: true,
        });
    }

    Ok(value.to_string())
}

fn normalize_since(since: Option<String>) -> Result<Option<String>, AppError> {
    let Some(value) = since
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };

    DateTime::parse_from_rfc3339(&value).map_err(|err| AppError::Validation {
        code: "validation.route_pool_since",
        message: "Route pool stats start time is invalid".to_string(),
        details: Some(err.to_string()),
        recoverable: true,
    })?;

    Ok(Some(value))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RequestPagination {
    page: i64,
    page_size: i64,
}

fn normalize_request_pagination(page: Option<i64>, page_size: Option<i64>) -> RequestPagination {
    RequestPagination {
        page: page.unwrap_or(DEFAULT_REQUEST_PAGE).max(1),
        page_size: page_size
            .unwrap_or(DEFAULT_REQUEST_PAGE_SIZE)
            .clamp(1, MAX_REQUEST_PAGE_SIZE),
    }
}

fn non_negative(value: i64, field: &'static str) -> Result<i64, AppError> {
    if value < 0 {
        return Err(AppError::Validation {
            code: "validation.route_pool_metric_negative",
            message: "Route metric values must be non-negative".to_string(),
            details: Some(field.to_string()),
            recoverable: true,
        });
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::repositories::route_credential_repository::RouteCredentialRepository;
    use crate::database::{create_memory_pool, run_migrations};
    use sqlx::SqlitePool;
    use uuid::Uuid;

    #[tokio::test]
    async fn route_pool_does_not_default_unknown_platform_to_codex() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let error = RoutePoolService::get(&pool, "custom-agent".to_string(), None, None, None)
            .await
            .expect_err("unknown platforms must fail closed");

        assert!(matches!(
            error,
            AppError::Validation {
                code: "platform.unknown",
                ..
            }
        ));
    }

    async fn account(pool: &SqlitePool, platform: &str, name: &str) -> String {
        credential(pool, platform, name, "ok").await
    }

    async fn credential(pool: &SqlitePool, platform: &str, name: &str, status: &str) -> String {
        RouteCredentialRepository::create(
            pool,
            platform,
            "official",
            name,
            Some(format!("{}@example.com", name.to_lowercase())),
            status,
            None,
            r#"{"access_token":"at","refresh_token":"rt"}"#,
            r#"{"type":"official"}"#,
            r#"{"settings_json":"{}"}"#,
        )
        .await
        .expect("account")
        .id
    }

    async fn usage_event_at(
        pool: &SqlitePool,
        account_id: &str,
        source_label: &str,
        metric_type: &str,
        amount: i64,
        unit: &str,
        metadata_json: &str,
        created_at: &str,
    ) {
        sqlx::query(
            "INSERT INTO usage_events
             (id, route_credential_id, source_label, metric_type, amount, unit, metadata_json, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(account_id)
        .bind(source_label)
        .bind(metric_type)
        .bind(amount)
        .bind(unit)
        .bind(metadata_json)
        .bind(created_at)
        .execute(pool)
        .await
        .expect("usage event");
    }

    #[tokio::test]
    async fn set_members_persists_account_ids_and_stats() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let account_id = account(&pool, "codex", "CodexOne").await;

        let now = chrono::Utc::now().to_rfc3339();
        for (metric_type, amount, unit) in [
            ("request", 1_i64, "count"),
            ("token", 4096_i64, "token"),
            ("cost", 2500_i64, "usd_micros"),
        ] {
            sqlx::query(
                "INSERT INTO usage_events
                 (id, route_credential_id, source_label, metric_type, amount, unit, metadata_json, created_at)
                 VALUES (?, ?, 'test', ?, ?, ?, '{}', ?)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(&account_id)
            .bind(metric_type)
            .bind(amount)
            .bind(unit)
            .bind(&now)
            .execute(&pool)
            .await
            .expect("usage event");
        }

        let state = RoutePoolService::set_members(
            &pool,
            SetRoutePoolMembersInput {
                platform: "codex".to_string(),
                account_ids: vec![account_id.clone(), account_id.clone()],
            },
        )
        .await
        .expect("state");

        assert_eq!(state.platform, "codex");
        assert_eq!(state.account_ids, vec![account_id]);
        assert_eq!(state.stats.member_count, 1);
        assert_eq!(state.stats.request_count, 1);
        assert_eq!(state.stats.token_count, 4096);
        assert_eq!(state.stats.cost_micros, 2500);
        assert_eq!(state.stats.recent_logs.len(), 3);
    }

    #[tokio::test]
    async fn get_filters_stats_by_since_and_returns_request_rows() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let account_id = account(&pool, "codex", "CodexOne").await;

        RoutePoolService::set_members(
            &pool,
            SetRoutePoolMembersInput {
                platform: "codex".to_string(),
                account_ids: vec![account_id.clone()],
            },
        )
        .await
        .expect("members");

        let old_time = "2026-07-01T00:00:00Z";
        let since = "2026-07-17T00:00:00Z";
        let new_time = "2026-07-17T08:00:00Z";

        usage_event_at(
            &pool,
            &account_id,
            "route_proxy",
            "request",
            1,
            "count",
            r#"{"path":"/v1/old","status":200}"#,
            old_time,
        )
        .await;
        usage_event_at(
            &pool,
            &account_id,
            "route_proxy",
            "token",
            100,
            "token",
            r#"{"path":"/v1/old","status":200}"#,
            old_time,
        )
        .await;
        usage_event_at(
            &pool,
            &account_id,
            "route_proxy",
            "request",
            1,
            "count",
            r#"{"path":"/v1/responses","status":201}"#,
            new_time,
        )
        .await;
        usage_event_at(
            &pool,
            &account_id,
            "route_proxy",
            "token",
            200,
            "token",
            r#"{"path":"/v1/responses","status":201}"#,
            new_time,
        )
        .await;
        usage_event_at(
            &pool,
            &account_id,
            "route_proxy",
            "cost",
            300,
            "usd_micros",
            r#"{"path":"/v1/responses","status":201}"#,
            new_time,
        )
        .await;

        let state = RoutePoolService::get(
            &pool,
            "codex".to_string(),
            Some(since.to_string()),
            Some(1),
            Some(20),
        )
        .await
        .expect("filtered state");

        assert_eq!(state.stats.member_count, 1);
        assert_eq!(state.stats.request_count, 1);
        assert_eq!(state.stats.token_count, 200);
        assert_eq!(state.stats.cost_micros, 300);
        assert_eq!(state.stats.recent_logs.len(), 3);
        assert_eq!(state.stats.requests.len(), 1);
        assert_eq!(state.stats.request_row_count, 1);
        assert_eq!(state.stats.request_page, 1);
        assert_eq!(state.stats.request_page_size, 20);
        assert_eq!(state.stats.requests[0].metric_type, "request");
        assert_eq!(state.stats.requests[0].source_label, "route_proxy");
        assert_eq!(
            state.stats.requests[0].account_name.as_deref(),
            Some("CodexOne")
        );
        assert!(state.stats.requests[0]
            .metadata_json
            .contains("/v1/responses"));
    }

    #[tokio::test]
    async fn stats_include_removed_pool_credentials_for_same_platform() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let removed_id = account(&pool, "codex", "RemovedCodex").await;
        let active_id = account(&pool, "codex", "ActiveCodex").await;
        let claude_id = account(&pool, "claude", "ClaudeOne").await;

        RoutePoolService::set_members(
            &pool,
            SetRoutePoolMembersInput {
                platform: "codex".to_string(),
                account_ids: vec![removed_id.clone(), active_id.clone()],
            },
        )
        .await
        .expect("initial members");

        usage_event_at(
            &pool,
            &removed_id,
            "route_proxy",
            "request",
            1,
            "count",
            r#"{"path":"/v1/removed","status":200}"#,
            "2026-07-17T08:00:00Z",
        )
        .await;
        usage_event_at(
            &pool,
            &removed_id,
            "route_proxy",
            "token",
            512,
            "token",
            r#"{"path":"/v1/removed","status":200}"#,
            "2026-07-17T08:00:01Z",
        )
        .await;
        usage_event_at(
            &pool,
            &active_id,
            "route_proxy",
            "request",
            1,
            "count",
            r#"{"path":"/v1/active","status":201}"#,
            "2026-07-17T08:01:00Z",
        )
        .await;
        usage_event_at(
            &pool,
            &claude_id,
            "route_proxy",
            "request",
            1,
            "count",
            r#"{"path":"/v1/claude","status":202}"#,
            "2026-07-17T08:02:00Z",
        )
        .await;

        RoutePoolService::set_members(
            &pool,
            SetRoutePoolMembersInput {
                platform: "codex".to_string(),
                account_ids: vec![active_id.clone()],
            },
        )
        .await
        .expect("removed one member");

        let state = RoutePoolService::get(&pool, "codex".to_string(), None, Some(1), Some(20))
            .await
            .expect("state");

        assert_eq!(state.stats.member_count, 1);
        assert_eq!(state.stats.request_count, 2);
        assert_eq!(state.stats.token_count, 512);
        assert_eq!(state.stats.request_row_count, 2);
        assert_eq!(state.stats.request_page, 1);
        assert_eq!(state.stats.request_page_size, 20);

        let request_names: Vec<&str> = state
            .stats
            .requests
            .iter()
            .filter_map(|request| request.account_name.as_deref())
            .collect();
        assert!(request_names.contains(&"RemovedCodex"));
        assert!(request_names.contains(&"ActiveCodex"));
        assert!(!request_names.contains(&"ClaudeOne"));
    }

    #[tokio::test]
    async fn stats_paginates_request_rows_and_reports_total() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let account_id = account(&pool, "codex", "CodexOne").await;

        RoutePoolService::set_members(
            &pool,
            SetRoutePoolMembersInput {
                platform: "codex".to_string(),
                account_ids: vec![account_id.clone()],
            },
        )
        .await
        .expect("members");

        usage_event_at(
            &pool,
            &account_id,
            "route_proxy",
            "request",
            1,
            "count",
            r#"{"path":"/v1/oldest","status":200}"#,
            "2026-07-17T08:00:00Z",
        )
        .await;
        usage_event_at(
            &pool,
            &account_id,
            "route_proxy",
            "request",
            1,
            "count",
            r#"{"path":"/v1/middle","status":200}"#,
            "2026-07-17T09:00:00Z",
        )
        .await;
        usage_event_at(
            &pool,
            &account_id,
            "route_proxy",
            "request",
            1,
            "count",
            r#"{"path":"/v1/newest","status":200}"#,
            "2026-07-17T10:00:00Z",
        )
        .await;

        let state = RoutePoolService::get(&pool, "codex".to_string(), None, Some(2), Some(2))
            .await
            .expect("page two");

        assert_eq!(state.stats.request_count, 3);
        assert_eq!(state.stats.request_row_count, 3);
        assert_eq!(state.stats.request_page, 2);
        assert_eq!(state.stats.request_page_size, 2);
        assert_eq!(state.stats.requests.len(), 1);
        assert!(state.stats.requests[0].metadata_json.contains("/v1/oldest"));
    }

    #[tokio::test]
    async fn stats_normalizes_request_pagination_values() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let state = RoutePoolService::get(&pool, "codex".to_string(), None, Some(0), Some(500))
            .await
            .expect("normalized pagination");

        assert_eq!(state.stats.request_page, 1);
        assert_eq!(state.stats.request_page_size, 100);
    }

    #[tokio::test]
    async fn get_rejects_invalid_since_timestamp() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let error = RoutePoolService::get(
            &pool,
            "codex".to_string(),
            Some("not-a-date".to_string()),
            None,
            None,
        )
        .await
        .expect_err("invalid since");

        match error {
            AppError::Validation { code, .. } => {
                assert_eq!(code, "validation.route_pool_since");
            }
            _ => panic!("expected validation error"),
        }
    }

    #[tokio::test]
    async fn set_members_rejects_accounts_from_another_platform() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let account_id = account(&pool, "claude_code", "ClaudeOne").await;

        let error = RoutePoolService::set_members(
            &pool,
            SetRoutePoolMembersInput {
                platform: "codex".to_string(),
                account_ids: vec![account_id],
            },
        )
        .await
        .expect_err("platform mismatch");

        match error {
            AppError::Validation { code, .. } => {
                assert_eq!(code, "validation.route_pool_platform_mismatch");
            }
            _ => panic!("expected validation error"),
        }
    }

    #[tokio::test]
    async fn set_members_accepts_credentials_in_any_status() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let warning_id = credential(&pool, "codex", "CodexWarning", "warning").await;
        let error_id = credential(&pool, "codex", "CodexError", "error").await;
        let revoked_id = credential(&pool, "codex", "CodexRevoked", "revoked").await;

        let state = RoutePoolService::set_members(
            &pool,
            SetRoutePoolMembersInput {
                platform: "codex".to_string(),
                account_ids: vec![warning_id.clone(), error_id.clone(), revoked_id.clone()],
            },
        )
        .await
        .expect("members in any status");

        assert_eq!(
            state.account_ids,
            vec![warning_id, error_id.clone(), revoked_id]
        );
        assert_eq!(state.stats.member_count, 3);

        let state = RoutePoolService::set_members(
            &pool,
            SetRoutePoolMembersInput {
                platform: "codex".to_string(),
                account_ids: vec![error_id.clone()],
            },
        )
        .await
        .expect("remove members regardless of retained status");

        assert_eq!(state.account_ids, vec![error_id]);
    }

    #[tokio::test]
    async fn route_once_skips_non_ok_pool_members() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let error_id = credential(&pool, "codex", "CodexError", "error").await;
        let healthy_id = account(&pool, "codex", "CodexHealthy").await;

        RoutePoolService::set_members(
            &pool,
            SetRoutePoolMembersInput {
                platform: "codex".to_string(),
                account_ids: vec![error_id, healthy_id.clone()],
            },
        )
        .await
        .expect("pool members");

        let outcome = RoutePoolService::route_once(
            &pool,
            RoutePoolRouteRequest {
                platform: "codex".to_string(),
                token_count: None,
                cost_micros: None,
                metadata_json: None,
            },
        )
        .await
        .expect("healthy route member");

        assert_eq!(outcome.selected_account_id, healthy_id);
    }

    #[tokio::test]
    async fn route_once_prefers_priority_and_falls_back_when_slots_are_full() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let first = account(&pool, "codex", "PriorityOne").await;
        let second = account(&pool, "codex", "PriorityTwo").await;
        sqlx::query(
            "UPDATE route_credentials
             SET route_priority = CASE id WHEN ? THEN 1 ELSE 2 END,
                 max_concurrency = 1
             WHERE id IN (?, ?)",
        )
        .bind(&first)
        .bind(&first)
        .bind(&second)
        .execute(&pool)
        .await
        .expect("routing settings");
        RoutePoolService::set_members(
            &pool,
            SetRoutePoolMembersInput {
                platform: "codex".to_string(),
                account_ids: vec![first.clone(), second.clone()],
            },
        )
        .await
        .expect("members");

        let activity = RouteCredentialActivityRegistry::default();
        let held_first = activity
            .try_acquire("codex", &first, 1)
            .await
            .expect("hold priority one slot");
        let fallback = RoutePoolService::route_once_with_activity(
            &pool,
            &activity,
            RoutePoolRouteRequest {
                platform: "codex".to_string(),
                token_count: None,
                cost_micros: None,
                metadata_json: None,
            },
        )
        .await
        .expect("fallback route");
        assert_eq!(fallback.selected_account_id, second);

        drop(held_first);
        let primary = RoutePoolService::route_once_with_activity(
            &pool,
            &activity,
            RoutePoolRouteRequest {
                platform: "codex".to_string(),
                token_count: None,
                cost_micros: None,
                metadata_json: None,
            },
        )
        .await
        .expect("priority one route");
        assert_eq!(primary.selected_account_id, first);

        let held_first = activity
            .try_acquire("codex", &first, 1)
            .await
            .expect("hold priority one slot again");
        let held_second = activity
            .try_acquire("codex", &second, 1)
            .await
            .expect("hold priority two slot");
        let error = RoutePoolService::route_once_with_activity(
            &pool,
            &activity,
            RoutePoolRouteRequest {
                platform: "codex".to_string(),
                token_count: None,
                cost_micros: None,
                metadata_json: None,
            },
        )
        .await
        .expect_err("all slots are full");
        assert!(matches!(
            error,
            AppError::Validation {
                code: "route_pool.concurrency_exhausted",
                ..
            }
        ));
        drop(held_first);
        drop(held_second);
    }

    #[tokio::test]
    async fn route_once_keeps_paused_members_but_never_selects_them() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let paused = credential(&pool, "codex", "Paused", "paused").await;
        let healthy = account(&pool, "codex", "Healthy").await;
        RoutePoolService::set_members(
            &pool,
            SetRoutePoolMembersInput {
                platform: "codex".to_string(),
                account_ids: vec![paused.clone(), healthy.clone()],
            },
        )
        .await
        .expect("members");

        let state = RoutePoolService::get(&pool, "codex".to_string(), None, None, None)
            .await
            .expect("pool state");
        assert_eq!(state.account_ids, vec![paused.clone(), healthy.clone()]);

        let outcome = RoutePoolService::route_once(
            &pool,
            RoutePoolRouteRequest {
                platform: "codex".to_string(),
                token_count: None,
                cost_micros: None,
                metadata_json: None,
            },
        )
        .await
        .expect("healthy route");
        assert_eq!(outcome.selected_account_id, healthy);
    }

    #[tokio::test]
    async fn route_once_selects_accounts_round_robin_and_records_usage() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let first = account(&pool, "codex", "CodexOne").await;
        let second = account(&pool, "codex", "CodexTwo").await;

        RoutePoolService::set_members(
            &pool,
            SetRoutePoolMembersInput {
                platform: "codex".to_string(),
                account_ids: vec![first.clone(), second.clone()],
            },
        )
        .await
        .expect("members");

        let first_outcome = RoutePoolService::route_once(
            &pool,
            RoutePoolRouteRequest {
                platform: "codex".to_string(),
                token_count: Some(512),
                cost_micros: Some(1200),
                metadata_json: Some(r#"{"source":"test"}"#.to_string()),
            },
        )
        .await
        .expect("first route");
        let second_outcome = RoutePoolService::route_once(
            &pool,
            RoutePoolRouteRequest {
                platform: "codex".to_string(),
                token_count: Some(256),
                cost_micros: None,
                metadata_json: None,
            },
        )
        .await
        .expect("second route");

        assert_eq!(first_outcome.selected_account_id, first);
        assert_eq!(second_outcome.selected_account_id, second);
        assert_eq!(second_outcome.stats.request_count, 2);
        assert_eq!(second_outcome.stats.token_count, 768);
        assert_eq!(second_outcome.stats.cost_micros, 1200);
    }

    #[tokio::test]
    async fn route_once_rejects_empty_pool() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let error = RoutePoolService::route_once(
            &pool,
            RoutePoolRouteRequest {
                platform: "codex".to_string(),
                token_count: None,
                cost_micros: None,
                metadata_json: None,
            },
        )
        .await
        .expect_err("empty pool");

        match error {
            AppError::Validation { code, .. } => {
                assert_eq!(code, "validation.route_pool_empty");
            }
            _ => panic!("expected validation error"),
        }
    }
}

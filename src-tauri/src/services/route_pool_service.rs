use crate::database::repositories::route_credential_repository::RouteCredentialRepository;
use crate::database::repositories::route_pool_repository::RoutePoolRepository;
use crate::error::AppError;
use crate::models::route_pool::{
    RoutePoolRouteOutcome, RoutePoolRouteRequest, RoutePoolState, SetRoutePoolMembersInput,
};
use chrono::DateTime;
use sqlx::SqlitePool;
use std::collections::HashSet;

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
        let platform = normalize_platform(&platform)?;
        let since = normalize_since(since)?;
        let pagination = normalize_request_pagination(request_page, request_page_size);
        Self::state(
            pool,
            &platform,
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
        let platform = normalize_platform(&input.platform)?;
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
            let credential = RouteCredentialRepository::get(pool, account_id).await?;
            let account_platform = normalize_platform(&account_platform)?;
            if account_platform != platform {
                return Err(AppError::Validation {
                    code: "validation.route_pool_platform_mismatch",
                    message: "Route pool account belongs to another platform".to_string(),
                    details: Some(format!("{account_id}:{account_platform}")),
                    recoverable: true,
                });
            }
            if credential.status != "ok" {
                return Err(AppError::Validation {
                    code: "validation.route_pool_credential_invalid",
                    message: "Route pool credential must be ok".to_string(),
                    details: Some(format!("{account_id}:{}", credential.status)),
                    recoverable: true,
                });
            }
        }

        RoutePoolRepository::replace_members(pool, &platform, &account_ids).await?;
        Self::state(
            pool,
            &platform,
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
        let platform = normalize_platform(&request.platform)?;
        let metadata_json = normalize_metadata_json(request.metadata_json)?;
        let token_count = non_negative(request.token_count.unwrap_or(0), "token_count")?;
        let cost_micros = non_negative(request.cost_micros.unwrap_or(0), "cost_micros")?;
        let members = RoutePoolRepository::member_accounts(pool, &platform).await?;

        if members.is_empty() {
            return Err(AppError::Validation {
                code: "validation.route_pool_empty",
                message: "Route pool has no enabled accounts".to_string(),
                details: Some(platform),
                recoverable: true,
            });
        }

        let cursor = RoutePoolRepository::next_cursor_index(pool, &platform).await?;
        let selected_index = cursor.rem_euclid(members.len() as i64) as usize;
        let next_index = (selected_index + 1) as i64 % members.len() as i64;
        let selected = members[selected_index].clone();

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

        RoutePoolRepository::save_cursor_index(pool, &platform, next_index).await?;

        Ok(RoutePoolRouteOutcome {
            platform: platform.clone(),
            selected_account_id: selected.id,
            selected_account_name: selected.display_name,
            stats: RoutePoolRepository::stats(
                pool,
                &platform,
                None,
                DEFAULT_REQUEST_PAGE,
                DEFAULT_REQUEST_PAGE_SIZE,
            )
            .await?,
        })
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

pub fn normalize_platform(platform: &str) -> Result<String, AppError> {
    let normalized = platform.trim().to_lowercase();
    if normalized.is_empty() {
        return Err(AppError::Validation {
            code: "validation.route_pool_platform_required",
            message: "Route pool platform is required".to_string(),
            details: None,
            recoverable: true,
        });
    }

    if normalized.contains("claude") {
        Ok("claude".to_string())
    } else if normalized.contains("grok") || normalized.contains("xai") || normalized.contains("x.ai") {
        Ok("grok".to_string())
    } else if normalized.contains("gemini") {
        Ok("gemini".to_string())
    } else if normalized.contains("opencode") {
        Ok("opencode".to_string())
    } else if normalized.contains("openclaw") {
        Ok("openclaw".to_string())
    } else if normalized.contains("hermes") {
        Ok("hermes".to_string())
    } else {
        Ok("codex".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::repositories::route_credential_repository::RouteCredentialRepository;
    use crate::database::{create_memory_pool, run_migrations};
    use sqlx::SqlitePool;
    use uuid::Uuid;

    #[test]
    fn normalizes_grok_platform_aliases() {
        assert_eq!(normalize_platform("grok").unwrap(), "grok");
        assert_eq!(normalize_platform("xai").unwrap(), "grok");
        assert_eq!(normalize_platform("x.ai").unwrap(), "grok");
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
    async fn set_members_rejects_non_ok_credentials() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let account_id = credential(&pool, "codex", "CodexWarning", "warning").await;

        let error = RoutePoolService::set_members(
            &pool,
            SetRoutePoolMembersInput {
                platform: "codex".to_string(),
                account_ids: vec![account_id],
            },
        )
        .await
        .expect_err("invalid credential");

        match error {
            AppError::Validation { code, .. } => {
                assert_eq!(code, "validation.route_pool_credential_invalid");
            }
            _ => panic!("expected validation error"),
        }
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

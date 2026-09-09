use super::route_credential_repository::database_error;
use super::route_pool_repository::RoutePoolRepository;
use crate::error::AppError;
use chrono::Utc;
use sqlx::{Row, SqlitePool};
use std::collections::HashSet;

fn invalid(code: &'static str, message: &str) -> AppError {
    AppError::Validation {
        code,
        message: message.into(),
        details: None,
        recoverable: true,
    }
}

impl RoutePoolRepository {
    pub async fn move_group_members(
        pool: &SqlitePool,
        platform: &str,
        group_id: &str,
        account_ids: &[String],
    ) -> Result<(), AppError> {
        if account_ids.len() > 10000 {
            return Err(invalid(
                "validation.route_pool_group_members_limit",
                "Select at most 10000 accounts",
            ));
        }
        let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await.map_err(|error| {
            database_error(
                "database.route_group_move",
                "Could not start group move",
                error,
            )
        })?;
        let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM route_pool_groups WHERE id=? AND platform=? AND deleted_at IS NULL)")
            .bind(group_id).bind(platform).fetch_one(&mut *transaction).await.map_err(|error|database_error("database.route_group_move","Could not load group",error))?;
        if !exists {
            return Err(invalid(
                "validation.route_pool_group_not_found",
                "Route group was not found",
            ));
        }
        let mut order: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(sort_order),-1) FROM route_pool_members WHERE group_id=?",
        )
        .bind(group_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| {
            database_error(
                "database.route_group_move",
                "Could not load member order",
                error,
            )
        })?;
        let now = Utc::now().to_rfc3339();
        let mut seen = HashSet::new();
        for account_id in account_ids {
            let account_id = account_id.trim();
            if account_id.is_empty() || !seen.insert(account_id) {
                continue;
            }
            let account_platform: Option<String> =
                sqlx::query_scalar("SELECT platform FROM route_credentials WHERE id=?")
                    .bind(account_id)
                    .fetch_optional(&mut *transaction)
                    .await
                    .map_err(|error| {
                        database_error("database.route_group_move", "Could not load account", error)
                    })?;
            if account_platform.as_deref() != Some(platform) {
                return Err(invalid(
                    "validation.route_pool_platform_mismatch",
                    "Account belongs to another platform or does not exist",
                ));
            }
            order = order.saturating_add(1);
            let result=sqlx::query("INSERT INTO route_pool_members(id,platform,route_credential_id,enabled,sort_order,group_id,created_at,updated_at) VALUES(?,?,?,1,?,?,?,?) ON CONFLICT(platform,route_credential_id) DO UPDATE SET group_id=excluded.group_id,sort_order=excluded.sort_order,enabled=1,updated_at=excluded.updated_at WHERE route_pool_members.group_id IS NOT excluded.group_id")
                .bind(uuid::Uuid::new_v4().to_string()).bind(platform).bind(account_id).bind(order).bind(group_id).bind(&now).bind(&now)
                .execute(&mut *transaction).await.map_err(|error|database_error("database.route_group_move","Could not move account",error))?;
            if result.rows_affected() == 0 {
                order = order.saturating_sub(1);
            }
        }
        transaction.commit().await.map_err(|error| {
            database_error(
                "database.route_group_move",
                "Could not commit group move",
                error,
            )
        })
    }
}

pub(super) async fn delete_group_atomic(
    pool: &SqlitePool,
    platform: &str,
    group_id: &str,
) -> Result<(), AppError> {
    let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await.map_err(|error| {
        database_error(
            "database.route_group_delete",
            "Could not begin deletion",
            error,
        )
    })?;
    let group=sqlx::query("SELECT is_active,EXISTS(SELECT 1 FROM route_pool_members WHERE group_id=groups.id) AS has_members FROM route_pool_groups groups WHERE id=? AND platform=? AND deleted_at IS NULL")
        .bind(group_id).bind(platform).fetch_optional(&mut *transaction).await.map_err(|error|database_error("database.route_group_delete","Could not load group",error))?
        .ok_or_else(||invalid("validation.route_pool_group_not_found","Route group was not found"))?;
    if group.get::<bool, _>("is_active") {
        return Err(invalid(
            "validation.route_pool_group_active_delete",
            "Switch the active group before deleting it",
        ));
    }
    if group.get::<bool, _>("has_members") {
        return Err(invalid(
            "validation.route_pool_group_nonempty_delete",
            "Move all accounts out before deleting this group",
        ));
    }
    let now = Utc::now().to_rfc3339();
    sqlx::query("UPDATE route_pool_groups SET deleted_at=?,updated_at=? WHERE id=? AND platform=?")
        .bind(&now)
        .bind(&now)
        .bind(group_id)
        .bind(platform)
        .execute(&mut *transaction)
        .await
        .map_err(|error| {
            database_error(
                "database.route_group_delete",
                "Could not delete group",
                error,
            )
        })?;
    transaction.commit().await.map_err(|error| {
        database_error(
            "database.route_group_delete",
            "Could not commit deletion",
            error,
        )
    })
}

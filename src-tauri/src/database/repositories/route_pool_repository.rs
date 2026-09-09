use crate::error::AppError;
use crate::models::route_pool::{
    ProxyRequestRow, RoutePoolGroup, RoutePoolMemberAccount, RoutePoolStats, RoutePoolUsageLog,
    RouteUsageBreakdown,
};
use crate::services::route_pool_model_mode::PoolModelMode;
use chrono::Utc;
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool, Transaction};
use std::collections::HashSet;
use uuid::Uuid;

pub struct RoutePoolRepository;

impl RoutePoolRepository {
    pub async fn migrate_legacy_pool_views(pool: &SqlitePool) -> Result<(), AppError> {
        let mut tx =
            pool.begin_with("BEGIN IMMEDIATE")
                .await
                .map_err(|err| AppError::Database {
                    code: "database.route_pool_group_migrate_tx",
                    message: "Could not start route group migration".to_string(),
                    details: Some(err.to_string()),
                    recoverable: false,
                })?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS route_pool_group_migrations(version INTEGER PRIMARY KEY)",
        )
        .execute(&mut *tx)
        .await
        .map_err(|error| {
            super::route_credential_repository::database_error(
                "database.route_group_migration",
                "Could not initialize group migration",
                error,
            )
        })?;
        let applied: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM route_pool_group_migrations WHERE version=1)",
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| {
            super::route_credential_repository::database_error(
                "database.route_group_migration",
                "Could not load group migration",
                error,
            )
        })?;
        if applied {
            return tx.commit().await.map_err(|error| {
                super::route_credential_repository::database_error(
                    "database.route_group_migration",
                    "Could not close group migration",
                    error,
                )
            });
        }
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            "UPDATE route_pool_members
             SET group_id = CASE
               WHEN rc.archived_at IS NOT NULL THEN printf('%s-archived', rc.platform)
               WHEN route_pool_members.enabled = 1 THEN printf('%s-default', rc.platform)
               ELSE printf('%s-out', rc.platform)
             END,
             updated_at = ?
             FROM route_credentials rc
             WHERE rc.id = route_pool_members.route_credential_id
               AND route_pool_members.group_id IS NULL",
        )
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_pool_group_migrate_update",
            message: "Could not assign legacy route pool memberships".to_string(),
            details: Some(err.to_string()),
            recoverable: false,
        })?;

        sqlx::query(
            "INSERT INTO route_pool_members
               (id, platform, route_credential_id, enabled, sort_order, group_id, created_at, updated_at)
             SELECT
               printf('legacy-%s', rc.id),
               rc.platform,
               rc.id,
               0,
               0,
               CASE WHEN rc.archived_at IS NOT NULL THEN printf('%s-archived', rc.platform)
                    ELSE printf('%s-out', rc.platform) END,
               ?,
               ?
             FROM route_credentials rc
             WHERE NOT EXISTS (
               SELECT 1 FROM route_pool_members existing
               WHERE existing.platform = rc.platform
                 AND existing.route_credential_id = rc.id
             )",
        )
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_pool_group_migrate_insert",
            message: "Could not create missing route group memberships".to_string(),
            details: Some(err.to_string()),
            recoverable: false,
        })?;

        sqlx::query(
            "UPDATE route_pool_groups
             SET is_active = CASE WHEN id = printf('%s-default', platform) THEN 1 ELSE 0 END,
                 updated_at = ?
             WHERE platform IN (SELECT DISTINCT platform FROM route_credentials)
               AND NOT EXISTS (
                 SELECT 1 FROM route_pool_groups active
                 WHERE active.platform = route_pool_groups.platform
                   AND active.is_active = 1
                   AND active.deleted_at IS NULL
               )",
        )
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_pool_group_migrate_active",
            message: "Could not activate default route groups".to_string(),
            details: Some(err.to_string()),
            recoverable: false,
        })?;

        sqlx::query("INSERT INTO route_pool_group_migrations(version) VALUES(1)")
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                super::route_credential_repository::database_error(
                    "database.route_group_migration",
                    "Could not record group migration",
                    error,
                )
            })?;
        tx.commit().await.map_err(|err| AppError::Database {
            code: "database.route_pool_group_migrate_commit",
            message: "Could not commit route group migration".to_string(),
            details: Some(err.to_string()),
            recoverable: false,
        })?;
        Ok(())
    }

    pub async fn list_groups(
        pool: &SqlitePool,
        platform: &str,
        include_deleted: bool,
    ) -> Result<Vec<RoutePoolGroup>, AppError> {
        let mut query = QueryBuilder::<Sqlite>::new(
            "SELECT g.id, g.platform, g.name, g.sort_order, g.is_internal, g.is_active,
                    g.created_at, g.updated_at, COUNT(m.route_credential_id) AS account_count
             FROM route_pool_groups g
             LEFT JOIN route_pool_members m ON m.group_id = g.id
             WHERE g.platform = ",
        );
        query.push_bind(platform);
        if !include_deleted {
            query.push(" AND g.deleted_at IS NULL");
        }
        query.push(" GROUP BY g.id ORDER BY g.sort_order ASC, g.created_at ASC");
        let rows = query
            .build_query_as::<(String, String, String, i64, i64, i64, String, String, i64)>()
            .fetch_all(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.route_pool_groups",
                message: "Could not load route groups".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?;

        Ok(rows
            .into_iter()
            .map(
                |(
                    id,
                    platform,
                    name,
                    sort_order,
                    is_internal,
                    is_active,
                    created_at,
                    updated_at,
                    account_count,
                )| {
                    RoutePoolGroup {
                        id,
                        platform,
                        name,
                        sort_order,
                        is_internal: is_internal != 0,
                        is_active: is_active != 0,
                        account_count,
                        created_at,
                        updated_at,
                    }
                },
            )
            .collect())
    }

    pub async fn active_group_id(
        pool: &SqlitePool,
        platform: &str,
    ) -> Result<Option<String>, AppError> {
        sqlx::query_scalar(
            "SELECT id FROM route_pool_groups
             WHERE platform = ? AND is_active = 1 AND deleted_at IS NULL
             ORDER BY sort_order ASC LIMIT 1",
        )
        .bind(platform)
        .fetch_optional(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_pool_group_active",
            message: "Could not load the active route group".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })
    }

    pub async fn group_id_for_account(
        pool: &SqlitePool,
        platform: &str,
        credential_id: &str,
    ) -> Result<Option<String>, AppError> {
        sqlx::query_scalar(
            "SELECT group_id FROM route_pool_members
             WHERE platform = ? AND route_credential_id = ?",
        )
        .bind(platform)
        .bind(credential_id)
        .fetch_optional(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_pool_group_for_account",
            message: "Could not load the account route group".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })
    }

    pub async fn list_member_ids(
        pool: &SqlitePool,
        platform: &str,
    ) -> Result<Vec<String>, AppError> {
        let rows = sqlx::query(
            "SELECT rpm.route_credential_id
             FROM route_pool_members rpm
             INNER JOIN route_pool_groups g ON g.id = rpm.group_id
             WHERE rpm.platform = ? AND g.is_active = 1 AND g.deleted_at IS NULL
             ORDER BY rpm.sort_order ASC, rpm.created_at ASC",
        )
        .bind(platform)
        .fetch_all(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_pool_members",
            message: "Could not load route pool members".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Ok(rows
            .into_iter()
            .map(|row| row.get::<String, _>("route_credential_id"))
            .collect())
    }

    pub async fn list_group_member_ids(
        pool: &SqlitePool,
        group_id: &str,
    ) -> Result<Vec<String>, AppError> {
        sqlx::query_scalar(
            "SELECT route_credential_id
             FROM route_pool_members
             WHERE group_id = ?
             ORDER BY sort_order ASC, created_at ASC",
        )
        .bind(group_id)
        .fetch_all(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_pool_group_members",
            message: "Could not load route group members".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })
    }

    pub async fn create_group(
        pool: &SqlitePool,
        platform: &str,
        name: &str,
        is_internal: bool,
    ) -> Result<String, AppError> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let sort_order = sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(MAX(sort_order), -1) + 1
             FROM route_pool_groups
             WHERE platform = ? AND deleted_at IS NULL",
        )
        .bind(platform)
        .fetch_one(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_pool_group_create_order",
            message: "Could not allocate route group order".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        sqlx::query(
            "INSERT INTO route_pool_groups
               (id, platform, name, sort_order, is_internal, is_active, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, 0, ?, ?)",
        )
        .bind(&id)
        .bind(platform)
        .bind(name)
        .bind(sort_order)
        .bind(is_internal as i64)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_pool_group_create",
            message: "Could not create route group".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Ok(id)
    }

    pub async fn pool_membership_map(
        pool: &SqlitePool,
        platform: &str,
        ids: &[String],
    ) -> Result<HashSet<String>, AppError> {
        let mut seen = HashSet::with_capacity(ids.len());
        let unique_ids = ids
            .iter()
            .filter(|id| seen.insert(id.as_str()))
            .collect::<Vec<_>>();
        if unique_ids.is_empty() {
            return Ok(HashSet::new());
        }

        let mut query = QueryBuilder::<Sqlite>::new(
            "SELECT route_credential_id
             FROM route_pool_members
             WHERE platform = ",
        );
        query
            .push_bind(platform)
            .push(" AND group_id IN (SELECT id FROM route_pool_groups WHERE is_active=1 AND deleted_at IS NULL) AND route_credential_id IN (");
        let mut separated = query.separated(", ");
        for id in unique_ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");

        query
            .build_query_scalar::<String>()
            .fetch_all(pool)
            .await
            .map(|rows| rows.into_iter().collect())
            .map_err(|err| AppError::Database {
                code: "database.route_pool_membership_map",
                message: "Could not load route pool membership".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }

    pub async fn update_group(
        pool: &SqlitePool,
        platform: &str,
        group_id: &str,
        name: Option<&str>,
        is_internal: Option<bool>,
        activate: bool,
        sort_order: Option<i64>,
    ) -> Result<(), AppError> {
        let mut tx = pool.begin().await.map_err(|err| AppError::Database {
            code: "database.route_pool_group_update_tx",
            message: "Could not start route group update".to_string(),
            details: Some(err.to_string()),
            recoverable: false,
        })?;
        let now = Utc::now().to_rfc3339();
        if activate {
            sqlx::query(
                "UPDATE route_pool_groups
                 SET is_active = 0, updated_at = ?
                 WHERE platform = ? AND deleted_at IS NULL",
            )
            .bind(&now)
            .bind(platform)
            .execute(&mut *tx)
            .await
            .map_err(|err| AppError::Database {
                code: "database.route_pool_group_deactivate",
                message: "Could not deactivate route groups".to_string(),
                details: Some(err.to_string()),
                recoverable: false,
            })?;
        }

        let mut query = QueryBuilder::<Sqlite>::new("UPDATE route_pool_groups SET updated_at = ");
        query.push_bind(&now);
        if let Some(name) = name {
            query.push(", name = ").push_bind(name.to_string());
        }
        if let Some(is_internal) = is_internal {
            query.push(", is_internal = ").push_bind(is_internal as i64);
        }
        if activate {
            query.push(", is_active = 1");
        }
        if let Some(sort_order) = sort_order {
            query.push(", sort_order = ").push_bind(sort_order);
        }
        query
            .push(" WHERE platform = ")
            .push_bind(platform)
            .push(" AND id = ")
            .push_bind(group_id)
            .push(" AND deleted_at IS NULL");
        let result = query
            .build()
            .execute(&mut *tx)
            .await
            .map_err(|err| AppError::Database {
                code: "database.route_pool_group_update",
                message: "Could not update route group".to_string(),
                details: Some(err.to_string()),
                recoverable: false,
            })?;
        if result.rows_affected() != 1 {
            return Err(AppError::Validation {
                code: "validation.route_pool_group_not_found",
                message: "Route group was not found".to_string(),
                details: Some(format!("{platform}:{group_id}")),
                recoverable: true,
            });
        }

        tx.commit().await.map_err(|err| AppError::Database {
            code: "database.route_pool_group_update_commit",
            message: "Could not commit route group update".to_string(),
            details: Some(err.to_string()),
            recoverable: false,
        })?;
        Ok(())
    }

    pub async fn delete_group(
        pool: &SqlitePool,
        platform: &str,
        group_id: &str,
    ) -> Result<(), AppError> {
        super::route_group_repository::delete_group_atomic(pool, platform, group_id).await
    }

    pub async fn replace_group_members(
        pool: &SqlitePool,
        platform: &str,
        group_id: &str,
        account_ids: &[String],
    ) -> Result<(), AppError> {
        let mut tx = pool.begin().await.map_err(|err| AppError::Database {
            code: "database.route_pool_group_members_tx",
            message: "Could not start route group member update".to_string(),
            details: Some(err.to_string()),
            recoverable: false,
        })?;
        let group_exists = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM route_pool_groups
             WHERE platform = ? AND id = ? AND deleted_at IS NULL",
        )
        .bind(platform)
        .bind(group_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_pool_group_members_group",
            message: "Could not verify route group".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;
        if group_exists != 1 {
            return Err(AppError::Validation {
                code: "validation.route_pool_group_not_found",
                message: "Route group was not found".to_string(),
                details: Some(format!("{platform}:{group_id}")),
                recoverable: true,
            });
        }

        let current: Vec<String> = sqlx::query_scalar(
            "SELECT route_credential_id FROM route_pool_members WHERE group_id=? AND platform=?",
        )
        .bind(group_id)
        .bind(platform)
        .fetch_all(&mut *tx)
        .await
        .map_err(|error| {
            super::route_credential_repository::database_error(
                "database.route_group_replace",
                "Could not load group members",
                error,
            )
        })?;
        let retained: HashSet<&str> = account_ids.iter().map(String::as_str).collect();
        let removed: Vec<&String> = current
            .iter()
            .filter(|id| !retained.contains(id.as_str()))
            .collect();
        if !removed.is_empty() {
            let fallback = format!("{platform}-out");
            let available:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM route_pool_groups WHERE id=? AND platform=? AND deleted_at IS NULL AND is_active=0)")
                .bind(&fallback).bind(platform).fetch_one(&mut *tx).await.map_err(|error|super::route_credential_repository::database_error("database.route_group_replace","Could not load destination",error))?;
            if fallback == group_id || !available {
                return Err(AppError::Validation {
                    code: "validation.route_pool_group_move_required",
                    message: "Move omitted accounts to an explicit group instead".into(),
                    details: None,
                    recoverable: true,
                });
            }
            for id in removed {
                sqlx::query("UPDATE route_pool_members SET group_id=?,enabled=0,updated_at=? WHERE route_credential_id=? AND platform=?")
                    .bind(&fallback).bind(Utc::now().to_rfc3339()).bind(id).bind(platform).execute(&mut *tx).await.map_err(|error|super::route_credential_repository::database_error("database.route_group_replace","Could not preserve membership",error))?;
            }
        }
        sqlx::query("DELETE FROM route_pool_members WHERE platform = ? AND group_id = ?")
            .bind(platform)
            .bind(group_id)
            .execute(&mut *tx)
            .await
            .map_err(|err| AppError::Database {
                code: "database.route_pool_group_members_clear",
                message: "Could not clear route group members".to_string(),
                details: Some(err.to_string()),
                recoverable: false,
            })?;

        if !account_ids.is_empty() {
            let mut delete_query =
                QueryBuilder::<Sqlite>::new("DELETE FROM route_pool_members WHERE platform = ");
            delete_query
                .push_bind(platform)
                .push(" AND route_credential_id IN (");
            let mut separated = delete_query.separated(", ");
            for account_id in account_ids {
                separated.push_bind(account_id);
            }
            separated.push_unseparated(")");
            delete_query
                .build()
                .execute(&mut *tx)
                .await
                .map_err(|err| AppError::Database {
                    code: "database.route_pool_group_members_remove",
                    message: "Could not remove prior group memberships".to_string(),
                    details: Some(err.to_string()),
                    recoverable: false,
                })?;

            let now = Utc::now().to_rfc3339();
            for (index, account_id) in account_ids.iter().enumerate() {
                sqlx::query(
                    "INSERT INTO route_pool_members
                       (id, platform, route_credential_id, enabled, sort_order, group_id, created_at, updated_at)
                     VALUES (?, ?, ?, 1, ?, ?, ?, ?)",
                )
                .bind(Uuid::new_v4().to_string())
                .bind(platform)
                .bind(account_id)
                .bind(index as i64)
                .bind(group_id)
                .bind(&now)
                .bind(&now)
                .execute(&mut *tx)
                .await
                .map_err(|err| AppError::Database {
                    code: "database.route_pool_group_members_insert",
                    message: "Could not save route group members".to_string(),
                    details: Some(err.to_string()),
                    recoverable: false,
                })?;
            }
        }

        tx.commit().await.map_err(|err| AppError::Database {
            code: "database.route_pool_group_members_commit",
            message: "Could not commit route group members".to_string(),
            details: Some(err.to_string()),
            recoverable: false,
        })?;
        Ok(())
    }

    pub async fn replace_members(
        pool: &SqlitePool,
        platform: &str,
        account_ids: &[String],
    ) -> Result<Vec<String>, AppError> {
        let group_id = Self::active_group_id(pool, platform)
            .await?
            .ok_or_else(|| AppError::Validation {
                code: "validation.route_pool_group_active_missing",
                message: "Platform has no active route group".to_string(),
                details: Some(platform.to_string()),
                recoverable: true,
            })?;
        Self::replace_group_members(pool, platform, &group_id, account_ids).await?;

        Self::list_member_ids(pool, platform).await
    }

    /// Append members outside of a caller-owned transaction, committing on success.
    /// Existing memberships are preserved (ON CONFLICT DO NOTHING).
    pub async fn append_members(
        pool: &SqlitePool,
        platform: &str,
        credential_ids: &[String],
    ) -> Result<usize, AppError> {
        if credential_ids.is_empty() {
            return Ok(0);
        }

        let mut tx = pool.begin().await.map_err(|err| AppError::Database {
            code: "database.route_pool_tx",
            message: "Could not start route pool update".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        let inserted = Self::append_members_tx(&mut tx, platform, credential_ids).await?;

        tx.commit().await.map_err(|err| AppError::Database {
            code: "database.route_pool_commit",
            message: "Could not save route pool members".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Ok(inserted)
    }

    pub async fn append_members_tx(
        tx: &mut Transaction<'_, Sqlite>,
        platform: &str,
        credential_ids: &[String],
    ) -> Result<usize, AppError> {
        if credential_ids.is_empty() {
            return Ok(0);
        }

        let mut current_max = sqlx::query_scalar::<_, Option<i64>>(
            "SELECT MAX(sort_order) FROM route_pool_members WHERE platform = ?",
        )
        .bind(platform)
        .fetch_one(&mut **tx)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_pool_append_order",
            message: "Could not allocate route pool member order".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?
        .unwrap_or(-1);
        let active_group_id = sqlx::query_scalar::<_, String>(
            "SELECT id FROM route_pool_groups
             WHERE platform = ? AND is_active = 1 AND deleted_at IS NULL
             ORDER BY sort_order ASC LIMIT 1",
        )
        .bind(platform)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_pool_append_group",
            message: "Could not load the active route group".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?
        .ok_or_else(|| AppError::Validation {
            code: "validation.route_pool_group_active_missing",
            message: "Platform has no active route group".to_string(),
            details: Some(platform.to_string()),
            recoverable: true,
        })?;
        let now = Utc::now().to_rfc3339();
        let mut inserted = 0usize;

        for credential_id in credential_ids {
            let next_sort_order = current_max.saturating_add(1);
            let result = sqlx::query(
                "INSERT INTO route_pool_members
                 (id, platform, route_credential_id, enabled, sort_order, group_id, created_at, updated_at)
                 VALUES (?, ?, ?, 1, ?, ?, ?, ?)
                 ON CONFLICT(platform, route_credential_id) DO UPDATE SET
                   group_id = excluded.group_id,
                   sort_order = excluded.sort_order,
                   updated_at = excluded.updated_at
                 WHERE route_pool_members.group_id IS NOT excluded.group_id",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(platform)
            .bind(credential_id)
            .bind(next_sort_order)
            .bind(&active_group_id)
            .bind(&now)
            .bind(&now)
            .execute(&mut **tx)
            .await
            .map_err(|err| AppError::Database {
                code: "database.route_pool_append",
                message: "Could not append route pool member".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?;

            if result.rows_affected() == 1 {
                current_max = next_sort_order;
                inserted += 1;
            }
        }

        Ok(inserted)
    }

    pub async fn member_accounts(
        pool: &SqlitePool,
        platform: &str,
    ) -> Result<Vec<RoutePoolMemberAccount>, AppError> {
        let group_id = Self::active_group_id(pool, platform)
            .await?
            .ok_or_else(|| AppError::Validation {
                code: "validation.route_pool_group_active_missing",
                message: "Platform has no active route group".to_string(),
                details: Some(platform.to_string()),
                recoverable: true,
            })?;
        Self::member_accounts_for_group(pool, &group_id).await
    }

    pub async fn member_accounts_for_group(
        pool: &SqlitePool,
        group_id: &str,
    ) -> Result<Vec<RoutePoolMemberAccount>, AppError> {
        let rows = sqlx::query(
            "SELECT a.id, a.display_name, a.status, a.route_priority, a.max_concurrency
             FROM route_pool_members rpm
             INNER JOIN route_credentials a ON a.id = rpm.route_credential_id
             WHERE rpm.group_id = ? AND a.archived_at IS NULL
             ORDER BY a.route_priority ASC, rpm.sort_order ASC, rpm.created_at ASC",
        )
        .bind(group_id)
        .fetch_all(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_pool_member_accounts",
            message: "Could not load route pool account records".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Ok(rows
            .into_iter()
            .map(|row| RoutePoolMemberAccount {
                id: row.get("id"),
                display_name: row.get("display_name"),
                status: row.get("status"),
                route_priority: row.get("route_priority"),
                max_concurrency: row.get("max_concurrency"),
            })
            .collect())
    }

    /// The platform's model catalog mode. A missing row — or an unrecognized
    /// value — reads as [`PoolModelMode::Aggregate`]: this switch must never be
    /// the thing that makes the pool unusable.
    pub async fn model_mode(pool: &SqlitePool, platform: &str) -> Result<PoolModelMode, AppError> {
        let row = sqlx::query("SELECT mode FROM route_pool_model_modes WHERE platform = ?")
            .bind(platform)
            .fetch_optional(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.route_pool_model_mode_get",
                message: "Could not load route pool model mode".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?;

        Ok(row
            .map(|row| PoolModelMode::parse(row.get::<String, _>("mode").as_str()))
            .unwrap_or_default())
    }

    pub async fn save_model_mode(
        pool: &SqlitePool,
        platform: &str,
        mode: PoolModelMode,
    ) -> Result<(), AppError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO route_pool_model_modes (platform, mode, updated_at)
             VALUES (?, ?, ?)
             ON CONFLICT(platform) DO UPDATE SET mode = excluded.mode, updated_at = excluded.updated_at",
        )
        .bind(platform)
        .bind(mode.as_str())
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_pool_model_mode_save",
            message: "Could not save route pool model mode".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Ok(())
    }

    pub async fn next_cursor_index(pool: &SqlitePool, platform: &str) -> Result<i64, AppError> {
        let row = sqlx::query("SELECT next_index FROM route_pool_cursors WHERE platform = ?")
            .bind(platform)
            .fetch_optional(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.route_pool_cursor_get",
                message: "Could not load route pool cursor".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?;

        Ok(row.map(|row| row.get("next_index")).unwrap_or(0))
    }

    pub async fn save_cursor_index(
        pool: &SqlitePool,
        platform: &str,
        next_index: i64,
    ) -> Result<(), AppError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO route_pool_cursors (platform, next_index, updated_at)
             VALUES (?, ?, ?)
             ON CONFLICT(platform) DO UPDATE SET next_index = excluded.next_index, updated_at = excluded.updated_at",
        )
        .bind(platform)
        .bind(next_index)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_pool_cursor_save",
            message: "Could not save route pool cursor".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Ok(())
    }

    pub async fn insert_usage_event(
        pool: &SqlitePool,
        account_id: &str,
        source_label: &str,
        metric_type: &str,
        amount: i64,
        unit: &str,
        metadata_json: &str,
    ) -> Result<(), AppError> {
        let now = Utc::now().to_rfc3339();
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
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.usage_event_insert",
            message: "Could not record route usage event".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Ok(())
    }

    pub async fn insert_request_event(
        pool: &SqlitePool,
        account_id: &str,
        source_label: &str,
        metadata_json: &str,
        usage: &RouteUsageBreakdown,
        upstream_response_id: Option<&str>,
    ) -> Result<(), AppError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO usage_events
             (id, route_credential_id, source_label, metric_type, amount, unit,
              metadata_json, input_tokens, output_tokens, cache_tokens,
              price_usd_micros, price_cny_micros, price_currency, price_source,
              upstream_response_id, created_at)
             VALUES (?, ?, ?, 'request', 1, 'count', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(account_id)
        .bind(source_label)
        .bind(metadata_json)
        .bind(usage.input_tokens)
        .bind(usage.output_tokens)
        .bind(usage.cache_tokens)
        .bind(usage.price_usd_micros)
        .bind(usage.price_cny_micros)
        .bind(&usage.price_currency)
        .bind(&usage.price_source)
        .bind(upstream_response_id)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.request_usage_insert",
            message: "Could not record route request usage".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Ok(())
    }

    /// Every proxied request in the window, across all platforms.
    ///
    /// Deliberately unfiltered by platform and by `archived_at`: the usage
    /// overview reports total spend, so a request stays counted after its
    /// account is archived. Archiving governs the account list, not history.
    ///
    /// The join is outer for the same reason. Deleting an account removes only
    /// the `route_credentials` row — `usage_events.route_credential_id` has no
    /// cascade — so an inner join would drop the whole history of every deleted
    /// account and understate spend without saying so. The request metadata
    /// carries the platform and the account name it was written with, which is
    /// exactly what is needed to label a row whose account is gone.
    pub async fn list_request_events(
        pool: &SqlitePool,
        since: Option<&str>,
    ) -> Result<Vec<ProxyRequestRow>, AppError> {
        let since_clause = if since.is_some() {
            " AND ue.created_at >= ?"
        } else {
            ""
        };
        let sql = format!(
            "SELECT ue.id,
                    COALESCE(a.platform, json_extract(ue.metadata_json, '$.platform'), '') AS platform,
                    ue.route_credential_id,
                    COALESCE(a.display_name, json_extract(ue.metadata_json, '$.route_credential_name')) AS account_name,
                    ue.source_label,
                    ue.metadata_json, ue.created_at,
                    ue.input_tokens, ue.output_tokens, ue.cache_tokens,
                    ue.price_usd_micros, ue.price_cny_micros, ue.price_currency,
                    ue.price_source, ue.upstream_response_id
             FROM usage_events ue
             LEFT JOIN route_credentials a ON a.id = ue.route_credential_id
             WHERE ue.metric_type = 'request'{since_clause}
             ORDER BY ue.created_at DESC, ue.id DESC"
        );
        let mut query = sqlx::query(&sql);
        if let Some(since) = since {
            query = query.bind(since);
        }
        let rows = query
            .fetch_all(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.usage_overview_requests",
                message: "Could not load proxied requests".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?;

        Ok(rows
            .into_iter()
            .map(|row| ProxyRequestRow {
                id: row.get("id"),
                platform: row.get("platform"),
                account_id: row.get("route_credential_id"),
                account_name: row.get("account_name"),
                source_label: row.get("source_label"),
                metadata_json: row.get("metadata_json"),
                created_at: row.get("created_at"),
                input_tokens: row.get("input_tokens"),
                output_tokens: row.get("output_tokens"),
                cache_tokens: row.get("cache_tokens"),
                price_usd_micros: row.get("price_usd_micros"),
                price_cny_micros: row.get("price_cny_micros"),
                price_currency: row.get("price_currency"),
                price_source: row.get("price_source"),
                upstream_response_id: row.get("upstream_response_id"),
            })
            .collect())
    }

    pub async fn stats(
        pool: &SqlitePool,
        platform: &str,
        since: Option<&str>,
        request_page: i64,
        request_page_size: i64,
    ) -> Result<RoutePoolStats, AppError> {
        let usage_since_clause = if since.is_some() {
            " AND ue.created_at >= ?"
        } else {
            ""
        };
        // Interpolated rather than hardcoded so the CNY rate lives in exactly one
        // place, shared with the local price estimator.
        let cny_per_usd = crate::services::model_pricing::CNY_PER_USD;
        let summary_sql = format!(
            "SELECT
               (SELECT COUNT(DISTINCT rpm.route_credential_id)
                FROM route_pool_members rpm
                INNER JOIN route_credentials a ON a.id = rpm.route_credential_id
                WHERE rpm.platform = ? AND rpm.enabled = 1 AND a.archived_at IS NULL) AS member_count,
               COALESCE(SUM(CASE WHEN ue.metric_type = 'request' THEN CASE WHEN ue.amount > 0 THEN ue.amount ELSE 1 END ELSE 0 END), 0) AS request_count,
               COALESCE(SUM(CASE WHEN ue.metric_type = 'request' THEN COALESCE(ue.input_tokens, 0) ELSE 0 END), 0) AS input_token_count,
               COALESCE(SUM(CASE WHEN ue.metric_type = 'request' THEN COALESCE(ue.output_tokens, 0) ELSE 0 END), 0) AS output_token_count,
               COALESCE(SUM(CASE WHEN ue.metric_type = 'request' THEN COALESCE(ue.cache_tokens, 0) ELSE 0 END), 0) AS cache_token_count,
               COALESCE(SUM(CASE WHEN ue.metric_type = 'request' THEN COALESCE(ue.input_tokens, 0) + COALESCE(ue.output_tokens, 0) ELSE 0 END), 0)
                 + COALESCE(SUM(CASE WHEN ue.metric_type = 'token' OR ue.unit = 'token' THEN ue.amount ELSE 0 END), 0) AS token_count,
               COALESCE(SUM(CASE
                   WHEN ue.metric_type = 'request' AND ue.price_currency = 'usd' THEN COALESCE(ue.price_usd_micros, 0)
                   WHEN ue.metric_type = 'request' AND ue.price_currency = 'cny' THEN CAST(ROUND(COALESCE(ue.price_cny_micros, 0) / {cny_per_usd}) AS INTEGER)
                   WHEN ue.metric_type = 'cost' AND ue.unit = 'usd_micros' THEN ue.amount
                   ELSE 0
               END), 0) AS cost_micros
             FROM usage_events ue
             INNER JOIN route_credentials a ON a.id = ue.route_credential_id
             WHERE a.platform = ? AND a.archived_at IS NULL{usage_since_clause}"
        );
        let mut summary_query = sqlx::query(&summary_sql).bind(platform).bind(platform);
        if let Some(since) = since {
            summary_query = summary_query.bind(since);
        }
        let row = summary_query
            .fetch_one(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.route_pool_stats",
                message: "Could not load route pool statistics".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?;

        let log_sql = format!(
            "SELECT ue.id, ue.route_credential_id, a.display_name AS account_name,
                    ue.source_label, ue.metric_type, ue.amount, ue.unit, ue.metadata_json, ue.created_at,
                    ue.input_tokens, ue.output_tokens, ue.cache_tokens,
                    ue.price_usd_micros, ue.price_cny_micros, ue.price_currency, ue.price_source,
                    ue.upstream_response_id
             FROM usage_events ue
             INNER JOIN route_credentials a ON a.id = ue.route_credential_id
             WHERE a.platform = ? AND a.archived_at IS NULL{usage_since_clause}
             ORDER BY ue.created_at DESC, ue.id DESC
             LIMIT 10"
        );
        let mut log_query = sqlx::query(&log_sql).bind(platform);
        if let Some(since) = since {
            log_query = log_query.bind(since);
        }
        let log_rows = log_query
            .fetch_all(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.route_pool_logs",
                message: "Could not load route pool logs".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?;

        let request_count_sql = format!(
            "SELECT COUNT(*) AS request_row_count
             FROM usage_events ue
             INNER JOIN route_credentials a ON a.id = ue.route_credential_id
             WHERE a.platform = ? AND a.archived_at IS NULL AND ue.metric_type = 'request'{usage_since_clause}"
        );
        let mut request_count_query = sqlx::query(&request_count_sql).bind(platform);
        if let Some(since) = since {
            request_count_query = request_count_query.bind(since);
        }
        let request_count_row =
            request_count_query
                .fetch_one(pool)
                .await
                .map_err(|err| AppError::Database {
                    code: "database.route_pool_request_count",
                    message: "Could not count route pool requests".to_string(),
                    details: Some(err.to_string()),
                    recoverable: true,
                })?;
        let request_row_count: i64 = request_count_row.get("request_row_count");

        let request_sql = format!(
            "SELECT ue.id, ue.route_credential_id, a.display_name AS account_name,
                    ue.source_label, ue.metric_type, ue.amount, ue.unit, ue.metadata_json, ue.created_at,
                    ue.input_tokens, ue.output_tokens, ue.cache_tokens,
                    ue.price_usd_micros, ue.price_cny_micros, ue.price_currency, ue.price_source,
                    ue.upstream_response_id
             FROM usage_events ue
             INNER JOIN route_credentials a ON a.id = ue.route_credential_id
             WHERE a.platform = ? AND a.archived_at IS NULL AND ue.metric_type = 'request'{usage_since_clause}
             ORDER BY ue.created_at DESC, ue.id DESC
             LIMIT ? OFFSET ?"
        );
        let offset = (request_page - 1) * request_page_size;
        let mut request_query = sqlx::query(&request_sql).bind(platform);
        if let Some(since) = since {
            request_query = request_query.bind(since);
        }
        let request_rows = request_query
            .bind(request_page_size)
            .bind(offset)
            .fetch_all(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.route_pool_requests",
                message: "Could not load route pool requests".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?;

        let map_usage_log = |row: sqlx::sqlite::SqliteRow| RoutePoolUsageLog {
            id: row.get("id"),
            account_id: row.get("route_credential_id"),
            account_name: row.get("account_name"),
            source_label: row.get("source_label"),
            metric_type: row.get("metric_type"),
            amount: row.get("amount"),
            unit: row.get("unit"),
            metadata_json: row.get("metadata_json"),
            created_at: row.get("created_at"),
            input_tokens: row.get("input_tokens"),
            output_tokens: row.get("output_tokens"),
            cache_tokens: row.get("cache_tokens"),
            price_usd_micros: row.get("price_usd_micros"),
            price_cny_micros: row.get("price_cny_micros"),
            price_currency: row.get("price_currency"),
            price_source: row.get("price_source"),
            upstream_response_id: row.get("upstream_response_id"),
        };

        Ok(RoutePoolStats {
            member_count: row.get("member_count"),
            request_count: row.get("request_count"),
            token_count: row.get("token_count"),
            input_token_count: row.get("input_token_count"),
            output_token_count: row.get("output_token_count"),
            cache_token_count: row.get("cache_token_count"),
            cost_micros: row.get("cost_micros"),
            recent_logs: log_rows.into_iter().map(map_usage_log).collect(),
            requests: request_rows.into_iter().map(map_usage_log).collect(),
            request_row_count,
            request_page,
            request_page_size,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::repositories::route_credential_repository::RouteCredentialRepository;

    async fn create_credential(pool: &SqlitePool, platform: &str, display_name: &str) -> String {
        RouteCredentialRepository::create(
            pool,
            platform,
            "api",
            display_name,
            None,
            "ok",
            None,
            r#"{"api_key":"sk-test"}"#,
            r#"{"base_url":"https://example.com","interface_format":"openai","model_mappings":[]}"#,
            "{}",
        )
        .await
        .unwrap()
        .id
    }

    #[tokio::test]
    async fn model_mode_defaults_to_aggregate_and_survives_a_round_trip() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();

        // No row yet: every pool that predates the switch keeps rotating.
        assert_eq!(
            RoutePoolRepository::model_mode(&pool, "codex")
                .await
                .unwrap(),
            PoolModelMode::Aggregate
        );

        RoutePoolRepository::save_model_mode(&pool, "codex", PoolModelMode::Precise)
            .await
            .unwrap();
        assert_eq!(
            RoutePoolRepository::model_mode(&pool, "codex")
                .await
                .unwrap(),
            PoolModelMode::Precise
        );
        // Per platform: switching codex must not move claude.
        assert_eq!(
            RoutePoolRepository::model_mode(&pool, "claude")
                .await
                .unwrap(),
            PoolModelMode::Aggregate
        );

        RoutePoolRepository::save_model_mode(&pool, "codex", PoolModelMode::Aggregate)
            .await
            .unwrap();
        assert_eq!(
            RoutePoolRepository::model_mode(&pool, "codex")
                .await
                .unwrap(),
            PoolModelMode::Aggregate
        );
    }

    #[tokio::test]
    async fn legacy_pool_views_migrate_to_dynamic_groups_once() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let pool_account = create_credential(&pool, "codex", "Pool").await;
        let outside_account = create_credential(&pool, "codex", "Outside").await;
        let archived_account = create_credential(&pool, "codex", "Archived").await;
        RoutePoolRepository::replace_members(&pool, "codex", &[pool_account.clone()])
            .await
            .unwrap();
        RouteCredentialRepository::set_archived(
            &pool,
            std::slice::from_ref(&archived_account),
            true,
        )
        .await
        .unwrap();

        sqlx::query("DELETE FROM route_pool_group_migrations")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE route_pool_members SET group_id = NULL")
            .execute(&pool)
            .await
            .unwrap();
        RoutePoolRepository::migrate_legacy_pool_views(&pool)
            .await
            .unwrap();
        RoutePoolRepository::migrate_legacy_pool_views(&pool)
            .await
            .unwrap();

        let groups = RoutePoolRepository::list_groups(&pool, "codex", false)
            .await
            .unwrap();
        assert_eq!(
            groups
                .iter()
                .map(|group| group.name.as_str())
                .collect::<Vec<_>>(),
            vec!["默认组", "未入池", "已归档"]
        );
        let active_group = RoutePoolRepository::active_group_id(&pool, "codex")
            .await
            .unwrap()
            .expect("default group is active");
        let groups_by_name = groups
            .iter()
            .map(|group| (group.name.as_str(), group.id.as_str()))
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(active_group, groups_by_name["默认组"]);
        assert_eq!(
            RoutePoolRepository::group_id_for_account(&pool, "codex", &pool_account)
                .await
                .unwrap()
                .as_deref(),
            Some(groups_by_name["默认组"])
        );
        assert_eq!(
            RoutePoolRepository::group_id_for_account(&pool, "codex", &outside_account)
                .await
                .unwrap()
                .as_deref(),
            Some(groups_by_name["未入池"])
        );
        assert_eq!(
            RoutePoolRepository::group_id_for_account(&pool, "codex", &archived_account)
                .await
                .unwrap()
                .as_deref(),
            Some(groups_by_name["已归档"])
        );
        assert_eq!(groups[0].account_count, 1);
        assert_eq!(groups[1].account_count, 1);
        assert_eq!(groups[2].account_count, 1);

        let archived_at: Option<String> =
            sqlx::query_scalar("SELECT archived_at FROM route_credentials WHERE id = ?")
                .bind(&archived_account)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(archived_at.is_some());
    }

    #[tokio::test]
    async fn member_accounts_follow_the_active_dynamic_group() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let default_account = create_credential(&pool, "codex", "Default").await;
        let alternate_account = create_credential(&pool, "codex", "Alternate").await;
        let default_group = RoutePoolRepository::active_group_id(&pool, "codex")
            .await
            .unwrap()
            .unwrap();
        RoutePoolRepository::replace_group_members(
            &pool,
            "codex",
            &default_group,
            &[default_account.clone()],
        )
        .await
        .unwrap();
        let alternate_group = RoutePoolRepository::create_group(&pool, "codex", "备选组", false)
            .await
            .unwrap();
        RoutePoolRepository::replace_group_members(
            &pool,
            "codex",
            &alternate_group,
            &[alternate_account.clone()],
        )
        .await
        .unwrap();

        let members = RoutePoolRepository::member_accounts(&pool, "codex")
            .await
            .unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].id, default_account);

        RoutePoolRepository::update_group(&pool, "codex", &alternate_group, None, None, true, None)
            .await
            .unwrap();
        let members = RoutePoolRepository::member_accounts(&pool, "codex")
            .await
            .unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].id, alternate_account);

        let group_members = RoutePoolRepository::member_accounts_for_group(&pool, &default_group)
            .await
            .unwrap();
        assert_eq!(group_members.len(), 1);
        assert_eq!(group_members[0].id, default_account);
    }

    #[tokio::test]
    async fn appended_members_join_the_active_dynamic_group() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let account_id = create_credential(&pool, "codex", "New").await;

        let inserted = RoutePoolRepository::append_members(&pool, "codex", &[account_id.clone()])
            .await
            .unwrap();

        assert_eq!(inserted, 1);
        assert_eq!(
            RoutePoolRepository::list_member_ids(&pool, "codex")
                .await
                .unwrap(),
            vec![account_id]
        );
    }

    #[tokio::test]
    async fn a_hand_edited_model_mode_reads_as_aggregate() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO route_pool_model_modes (platform, mode, updated_at) VALUES (?, ?, ?)",
        )
        .bind("codex")
        .bind("per-account")
        .bind("2026-09-06T00:00:00Z")
        .execute(&pool)
        .await
        .unwrap();

        assert_eq!(
            RoutePoolRepository::model_mode(&pool, "codex")
                .await
                .unwrap(),
            PoolModelMode::Aggregate
        );
    }

    #[tokio::test]
    async fn member_accounts_excludes_archived_but_membership_ids_are_preserved() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let archived = create_credential(&pool, "codex", "Archived").await;
        let active = create_credential(&pool, "codex", "Active").await;
        RoutePoolRepository::replace_members(&pool, "codex", &[archived.clone(), active.clone()])
            .await
            .unwrap();
        RouteCredentialRepository::set_archived(&pool, std::slice::from_ref(&archived), true)
            .await
            .unwrap();

        let runtime_members = RoutePoolRepository::member_accounts(&pool, "codex")
            .await
            .unwrap();
        assert_eq!(runtime_members.len(), 1);
        assert_eq!(runtime_members[0].id, active);
        assert_eq!(
            RoutePoolRepository::list_member_ids(&pool, "codex")
                .await
                .unwrap(),
            vec![archived, runtime_members[0].id.clone()]
        );
    }

    #[tokio::test]
    async fn pool_membership_map_uses_enabled_members_from_full_selection() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let mut ids = Vec::new();
        for index in 0..21 {
            ids.push(create_credential(&pool, "codex", &format!("Credential {index}")).await);
        }
        let other_platform_id = create_credential(&pool, "claude", "Claude credential").await;
        RoutePoolRepository::replace_members(&pool, "codex", &[ids[0].clone(), ids[20].clone()])
            .await
            .unwrap();
        RoutePoolRepository::replace_members(&pool, "claude", &[other_platform_id.clone()])
            .await
            .unwrap();
        sqlx::query(
            "UPDATE route_pool_members SET enabled = 0, group_id = 'codex-out' WHERE platform = ? AND route_credential_id = ?",
        )
        .bind("codex")
        .bind(&ids[0])
        .execute(&pool)
        .await
        .unwrap();

        let mut selected_ids = ids.clone();
        selected_ids.push(ids[20].clone());
        selected_ids.push(other_platform_id.clone());
        selected_ids.push("missing".to_string());
        let memberships = RoutePoolRepository::pool_membership_map(&pool, "codex", &selected_ids)
            .await
            .unwrap();

        assert_eq!(
            memberships,
            std::collections::HashSet::from([ids[20].clone()])
        );
    }

    #[tokio::test]
    async fn pool_membership_map_returns_empty_for_empty_input() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();

        let memberships = RoutePoolRepository::pool_membership_map(&pool, "codex", &[])
            .await
            .unwrap();

        assert!(memberships.is_empty());
    }

    #[tokio::test]
    async fn request_usage_event_persists_breakdown_and_converts_cny_cost() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let account_id = create_credential(&pool, "codex", "Usage account").await;
        let usage = RouteUsageBreakdown {
            input_tokens: Some(120),
            output_tokens: Some(30),
            cache_tokens: Some(80),
            price_usd_micros: None,
            price_cny_micros: Some(7_100_000),
            price_currency: Some("cny".to_string()),
            price_source: Some("upstream".to_string()),
        };

        RoutePoolRepository::insert_request_event(
            &pool,
            &account_id,
            "route_proxy",
            r#"{"path":"/chat/completions","status":200}"#,
            &usage,
            None,
        )
        .await
        .unwrap();

        let stats = RoutePoolRepository::stats(&pool, "codex", None, 1, 20)
            .await
            .unwrap();
        assert_eq!(stats.input_token_count, 120);
        assert_eq!(stats.output_token_count, 30);
        assert_eq!(stats.cache_token_count, 80);
        assert_eq!(stats.token_count, 150);
        assert_eq!(stats.cost_micros, 1_000_000);
        assert_eq!(stats.requests.len(), 1);
        assert_eq!(stats.requests[0].input_tokens, Some(120));
        assert_eq!(stats.requests[0].output_tokens, Some(30));
        assert_eq!(stats.requests[0].cache_tokens, Some(80));
        assert_eq!(stats.requests[0].price_cny_micros, Some(7_100_000));
        assert_eq!(stats.requests[0].price_currency.as_deref(), Some("cny"));
        assert_eq!(stats.requests[0].price_source.as_deref(), Some("upstream"));
    }

    #[tokio::test]
    async fn request_event_persists_the_upstream_response_id() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let account_id = create_credential(&pool, "claude", "ClaudeOne").await;

        RoutePoolRepository::insert_request_event(
            &pool,
            &account_id,
            "route_proxy",
            r#"{"platform":"claude","success":true}"#,
            &RouteUsageBreakdown::default(),
            Some("msg_abc123"),
        )
        .await
        .unwrap();

        let stats = RoutePoolRepository::stats(&pool, "claude", None, 1, 20)
            .await
            .unwrap();
        assert_eq!(
            stats.requests[0].upstream_response_id.as_deref(),
            Some("msg_abc123")
        );
    }

    #[tokio::test]
    async fn request_event_without_a_response_id_stores_null() {
        // A transport failure never produced a response, so there is no id to
        // record. It must read as unknown rather than as an empty-string key,
        // which would collide with every other id-less row during merging.
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let account_id = create_credential(&pool, "claude", "ClaudeOne").await;

        RoutePoolRepository::insert_request_event(
            &pool,
            &account_id,
            "route_proxy",
            r#"{"platform":"claude","success":false}"#,
            &RouteUsageBreakdown::default(),
            None,
        )
        .await
        .unwrap();

        let stats = RoutePoolRepository::stats(&pool, "claude", None, 1, 20)
            .await
            .unwrap();
        assert_eq!(stats.requests[0].upstream_response_id, None);
    }

    #[tokio::test]
    async fn list_request_events_spans_platforms_and_includes_archived_accounts() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let claude_id = create_credential(&pool, "claude", "ClaudeOne").await;
        let codex_id = create_credential(&pool, "codex", "CodexOne").await;

        for (account_id, response_id) in [(&claude_id, "msg_a"), (&codex_id, "resp_b")] {
            RoutePoolRepository::insert_request_event(
                &pool,
                account_id,
                "route_proxy",
                r#"{"success":true}"#,
                &RouteUsageBreakdown::default(),
                Some(response_id),
            )
            .await
            .unwrap();
        }

        // Archiving an account hides it from the account list; it must not erase
        // the spend it already incurred.
        RouteCredentialRepository::set_archived(&pool, std::slice::from_ref(&claude_id), true)
            .await
            .unwrap();

        let rows = RoutePoolRepository::list_request_events(&pool, None)
            .await
            .unwrap();

        assert_eq!(rows.len(), 2, "both platforms, archived included");
        let platforms: HashSet<&str> = rows.iter().map(|row| row.platform.as_str()).collect();
        assert!(platforms.contains("claude") && platforms.contains("codex"));
    }

    #[tokio::test]
    async fn list_request_events_keeps_history_after_the_account_is_deleted() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let account_id = create_credential(&pool, "codex", "CodexOne").await;

        RoutePoolRepository::insert_request_event(
            &pool,
            &account_id,
            "route_proxy",
            r#"{"success":true,"platform":"codex","route_credential_name":"CodexOne"}"#,
            &RouteUsageBreakdown::default(),
            Some("resp_kept"),
        )
        .await
        .unwrap();

        // Deleting an account drops only its own row: usage_events has no
        // cascade, so the spend is still on the books and still has to be
        // reported. Losing it would silently understate the total.
        RouteCredentialRepository::delete(&pool, &account_id)
            .await
            .unwrap();

        let rows = RoutePoolRepository::list_request_events(&pool, None)
            .await
            .unwrap();

        assert_eq!(rows.len(), 1);
        // Labels come from the metadata the request was written with.
        assert_eq!(rows[0].platform, "codex");
        assert_eq!(rows[0].account_name.as_deref(), Some("CodexOne"));
    }

    #[tokio::test]
    async fn list_request_events_filters_by_since_and_excludes_non_request_metrics() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let account_id = create_credential(&pool, "codex", "CodexOne").await;

        for (metric_type, amount, unit, created_at) in [
            ("request", 1_i64, "count", "2026-08-01T00:00:00Z"),
            ("request", 1_i64, "count", "2026-08-20T00:00:00Z"),
            // A legacy token row must not become a phantom request.
            ("token", 4096_i64, "token", "2026-08-20T00:00:00Z"),
        ] {
            sqlx::query(
                "INSERT INTO usage_events
                 (id, route_credential_id, source_label, metric_type, amount, unit, metadata_json, created_at)
                 VALUES (?, ?, 'route_proxy', ?, ?, ?, '{}', ?)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(&account_id)
            .bind(metric_type)
            .bind(amount)
            .bind(unit)
            .bind(created_at)
            .execute(&pool)
            .await
            .unwrap();
        }

        let rows = RoutePoolRepository::list_request_events(&pool, Some("2026-08-10T00:00:00Z"))
            .await
            .unwrap();

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].created_at, "2026-08-20T00:00:00Z");
    }

    #[tokio::test]
    async fn estimated_and_upstream_prices_both_count_toward_the_total() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let account_id = create_credential(&pool, "codex", "Mixed price account").await;

        for (price_usd_micros, price_source) in [
            (Some(2_000_000_i64), "upstream"),
            (Some(3_000_000_i64), "estimated"),
        ] {
            RoutePoolRepository::insert_request_event(
                &pool,
                &account_id,
                "route_proxy",
                r#"{"status":200}"#,
                &RouteUsageBreakdown {
                    input_tokens: Some(10),
                    output_tokens: Some(5),
                    cache_tokens: None,
                    price_usd_micros,
                    price_cny_micros: None,
                    price_currency: Some("usd".to_string()),
                    price_source: Some(price_source.to_string()),
                },
                None,
            )
            .await
            .unwrap();
        }

        let stats = RoutePoolRepository::stats(&pool, "codex", None, 1, 20)
            .await
            .unwrap();

        // A locally estimated amount is still an amount: the summary must not
        // silently drop it, or the total reads as 0 for every upstream that
        // returns tokens without a price.
        assert_eq!(stats.cost_micros, 5_000_000);
        assert_eq!(stats.request_count, 2);
        let sources: Vec<_> = stats
            .requests
            .iter()
            .filter_map(|request| request.price_source.as_deref())
            .collect();
        assert!(sources.contains(&"upstream"));
        assert!(sources.contains(&"estimated"));
    }

    #[tokio::test]
    async fn append_members_tx_appends_after_max_without_consuming_duplicate_positions() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let existing = create_credential(&pool, "codex", "Existing").await;
        let high = create_credential(&pool, "codex", "High").await;
        let first = create_credential(&pool, "codex", "First append").await;
        let second = create_credential(&pool, "codex", "Second append").await;
        RoutePoolRepository::replace_members(&pool, "codex", &[existing.clone(), high.clone()])
            .await
            .unwrap();
        sqlx::query(
            "UPDATE route_pool_members SET sort_order = 7 WHERE platform = ? AND route_credential_id = ?",
        )
        .bind("codex")
        .bind(&high)
        .execute(&pool)
        .await
        .unwrap();

        let mut tx = pool.begin().await.unwrap();
        let inserted = RoutePoolRepository::append_members_tx(
            &mut tx,
            "codex",
            &[
                existing.clone(),
                first.clone(),
                first.clone(),
                second.clone(),
            ],
        )
        .await
        .unwrap();
        assert_eq!(inserted, 2);
        tx.commit().await.unwrap();

        let rows = sqlx::query_as::<_, (String, i64)>(
            "SELECT route_credential_id, sort_order FROM route_pool_members WHERE platform = ? ORDER BY sort_order, route_credential_id",
        )
        .bind("codex")
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            rows,
            vec![(existing, 0), (high, 7), (first, 8), (second, 9)]
        );
    }

    #[tokio::test]
    async fn append_members_tx_obeys_caller_rollback() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let existing = create_credential(&pool, "claude", "Existing").await;
        let appended = create_credential(&pool, "claude", "Appended").await;
        RoutePoolRepository::replace_members(&pool, "claude", &[existing.clone()])
            .await
            .unwrap();

        let mut tx = pool.begin().await.unwrap();
        assert_eq!(
            RoutePoolRepository::append_members_tx(
                &mut tx,
                "claude",
                std::slice::from_ref(&appended),
            )
            .await
            .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM route_pool_members WHERE platform = ? AND route_credential_id = ?",
            )
            .bind("claude")
            .bind(&appended)
            .fetch_one(&mut *tx)
            .await
            .unwrap(),
            1
        );
        tx.rollback().await.unwrap();

        let members = RoutePoolRepository::list_member_ids(&pool, "claude")
            .await
            .unwrap();
        assert_eq!(members, vec![existing]);
    }
    #[tokio::test]
    async fn group_moves_are_additive_atomic_and_preserve_every_account_membership() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let first = create_credential(&pool, "codex", "first").await;
        let second = create_credential(&pool, "codex", "second").await;
        let foreign = create_credential(&pool, "claude", "foreign").await;
        RoutePoolRepository::move_group_members(
            &pool,
            "codex",
            "codex-default",
            std::slice::from_ref(&first),
        )
        .await
        .unwrap();
        RoutePoolRepository::move_group_members(
            &pool,
            "codex",
            "codex-default",
            std::slice::from_ref(&second),
        )
        .await
        .unwrap();
        assert_eq!(
            RoutePoolRepository::list_member_ids(&pool, "codex")
                .await
                .unwrap(),
            vec![first.clone(), second.clone()]
        );
        assert!(RoutePoolRepository::move_group_members(
            &pool,
            "codex",
            "codex-archived",
            &[first.clone(), foreign]
        )
        .await
        .is_err());
        assert_eq!(
            RoutePoolRepository::group_id_for_account(&pool, "codex", &first)
                .await
                .unwrap()
                .as_deref(),
            Some("codex-default")
        );
        RoutePoolRepository::replace_members(&pool, "codex", std::slice::from_ref(&second))
            .await
            .unwrap();
        assert_eq!(
            RoutePoolRepository::group_id_for_account(&pool, "codex", &first)
                .await
                .unwrap()
                .as_deref(),
            Some("codex-out")
        );
        assert!(
            RoutePoolRepository::delete_group(&pool, "codex", "codex-default")
                .await
                .is_err()
        );
    }
}

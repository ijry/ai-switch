use crate::database::repositories::route_credential_repository::{
    database_error, semantic_failure_fingerprint, truncate_failure_message,
    truncate_failure_response,
};
use crate::error::AppError;
use crate::models::route_credential_model::{
    RouteCredentialModelState, MODEL_STATUS_ERROR, MODEL_STATUS_OK, MODEL_STATUS_PAUSED,
};
use chrono::Utc;
use sqlx::{QueryBuilder, Sqlite, SqliteConnection, SqlitePool};
use std::collections::HashMap;

const STATE_SELECT: &str = "SELECT route_credential_id, model_key, status,
    transient_failure_count, cooldown_until, semantic_failure_streak_count,
    semantic_failure_streak_fingerprint, last_failure_kind, last_failure_message,
    last_failure_response_json, created_at, updated_at
 FROM route_credential_models";

pub struct RouteCredentialModelRepository;

impl RouteCredentialModelRepository {
    /// Batch-load exactly the `(account, model)` pairs a request needs. Two
    /// accounts can map the same requested model to different upstream names, so
    /// the key is the pair — never the model alone.
    pub async fn load_states(
        pool: &SqlitePool,
        keys: &[(String, String)],
    ) -> Result<HashMap<(String, String), RouteCredentialModelState>, AppError> {
        if keys.is_empty() {
            return Ok(HashMap::new());
        }
        let mut builder = QueryBuilder::<Sqlite>::new(STATE_SELECT);
        builder.push(" WHERE (route_credential_id, model_key) IN (");
        let mut separated = builder.separated(", ");
        for (credential_id, model_key) in keys {
            separated.push("(");
            separated.push_bind_unseparated(credential_id);
            separated.push_unseparated(", ");
            separated.push_bind_unseparated(model_key);
            separated.push_unseparated(")");
        }
        builder.push(")");
        let states = builder
            .build_query_as::<RouteCredentialModelState>()
            .fetch_all(pool)
            .await
            .map_err(|err| {
                database_error(
                    "database.route_credential_model_states",
                    "Could not load per-model failure state",
                    err,
                )
            })?;
        Ok(states
            .into_iter()
            .map(|state| {
                (
                    (state.route_credential_id.clone(), state.model_key.clone()),
                    state,
                )
            })
            .collect())
    }

    pub async fn list_for_credentials(
        pool: &SqlitePool,
        credential_ids: &[String],
    ) -> Result<Vec<RouteCredentialModelState>, AppError> {
        if credential_ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut builder = QueryBuilder::<Sqlite>::new(STATE_SELECT);
        builder.push(" WHERE route_credential_id IN (");
        let mut separated = builder.separated(", ");
        for credential_id in credential_ids {
            separated.push_bind(credential_id);
        }
        builder.push(") ORDER BY route_credential_id, model_key");
        builder
            .build_query_as::<RouteCredentialModelState>()
            .fetch_all(pool)
            .await
            .map_err(|err| {
                database_error(
                    "database.route_credential_model_list",
                    "Could not list per-model failure state",
                    err,
                )
            })
    }

    /// Record one model-scoped failure. Unlike the account-level pair of
    /// functions, the cooldown window and the semantic streak accumulate
    /// together — keeping them mutually exclusive would mean a cooling model
    /// could never reach `semantic_error_threshold`.
    ///
    /// `cooldown_seconds` is `None` when the account has cooldown switched off:
    /// the failure is still counted, the model just stays selectable.
    #[allow(clippy::too_many_arguments)]
    pub async fn record_transient_failure(
        conn: &mut SqliteConnection,
        credential_id: &str,
        model_key: &str,
        kind: &str,
        message: &str,
        response_body: Option<&[u8]>,
        cooldown_seconds: Option<u32>,
        response_status: Option<u16>,
        semantic_error_threshold: i64,
        error_status_enabled: bool,
    ) -> Result<(), AppError> {
        let now = Utc::now();
        let now_text = now.to_rfc3339();
        let cooldown_until = cooldown_seconds
            .map(|seconds| (now + chrono::Duration::seconds(i64::from(seconds))).to_rfc3339());
        let fingerprint = semantic_failure_fingerprint(response_status, message);
        let threshold = semantic_error_threshold.max(1);
        let message = truncate_failure_message(message);
        let response = truncate_failure_response(response_body);

        sqlx::query(
            "INSERT INTO route_credential_models
                 (route_credential_id, model_key, status, transient_failure_count,
                  cooldown_until, semantic_failure_streak_count,
                  semantic_failure_streak_fingerprint, last_failure_kind,
                  last_failure_message, last_failure_response_json, created_at, updated_at)
             VALUES (?, ?, ?, 1, ?, 1, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(route_credential_id, model_key) DO UPDATE SET
                 transient_failure_count = transient_failure_count + 1,
                 cooldown_until = excluded.cooldown_until,
                 semantic_failure_streak_count = CASE
                     WHEN semantic_failure_streak_fingerprint = excluded.semantic_failure_streak_fingerprint
                         THEN MIN(semantic_failure_streak_count + 1, ?)
                     ELSE 1
                 END,
                 semantic_failure_streak_fingerprint = excluded.semantic_failure_streak_fingerprint,
                 status = CASE
                     WHEN status = ? THEN status
                     WHEN NOT ? THEN status
                     WHEN CASE
                         WHEN semantic_failure_streak_fingerprint = excluded.semantic_failure_streak_fingerprint
                             THEN MIN(semantic_failure_streak_count + 1, ?)
                         ELSE 1
                     END >= ? THEN ?
                     ELSE status
                 END,
                 last_failure_kind = excluded.last_failure_kind,
                 last_failure_message = excluded.last_failure_message,
                 last_failure_response_json = excluded.last_failure_response_json,
                 updated_at = excluded.updated_at",
        )
        .bind(credential_id)
        .bind(model_key)
        .bind(if error_status_enabled && threshold <= 1 {
            MODEL_STATUS_ERROR
        } else {
            MODEL_STATUS_OK
        })
        .bind(cooldown_until.as_deref())
        .bind(&fingerprint)
        .bind(kind)
        .bind(&message)
        .bind(&response)
        .bind(&now_text)
        .bind(&now_text)
        .bind(threshold)
        .bind(MODEL_STATUS_PAUSED)
        .bind(error_status_enabled)
        .bind(threshold)
        .bind(threshold)
        .bind(MODEL_STATUS_ERROR)
        .execute(conn)
        .await
        .map_err(|err| {
            database_error(
                "database.route_credential_model_failure",
                "Could not record per-model failure",
                err,
            )
        })?;
        Ok(())
    }

    /// A success proves this model works. Delete the row — unless the user
    /// paused it, in which case keep the status and only reset the failure
    /// bookkeeping.
    pub async fn clear(
        pool: &SqlitePool,
        credential_id: &str,
        model_key: &str,
    ) -> Result<(), AppError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE route_credential_models
             SET transient_failure_count = 0, cooldown_until = NULL,
                 semantic_failure_streak_count = 0,
                 semantic_failure_streak_fingerprint = NULL,
                 last_failure_kind = NULL, last_failure_message = NULL,
                 last_failure_response_json = NULL, updated_at = ?
             WHERE route_credential_id = ? AND model_key = ? AND status = ?",
        )
        .bind(&now)
        .bind(credential_id)
        .bind(model_key)
        .bind(MODEL_STATUS_PAUSED)
        .execute(pool)
        .await
        .map_err(|err| {
            database_error(
                "database.route_credential_model_clear",
                "Could not reset paused per-model state",
                err,
            )
        })?;

        sqlx::query(
            "DELETE FROM route_credential_models
             WHERE route_credential_id = ? AND model_key = ? AND status != ?",
        )
        .bind(credential_id)
        .bind(model_key)
        .bind(MODEL_STATUS_PAUSED)
        .execute(pool)
        .await
        .map_err(|err| {
            database_error(
                "database.route_credential_model_clear",
                "Could not clear per-model state",
                err,
            )
        })?;
        Ok(())
    }

    /// Account-level reactivation (scheduled recovery, explicit account test)
    /// wipes automatic model state but leaves paused models paused.
    pub async fn clear_all_unpaused(
        conn: &mut SqliteConnection,
        credential_id: &str,
    ) -> Result<(), AppError> {
        sqlx::query(
            "DELETE FROM route_credential_models
             WHERE route_credential_id = ? AND status != ?",
        )
        .bind(credential_id)
        .bind(MODEL_STATUS_PAUSED)
        .execute(conn)
        .await
        .map_err(|err| {
            database_error(
                "database.route_credential_model_clear_all",
                "Could not clear per-model state for account",
                err,
            )
        })?;
        Ok(())
    }

    /// Only `ok` and `paused` are valid inputs — `error` is reached exclusively
    /// through a semantic failure streak. Creates the row when the model has
    /// never failed, since a healthy model has no row to update.
    pub async fn set_status(
        pool: &SqlitePool,
        credential_id: &str,
        model_key: &str,
        status: &str,
    ) -> Result<(), AppError> {
        if status == MODEL_STATUS_OK {
            return Self::clear_status(pool, credential_id, model_key).await;
        }
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO route_credential_models
                 (route_credential_id, model_key, status, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(route_credential_id, model_key) DO UPDATE SET
                 status = excluded.status, updated_at = excluded.updated_at",
        )
        .bind(credential_id)
        .bind(model_key)
        .bind(status)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| {
            database_error(
                "database.route_credential_model_status",
                "Could not set per-model status",
                err,
            )
        })?;
        Ok(())
    }

    async fn clear_status(
        pool: &SqlitePool,
        credential_id: &str,
        model_key: &str,
    ) -> Result<(), AppError> {
        sqlx::query(
            "DELETE FROM route_credential_models
             WHERE route_credential_id = ? AND model_key = ?",
        )
        .bind(credential_id)
        .bind(model_key)
        .execute(pool)
        .await
        .map_err(|err| {
            database_error(
                "database.route_credential_model_status",
                "Could not clear per-model status",
                err,
            )
        })?;
        Ok(())
    }

    /// Models this account cannot serve right now: still cooling, flipped to
    /// `error`, or paused by the user.
    pub async fn unavailable_keys(
        conn: &mut SqliteConnection,
        credential_id: &str,
        now_rfc3339: &str,
    ) -> Result<Vec<String>, AppError> {
        sqlx::query_scalar::<_, String>(
            "SELECT model_key FROM route_credential_models
             WHERE route_credential_id = ?
               AND (status != ? OR (cooldown_until IS NOT NULL AND cooldown_until > ?))",
        )
        .bind(credential_id)
        .bind(MODEL_STATUS_OK)
        .bind(now_rfc3339)
        .fetch_all(conn)
        .await
        .map_err(|err| {
            database_error(
                "database.route_credential_model_unavailable",
                "Could not read unavailable models",
                err,
            )
        })
    }

    /// Models the user explicitly paused. Excluded from the escalation
    /// denominator so a human decision cannot masquerade as an outage.
    pub async fn paused_keys(
        conn: &mut SqliteConnection,
        credential_id: &str,
    ) -> Result<Vec<String>, AppError> {
        sqlx::query_scalar::<_, String>(
            "SELECT model_key FROM route_credential_models
             WHERE route_credential_id = ? AND status = ?",
        )
        .bind(credential_id)
        .bind(MODEL_STATUS_PAUSED)
        .fetch_all(conn)
        .await
        .map_err(|err| {
            database_error(
                "database.route_credential_model_paused",
                "Could not read paused models",
                err,
            )
        })
    }

    /// The model a healthcheck probe should target: the stalest row that is not
    /// paused and whose cooldown has already expired. Probing a paused model
    /// would fight the user's decision.
    pub async fn oldest_recoverable_key(
        pool: &SqlitePool,
        credential_id: &str,
        now_rfc3339: &str,
    ) -> Result<Option<String>, AppError> {
        sqlx::query_scalar::<_, String>(
            "SELECT model_key FROM route_credential_models
             WHERE route_credential_id = ?
               AND status != ?
               AND (cooldown_until IS NULL OR cooldown_until <= ?)
             ORDER BY updated_at ASC, model_key ASC
             LIMIT 1",
        )
        .bind(credential_id)
        .bind(MODEL_STATUS_PAUSED)
        .bind(now_rfc3339)
        .fetch_optional(pool)
        .await
        .map_err(|err| {
            database_error(
                "database.route_credential_model_probe",
                "Could not pick a probe model",
                err,
            )
        })
    }

    /// Whether the recovery scheduler should consider this account even though
    /// its account-level columns look healthy.
    pub async fn has_unpaused_rows(
        pool: &SqlitePool,
        credential_id: &str,
    ) -> Result<bool, AppError> {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM route_credential_models
             WHERE route_credential_id = ? AND status != ?",
        )
        .bind(credential_id)
        .bind(MODEL_STATUS_PAUSED)
        .fetch_one(pool)
        .await
        .map_err(|err| {
            database_error(
                "database.route_credential_model_has_rows",
                "Could not count per-model rows",
                err,
            )
        })?;
        Ok(count > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{create_memory_pool, run_migrations};

    async fn seed(pool: &SqlitePool) -> String {
        sqlx::query(
            "INSERT INTO route_credentials
             (id, platform, kind, display_name, status, sort_order, secret_payload_json,
              config_json, preview_json, created_at, updated_at)
             VALUES ('cred-1', 'codex', 'api', 'Fixture', 'ok', 0, '{}', '{}', '{}',
                     '2026-09-02T00:00:00Z', '2026-09-02T00:00:00Z')",
        )
        .execute(pool)
        .await
        .expect("seed credential");
        "cred-1".to_string()
    }

    #[tokio::test]
    async fn transient_failure_writes_a_cooldown_and_success_deletes_the_row() {
        let pool = create_memory_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let id = seed(&pool).await;

        let mut conn = pool.acquire().await.expect("conn");
        RouteCredentialModelRepository::record_transient_failure(
            &mut conn,
            &id,
            "upstream-sol",
            "upstream_status",
            "upstream returned 429",
            None,
            Some(30),
            Some(429),
            10,
            true,
        )
        .await
        .expect("record");
        drop(conn);

        let states = RouteCredentialModelRepository::list_for_credentials(&pool, &[id.clone()])
            .await
            .expect("list");
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].model_key, "upstream-sol");
        assert_eq!(states[0].status, MODEL_STATUS_OK);
        assert_eq!(states[0].transient_failure_count, 1);
        assert!(states[0].cooldown_until.is_some());
        assert_eq!(
            states[0].last_failure_kind.as_deref(),
            Some("upstream_status")
        );

        RouteCredentialModelRepository::clear(&pool, &id, "upstream-sol")
            .await
            .expect("clear");
        assert!(
            RouteCredentialModelRepository::list_for_credentials(&pool, &[id])
                .await
                .expect("list after clear")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn disabled_cooldown_counts_without_parking_the_model() {
        let pool = create_memory_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let id = seed(&pool).await;

        let mut conn = pool.acquire().await.expect("conn");
        RouteCredentialModelRepository::record_transient_failure(
            &mut conn,
            &id,
            "upstream-sol",
            "upstream_status",
            "boom",
            None,
            None,
            Some(500),
            10,
            true,
        )
        .await
        .expect("record");
        drop(conn);

        let states = RouteCredentialModelRepository::list_for_credentials(&pool, &[id])
            .await
            .expect("list");
        assert_eq!(states[0].transient_failure_count, 1);
        assert!(states[0].cooldown_until.is_none());
    }

    #[tokio::test]
    async fn clear_keeps_a_paused_row_but_resets_its_failure_fields() {
        let pool = create_memory_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let id = seed(&pool).await;

        RouteCredentialModelRepository::set_status(&pool, &id, "upstream-sol", MODEL_STATUS_PAUSED)
            .await
            .expect("pause");
        let mut conn = pool.acquire().await.expect("conn");
        RouteCredentialModelRepository::record_transient_failure(
            &mut conn,
            &id,
            "upstream-sol",
            "upstream_status",
            "boom",
            None,
            Some(30),
            Some(429),
            10,
            true,
        )
        .await
        .expect("record");
        drop(conn);

        RouteCredentialModelRepository::clear(&pool, &id, "upstream-sol")
            .await
            .expect("clear");
        let states = RouteCredentialModelRepository::list_for_credentials(&pool, &[id])
            .await
            .expect("list");
        // A success must not silently un-pause what the user paused.
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].status, MODEL_STATUS_PAUSED);
        assert_eq!(states[0].transient_failure_count, 0);
        assert!(states[0].cooldown_until.is_none());
    }

    #[tokio::test]
    async fn repeated_semantic_failures_flip_the_model_to_error_at_the_threshold() {
        let pool = create_memory_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let id = seed(&pool).await;

        for expected in 1..=3 {
            let mut conn = pool.acquire().await.expect("conn");
            RouteCredentialModelRepository::record_transient_failure(
                &mut conn,
                &id,
                "upstream-sol",
                "semantic_response_transient",
                "content blocked",
                None,
                Some(30),
                Some(200),
                3,
                true,
            )
            .await
            .expect("record");
            drop(conn);
            let states = RouteCredentialModelRepository::list_for_credentials(&pool, &[id.clone()])
                .await
                .expect("list");
            assert_eq!(states[0].semantic_failure_streak_count, expected);
            // Cooldown and streak accumulate together: unlike the account-level
            // pair of functions these are not mutually exclusive, otherwise a
            // cooling model could never reach the threshold.
            assert!(states[0].cooldown_until.is_some());
            let expected_status = if expected >= 3 {
                MODEL_STATUS_ERROR
            } else {
                MODEL_STATUS_OK
            };
            assert_eq!(states[0].status, expected_status);
        }
    }

    #[tokio::test]
    async fn a_different_failure_fingerprint_restarts_the_streak() {
        let pool = create_memory_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let id = seed(&pool).await;

        for message in ["first reason", "first reason", "second reason"] {
            let mut conn = pool.acquire().await.expect("conn");
            RouteCredentialModelRepository::record_transient_failure(
                &mut conn,
                &id,
                "upstream-sol",
                "semantic_response_transient",
                message,
                None,
                Some(30),
                Some(200),
                3,
                true,
            )
            .await
            .expect("record");
            drop(conn);
        }
        let states = RouteCredentialModelRepository::list_for_credentials(&pool, &[id])
            .await
            .expect("list");
        assert_eq!(states[0].semantic_failure_streak_count, 1);
        assert_eq!(states[0].status, MODEL_STATUS_OK);
    }

    #[tokio::test]
    async fn error_status_toggle_off_keeps_counting_without_flipping_status() {
        let pool = create_memory_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let id = seed(&pool).await;

        for _ in 0..5 {
            let mut conn = pool.acquire().await.expect("conn");
            RouteCredentialModelRepository::record_transient_failure(
                &mut conn,
                &id,
                "upstream-sol",
                "semantic_response_transient",
                "blocked",
                None,
                Some(30),
                Some(200),
                2,
                false,
            )
            .await
            .expect("record");
            drop(conn);
        }
        let states = RouteCredentialModelRepository::list_for_credentials(&pool, &[id])
            .await
            .expect("list");
        assert_eq!(states[0].status, MODEL_STATUS_OK);
        assert!(states[0].semantic_failure_streak_count >= 2);
    }

    #[tokio::test]
    async fn unavailable_keys_reports_cooling_error_and_paused_models() {
        let pool = create_memory_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let id = seed(&pool).await;

        let mut conn = pool.acquire().await.expect("conn");
        RouteCredentialModelRepository::record_transient_failure(
            &mut conn,
            &id,
            "cooling",
            "upstream_status",
            "boom",
            None,
            Some(600),
            Some(429),
            10,
            true,
        )
        .await
        .expect("cooling");
        RouteCredentialModelRepository::record_transient_failure(
            &mut conn,
            &id,
            "expired",
            "upstream_status",
            "boom",
            None,
            None,
            Some(429),
            10,
            true,
        )
        .await
        .expect("expired");
        drop(conn);
        RouteCredentialModelRepository::set_status(&pool, &id, "paused", MODEL_STATUS_PAUSED)
            .await
            .expect("pause");

        let mut conn = pool.acquire().await.expect("conn");
        let mut keys = RouteCredentialModelRepository::unavailable_keys(
            &mut conn,
            &id,
            "2026-09-02T00:00:00Z",
        )
        .await
        .expect("unavailable");
        drop(conn);
        keys.sort();
        // "expired" has no cooldown timestamp at all, so it stays selectable.
        assert_eq!(keys, vec!["cooling", "paused"]);
    }

    #[tokio::test]
    async fn paused_keys_reports_only_what_the_user_paused() {
        let pool = create_memory_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let id = seed(&pool).await;

        let mut conn = pool.acquire().await.expect("conn");
        RouteCredentialModelRepository::record_transient_failure(
            &mut conn,
            &id,
            "cooling",
            "upstream_status",
            "boom",
            None,
            Some(600),
            Some(429),
            10,
            true,
        )
        .await
        .expect("cooling");
        RouteCredentialModelRepository::record_transient_failure(
            &mut conn,
            &id,
            "broken",
            "semantic_response_transient",
            "blocked",
            None,
            Some(600),
            Some(200),
            1,
            true,
        )
        .await
        .expect("broken");
        drop(conn);
        RouteCredentialModelRepository::set_status(&pool, &id, "held", MODEL_STATUS_PAUSED)
            .await
            .expect("pause");

        let mut conn = pool.acquire().await.expect("conn");
        let keys = RouteCredentialModelRepository::paused_keys(&mut conn, &id)
            .await
            .expect("paused");
        drop(conn);
        // A cooling or errored model is unavailable but not a human decision, so
        // it must stay in the escalation denominator.
        assert_eq!(keys, vec!["held"]);
    }

    #[tokio::test]
    async fn clear_all_unpaused_keeps_paused_rows() {
        let pool = create_memory_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let id = seed(&pool).await;

        let mut conn = pool.acquire().await.expect("conn");
        RouteCredentialModelRepository::record_transient_failure(
            &mut conn,
            &id,
            "cooling",
            "upstream_status",
            "boom",
            None,
            Some(600),
            Some(429),
            10,
            true,
        )
        .await
        .expect("cooling");
        drop(conn);
        RouteCredentialModelRepository::set_status(&pool, &id, "held", MODEL_STATUS_PAUSED)
            .await
            .expect("pause");

        let mut conn = pool.acquire().await.expect("conn");
        RouteCredentialModelRepository::clear_all_unpaused(&mut conn, &id)
            .await
            .expect("clear all");
        drop(conn);

        let states = RouteCredentialModelRepository::list_for_credentials(&pool, &[id])
            .await
            .expect("list");
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].model_key, "held");
    }

    #[tokio::test]
    async fn load_states_only_returns_the_requested_pairs() {
        let pool = create_memory_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let id = seed(&pool).await;

        let mut conn = pool.acquire().await.expect("conn");
        for key in ["a", "b"] {
            RouteCredentialModelRepository::record_transient_failure(
                &mut conn,
                &id,
                key,
                "upstream_status",
                "boom",
                None,
                Some(30),
                Some(429),
                10,
                true,
            )
            .await
            .expect("record");
        }
        drop(conn);

        let states = RouteCredentialModelRepository::load_states(
            &pool,
            &[
                (id.clone(), "a".to_string()),
                (id.clone(), "missing".to_string()),
            ],
        )
        .await
        .expect("load");
        assert_eq!(states.len(), 1);
        assert!(states.contains_key(&(id, "a".to_string())));
    }

    #[tokio::test]
    async fn oldest_recoverable_key_prefers_the_stalest_expired_model() {
        let pool = create_memory_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let id = seed(&pool).await;

        for (key, updated_at) in [
            ("stale", "2026-09-01T00:00:00Z"),
            ("fresh", "2026-09-02T00:00:00Z"),
        ] {
            sqlx::query(
                "INSERT INTO route_credential_models
                 (route_credential_id, model_key, status, transient_failure_count,
                  cooldown_until, created_at, updated_at)
                 VALUES (?, ?, 'ok', 1, NULL, ?, ?)",
            )
            .bind(&id)
            .bind(key)
            .bind(updated_at)
            .bind(updated_at)
            .execute(&pool)
            .await
            .expect("seed model row");
        }
        sqlx::query(
            "INSERT INTO route_credential_models
             (route_credential_id, model_key, status, created_at, updated_at)
             VALUES (?, 'held', 'paused', '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z')",
        )
        .bind(&id)
        .execute(&pool)
        .await
        .expect("seed paused");

        let key = RouteCredentialModelRepository::oldest_recoverable_key(
            &pool,
            &id,
            "2026-09-03T00:00:00Z",
        )
        .await
        .expect("oldest");
        // A paused row is older still, but probing it would fight the user.
        assert_eq!(key.as_deref(), Some("stale"));
    }

    #[tokio::test]
    async fn has_unpaused_rows_ignores_paused_only_accounts() {
        let pool = create_memory_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let id = seed(&pool).await;

        RouteCredentialModelRepository::set_status(&pool, &id, "held", MODEL_STATUS_PAUSED)
            .await
            .expect("pause");
        assert!(
            !RouteCredentialModelRepository::has_unpaused_rows(&pool, &id)
                .await
                .expect("paused only")
        );

        let mut conn = pool.acquire().await.expect("conn");
        RouteCredentialModelRepository::record_transient_failure(
            &mut conn,
            &id,
            "cooling",
            "upstream_status",
            "boom",
            None,
            Some(600),
            Some(429),
            10,
            true,
        )
        .await
        .expect("cooling");
        drop(conn);
        assert!(
            RouteCredentialModelRepository::has_unpaused_rows(&pool, &id)
                .await
                .expect("with cooling")
        );
    }
}

use crate::error::AppError;
use chrono::Utc;
use sqlx::{Sqlite, SqlitePool, Transaction};
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, PartialEq, Eq)]
pub struct TransferOrigin {
    pub route_credential_id: String,
    pub source_instance_id: String,
    pub source_credential_id: String,
    pub source_platform: String,
    pub source_kind: String,
    pub source_schema_version: i64,
    pub source_fingerprint: String,
    pub imported_at: String,
}

fn database_error(code: &'static str, message: &str, error: impl ToString) -> AppError {
    AppError::Database {
        code,
        message: message.to_string(),
        details: Some(error.to_string()),
        recoverable: true,
    }
}

pub async fn get_or_create_installation_id(pool: &SqlitePool) -> Result<String, AppError> {
    let candidate = Uuid::new_v4().to_string();
    let created_at = Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO transfer_installation_identity (singleton, instance_id, created_at)
         VALUES (1, ?, ?)
         ON CONFLICT(singleton) DO NOTHING",
    )
    .bind(candidate)
    .bind(created_at)
    .execute(pool)
    .await
    .map_err(|error| {
        database_error(
            "database.route_credential_transfer_installation_insert",
            "Could not initialize transfer installation identity",
            error,
        )
    })?;

    sqlx::query_scalar::<_, String>(
        "SELECT instance_id FROM transfer_installation_identity WHERE singleton = 1",
    )
    .fetch_one(pool)
    .await
    .map_err(|error| {
        database_error(
            "database.route_credential_transfer_installation_get",
            "Could not load transfer installation identity",
            error,
        )
    })
}

pub async fn find_origin_by_identity(
    pool: &SqlitePool,
    source_instance_id: &str,
    source_credential_id: &str,
    platform: &str,
    kind: &str,
) -> Result<Option<TransferOrigin>, AppError> {
    sqlx::query_as::<_, TransferOrigin>(
        "SELECT route_credential_id, source_instance_id, source_credential_id,
                source_platform, source_kind, source_schema_version,
                source_fingerprint, imported_at
         FROM route_credential_transfer_origins
         WHERE source_instance_id = ?
           AND source_credential_id = ?
           AND source_platform = ?
           AND source_kind = ?",
    )
    .bind(source_instance_id)
    .bind(source_credential_id)
    .bind(platform)
    .bind(kind)
    .fetch_optional(pool)
    .await
    .map_err(|error| {
        database_error(
            "database.route_credential_transfer_origin_find",
            "Could not load route credential transfer origin",
            error,
        )
    })
}

pub async fn find_origin_by_identity_tx(
    tx: &mut Transaction<'_, Sqlite>,
    source_instance_id: &str,
    source_credential_id: &str,
    platform: &str,
    kind: &str,
) -> Result<Option<TransferOrigin>, AppError> {
    sqlx::query_as::<_, TransferOrigin>(
        "SELECT route_credential_id, source_instance_id, source_credential_id,
                source_platform, source_kind, source_schema_version,
                source_fingerprint, imported_at
         FROM route_credential_transfer_origins
         WHERE source_instance_id = ?
           AND source_credential_id = ?
           AND source_platform = ?
           AND source_kind = ?",
    )
    .bind(source_instance_id)
    .bind(source_credential_id)
    .bind(platform)
    .bind(kind)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| {
        database_error(
            "database.route_credential_transfer_origin_find_tx",
            "Could not load route credential transfer origin in transaction",
            error,
        )
    })
}

pub async fn insert_origin_tx(
    tx: &mut Transaction<'_, Sqlite>,
    origin: &TransferOrigin,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO route_credential_transfer_origins (
            route_credential_id, source_instance_id, source_credential_id,
            source_platform, source_kind, source_schema_version,
            source_fingerprint, imported_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&origin.route_credential_id)
    .bind(&origin.source_instance_id)
    .bind(&origin.source_credential_id)
    .bind(&origin.source_platform)
    .bind(&origin.source_kind)
    .bind(origin.source_schema_version)
    .bind(&origin.source_fingerprint)
    .bind(&origin.imported_at)
    .execute(&mut **tx)
    .await
    .map_err(|error| {
        database_error(
            "database.route_credential_transfer_origin_insert",
            "Could not save route credential transfer origin",
            error,
        )
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{create_memory_pool, run_migrations};
    use sqlx::Row;

    async fn insert_route_credential(pool: &SqlitePool, id: &str) {
        sqlx::query(
            "INSERT INTO route_credentials (
                id, platform, kind, display_name, status, sort_order,
                secret_payload_json, config_json, preview_json, created_at, updated_at
             ) VALUES (?, 'codex', 'api', ?, 'ok', 0, '{}', '{}', '{}', ?, ?)",
        )
        .bind(id)
        .bind(format!("Credential {id}"))
        .bind("2026-08-04T00:00:00Z")
        .bind("2026-08-04T00:00:00Z")
        .execute(pool)
        .await
        .expect("insert route credential");
    }

    fn origin(route_credential_id: &str) -> TransferOrigin {
        TransferOrigin {
            route_credential_id: route_credential_id.to_string(),
            source_instance_id: "source-instance".to_string(),
            source_credential_id: "source-credential".to_string(),
            source_platform: "codex".to_string(),
            source_kind: "api".to_string(),
            source_schema_version: 1,
            source_fingerprint: "sha256:source".to_string(),
            imported_at: "2026-08-04T00:00:00Z".to_string(),
        }
    }

    #[tokio::test]
    async fn route_credential_transfer_migration_creates_both_tables() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        for table in [
            "transfer_installation_identity",
            "route_credential_transfer_origins",
        ] {
            let count = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?",
            )
            .bind(table)
            .fetch_one(&pool)
            .await
            .expect("table lookup");
            assert_eq!(count, 1, "missing table {table}");
        }
    }

    #[tokio::test]
    async fn route_credential_transfer_installation_identity_is_stable() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let first = get_or_create_installation_id(&pool)
            .await
            .expect("first installation id");
        let second = get_or_create_installation_id(&pool)
            .await
            .expect("second installation id");

        assert_eq!(first, second);
        assert_eq!(Uuid::parse_str(&first).expect("UUID").to_string(), first);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM transfer_installation_identity")
                .fetch_one(&pool)
                .await
                .expect("identity count"),
            1
        );
    }

    #[tokio::test]
    async fn route_credential_transfer_origin_identity_is_unique_and_findable() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        insert_route_credential(&pool, "local-1").await;
        insert_route_credential(&pool, "local-2").await;

        let first = origin("local-1");
        let mut tx = pool.begin().await.expect("first transaction");
        insert_origin_tx(&mut tx, &first)
            .await
            .expect("insert first origin");
        assert_eq!(
            find_origin_by_identity_tx(
                &mut tx,
                "source-instance",
                "source-credential",
                "codex",
                "api",
            )
            .await
            .expect("find origin in transaction"),
            Some(first.clone())
        );
        tx.commit().await.expect("commit first origin");

        assert_eq!(
            find_origin_by_identity(
                &pool,
                "source-instance",
                "source-credential",
                "codex",
                "api",
            )
            .await
            .expect("find origin"),
            Some(first)
        );

        let mut duplicate_tx = pool.begin().await.expect("duplicate transaction");
        let duplicate_error = insert_origin_tx(&mut duplicate_tx, &origin("local-2"))
            .await
            .expect_err("duplicate identity must fail");
        duplicate_tx.rollback().await.expect("rollback duplicate");
        assert!(matches!(
            duplicate_error,
            AppError::Database {
                code: "database.route_credential_transfer_origin_insert",
                ..
            }
        ));
    }

    #[tokio::test]
    async fn route_credential_transfer_origin_cascades_with_credential_delete() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        insert_route_credential(&pool, "local-1").await;

        let mut tx = pool.begin().await.expect("transaction");
        insert_origin_tx(&mut tx, &origin("local-1"))
            .await
            .expect("insert origin");
        tx.commit().await.expect("commit origin");

        sqlx::query("DELETE FROM route_credentials WHERE id = 'local-1'")
            .execute(&pool)
            .await
            .expect("delete credential");

        let row = sqlx::query("SELECT COUNT(*) AS count FROM route_credential_transfer_origins")
            .fetch_one(&pool)
            .await
            .expect("origin count");
        assert_eq!(row.get::<i64, _>("count"), 0);
    }
}

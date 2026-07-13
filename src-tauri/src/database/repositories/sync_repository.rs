use crate::error::AppError;
use crate::models::sync::{NewSyncProfile, NewSyncSnapshot, SyncProfile, SyncSnapshot};
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct SyncRepository;

impl SyncRepository {
    pub async fn create_profile(
        pool: &SqlitePool,
        input: NewSyncProfile,
    ) -> Result<SyncProfile, AppError> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let enabled = i64::from(input.enabled);

        sqlx::query(
            "INSERT INTO sync_profiles (id, name, provider, endpoint_url, auth_ref, scope_json, enabled, notes, status, sort_order, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'configured', 0, ?, ?)",
        )
        .bind(&id)
        .bind(&input.name)
        .bind(&input.provider)
        .bind(&input.endpoint_url)
        .bind(&input.auth_ref)
        .bind(&input.scope_json)
        .bind(enabled)
        .bind(&input.notes)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.sync_profile_create",
            message: "Could not create sync profile".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Self::get_profile(pool, &id).await
    }

    pub async fn get_profile(pool: &SqlitePool, id: &str) -> Result<SyncProfile, AppError> {
        sqlx::query_as::<_, SyncProfile>("SELECT * FROM sync_profiles WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.sync_profile_get",
                message: "Could not load sync profile".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }

    pub async fn list_profiles(pool: &SqlitePool) -> Result<Vec<SyncProfile>, AppError> {
        sqlx::query_as::<_, SyncProfile>(
            "SELECT * FROM sync_profiles ORDER BY sort_order ASC, created_at DESC",
        )
        .fetch_all(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.sync_profile_list",
            message: "Could not list sync profiles".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })
    }

    pub async fn create_snapshot(
        pool: &SqlitePool,
        input: NewSyncSnapshot,
    ) -> Result<SyncSnapshot, AppError> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO sync_snapshots (id, profile_id, direction, status, item_counts_json, manifest_json, artifact_ref, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&input.profile_id)
        .bind(&input.direction)
        .bind(&input.status)
        .bind(&input.item_counts_json)
        .bind(&input.manifest_json)
        .bind(&input.artifact_ref)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.sync_snapshot_create",
            message: "Could not create sync snapshot".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Self::get_snapshot(pool, &id).await
    }

    pub async fn get_snapshot(pool: &SqlitePool, id: &str) -> Result<SyncSnapshot, AppError> {
        sqlx::query_as::<_, SyncSnapshot>("SELECT * FROM sync_snapshots WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.sync_snapshot_get",
                message: "Could not load sync snapshot".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }

    pub async fn list_snapshots(pool: &SqlitePool) -> Result<Vec<SyncSnapshot>, AppError> {
        sqlx::query_as::<_, SyncSnapshot>("SELECT * FROM sync_snapshots ORDER BY created_at DESC")
            .fetch_all(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.sync_snapshot_list",
                message: "Could not list sync snapshots".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{create_memory_pool, run_migrations};

    #[tokio::test]
    async fn creates_profiles_and_snapshots() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let profile = SyncRepository::create_profile(
            &pool,
            NewSyncProfile {
                name: "Team WebDAV".to_string(),
                provider: "webdav".to_string(),
                endpoint_url: Some("https://sync.example.com/ai-switch".to_string()),
                auth_ref: Some("env://WEBDAV_TOKEN".to_string()),
                scope_json: "{\"providers\":true}".to_string(),
                enabled: true,
                notes: None,
            },
        )
        .await
        .expect("profile");

        SyncRepository::create_snapshot(
            &pool,
            NewSyncSnapshot {
                profile_id: Some(profile.id),
                direction: "export".to_string(),
                status: "recorded".to_string(),
                item_counts_json: "{\"providers\":0}".to_string(),
                manifest_json: "{\"schema\":\"ai-switch.sync.snapshot.v1\"}".to_string(),
                artifact_ref: None,
            },
        )
        .await
        .expect("snapshot");

        assert_eq!(
            SyncRepository::list_profiles(&pool)
                .await
                .expect("profiles")
                .len(),
            1
        );
        assert_eq!(
            SyncRepository::list_snapshots(&pool)
                .await
                .expect("snapshots")
                .len(),
            1
        );
    }
}

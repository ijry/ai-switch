use crate::error::AppError;
use crate::models::updater::{NewUpdateChannel, NewUpdateCheck, UpdateChannel, UpdateCheck};
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct UpdaterRepository;

impl UpdaterRepository {
    pub async fn create_channel(
        pool: &SqlitePool,
        input: NewUpdateChannel,
    ) -> Result<UpdateChannel, AppError> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let enabled = i64::from(input.enabled);

        sqlx::query(
            "INSERT INTO update_channels (id, name, channel, feed_url, enabled, notes, status, sort_order, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, 'configured', 0, ?, ?)",
        )
        .bind(&id)
        .bind(&input.name)
        .bind(&input.channel)
        .bind(&input.feed_url)
        .bind(enabled)
        .bind(&input.notes)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.update_channel_create",
            message: "Could not create update channel".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Self::get_channel(pool, &id).await
    }

    pub async fn get_channel(pool: &SqlitePool, id: &str) -> Result<UpdateChannel, AppError> {
        sqlx::query_as::<_, UpdateChannel>("SELECT * FROM update_channels WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.update_channel_get",
                message: "Could not load update channel".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }

    pub async fn list_channels(pool: &SqlitePool) -> Result<Vec<UpdateChannel>, AppError> {
        sqlx::query_as::<_, UpdateChannel>(
            "SELECT * FROM update_channels ORDER BY sort_order ASC, created_at DESC",
        )
        .fetch_all(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.update_channel_list",
            message: "Could not list update channels".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })
    }

    pub async fn create_check(
        pool: &SqlitePool,
        input: NewUpdateCheck,
    ) -> Result<UpdateCheck, AppError> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO update_checks (id, channel_id, current_version, latest_version, status, release_notes_url, details_json, checked_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&input.channel_id)
        .bind(&input.current_version)
        .bind(&input.latest_version)
        .bind(&input.status)
        .bind(&input.release_notes_url)
        .bind(&input.details_json)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.update_check_create",
            message: "Could not create update check".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Self::get_check(pool, &id).await
    }

    pub async fn get_check(pool: &SqlitePool, id: &str) -> Result<UpdateCheck, AppError> {
        sqlx::query_as::<_, UpdateCheck>("SELECT * FROM update_checks WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.update_check_get",
                message: "Could not load update check".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }

    pub async fn list_checks(pool: &SqlitePool) -> Result<Vec<UpdateCheck>, AppError> {
        sqlx::query_as::<_, UpdateCheck>("SELECT * FROM update_checks ORDER BY checked_at DESC")
            .fetch_all(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.update_check_list",
                message: "Could not list update checks".to_string(),
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
    async fn creates_update_channels_and_checks() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let channel = UpdaterRepository::create_channel(
            &pool,
            NewUpdateChannel {
                name: "Stable".to_string(),
                channel: "stable".to_string(),
                feed_url: Some("https://updates.example.com/stable.json".to_string()),
                enabled: true,
                notes: None,
            },
        )
        .await
        .expect("channel");

        UpdaterRepository::create_check(
            &pool,
            NewUpdateCheck {
                channel_id: Some(channel.id),
                current_version: "0.1.0".to_string(),
                latest_version: Some("0.1.1".to_string()),
                status: "available".to_string(),
                release_notes_url: Some("https://updates.example.com/releases/0.1.1".to_string()),
                details_json: "{}".to_string(),
            },
        )
        .await
        .expect("check");

        assert_eq!(
            UpdaterRepository::list_channels(&pool)
                .await
                .expect("channels")
                .len(),
            1
        );
        assert_eq!(
            UpdaterRepository::list_checks(&pool)
                .await
                .expect("checks")
                .len(),
            1
        );
    }
}

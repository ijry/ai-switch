use crate::error::AppError;
use crate::models::session::{NewSessionEvent, NewSessionRecord, SessionEvent, SessionRecord};
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct SessionRepository;

impl SessionRepository {
    pub async fn create_session(
        pool: &SqlitePool,
        input: NewSessionRecord,
    ) -> Result<SessionRecord, AppError> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO sessions (id, title, target_app_id, provider_id, official_account_id, prompt_asset_id, mcp_server_ids_json, tags_json, status, notes, sort_order, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?)",
        )
        .bind(&id)
        .bind(&input.title)
        .bind(&input.target_app_id)
        .bind(&input.provider_id)
        .bind(&input.official_account_id)
        .bind(&input.prompt_asset_id)
        .bind(&input.mcp_server_ids_json)
        .bind(&input.tags_json)
        .bind(&input.status)
        .bind(&input.notes)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.session_create",
            message: "Could not create session".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Self::get_session(pool, &id).await
    }

    pub async fn get_session(pool: &SqlitePool, id: &str) -> Result<SessionRecord, AppError> {
        sqlx::query_as::<_, SessionRecord>("SELECT * FROM sessions WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.session_get",
                message: "Could not load session".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }

    pub async fn list_sessions(pool: &SqlitePool) -> Result<Vec<SessionRecord>, AppError> {
        sqlx::query_as::<_, SessionRecord>(
            "SELECT * FROM sessions ORDER BY sort_order ASC, created_at DESC",
        )
        .fetch_all(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.session_list",
            message: "Could not list sessions".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })
    }

    pub async fn set_session_status(
        pool: &SqlitePool,
        id: &str,
        status: &str,
    ) -> Result<SessionRecord, AppError> {
        let now = Utc::now().to_rfc3339();

        sqlx::query("UPDATE sessions SET status = ?, updated_at = ? WHERE id = ?")
            .bind(status)
            .bind(&now)
            .bind(id)
            .execute(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.session_status",
                message: "Could not update session status".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?;

        Self::get_session(pool, id).await
    }

    pub async fn create_event(
        pool: &SqlitePool,
        input: NewSessionEvent,
    ) -> Result<SessionEvent, AppError> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO session_events (id, session_id, event_type, message, metadata_json, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&input.session_id)
        .bind(&input.event_type)
        .bind(&input.message)
        .bind(&input.metadata_json)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.session_event_create",
            message: "Could not create session event".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Self::get_event(pool, &id).await
    }

    pub async fn get_event(pool: &SqlitePool, id: &str) -> Result<SessionEvent, AppError> {
        sqlx::query_as::<_, SessionEvent>("SELECT * FROM session_events WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.session_event_get",
                message: "Could not load session event".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }

    pub async fn list_events(
        pool: &SqlitePool,
        session_id: Option<&str>,
    ) -> Result<Vec<SessionEvent>, AppError> {
        let query = if let Some(session_id) = session_id {
            sqlx::query_as::<_, SessionEvent>(
                "SELECT * FROM session_events WHERE session_id = ? ORDER BY created_at DESC",
            )
            .bind(session_id)
        } else {
            sqlx::query_as::<_, SessionEvent>(
                "SELECT * FROM session_events ORDER BY created_at DESC",
            )
        };

        query
            .fetch_all(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.session_event_list",
                message: "Could not list session events".to_string(),
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
    async fn creates_sessions_and_events() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let session = SessionRepository::create_session(
            &pool,
            NewSessionRecord {
                title: "Release review".to_string(),
                target_app_id: None,
                provider_id: None,
                official_account_id: None,
                prompt_asset_id: None,
                mcp_server_ids_json: "[]".to_string(),
                tags_json: "[\"review\"]".to_string(),
                status: "draft".to_string(),
                notes: Some("Prepare release notes".to_string()),
            },
        )
        .await
        .expect("session");

        let active = SessionRepository::set_session_status(&pool, &session.id, "active")
            .await
            .expect("active");
        assert_eq!(active.status, "active");

        SessionRepository::create_event(
            &pool,
            NewSessionEvent {
                session_id: session.id.clone(),
                event_type: "note".to_string(),
                message: "Started review".to_string(),
                metadata_json: "{}".to_string(),
            },
        )
        .await
        .expect("event");

        assert_eq!(
            SessionRepository::list_sessions(&pool)
                .await
                .expect("sessions")
                .len(),
            1
        );
        assert_eq!(
            SessionRepository::list_events(&pool, Some(&session.id))
                .await
                .expect("events")
                .len(),
            1
        );
    }
}

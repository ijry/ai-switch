use super::models::*;
use crate::error::AppError;
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct ImageGenerationRepository;

impl ImageGenerationRepository {
    pub async fn create_session(
        pool: &SqlitePool,
        input: CreateImageSessionInput,
    ) -> Result<ImageSession, AppError> {
        let title = input.title.trim();
        if title.is_empty() || !matches!(input.platform.as_str(), "codex" | "gemini") {
            return Err(validation(
                "validation.image_session",
                "A title and supported platform are required",
            ));
        }
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO imagegen_sessions (id,title,platform,archived,created_at,updated_at) VALUES (?,?,?,0,?,?)")
            .bind(&id).bind(title).bind(&input.platform).bind(&now).bind(&now)
            .execute(pool).await.map_err(database)?;
        Self::get_session(pool, &id).await
    }

    pub async fn list_sessions(
        pool: &SqlitePool,
        include_archived: bool,
    ) -> Result<Vec<ImageSession>, AppError> {
        let sql = if include_archived {
            "SELECT id,title,platform,archived,created_at,updated_at FROM imagegen_sessions ORDER BY updated_at DESC"
        } else {
            "SELECT id,title,platform,archived,created_at,updated_at FROM imagegen_sessions WHERE archived=0 ORDER BY updated_at DESC"
        };
        sqlx::query_as(sql).fetch_all(pool).await.map_err(database)
    }

    pub async fn get_session(pool: &SqlitePool, id: &str) -> Result<ImageSession, AppError> {
        sqlx::query_as("SELECT id,title,platform,archived,created_at,updated_at FROM imagegen_sessions WHERE id=?")
            .bind(id).fetch_optional(pool).await.map_err(database)?
            .ok_or_else(|| validation("imagegen.session_not_found", "Image session was not found"))
    }

    pub async fn update_session(
        pool: &SqlitePool,
        input: UpdateImageSessionInput,
    ) -> Result<ImageSession, AppError> {
        let current = Self::get_session(pool, &input.id).await?;
        let title = input
            .title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(&current.title);
        let archived = input.archived.unwrap_or(current.archived);
        sqlx::query("UPDATE imagegen_sessions SET title=?, archived=?, updated_at=? WHERE id=?")
            .bind(title)
            .bind(archived)
            .bind(Utc::now().to_rfc3339())
            .bind(&input.id)
            .execute(pool)
            .await
            .map_err(database)?;
        Self::get_session(pool, &input.id).await
    }

    pub async fn delete_session(pool: &SqlitePool, id: &str) -> Result<(), AppError> {
        let result = sqlx::query("DELETE FROM imagegen_sessions WHERE id=?")
            .bind(id)
            .execute(pool)
            .await
            .map_err(database)?;
        if result.rows_affected() == 0 {
            return Err(validation(
                "imagegen.session_not_found",
                "Image session was not found",
            ));
        }
        Ok(())
    }

    pub async fn conversation(pool: &SqlitePool, id: &str) -> Result<ImageConversation, AppError> {
        let session = Self::get_session(pool, id).await?;
        let messages = sqlx::query_as("SELECT id,session_id,role,prompt,request_json,status,model,upstream_model,member_id,upstream_response_id,error_message,image_count,cost_micros,created_at,updated_at FROM imagegen_messages WHERE session_id=? ORDER BY created_at ASC")
            .bind(id).fetch_all(pool).await.map_err(database)?;
        let assets = sqlx::query_as("SELECT id,session_id,message_id,relative_path,mime_type,sha256,width,height,byte_size,created_at FROM imagegen_assets WHERE session_id=? ORDER BY created_at ASC")
            .bind(id).fetch_all(pool).await.map_err(database)?;
        Ok(ImageConversation {
            session,
            messages,
            assets,
        })
    }

    pub async fn add_message(
        pool: &SqlitePool,
        input: NewImageMessage,
    ) -> Result<ImageMessage, AppError> {
        Self::get_session(pool, &input.session_id).await?;
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO imagegen_messages (id,session_id,role,prompt,request_json,status,model,created_at,updated_at) VALUES (?,?,?,?,?,?,?,?,?)")
            .bind(&id).bind(&input.session_id).bind(&input.role).bind(&input.prompt).bind(&input.request_json).bind(&input.status).bind(&input.model).bind(&now).bind(&now)
            .execute(pool).await.map_err(database)?;
        sqlx::query("UPDATE imagegen_sessions SET updated_at=? WHERE id=?")
            .bind(&now)
            .bind(&input.session_id)
            .execute(pool)
            .await
            .map_err(database)?;
        Self::get_message(pool, &id).await
    }

    pub async fn get_message(pool: &SqlitePool, id: &str) -> Result<ImageMessage, AppError> {
        sqlx::query_as("SELECT id,session_id,role,prompt,request_json,status,model,upstream_model,member_id,upstream_response_id,error_message,image_count,cost_micros,created_at,updated_at FROM imagegen_messages WHERE id=?")
            .bind(id).fetch_optional(pool).await.map_err(database)?
            .ok_or_else(|| validation("imagegen.message_not_found", "Image message was not found"))
    }

    pub async fn finish_message(
        pool: &SqlitePool,
        id: &str,
        status: &str,
        upstream_model: Option<&str>,
        member_id: Option<&str>,
        response_id: Option<&str>,
        error: Option<&str>,
        image_count: i64,
    ) -> Result<ImageMessage, AppError> {
        sqlx::query("UPDATE imagegen_messages SET status=?,upstream_model=?,member_id=?,upstream_response_id=?,error_message=?,image_count=?,updated_at=? WHERE id=?")
            .bind(status).bind(upstream_model).bind(member_id).bind(response_id).bind(error).bind(image_count).bind(Utc::now().to_rfc3339()).bind(id)
            .execute(pool).await.map_err(database)?;
        Self::get_message(pool, id).await
    }

    pub async fn add_asset(
        pool: &SqlitePool,
        input: NewImageAsset,
    ) -> Result<ImageAsset, AppError> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO imagegen_assets (id,session_id,message_id,relative_path,mime_type,sha256,width,height,byte_size,created_at) VALUES (?,?,?,?,?,?,?,?,?,?)")
            .bind(&id).bind(&input.session_id).bind(&input.message_id).bind(&input.relative_path).bind(&input.mime_type).bind(&input.sha256).bind(input.width).bind(input.height).bind(input.byte_size).bind(&now)
            .execute(pool).await.map_err(database)?;
        Self::get_asset(pool, &id).await
    }

    pub async fn get_asset(pool: &SqlitePool, id: &str) -> Result<ImageAsset, AppError> {
        sqlx::query_as("SELECT id,session_id,message_id,relative_path,mime_type,sha256,width,height,byte_size,created_at FROM imagegen_assets WHERE id=?")
            .bind(id).fetch_optional(pool).await.map_err(database)?
            .ok_or_else(|| validation("imagegen.asset_not_found", "Image asset was not found"))
    }
}

fn validation(code: &'static str, message: &str) -> AppError {
    AppError::Validation {
        code,
        message: message.to_string(),
        details: None,
        recoverable: true,
    }
}

fn database(error: sqlx::Error) -> AppError {
    AppError::Database {
        code: "database.imagegen",
        message: "Could not access image generation data".to_string(),
        details: Some(error.to_string()),
        recoverable: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{create_memory_pool, run_migrations};

    #[tokio::test]
    async fn sessions_messages_assets_and_cascade_delete() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let session = ImageGenerationRepository::create_session(
            &pool,
            CreateImageSessionInput {
                title: "Poster".into(),
                platform: "codex".into(),
            },
        )
        .await
        .expect("session");
        let message = ImageGenerationRepository::add_message(
            &pool,
            NewImageMessage {
                session_id: session.id.clone(),
                role: "user".into(),
                prompt: "A fox".into(),
                request_json: "{}".into(),
                status: "pending".into(),
                model: Some("gpt-image".into()),
            },
        )
        .await
        .expect("message");
        ImageGenerationRepository::add_asset(
            &pool,
            NewImageAsset {
                session_id: session.id.clone(),
                message_id: message.id,
                relative_path: "a.png".into(),
                mime_type: "image/png".into(),
                sha256: "hash".into(),
                width: Some(1),
                height: Some(1),
                byte_size: 4,
            },
        )
        .await
        .expect("asset");
        assert_eq!(
            ImageGenerationRepository::conversation(&pool, &session.id)
                .await
                .expect("conversation")
                .assets
                .len(),
            1
        );
        ImageGenerationRepository::delete_session(&pool, &session.id)
            .await
            .expect("delete");
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM imagegen_assets")
                .fetch_one(&pool)
                .await
                .expect("count"),
            0
        );
    }
}

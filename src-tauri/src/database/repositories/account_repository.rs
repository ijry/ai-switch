use crate::error::AppError;
use crate::models::account::{NewOfficialAccount, OfficialAccount, UpdateOfficialAccount};
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct AccountRepository;

impl AccountRepository {
    pub async fn create(
        pool: &SqlitePool,
        input: NewOfficialAccount,
    ) -> Result<OfficialAccount, AppError> {
        let now = Utc::now().to_rfc3339();
        let id = Uuid::new_v4().to_string();

        sqlx::query(
            "INSERT INTO official_accounts (id, platform, display_name, email, plan, account_metadata_json, secret_ref, status, sort_order, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, 'ok', 0, ?, ?)"
        )
        .bind(&id)
        .bind(&input.platform)
        .bind(&input.display_name)
        .bind(&input.email)
        .bind(&input.plan)
        .bind(&input.account_metadata_json)
        .bind(&input.secret_ref)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.account_create",
            message: "Could not create official account".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Self::get(pool, &id).await
    }

    pub async fn get(pool: &SqlitePool, id: &str) -> Result<OfficialAccount, AppError> {
        sqlx::query_as::<_, OfficialAccount>("SELECT * FROM official_accounts WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.account_get",
                message: "Could not load official account".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }

    pub async fn update(
        pool: &SqlitePool,
        id: &str,
        input: UpdateOfficialAccount,
    ) -> Result<OfficialAccount, AppError> {
        let now = Utc::now().to_rfc3339();

        let result = sqlx::query(
            "UPDATE official_accounts
             SET display_name = ?, email = ?, plan = ?, account_metadata_json = ?, secret_ref = ?, status = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(&input.display_name)
        .bind(&input.email)
        .bind(&input.plan)
        .bind(&input.account_metadata_json)
        .bind(&input.secret_ref)
        .bind(&input.status)
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.account_update",
            message: "Could not update official account".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        if result.rows_affected() == 0 {
            return Err(AppError::Validation {
                code: "validation.account_not_found",
                message: "Official account does not exist".to_string(),
                details: Some(id.to_string()),
                recoverable: true,
            });
        }

        Self::get(pool, id).await
    }
}

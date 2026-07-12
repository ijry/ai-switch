use crate::database::repositories::target_repository::TargetRepository;
use crate::error::AppError;
use crate::models::target_app::TargetApp;
use sqlx::SqlitePool;

pub struct TargetService;

impl TargetService {
    pub async fn list_targets(pool: &SqlitePool) -> Result<Vec<TargetApp>, AppError> {
        TargetRepository::ensure_defaults(pool).await
    }
}

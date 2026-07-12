use crate::paths::AppPaths;
use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct AppState {
    pub paths: AppPaths,
    pub pool: SqlitePool,
}

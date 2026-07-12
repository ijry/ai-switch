use crate::error::AppError;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::path::Path;
use std::str::FromStr;

pub mod repositories;

#[cfg(test)]
mod test_support;

pub async fn create_pool(database_file: &Path) -> Result<SqlitePool, AppError> {
    if let Some(parent) = database_file.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let url = format!("sqlite://{}", database_file.display());
    let options = SqliteConnectOptions::from_str(&url)
        .map_err(|err| AppError::Database {
            code: "database.connect_options",
            message: "Could not create SQLite connection options".to_string(),
            details: Some(err.to_string()),
            recoverable: false,
        })?
        .create_if_missing(true)
        .foreign_keys(true);

    SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .map_err(|err| AppError::Database {
            code: "database.connect",
            message: "Could not connect to SQLite database".to_string(),
            details: Some(err.to_string()),
            recoverable: false,
        })
}

#[cfg(test)]
pub async fn create_memory_pool() -> Result<SqlitePool, AppError> {
    let options = SqliteConnectOptions::from_str("sqlite::memory:")
        .map_err(|err| AppError::Database {
            code: "database.connect_options",
            message: "Could not create in-memory SQLite options".to_string(),
            details: Some(err.to_string()),
            recoverable: false,
        })?
        .foreign_keys(true);

    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|err| AppError::Database {
            code: "database.connect",
            message: "Could not connect to in-memory SQLite database".to_string(),
            details: Some(err.to_string()),
            recoverable: false,
        })
}

pub async fn run_migrations(pool: &SqlitePool) -> Result<(), AppError> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.migration",
            message: "Could not apply SQLite migrations".to_string(),
            details: Some(err.to_string()),
            recoverable: false,
        })
}

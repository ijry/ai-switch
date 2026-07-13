use crate::error::AppError;
use chrono::Utc;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
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
        .map_err(|err| {
            let details = err.to_string();
            let recoverable = is_migration_conflict_message(&details);
            AppError::Database {
                code: "database.migration",
                message: "Could not apply SQLite migrations".to_string(),
                details: Some(details),
                recoverable,
            }
        })
}

/// Open the app database and apply migrations.
///
/// If an older local DB has a modified migration checksum (common during active
/// development), quarantine the conflicting files under backups/ and recreate a
/// fresh database so the app can still start.
pub async fn open_migrated_pool(
    database_file: &Path,
    backups_dir: &Path,
) -> Result<SqlitePool, AppError> {
    let pool = create_pool(database_file).await?;
    match run_migrations(&pool).await {
        Ok(()) => Ok(pool),
        Err(err) if is_recoverable_migration_conflict(&err) => {
            pool.close().await;
            quarantine_database_files(database_file, backups_dir).await?;
            let pool = create_pool(database_file).await?;
            run_migrations(&pool).await?;
            Ok(pool)
        }
        Err(err) => Err(err),
    }
}

fn is_recoverable_migration_conflict(err: &AppError) -> bool {
    match err {
        AppError::Database {
            code,
            details,
            recoverable,
            ..
        } if *code == "database.migration" => {
            *recoverable
                || details
                    .as_deref()
                    .is_some_and(is_migration_conflict_message)
        }
        _ => false,
    }
}

fn is_migration_conflict_message(details: &str) -> bool {
    let lower = details.to_ascii_lowercase();
    lower.contains("was previously applied but has been modified")
        || lower.contains("versionmismatch")
        || lower.contains("migration version") && lower.contains("mismatch")
        || lower.contains("checksum") && lower.contains("migration")
}

async fn quarantine_database_files(
    database_file: &Path,
    backups_dir: &Path,
) -> Result<(), AppError> {
    tokio::fs::create_dir_all(backups_dir).await?;

    let stamp = Utc::now().format("%Y%m%d-%H%M%S");
    let base_name = database_file
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("ai-switch.db");

    for path in database_sidecar_paths(database_file) {
        if !tokio::fs::try_exists(&path).await.unwrap_or(false) {
            continue;
        }

        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(base_name);
        let backup_name = format!("{file_name}.migration-conflict-{stamp}");
        let backup_path = backups_dir.join(backup_name);
        tokio::fs::rename(&path, &backup_path)
            .await
            .map_err(|err| AppError::Filesystem {
                code: "filesystem.migration_quarantine",
                message: "Could not quarantine the conflicting database file".to_string(),
                details: Some(format!(
                    "{} -> {}: {err}",
                    path.display(),
                    backup_path.display()
                )),
                recoverable: false,
            })?;
    }

    let note_path = backups_dir.join(format!("{base_name}.migration-conflict-{stamp}.txt"));
    let note = format!(
        "AI Switch quarantined a local database because SQLite migrations no longer matched.\n\
         Original database: {}\n\
         Timestamp: {}\n\
         Action: moved conflicting db files into backups and created a fresh database on next open.\n",
        database_file.display(),
        stamp
    );
    tokio::fs::write(&note_path, note).await?;
    Ok(())
}

fn database_sidecar_paths(database_file: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::with_capacity(3);
    paths.push(database_file.to_path_buf());
    paths.push(append_suffix(database_file, "-wal"));
    paths.push(append_suffix(database_file, "-shm"));
    paths
}

fn append_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut value = OsString::from(path.as_os_str());
    value.push(suffix);
    PathBuf::from(value)
}

#[cfg(test)]
mod recovery_tests {
    use super::{open_migrated_pool, run_migrations};
    use sqlx::Row;
    use tempfile::tempdir;

    #[tokio::test]
    async fn open_migrated_pool_recovers_from_modified_migration_checksum() {
        let dir = tempdir().expect("tempdir");
        let database_file = dir.path().join("ai-switch.db");
        let backups_dir = dir.path().join("backups");
        tokio::fs::create_dir_all(&backups_dir)
            .await
            .expect("backups dir");

        let pool = open_migrated_pool(&database_file, &backups_dir)
            .await
            .expect("initial open");
        sqlx::query("UPDATE _sqlx_migrations SET checksum = x'deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef' WHERE version = 202607130004")
            .execute(&pool)
            .await
            .expect("corrupt checksum");
        pool.close().await;

        let recovered = open_migrated_pool(&database_file, &backups_dir)
            .await
            .expect("recovered open");
        run_migrations(&recovered)
            .await
            .expect("migrations still apply after recovery");

        let row = sqlx::query(
            "SELECT COUNT(*) AS count FROM sqlite_master WHERE type = 'table' AND name = 'route_pool_members'",
        )
        .fetch_one(&recovered)
        .await
        .expect("table lookup");
        let count: i64 = row.get("count");
        assert_eq!(count, 1);

        let mut entries = tokio::fs::read_dir(&backups_dir)
            .await
            .expect("read backups");
        let mut backup_count = 0usize;
        while let Some(entry) = entries.next_entry().await.expect("backup entry") {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.contains("migration-conflict-") {
                backup_count += 1;
            }
        }
        assert!(
            backup_count >= 1,
            "expected quarantined database backup files"
        );
    }
}

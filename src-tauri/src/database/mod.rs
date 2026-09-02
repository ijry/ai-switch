use crate::error::AppError;
use chrono::Utc;
use sha2::{Digest, Sha384};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::str::FromStr;

pub mod repositories;

#[cfg(test)]
mod test_support;

/// Compiled-in migrations. Kept as one static so the embedded SQL is stored
/// once and the checksum repair below compares against exactly the set that
/// [`run_migrations`] applies.
static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Tables that only ever hold what the user created or accumulated. Seeded
/// tables such as `target_apps` are deliberately absent: they are filled on
/// every fresh start and would make an empty database look occupied.
const USER_DATA_TABLES: &[&str] = &[
    "route_credentials",
    "providers",
    "official_accounts",
    "route_proxy_keys",
    "mcp_servers",
    "batches",
    "prompt_assets",
    "sessions",
    "usage_events",
    "config_snapshots",
];

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
    MIGRATOR.run(pool).await.map_err(|err| {
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
/// Migration checksums are a SHA-384 over the raw bytes of each `.sql` file, so
/// anything that rewrites those bytes without changing a single statement — a
/// CRLF-to-LF normalization, a reformat, a stray trailing newline — makes sqlx
/// report `VersionMismatch` for migrations that already ran. Left alone that
/// aborts startup; quarantining is what the app used to do, and it looks to the
/// user exactly like every account was deleted.
///
/// So before quarantining anything, try to repair: if a stored checksum differs
/// from the shipped file only by line endings, the applied SQL was byte-for-byte
/// equivalent and the ledger entry is simply stale. Rewrite it and carry on with
/// the user's data intact. Quarantine remains as the last resort for genuine
/// content changes, and only when the database has no user data to lose.
pub async fn open_migrated_pool(
    database_file: &Path,
    backups_dir: &Path,
) -> Result<SqlitePool, AppError> {
    let pool = create_pool(database_file).await?;
    match run_migrations(&pool).await {
        Ok(()) => restore_quarantined_database(pool, database_file, backups_dir).await,
        Err(err) if is_recoverable_migration_conflict(&err) => {
            let repaired = repair_line_ending_checksums(&pool).await?;
            if repaired > 0 {
                if let Ok(()) = run_migrations(&pool).await {
                    return restore_quarantined_database(pool, database_file, backups_dir).await;
                }
            }

            // Still mismatched: a migration's SQL really did change. Refuse to
            // throw away a populated database — a hard startup error the user
            // can report is recoverable, a silently emptied account list is not.
            if has_user_data(&pool).await? {
                return Err(preserve_instead_of_quarantine(database_file, &err));
            }

            pool.close().await;
            quarantine_database_files(database_file, backups_dir).await?;
            let pool = create_pool(database_file).await?;
            run_migrations(&pool).await?;
            restore_quarantined_database(pool, database_file, backups_dir).await
        }
        Err(err) => Err(err),
    }
}

/// Rewrite `_sqlx_migrations` checksums that match a shipped migration once its
/// line endings are normalized. Returns how many rows were corrected.
async fn repair_line_ending_checksums(pool: &SqlitePool) -> Result<usize, AppError> {
    let applied: Vec<(i64, Vec<u8>)> =
        sqlx::query_as("SELECT version, checksum FROM _sqlx_migrations")
            .fetch_all(pool)
            .await
            .map_err(|err| migration_repair_error("read the applied migration ledger", err))?;
    let applied: HashMap<i64, Vec<u8>> = applied.into_iter().collect();

    let mut repaired = 0usize;
    for migration in MIGRATOR.iter() {
        let Some(stored) = applied.get(&migration.version) else {
            continue;
        };
        if stored.as_slice() == migration.checksum.as_ref() {
            continue;
        }
        if !line_ending_variants_match(&migration.sql, stored) {
            continue;
        }

        sqlx::query("UPDATE _sqlx_migrations SET checksum = ? WHERE version = ?")
            .bind(migration.checksum.as_ref())
            .bind(migration.version)
            .execute(pool)
            .await
            .map_err(|err| migration_repair_error("rewrite a stale migration checksum", err))?;
        repaired += 1;
    }

    Ok(repaired)
}

/// True when `stored` is the SHA-384 of this migration's SQL under some line
/// ending convention, i.e. the two differ only in CR bytes.
fn line_ending_variants_match(sql: &str, stored: &[u8]) -> bool {
    let lf = sql.replace("\r\n", "\n");
    let crlf = lf.replace('\n', "\r\n");
    [lf, crlf]
        .iter()
        .any(|variant| Sha384::digest(variant.as_bytes()).as_slice() == stored)
}

async fn has_user_data(pool: &SqlitePool) -> Result<bool, AppError> {
    for table in USER_DATA_TABLES {
        // EXISTS stops at the first row, so this stays cheap even next to a
        // usage_events table with a million rows. A table missing from an older
        // schema simply holds no rows here.
        let present: Option<bool> =
            sqlx::query_scalar(&format!("SELECT EXISTS(SELECT 1 FROM \"{table}\")"))
                .fetch_one(pool)
                .await
                .ok();
        if present.unwrap_or(false) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Bring back a database that an earlier version quarantined.
///
/// 0.7.3 quarantined and replaced databases whose migration checksums differed
/// only by line endings, so upgraded installs started on an empty account list
/// while the real data sat in `backups/` untouched. Once the checksum repair
/// above can reconcile such a file, restoring it is the other half of the fix.
///
/// Deliberately conservative: this only runs when the live database holds no
/// user data at all, so a restore can never overwrite something the user
/// created after the quarantine. In that case there is nothing to weigh up —
/// an empty database is exactly what the bug produced.
async fn restore_quarantined_database(
    pool: SqlitePool,
    database_file: &Path,
    backups_dir: &Path,
) -> Result<SqlitePool, AppError> {
    if has_user_data(&pool).await? {
        return Ok(pool);
    }
    let Some(candidate) = newest_quarantined_database(database_file, backups_dir).await else {
        return Ok(pool);
    };

    // Migrate a scratch copy first: a quarantine file that cannot be brought up
    // to the current schema must not replace the working database.
    let staged = append_suffix(database_file, ".restore-candidate");
    let _ = remove_database_files(&staged).await;
    tokio::fs::copy(&candidate, &staged).await?;
    let restored = match prepare_restored_copy(&staged).await {
        Ok(true) => true,
        Ok(false) | Err(_) => false,
    };
    if !restored {
        let _ = remove_database_files(&staged).await;
        return Ok(pool);
    }

    pool.close().await;
    remove_database_files(database_file).await?;
    tokio::fs::rename(&staged, database_file).await?;
    // Keep the bytes, but take the file out of the candidate set so a later
    // start cannot restore it a second time over newer data.
    let _ = tokio::fs::rename(&candidate, append_suffix(&candidate, ".restored")).await;

    create_pool(database_file).await
}

/// Migrate a staged copy of a quarantined database. `Ok(false)` means the copy
/// is not usable as a replacement and should be discarded.
async fn prepare_restored_copy(staged: &Path) -> Result<bool, AppError> {
    let pool = create_pool(staged).await?;
    let outcome = async {
        if run_migrations(&pool).await.is_err() {
            repair_line_ending_checksums(&pool).await?;
            run_migrations(&pool).await?;
        }
        has_user_data(&pool).await
    }
    .await;
    pool.close().await;
    outcome
}

/// Newest `*.migration-conflict-*` copy of this database, ignoring the `-wal`
/// and `-shm` sidecars and anything already restored.
async fn newest_quarantined_database(database_file: &Path, backups_dir: &Path) -> Option<PathBuf> {
    let base_name = database_file.file_name()?.to_str()?;
    let prefix = format!("{base_name}.migration-conflict-");

    let mut entries = tokio::fs::read_dir(backups_dir).await.ok()?;
    let mut newest: Option<(String, PathBuf)> = None;
    while let Some(entry) = entries.next_entry().await.ok()? {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // The trailing timestamp sorts lexicographically, so the plain string
        // comparison below picks the most recent quarantine.
        let Some(stamp) = name.strip_prefix(&prefix) else {
            continue;
        };
        if !stamp.chars().all(|c| c.is_ascii_digit() || c == '-') {
            continue;
        }
        if newest
            .as_ref()
            .is_none_or(|(newest_stamp, _)| stamp > newest_stamp.as_str())
        {
            newest = Some((stamp.to_string(), entry.path()));
        }
    }

    newest.map(|(_, path)| path)
}

async fn remove_database_files(database_file: &Path) -> Result<(), AppError> {
    for path in database_sidecar_paths(database_file) {
        match tokio::fs::remove_file(&path).await {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.into()),
        }
    }
    Ok(())
}

fn migration_repair_error(action: &str, err: sqlx::Error) -> AppError {
    AppError::Database {
        code: "database.migration_repair",
        message: format!("Could not {action}"),
        details: Some(err.to_string()),
        recoverable: false,
    }
}

fn preserve_instead_of_quarantine(database_file: &Path, err: &AppError) -> AppError {
    let details = match err {
        AppError::Database { details, .. } => details.clone().unwrap_or_default(),
        _ => String::new(),
    };
    AppError::Database {
        code: "database.migration_conflict_preserved",
        message: "SQLite migrations no longer match this database, which still holds your data. \
                  It was left untouched instead of being replaced."
            .to_string(),
        details: Some(format!("{}: {details}", database_file.display())),
        recoverable: false,
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
mod migration_checksum_manifest_tests {
    use super::MIGRATOR;
    use std::collections::BTreeMap;

    /// Pinned SHA-384 of every migration that has already shipped.
    const MANIFEST: &str = include_str!("migration_checksums.txt");

    fn manifest_entries() -> BTreeMap<String, String> {
        MANIFEST
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(|line| {
                let (name, checksum) = line
                    .split_once(' ')
                    .unwrap_or_else(|| panic!("malformed manifest line: {line}"));
                (name.to_string(), checksum.to_ascii_lowercase())
            })
            .collect()
    }

    fn compiled_entries() -> BTreeMap<String, String> {
        MIGRATOR
            .iter()
            .map(|migration| {
                let name = format!(
                    "{}_{}{}",
                    migration.version,
                    migration.description.replace(' ', "_"),
                    migration.migration_type.suffix()
                );
                let checksum = migration
                    .checksum
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>();
                (name, checksum)
            })
            .collect()
    }

    /// The 0.7.3 data-loss bug started as a whitespace-only reformat of two
    /// migrations. sqlx hashes migration files byte for byte, so that alone was
    /// enough to make every existing install fail its checksum check. This test
    /// is the tripwire: touching a migration that has shipped fails here, and
    /// the fix is always a new migration rather than an edit.
    #[test]
    fn shipped_migrations_keep_their_original_checksums() {
        let manifest = manifest_entries();
        let compiled = compiled_entries();

        let mut changed = Vec::new();
        for (name, expected) in &manifest {
            match compiled.get(name) {
                Some(actual) if actual == expected => {}
                Some(_) => changed.push(format!(
                    "{name}: contents changed (line endings count) — revert it and add a new migration"
                )),
                None => changed.push(format!("{name}: removed or renamed")),
            }
        }
        assert!(
            changed.is_empty(),
            "shipped migrations were modified:\n{}",
            changed.join("\n")
        );

        let unpinned: Vec<&String> = compiled
            .keys()
            .filter(|name| !manifest.contains_key(*name))
            .collect();
        assert!(
            unpinned.is_empty(),
            "new migrations are missing from migration_checksums.txt: {unpinned:?}"
        );
    }

    /// Every migration must be LF-only. A CRLF file hashes differently, which is
    /// exactly how 0.7.3 broke, and the difference is invisible in review.
    #[test]
    fn migrations_contain_no_carriage_returns() {
        let offenders: Vec<String> = MIGRATOR
            .iter()
            .filter(|migration| migration.sql.contains('\r'))
            .map(|migration| migration.version.to_string())
            .collect();
        assert!(
            offenders.is_empty(),
            "migrations must use LF line endings: {offenders:?}"
        );
    }
}

#[cfg(test)]
mod recovery_tests {
    use super::{append_suffix, open_migrated_pool, run_migrations, MIGRATOR};
    use sha2::{Digest, Sha384};
    use sqlx::{Row, SqlitePool};
    use std::path::Path;
    use tempfile::tempdir;

    /// Rewrite the stored checksum for `version` to the SHA-384 of the same
    /// migration with CRLF line endings, reproducing what a checkout on a
    /// Windows machine (or the reverse normalization in 0.7.3) leaves behind.
    async fn store_crlf_checksum(pool: &SqlitePool, version: i64) {
        let migration = MIGRATOR
            .iter()
            .find(|migration| migration.version == version)
            .expect("migration exists");
        let crlf = migration.sql.replace("\r\n", "\n").replace('\n', "\r\n");
        let checksum = Sha384::digest(crlf.as_bytes()).to_vec();

        sqlx::query("UPDATE _sqlx_migrations SET checksum = ? WHERE version = ?")
            .bind(checksum)
            .bind(version)
            .execute(pool)
            .await
            .expect("store crlf checksum");
    }

    async fn insert_account(pool: &SqlitePool, id: &str) {
        sqlx::query(
            "INSERT INTO route_credentials (id, platform, kind, display_name, created_at, updated_at)
             VALUES (?, 'claude', 'api', 'kept account', '2026-09-02T00:00:00Z', '2026-09-02T00:00:00Z')",
        )
        .bind(id)
        .execute(pool)
        .await
        .expect("insert account");
    }

    async fn account_count(pool: &SqlitePool) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM route_credentials")
            .fetch_one(pool)
            .await
            .expect("account count")
    }

    async fn quarantine_file_count(backups_dir: &Path) -> usize {
        // A missing directory is the strongest form of "nothing was
        // quarantined": only quarantine_database_files creates it.
        let Ok(mut entries) = tokio::fs::read_dir(backups_dir).await else {
            return 0;
        };
        let mut count = 0usize;
        while let Some(entry) = entries.next_entry().await.expect("backup entry") {
            if entry
                .file_name()
                .to_string_lossy()
                .contains("migration-conflict-")
            {
                count += 1;
            }
        }
        count
    }

    // The 0.7.3 regression: normalizing the migrations from CRLF to LF changed
    // every checksum, so an upgraded install hit VersionMismatch and had its
    // database quarantined — the account list came up empty.
    #[tokio::test]
    async fn line_ending_only_checksum_change_keeps_accounts_and_skips_quarantine() {
        let dir = tempdir().expect("tempdir");
        let database_file = dir.path().join("ai-switch.db");
        let backups_dir = dir.path().join("backups");

        let pool = open_migrated_pool(&database_file, &backups_dir)
            .await
            .expect("initial open");
        insert_account(&pool, "account-kept").await;
        for version in [202607130001, 202607220001, 202608200001] {
            store_crlf_checksum(&pool, version).await;
        }
        pool.close().await;

        let reopened = open_migrated_pool(&database_file, &backups_dir)
            .await
            .expect("reopen after line ending normalization");

        assert_eq!(account_count(&reopened).await, 1);
        assert_eq!(quarantine_file_count(&backups_dir).await, 0);
        run_migrations(&reopened)
            .await
            .expect("migrations clean on the next start");
    }

    // Recovering the installs 0.7.3 already broke: the accounts are sitting in
    // backups/ next to an empty live database, and a start on the fixed build
    // has to pull them back.
    #[tokio::test]
    async fn quarantined_database_is_restored_over_an_empty_one() {
        let dir = tempdir().expect("tempdir");
        let database_file = dir.path().join("ai-switch.db");
        let backups_dir = dir.path().join("backups");
        tokio::fs::create_dir_all(&backups_dir)
            .await
            .expect("backups dir");

        // Stand in for the 0.7.3 quarantine: a fully migrated database holding
        // the user's accounts, parked under backups/.
        let orphan = dir.path().join("orphan.db");
        let orphan_pool = open_migrated_pool(&orphan, &backups_dir)
            .await
            .expect("orphan open");
        insert_account(&orphan_pool, "account-quarantined").await;
        orphan_pool.close().await;
        let quarantined = backups_dir.join("ai-switch.db.migration-conflict-20260901-193257");
        tokio::fs::rename(&orphan, &quarantined)
            .await
            .expect("park the quarantined database");

        let pool = open_migrated_pool(&database_file, &backups_dir)
            .await
            .expect("open with an empty live database");

        assert_eq!(account_count(&pool).await, 1);
        let restored_id: String = sqlx::query_scalar("SELECT id FROM route_credentials")
            .fetch_one(&pool)
            .await
            .expect("restored account id");
        assert_eq!(restored_id, "account-quarantined");
        run_migrations(&pool)
            .await
            .expect("restored database is fully migrated");

        // The quarantine file is renamed, not deleted, and no longer qualifies
        // as a restore candidate.
        assert!(!quarantined.exists());
        assert!(append_suffix(&quarantined, ".restored").exists());
    }

    // Restoring must never overwrite accounts the user added after the
    // quarantine — at that point the empty-list symptom is already behind them.
    #[tokio::test]
    async fn quarantined_database_is_left_alone_when_the_live_one_has_accounts() {
        let dir = tempdir().expect("tempdir");
        let database_file = dir.path().join("ai-switch.db");
        let backups_dir = dir.path().join("backups");
        tokio::fs::create_dir_all(&backups_dir)
            .await
            .expect("backups dir");

        let orphan = dir.path().join("orphan.db");
        let orphan_pool = open_migrated_pool(&orphan, &backups_dir)
            .await
            .expect("orphan open");
        insert_account(&orphan_pool, "account-quarantined").await;
        orphan_pool.close().await;

        let seeded = open_migrated_pool(&database_file, &backups_dir)
            .await
            .expect("first open");
        insert_account(&seeded, "account-added-after").await;
        seeded.close().await;

        // Only now does the quarantine file appear, so the restore decision is
        // made against a live database that already holds an account.
        let quarantined = backups_dir.join("ai-switch.db.migration-conflict-20260901-193257");
        tokio::fs::rename(&orphan, &quarantined)
            .await
            .expect("park the quarantined database");

        let pool = open_migrated_pool(&database_file, &backups_dir)
            .await
            .expect("reopen");

        let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM route_credentials")
            .fetch_all(&pool)
            .await
            .expect("account ids");
        assert_eq!(ids, vec!["account-added-after".to_string()]);
        assert!(quarantined.exists());
    }

    // A real content change to an applied migration is not repairable. Losing
    // the user's accounts to it is worse than refusing to start, so the
    // populated database must survive even though the app cannot open it.
    #[tokio::test]
    async fn unrepairable_checksum_change_preserves_a_populated_database() {
        let dir = tempdir().expect("tempdir");
        let database_file = dir.path().join("ai-switch.db");
        let backups_dir = dir.path().join("backups");

        let pool = open_migrated_pool(&database_file, &backups_dir)
            .await
            .expect("initial open");
        insert_account(&pool, "account-kept").await;
        sqlx::query("UPDATE _sqlx_migrations SET checksum = x'deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef' WHERE version = 202607130004")
            .execute(&pool)
            .await
            .expect("corrupt checksum");
        pool.close().await;

        let error = open_migrated_pool(&database_file, &backups_dir)
            .await
            .expect_err("must refuse to replace a populated database");
        assert!(
            matches!(
                error,
                crate::error::AppError::Database {
                    code: "database.migration_conflict_preserved",
                    ..
                }
            ),
            "unexpected error: {error:?}"
        );

        assert_eq!(quarantine_file_count(&backups_dir).await, 0);
        let survivor = super::create_pool(&database_file).await.expect("reopen");
        assert_eq!(account_count(&survivor).await, 1);
    }

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

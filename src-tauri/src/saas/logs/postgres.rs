use super::{LogConfig, LogPage, LogQuery, LogRecord, LogStore};
use async_trait::async_trait;
use chrono::{Duration as ChronoDuration, Utc};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions, PgSslMode};
use sqlx::{ConnectOptions, PgPool, Postgres, QueryBuilder, Row};
use std::str::FromStr;
use std::time::Duration;

#[derive(Clone)]
pub struct PostgresLogStore {
    pool: PgPool,
}

impl PostgresLogStore {
    pub async fn connect(config: &LogConfig) -> Result<Self, String> {
        config.validate()?;
        let value = config
            .postgres_url
            .as_deref()
            .ok_or("PostgreSQL URL is required")?;
        let url = url::Url::parse(value).map_err(|_| "invalid PostgreSQL URL")?;
        if !matches!(url.scheme(), "postgres" | "postgresql") {
            return Err("PostgreSQL log storage requires a PostgreSQL URL".into());
        }
        let mut options = PgConnectOptions::from_str(value)
            .map_err(|_| "invalid PostgreSQL connection options")?
            .application_name("ai-switch-saas-logs")
            .options([("statement_timeout", "5000"), ("lock_timeout", "3000")])
            .disable_statement_logging();
        if !url
            .query_pairs()
            .any(|(key, _)| key == "sslmode" || key == "ssl-mode")
        {
            options = options.ssl_mode(PgSslMode::VerifyFull);
        }
        let pool = PgPoolOptions::new()
            .max_connections(config.postgres_max_connections)
            .min_connections(0)
            .acquire_timeout(Duration::from_secs(5))
            .connect_with(options)
            .await
            .map_err(|_| "PostgreSQL log connection failed")?;
        let store = Self { pool };
        if let Err(error) = store.initialize().await {
            store.pool.close().await;
            return Err(error);
        }
        Ok(store)
    }

    async fn initialize(&self) -> Result<(), String> {
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|_| "PostgreSQL log schema transaction failed")?;
        for statement in [
            "SELECT pg_advisory_xact_lock(731914062807)",
            "CREATE TABLE IF NOT EXISTS saas_log_schema (name TEXT PRIMARY KEY, version INTEGER NOT NULL)",
            "CREATE TABLE IF NOT EXISTS saas_request_logs (request_id VARCHAR(128) PRIMARY KEY, user_id VARCHAR(128) NOT NULL, key_id VARCHAR(128) NOT NULL, group_id VARCHAR(128) NOT NULL, platform VARCHAR(64) NOT NULL, model VARCHAR(200) NOT NULL, created_at TIMESTAMPTZ NOT NULL, status INTEGER NOT NULL, record JSONB NOT NULL)",
            "CREATE INDEX IF NOT EXISTS saas_request_logs_user_time ON saas_request_logs (user_id, created_at DESC)",
            "CREATE INDEX IF NOT EXISTS saas_request_logs_key_time ON saas_request_logs (key_id, created_at DESC)",
            "CREATE INDEX IF NOT EXISTS saas_request_logs_time ON saas_request_logs (created_at DESC)",
            "INSERT INTO saas_log_schema (name, version) VALUES ('request_logs', 1) ON CONFLICT (name) DO NOTHING",
            "INSERT INTO saas_request_logs (request_id, user_id, key_id, group_id, platform, model, created_at, status, record) SELECT '', '', '', '', '', '', NOW(), 0, '{}'::jsonb WHERE FALSE",
            "DELETE FROM saas_request_logs WHERE FALSE",
            "SELECT request_id, user_id, key_id, group_id, platform, model, created_at, status, record FROM saas_request_logs LIMIT 0",
        ] {
            sqlx::query(statement).execute(&mut *transaction).await.map_err(|_| "PostgreSQL log schema or permissions validation failed")?;
        }
        let version: i32 =
            sqlx::query_scalar("SELECT version FROM saas_log_schema WHERE name = 'request_logs'")
                .fetch_one(&mut *transaction)
                .await
                .map_err(|_| "PostgreSQL log schema version unavailable")?;
        if version != 1 {
            return Err("unsupported PostgreSQL log schema version".into());
        }
        transaction
            .commit()
            .await
            .map_err(|_| "PostgreSQL log schema commit failed")?;
        Ok(())
    }
}

#[async_trait]
impl LogStore for PostgresLogStore {
    async fn append_batch(&self, records: &[LogRecord]) -> Result<(), String> {
        if records.is_empty() {
            return Ok(());
        }
        if records.len() > 500 {
            return Err("log batch exceeds maximum size".into());
        }
        let records = records
            .iter()
            .map(LogRecord::sanitized)
            .collect::<Result<Vec<_>, _>>()?;
        let values = records
            .iter()
            .map(serde_json::to_value)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "log serialization failed")?;
        let mut builder: QueryBuilder<Postgres> = QueryBuilder::new("INSERT INTO saas_request_logs (request_id, user_id, key_id, group_id, platform, model, created_at, status, record) ");
        builder.push_values(
            records.iter().zip(values.iter()),
            |mut row, (record, value)| {
                row.push_bind(&record.request_id)
                    .push_bind(&record.user_id)
                    .push_bind(&record.key_id)
                    .push_bind(&record.group_id)
                    .push_bind(&record.platform)
                    .push_bind(&record.model)
                    .push_bind(record.created_at)
                    .push_bind(i32::from(record.status))
                    .push_bind(value);
            },
        );
        builder.push(" ON CONFLICT (request_id) DO NOTHING");
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|_| "PostgreSQL log transaction failed")?;
        builder
            .build()
            .execute(&mut *transaction)
            .await
            .map_err(|_| "PostgreSQL log batch insert failed")?;
        transaction
            .commit()
            .await
            .map_err(|_| "PostgreSQL log batch commit failed")?;
        Ok(())
    }

    async fn query(&self, query: LogQuery) -> Result<LogPage, String> {
        let query = query.normalized()?;
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|_| "PostgreSQL log query transaction failed")?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *transaction)
            .await
            .map_err(|_| "PostgreSQL log query snapshot failed")?;
        let mut count: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT COUNT(*) FROM saas_request_logs");
        filters(&mut count, &query);
        let total: i64 = count
            .build_query_scalar()
            .fetch_one(&mut *transaction)
            .await
            .map_err(|_| "PostgreSQL log count failed")?;
        let mut builder: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT record FROM saas_request_logs");
        filters(&mut builder, &query);
        builder
            .push(" ORDER BY created_at DESC, request_id COLLATE \"C\" DESC LIMIT ")
            .push_bind(i64::from(query.page_size))
            .push(" OFFSET ")
            .push_bind(i64::from(query.page - 1) * i64::from(query.page_size));
        let rows = builder
            .build()
            .fetch_all(&mut *transaction)
            .await
            .map_err(|_| "PostgreSQL log query failed")?;
        transaction
            .commit()
            .await
            .map_err(|_| "PostgreSQL log query commit failed")?;
        let mut items = Vec::with_capacity(rows.len());
        let mut malformed_lines = 0;
        for row in rows {
            let record = row
                .try_get::<serde_json::Value, _>("record")
                .ok()
                .and_then(|value| serde_json::from_value::<LogRecord>(value).ok())
                .and_then(|record| record.sanitized().ok());
            match record {
                Some(record) if query.matches(&record) => items.push(record),
                _ => malformed_lines += 1,
            }
        }
        Ok(LogPage {
            items,
            total: u64::try_from(total).map_err(|_| "invalid PostgreSQL log count")?,
            page: query.page,
            page_size: query.page_size,
            malformed_lines,
        })
    }

    async fn retention(&self, days: Option<u32>) -> Result<u64, String> {
        let Some(days) = days else {
            return Ok(0);
        };
        if days == 0 || days > 3650 {
            return Err("invalid log retention period".into());
        }
        let cutoff = Utc::now() - ChronoDuration::days(i64::from(days));
        let result = sqlx::query("DELETE FROM saas_request_logs WHERE created_at < $1")
            .bind(cutoff)
            .execute(&self.pool)
            .await
            .map_err(|_| "PostgreSQL log retention failed")?;
        Ok(result.rows_affected())
    }
}

fn filters<'query>(builder: &mut QueryBuilder<'query, Postgres>, query: &'query LogQuery) {
    builder
        .push(" WHERE created_at >= ")
        .push_bind(query.from.unwrap())
        .push(" AND created_at < ")
        .push_bind(query.to.unwrap());
    for (column, value) in [
        ("user_id", &query.user_id),
        ("key_id", &query.key_id),
        ("group_id", &query.group_id),
        ("platform", &query.platform),
        ("model", &query.model),
    ] {
        if let Some(value) = value {
            builder
                .push(" AND ")
                .push(column)
                .push(" = ")
                .push_bind(value);
        }
    }
    if let Some(status) = query.status {
        builder.push(" AND status = ").push_bind(i32::from(status));
    }
}

#[cfg(test)]
pub(super) async fn cleanup_test_records(store: &PostgresLogStore, requests: &[String]) {
    sqlx::query("DELETE FROM saas_request_logs WHERE request_id = ANY($1)")
        .bind(requests)
        .execute(&store.pool)
        .await
        .unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn postgres_rejects_invalid_or_unreachable_urls_without_leaking_them() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        for url in [
            "sqlite://SECRET".to_string(),
            format!("postgres://name:SECRET@127.0.0.1:{port}/logs?sslmode=disable"),
        ] {
            let result = PostgresLogStore::connect(&LogConfig {
                postgres_url: Some(url),
                ..LogConfig::default()
            })
            .await;
            assert!(result.is_err());
            assert!(!result.err().unwrap().contains("SECRET"));
        }
    }

    #[test]
    fn postgres_filters_bind_user_inputs_instead_of_interpolating_sql() {
        let query = LogQuery {
            model: Some("anything' OR TRUE --".into()),
            ..LogQuery::for_user("owner")
        }
        .normalized()
        .unwrap();
        let mut builder: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT record FROM saas_request_logs");
        filters(&mut builder, &query);
        assert!(builder.sql().contains("user_id = $3"));
        assert!(builder.sql().contains("model = $4"));
        assert!(!builder.sql().contains("anything"));
        assert!(!builder.sql().contains("owner"));
    }
}

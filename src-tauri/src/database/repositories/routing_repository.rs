use crate::error::AppError;
use crate::models::routing::{
    FailoverPolicy, NewFailoverPolicy, NewProxyProfile, NewUsageEvent, ProxyProfile, UsageEvent,
};
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct RoutingRepository;

impl RoutingRepository {
    pub async fn create_proxy_profile(
        pool: &SqlitePool,
        input: NewProxyProfile,
    ) -> Result<ProxyProfile, AppError> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let enabled = i64::from(input.enabled);

        sqlx::query(
            "INSERT INTO proxy_profiles (id, name, endpoint_url, auth_ref, enabled, notes, status, sort_order, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, 'configured', 0, ?, ?)",
        )
        .bind(&id)
        .bind(&input.name)
        .bind(&input.endpoint_url)
        .bind(&input.auth_ref)
        .bind(enabled)
        .bind(&input.notes)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.proxy_profile_create",
            message: "Could not create proxy profile".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Self::get_proxy_profile(pool, &id).await
    }

    pub async fn get_proxy_profile(pool: &SqlitePool, id: &str) -> Result<ProxyProfile, AppError> {
        sqlx::query_as::<_, ProxyProfile>("SELECT * FROM proxy_profiles WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.proxy_profile_get",
                message: "Could not load proxy profile".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }

    pub async fn list_proxy_profiles(pool: &SqlitePool) -> Result<Vec<ProxyProfile>, AppError> {
        sqlx::query_as::<_, ProxyProfile>(
            "SELECT * FROM proxy_profiles ORDER BY sort_order ASC, created_at DESC",
        )
        .fetch_all(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.proxy_profile_list",
            message: "Could not list proxy profiles".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })
    }

    pub async fn create_failover_policy(
        pool: &SqlitePool,
        input: NewFailoverPolicy,
    ) -> Result<FailoverPolicy, AppError> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let enabled = i64::from(input.enabled);

        sqlx::query(
            "INSERT INTO failover_policies (id, name, strategy, provider_ids_json, enabled, notes, status, sort_order, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, 'configured', 0, ?, ?)",
        )
        .bind(&id)
        .bind(&input.name)
        .bind(&input.strategy)
        .bind(&input.provider_ids_json)
        .bind(enabled)
        .bind(&input.notes)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.failover_policy_create",
            message: "Could not create failover policy".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Self::get_failover_policy(pool, &id).await
    }

    pub async fn get_failover_policy(
        pool: &SqlitePool,
        id: &str,
    ) -> Result<FailoverPolicy, AppError> {
        sqlx::query_as::<_, FailoverPolicy>("SELECT * FROM failover_policies WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.failover_policy_get",
                message: "Could not load failover policy".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }

    pub async fn list_failover_policies(
        pool: &SqlitePool,
    ) -> Result<Vec<FailoverPolicy>, AppError> {
        sqlx::query_as::<_, FailoverPolicy>(
            "SELECT * FROM failover_policies ORDER BY sort_order ASC, created_at DESC",
        )
        .fetch_all(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.failover_policy_list",
            message: "Could not list failover policies".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })
    }

    pub async fn create_usage_event(
        pool: &SqlitePool,
        input: NewUsageEvent,
    ) -> Result<UsageEvent, AppError> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO usage_events (id, provider_id, official_account_id, source_label, metric_type, amount, unit, metadata_json, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&input.provider_id)
        .bind(&input.official_account_id)
        .bind(&input.source_label)
        .bind(&input.metric_type)
        .bind(input.amount)
        .bind(&input.unit)
        .bind(&input.metadata_json)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.usage_event_create",
            message: "Could not create usage event".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Self::get_usage_event(pool, &id).await
    }

    pub async fn get_usage_event(pool: &SqlitePool, id: &str) -> Result<UsageEvent, AppError> {
        sqlx::query_as::<_, UsageEvent>("SELECT * FROM usage_events WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.usage_event_get",
                message: "Could not load usage event".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }

    pub async fn list_usage_events(pool: &SqlitePool) -> Result<Vec<UsageEvent>, AppError> {
        sqlx::query_as::<_, UsageEvent>("SELECT * FROM usage_events ORDER BY created_at DESC")
            .fetch_all(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.usage_event_list",
                message: "Could not list usage events".to_string(),
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
    async fn creates_and_lists_routing_records() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        RoutingRepository::create_proxy_profile(
            &pool,
            NewProxyProfile {
                name: "Local Proxy".to_string(),
                endpoint_url: "http://127.0.0.1:7890".to_string(),
                auth_ref: None,
                enabled: true,
                notes: Some("Local only".to_string()),
            },
        )
        .await
        .expect("proxy");
        RoutingRepository::create_failover_policy(
            &pool,
            NewFailoverPolicy {
                name: "Primary then backup".to_string(),
                strategy: "ordered".to_string(),
                provider_ids_json: "[\"provider-1\",\"provider-2\"]".to_string(),
                enabled: true,
                notes: None,
            },
        )
        .await
        .expect("failover");
        RoutingRepository::create_usage_event(
            &pool,
            NewUsageEvent {
                provider_id: None,
                official_account_id: None,
                source_label: "manual".to_string(),
                metric_type: "request".to_string(),
                amount: 3,
                unit: "count".to_string(),
                metadata_json: "{}".to_string(),
            },
        )
        .await
        .expect("usage");

        assert_eq!(
            RoutingRepository::list_proxy_profiles(&pool)
                .await
                .expect("proxies")
                .len(),
            1
        );
        assert_eq!(
            RoutingRepository::list_failover_policies(&pool)
                .await
                .expect("policies")
                .len(),
            1
        );
        assert_eq!(
            RoutingRepository::list_usage_events(&pool)
                .await
                .expect("usage")
                .len(),
            1
        );
    }
}

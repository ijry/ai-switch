use crate::error::AppError;
use crate::saas::repository::{self, db_error, invalid};
use crate::services::route_model_capability::{
    advertised_model_ids, catalog_members, model_state_key, supports_requested_capability,
    supports_requested_model, CatalogMemberInput,
};
use crate::services::route_pool_model_mode::PoolModelMode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{FromRow, SqliteConnection, SqlitePool};
use std::collections::BTreeSet;

const SUPPORTED_PLATFORMS: [&str; 3] = ["codex", "claude", "gemini"];

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GroupModel {
    pub model: String,
    #[serde(default)]
    pub upstream_model: String,
    pub input_price_micros: i64,
    pub cache_price_micros: i64,
    pub output_price_micros: i64,
    #[serde(default)]
    pub image_price_micros: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GroupInput {
    id: String,
    multiplier_micros: i64,
    max_output_tokens: i64,
    timeout_seconds: i64,
    max_concurrency: i64,
    #[serde(default = "enabled_by_default")]
    allow_subscription: bool,
    #[serde(default = "enabled_by_default")]
    allow_balance: bool,
    models: Vec<GroupModel>,
}

fn enabled_by_default() -> bool {
    true
}

#[derive(Debug, Clone, FromRow)]
struct CoreGroup {
    id: String,
    name: String,
    platform: String,
    is_internal: bool,
    is_active: bool,
}

#[derive(Debug, Clone, FromRow)]
struct GroupSettings {
    multiplier_micros: i64,
    max_output_tokens: i64,
    timeout_seconds: i64,
    max_concurrency: i64,
    allow_subscription: bool,
    allow_balance: bool,
    version: i64,
}

#[derive(FromRow)]
struct Account {
    id: String,
    platform: String,
    kind: String,
    display_name: String,
    config_json: String,
}

async fn core_group(
    connection: &mut SqliteConnection,
    group_id: &str,
) -> Result<Option<CoreGroup>, AppError> {
    sqlx::query_as(
        "SELECT id,name,platform,is_internal,is_active
         FROM route_pool_groups
         WHERE id=? AND deleted_at IS NULL AND platform IN ('codex','claude','gemini')",
    )
    .bind(group_id)
    .fetch_optional(&mut *connection)
    .await
    .map_err(db_error)
}

async fn settings(
    connection: &mut SqliteConnection,
    group_id: &str,
) -> Result<Option<GroupSettings>, AppError> {
    sqlx::query_as(
        "SELECT multiplier_micros,max_output_tokens,timeout_seconds,max_concurrency,allow_subscription,allow_balance,version
         FROM saas_group_settings WHERE group_id=?",
    )
    .bind(group_id)
    .fetch_optional(&mut *connection)
    .await
    .map_err(db_error)
}

async fn models(
    connection: &mut SqliteConnection,
    group_id: &str,
) -> Result<Vec<GroupModel>, AppError> {
    sqlx::query_as(
        "SELECT model,upstream_model,input_price_micros,cache_price_micros,output_price_micros,image_price_micros
         FROM saas_group_models WHERE group_id=? ORDER BY model",
    )
    .bind(group_id)
    .fetch_all(&mut *connection)
    .await
    .map_err(db_error)
}

async fn eligible_accounts(
    connection: &mut SqliteConnection,
    group: &CoreGroup,
) -> Result<Vec<Account>, AppError> {
    sqlx::query_as(
        "SELECT rc.id,rc.platform,rc.kind,rc.display_name,rc.config_json
         FROM route_credentials rc
         JOIN route_pool_members pm ON pm.route_credential_id=rc.id AND pm.platform=rc.platform
         WHERE pm.group_id=? AND rc.platform=? AND rc.status='ok' AND rc.archived_at IS NULL
         ORDER BY rc.id",
    )
    .bind(&group.id)
    .bind(&group.platform)
    .fetch_all(&mut *connection)
    .await
    .map_err(db_error)
}

pub async fn permitted_accounts(
    pool: &SqlitePool,
    group_id: &str,
) -> Result<Vec<String>, AppError> {
    let mut connection = pool.acquire().await.map_err(db_error)?;
    permitted_accounts_connection(&mut *connection, group_id, None).await
}

pub async fn permitted_accounts_for_image_model(
    pool: &SqlitePool,
    group_id: &str,
    model: &str,
) -> Result<Vec<String>, AppError> {
    let mut connection = pool.acquire().await.map_err(db_error)?;
    permitted_accounts_for_capability_connection(&mut connection, group_id, model, "image.generate")
        .await
}

pub async fn permitted_accounts_for_model(
    pool: &SqlitePool,
    group_id: &str,
    model: &str,
) -> Result<Vec<String>, AppError> {
    let mut connection = pool.acquire().await.map_err(db_error)?;
    permitted_accounts_connection(&mut *connection, group_id, Some(model)).await
}

pub(crate) async fn permitted_accounts_connection(
    connection: &mut SqliteConnection,
    group_id: &str,
    model: Option<&str>,
) -> Result<Vec<String>, AppError> {
    let Some(group) = core_group(connection, group_id).await? else {
        return Ok(Vec::new());
    };
    if group.is_internal {
        return Ok(Vec::new());
    }
    let upstream_model = if let Some(model) = model {
        let allowed: Option<String> = sqlx::query_scalar(
            "SELECT upstream_model FROM saas_group_models WHERE group_id=? AND model=?",
        )
        .bind(group_id)
        .bind(model)
        .fetch_optional(&mut *connection)
        .await
        .map_err(db_error)?;
        if allowed.is_none() {
            return Ok(Vec::new());
        }
        allowed
    } else {
        None
    };

    let accounts = eligible_accounts(&mut *connection, &group).await?;
    let members = catalog_members(
        &accounts
            .iter()
            .map(|account| CatalogMemberInput {
                id: &account.id,
                display_name: &account.display_name,
                kind: &account.kind,
                config_json: &account.config_json,
            })
            .collect::<Vec<_>>(),
    );
    let mut permitted = Vec::new();
    for (account, member) in accounts.iter().zip(members) {
        if let Some(model) = upstream_model.as_deref() {
            if !supports_requested_model(&group.platform, &member.capability, Some(model)) {
                continue;
            }
            let upstream =
                model_state_key(&group.platform, &member.capability, &account.kind, model);
            let state: Option<(String, Option<String>)> = sqlx::query_as(
                "SELECT status,cooldown_until FROM route_credential_models
                 WHERE route_credential_id=? AND model_key=?",
            )
            .bind(&account.id)
            .bind(upstream)
            .fetch_optional(&mut *connection)
            .await
            .map_err(db_error)?;
            if let Some((status, cooldown)) = state {
                if status == "paused"
                    || (status == "error"
                        && cooldown
                            .as_deref()
                            .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
                            .map(|value| value.timestamp() > repository::now())
                            .unwrap_or(true))
                {
                    continue;
                }
            }
        }
        permitted.push(account.id.clone());
    }
    Ok(permitted)
}

pub(crate) async fn permitted_accounts_for_capability_connection(
    connection: &mut SqliteConnection,
    group_id: &str,
    model: &str,
    capability: &str,
) -> Result<Vec<String>, AppError> {
    let Some(group) = core_group(connection, group_id).await? else {
        return Ok(Vec::new());
    };
    if group.is_internal {
        return Ok(Vec::new());
    }
    let upstream_model: Option<String> = sqlx::query_scalar(
        "SELECT upstream_model FROM saas_group_models WHERE group_id=? AND model=?",
    )
    .bind(group_id)
    .bind(model)
    .fetch_optional(&mut *connection)
    .await
    .map_err(db_error)?;
    let Some(upstream_model) = upstream_model else {
        return Ok(Vec::new());
    };
    let accounts = eligible_accounts(&mut *connection, &group).await?;
    let members = catalog_members(
        &accounts
            .iter()
            .map(|account| CatalogMemberInput {
                id: &account.id,
                display_name: &account.display_name,
                kind: &account.kind,
                config_json: &account.config_json,
            })
            .collect::<Vec<_>>(),
    );
    Ok(accounts
        .iter()
        .zip(members)
        .filter(|(_, member)| {
            supports_requested_capability(
                &group.platform,
                &member.capability,
                Some(&upstream_model),
                capability,
            )
        })
        .map(|(account, _)| account.id.clone())
        .collect())
}

async fn group_json(connection: &mut SqliteConnection, group_id: &str) -> Result<Value, AppError> {
    let group = core_group(&mut *connection, group_id)
        .await?
        .ok_or_else(|| invalid("saas.group_not_found", "Core group is unavailable"))?;
    let group_settings = settings(&mut *connection, group_id).await?;
    let group_models = models(&mut *connection, group_id).await?;
    let account_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM route_pool_members pm
         JOIN route_credentials rc ON rc.id=pm.route_credential_id
         WHERE pm.group_id=? AND rc.archived_at IS NULL",
    )
    .bind(group_id)
    .fetch_one(&mut *connection)
    .await
    .map_err(db_error)?;
    let available_count = permitted_accounts_connection(&mut *connection, group_id, None)
        .await?
        .len() as i64;
    let configured = group_settings.is_some() && !group_models.is_empty();
    let mut model_values = Vec::new();
    for model in &group_models {
        let count = permitted_accounts_connection(&mut *connection, group_id, Some(&model.model))
            .await?
            .len();
        let mut value = serde_json::to_value(model)
            .map_err(|_| invalid("saas.serialization", "Could not read model configuration"))?;
        value["availableAccountCount"] = json!(count);
        model_values.push(value);
    }

    let defaults = GroupSettings {
        multiplier_micros: 1_000_000,
        max_output_tokens: 4096,
        timeout_seconds: 120,
        max_concurrency: 4,
        allow_subscription: true,
        allow_balance: true,
        version: 0,
    };
    let values = group_settings.unwrap_or(defaults);
    Ok(json!({
        "id": group.id,
        "name": group.name,
        "platform": group.platform,
        "isInternal": group.is_internal,
        "isActive": group.is_active,
        "accountCount": account_count,
        "availableAccountCount": available_count,
        "configured": configured,
        "multiplierMicros": values.multiplier_micros,
        "maxOutputTokens": values.max_output_tokens,
        "timeoutSeconds": values.timeout_seconds,
        "maxConcurrency": values.max_concurrency,
        "allowSubscription": values.allow_subscription,
        "allowBalance": values.allow_balance,
        "version": values.version,
        "models": model_values,
    }))
}

pub async fn list(pool: &SqlitePool, payload: Value, _user_only: bool) -> Result<Value, AppError> {
    let (limit, offset) = repository::page(&payload)?;
    let platform = payload.get("platform").and_then(Value::as_str);
    if let Some(platform) = platform {
        if !SUPPORTED_PLATFORMS.contains(&platform) {
            return Err(invalid("saas.validation", "Unsupported platform"));
        }
    }

    let mut connection = pool.acquire().await.map_err(db_error)?;
    let rows: Vec<CoreGroup> = if let Some(platform) = platform {
        sqlx::query_as(
            "SELECT id,name,platform,is_internal,is_active
             FROM route_pool_groups
             WHERE platform=? AND deleted_at IS NULL
             ORDER BY sort_order,created_at,id",
        )
        .bind(platform)
        .fetch_all(&mut *connection)
        .await
        .map_err(db_error)?
    } else {
        sqlx::query_as(
            "SELECT id,name,platform,is_internal,is_active
             FROM route_pool_groups
             WHERE platform IN ('codex','claude','gemini') AND deleted_at IS NULL
             ORDER BY platform,sort_order,created_at,id",
        )
        .fetch_all(&mut *connection)
        .await
        .map_err(db_error)?
    };

    let mut items = Vec::new();
    for row in rows {
        let value = group_json(&mut *connection, &row.id).await?;
        if value["isInternal"].as_bool() == Some(false)
            && value["configured"].as_bool() == Some(true)
        {
            items.push(value);
        }
    }
    let total = items.len() as i64;
    let page_items = items
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .collect::<Vec<_>>();
    Ok(json!({"items":page_items,"total":total}))
}

pub async fn available(pool: &SqlitePool, payload: Value) -> Result<Value, AppError> {
    let (limit, offset) = repository::page(&payload)?;
    let mut connection = pool.acquire().await.map_err(db_error)?;
    let rows: Vec<CoreGroup> = sqlx::query_as(
        "SELECT groups.id,groups.name,groups.platform,groups.is_internal,groups.is_active
         FROM route_pool_groups groups
         WHERE groups.platform IN ('codex','claude','gemini')
           AND groups.deleted_at IS NULL
           AND groups.is_internal=0
           AND NOT EXISTS (SELECT 1 FROM saas_group_models models WHERE models.group_id=groups.id)
         ORDER BY groups.platform,groups.sort_order,groups.created_at,groups.id",
    )
    .fetch_all(&mut *connection)
    .await
    .map_err(db_error)?;
    let total = rows.len() as i64;
    let mut items = Vec::new();
    for row in rows.into_iter().skip(offset as usize).take(limit as usize) {
        items.push(group_json(&mut *connection, &row.id).await?);
    }
    Ok(json!({"items":items,"total":total}))
}

pub async fn catalog(pool: &SqlitePool, payload: Value) -> Result<Value, AppError> {
    let group_id = repository::text(&payload, "groupId")?;
    let mut connection = pool.acquire().await.map_err(db_error)?;
    let group = core_group(&mut *connection, group_id)
        .await?
        .ok_or_else(|| invalid("saas.group_not_found", "Core group is unavailable"))?;
    let accounts = eligible_accounts(&mut *connection, &group).await?;
    let members = catalog_members(
        &accounts
            .iter()
            .map(|account| CatalogMemberInput {
                id: &account.id,
                display_name: &account.display_name,
                kind: &account.kind,
                config_json: &account.config_json,
            })
            .collect::<Vec<_>>(),
    );
    let models = advertised_model_ids(&group.platform, &members, PoolModelMode::Aggregate);
    Ok(json!({
        "platforms": SUPPORTED_PLATFORMS,
        "platform": group.platform,
        "groupId": group.id,
        "name": group.name,
        "models": models,
        "availableAccountCount": accounts.len(),
        "accounts": accounts.iter().map(|account| json!({
            "id": account.id,
            "platform": account.platform,
        })).collect::<Vec<_>>(),
    }))
}

pub async fn save(pool: &SqlitePool, payload: Value) -> Result<Value, AppError> {
    let mut input: GroupInput = serde_json::from_value(payload)
        .map_err(|_| invalid("saas.validation", "Invalid group extension"))?;
    if input.id.trim().is_empty() {
        return Err(invalid("saas.validation", "id is required"));
    }
    let mut seen = BTreeSet::new();
    for model in &mut input.models {
        model.model = model.model.trim().to_string();
        model.upstream_model = model.upstream_model.trim().to_string();
        if model.upstream_model.is_empty() {
            model.upstream_model = model.model.clone();
        }
        if model.model.is_empty()
            || model.model.len() > 256
            || model.upstream_model.len() > 256
            || model.model.chars().any(char::is_control)
            || model.upstream_model.chars().any(char::is_control)
            || !seen.insert(model.model.clone())
            || [
                model.input_price_micros,
                model.cache_price_micros,
                model.output_price_micros,
                model.image_price_micros,
            ]
            .iter()
            .any(|price| !(0..=repository::MAX_MONEY).contains(price))
        {
            return Err(invalid("saas.validation", "Invalid model configuration"));
        }
    }
    if input.models.len() > 1000
        || !(1..=1_000_000_000).contains(&input.multiplier_micros)
        || !(1..=1_000_000).contains(&input.max_output_tokens)
        || !(1..=600).contains(&input.timeout_seconds)
        || !(1..=1_000).contains(&input.max_concurrency)
    {
        return Err(invalid("saas.validation", "Invalid group extension"));
    }

    let identifier = input.id.trim().to_string();
    let mut transaction = repository::begin(pool).await?;
    let group = core_group(&mut *transaction, &identifier)
        .await?
        .ok_or_else(|| invalid("saas.group_not_found", "Core group is unavailable"))?;
    let version = settings(&mut *transaction, &identifier)
        .await?
        .map(|values| values.version + 1)
        .unwrap_or(1);
    let now = repository::now();
    sqlx::query(
        "INSERT INTO saas_group_settings
           (group_id,multiplier_micros,max_output_tokens,timeout_seconds,max_concurrency,allow_subscription,allow_balance,version,created_at,updated_at)
         VALUES(?,?,?,?,?,?,?,?,?,?)
         ON CONFLICT(group_id) DO UPDATE SET
           multiplier_micros=excluded.multiplier_micros,
           max_output_tokens=excluded.max_output_tokens,
           timeout_seconds=excluded.timeout_seconds,
           max_concurrency=excluded.max_concurrency,
           allow_subscription=excluded.allow_subscription,
           allow_balance=excluded.allow_balance,
           version=excluded.version,
           updated_at=excluded.updated_at",
    )
    .bind(&identifier)
    .bind(input.multiplier_micros)
    .bind(input.max_output_tokens)
    .bind(input.timeout_seconds)
    .bind(input.max_concurrency)
    .bind(input.allow_subscription)
    .bind(input.allow_balance)
    .bind(version)
    .bind(now)
    .bind(now)
    .execute(&mut *transaction)
    .await
    .map_err(db_error)?;
    sqlx::query("DELETE FROM saas_group_models WHERE group_id=?")
        .bind(&identifier)
        .execute(&mut *transaction)
        .await
        .map_err(db_error)?;
    for model in input.models {
        sqlx::query(
            "INSERT INTO saas_group_models
               (group_id,model,upstream_model,input_price_micros,cache_price_micros,output_price_micros,image_price_micros,version)
             VALUES(?,?,?,?,?,?,?,?)",
        )
        .bind(&identifier)
        .bind(model.model)
        .bind(model.upstream_model)
        .bind(model.input_price_micros)
        .bind(model.cache_price_micros)
        .bind(model.output_price_micros)
        .bind(model.image_price_micros)
        .bind(version)
        .execute(&mut *transaction)
        .await
        .map_err(db_error)?;
    }
    repository::audit(
        &mut *transaction,
        "groups.save",
        Some(&identifier),
        json!({"version":version,"platform":group.platform}),
    )
    .await?;
    let result = group_json(&mut *transaction, &identifier).await?;
    transaction.commit().await.map_err(db_error)?;
    Ok(result)
}

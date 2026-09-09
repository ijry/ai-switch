use crate::error::AppError;
use crate::saas::{
    config,
    repository::{self, db_error, invalid},
};
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::{FromRow, SqliteConnection, SqlitePool};

#[derive(FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
struct KeyRow {
    id: String,
    user_id: String,
    group_id: String,
    name: String,
    prefix: String,
    suffix: String,
    status: String,
    expires_at: Option<i64>,
    limit_micros: Option<i64>,
    spent_micros: i64,
    frozen_micros: i64,
    created_at: i64,
}

fn key_json(row: KeyRow) -> Value {
    json!({"id":row.id,"userId":row.user_id,"groupId":row.group_id,"name":row.name,"prefix":row.prefix,"suffix":row.suffix,"status":row.status,
        "expiresAt":row.expires_at.map(repository::timestamp),"limitMicros":row.limit_micros,"spentMicros":row.spent_micros,"frozenMicros":row.frozen_micros,"createdAt":repository::timestamp(row.created_at)})
}

async fn load(
    connection: &mut SqliteConnection,
    user_id: &str,
    identifier: &str,
) -> Result<KeyRow, AppError> {
    sqlx::query_as("SELECT id,user_id,group_id,name,prefix,suffix,status,expires_at,limit_micros,spent_micros,frozen_micros,created_at FROM saas_api_keys WHERE id=? AND user_id=?")
        .bind(identifier).bind(user_id).fetch_optional(connection).await.map_err(db_error)?.ok_or_else(|| invalid("saas.key_not_found", "API key is unavailable"))
}

fn limit(payload: &Value) -> Result<Option<i64>, AppError> {
    match payload.get("limitMicros") {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_i64()
            .filter(|value| (0..=repository::MAX_MONEY).contains(value))
            .map(Some)
            .ok_or_else(|| {
                invalid(
                    "saas.validation",
                    "limitMicros must be a nonnegative integer",
                )
            }),
    }
}

pub async fn list(pool: &SqlitePool, user_id: &str, payload: Value) -> Result<Value, AppError> {
    let (size, offset) = repository::page(&payload)?;
    let mut transaction = pool.begin().await.map_err(db_error)?;
    repository::require_user(&mut transaction, user_id).await?;
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM saas_api_keys WHERE user_id=?")
        .bind(user_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(db_error)?;
    let rows: Vec<KeyRow> = sqlx::query_as("SELECT id,user_id,group_id,name,prefix,suffix,status,expires_at,limit_micros,spent_micros,frozen_micros,created_at FROM saas_api_keys WHERE user_id=? ORDER BY created_at DESC,id LIMIT ? OFFSET ?")
        .bind(user_id).bind(size).bind(offset).fetch_all(&mut *transaction).await.map_err(db_error)?;
    transaction.commit().await.map_err(db_error)?;
    Ok(json!({"items":rows.into_iter().map(key_json).collect::<Vec<_>>(),"total":total}))
}

pub async fn create(pool: &SqlitePool, user_id: &str, payload: Value) -> Result<Value, AppError> {
    let group_id = repository::text(&payload, "groupId")?;
    let name = repository::text(&payload, "name")?.trim();
    if name.len() > 120 {
        return Err(invalid("saas.validation", "Key name is too long"));
    }
    let expires = repository::parse_expiry(payload.get("expiresAt"))?;
    let limit = limit(&payload)?;
    let mut transaction = repository::begin(pool).await?;
    config::require_enabled(&mut transaction).await?;
    repository::require_user(&mut transaction, user_id).await?;
    let group: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM route_pool_groups groups
         JOIN saas_group_settings settings ON settings.group_id=groups.id
         WHERE groups.id=?
           AND groups.platform IN ('codex','claude')
           AND groups.is_internal=0
           AND groups.deleted_at IS NULL
           AND EXISTS (SELECT 1 FROM saas_group_models models WHERE models.group_id=groups.id)",
    )
    .bind(group_id)
    .fetch_one(&mut *transaction)
    .await
    .map_err(db_error)?;
    if group == 0 {
        return Err(invalid("saas.group_unavailable", "Group is unavailable"));
    }
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM saas_api_keys WHERE user_id=? AND status!='revoked'",
    )
    .bind(user_id)
    .fetch_one(&mut *transaction)
    .await
    .map_err(db_error)?;
    if count >= 100 {
        return Err(invalid(
            "saas.key_limit",
            "A user may have at most 100 active or disabled keys",
        ));
    }
    let identifier = uuid::Uuid::new_v4().to_string();
    let plaintext = repository::random_secret("sk-saas-");
    let current = repository::now();
    sqlx::query("INSERT INTO saas_api_keys(id,user_id,group_id,name,token_hash,prefix,suffix,expires_at,limit_micros,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)")
        .bind(&identifier).bind(user_id).bind(group_id).bind(name).bind(repository::hash_secret(&plaintext)).bind(&plaintext[..16]).bind(&plaintext[plaintext.len()-4..]).bind(expires).bind(limit).bind(current).bind(current)
        .execute(&mut *transaction).await.map_err(db_error)?;
    let mut result = key_json(load(&mut transaction, user_id, &identifier).await?);
    result["plaintextKey"] = json!(plaintext);
    transaction.commit().await.map_err(db_error)?;
    Ok(result)
}

pub async fn update(
    pool: &SqlitePool,
    user_id: &str,
    payload: Value,
    rotate: bool,
) -> Result<Value, AppError> {
    let identifier = repository::text(&payload, "id")?;
    let allowed = ["id", "name", "status", "expiresAt", "limitMicros"];
    if payload
        .as_object()
        .is_none_or(|object| object.keys().any(|key| !allowed.contains(&key.as_str())))
    {
        return Err(invalid(
            "saas.validation",
            "Unknown key field; a key's group cannot be changed",
        ));
    }
    let mut transaction = repository::begin(pool).await?;
    config::require_enabled(&mut transaction).await?;
    repository::require_user(&mut transaction, user_id).await?;
    let old = load(&mut transaction, user_id, identifier).await?;
    if old.status == "revoked" {
        return Err(invalid(
            "saas.key_revoked",
            "A revoked key cannot be changed",
        ));
    }
    let name = if payload.get("name").is_some() {
        repository::text(&payload, "name")?.trim()
    } else {
        &old.name
    };
    if name.len() > 120 {
        return Err(invalid("saas.validation", "Key name is too long"));
    }
    let status = if payload.get("status").is_some() {
        repository::text(&payload, "status")?
    } else {
        &old.status
    };
    if !["active", "disabled", "revoked"].contains(&status) {
        return Err(invalid("saas.validation", "Invalid key status"));
    }
    let expires = if payload.get("expiresAt").is_some() {
        repository::parse_expiry(payload.get("expiresAt"))?
    } else {
        old.expires_at
    };
    let limit = if payload.get("limitMicros").is_some() {
        limit(&payload)?
    } else {
        old.limit_micros
    };
    sqlx::query("UPDATE saas_api_keys SET name=?,status=?,expires_at=?,limit_micros=?,updated_at=? WHERE id=? AND user_id=?")
        .bind(name).bind(status).bind(expires).bind(limit).bind(repository::now()).bind(identifier).bind(user_id).execute(&mut *transaction).await.map_err(db_error)?;
    let plaintext = if rotate {
        if status == "revoked" {
            return Err(invalid(
                "saas.key_revoked",
                "A revoked key cannot be rotated",
            ));
        }
        let token = repository::random_secret("sk-saas-");
        sqlx::query(
            "UPDATE saas_api_keys SET token_hash=?,prefix=?,suffix=? WHERE id=? AND user_id=?",
        )
        .bind(repository::hash_secret(&token))
        .bind(&token[..16])
        .bind(&token[token.len() - 4..])
        .bind(identifier)
        .bind(user_id)
        .execute(&mut *transaction)
        .await
        .map_err(db_error)?;
        Some(token)
    } else {
        None
    };
    let mut result = key_json(load(&mut transaction, user_id, identifier).await?);
    if let Some(plaintext) = plaintext {
        result["plaintextKey"] = json!(plaintext);
    }
    transaction.commit().await.map_err(db_error)?;
    Ok(result)
}

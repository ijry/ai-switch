use crate::error::AppError;
use crate::saas::{
    billing::operations,
    config,
    repository::{self, db_error, invalid},
};
use serde_json::{json, Value};
use sqlx::{SqliteConnection, SqlitePool};

pub(crate) async fn ensure_referral_code(
    connection: &mut SqliteConnection,
    user_id: &str,
) -> Result<String, AppError> {
    if let Some(code) =
        sqlx::query_scalar::<_, Option<String>>("SELECT referral_code FROM saas_users WHERE id=?")
            .bind(user_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(db_error)?
            .flatten()
    {
        return Ok(code);
    }
    let code = repository::random_secret("")[..12].to_ascii_uppercase();
    sqlx::query("UPDATE saas_users SET referral_code=?,updated_at=? WHERE id=?")
        .bind(&code)
        .bind(repository::now())
        .bind(user_id)
        .execute(connection)
        .await
        .map_err(db_error)?;
    Ok(code)
}

pub async fn create_codes(pool: &SqlitePool, payload: Value) -> Result<Value, AppError> {
    let count = payload
        .get("count")
        .and_then(Value::as_i64)
        .filter(|v| (1..=200).contains(v))
        .ok_or_else(|| invalid("saas.validation", "Invalid invite code count"))?;
    let max_uses = payload
        .get("maxUses")
        .and_then(Value::as_i64)
        .filter(|v| (1..=1_000_000).contains(v));
    let expires_at = repository::parse_expiry(payload.get("expiresAt"))?;
    let mut tx = repository::begin(pool).await?;
    let mut codes = Vec::new();
    for _ in 0..count {
        let plain = repository::random_secret("invite-");
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO saas_invite_codes(id,token_hash,prefix,suffix,max_uses,expires_at,created_at) VALUES(?,?,?,?,?,?,?)")
            .bind(&id).bind(repository::hash_secret(&plain)).bind(&plain[..12]).bind(&plain[plain.len()-4..]).bind(max_uses).bind(expires_at).bind(repository::now()).execute(&mut *tx).await.map_err(db_error)?;
        codes.push(json!({"id":id,"code":plain,"maxUses":max_uses,"expiresAt":expires_at.map(repository::timestamp)}));
    }
    repository::audit(
        &mut tx,
        "invites.codes.create",
        None,
        json!({"count":count}),
    )
    .await?;
    tx.commit().await.map_err(db_error)?;
    Ok(json!({"items":codes,"total":codes.len()}))
}

pub async fn disable_code(pool: &SqlitePool, payload: Value) -> Result<Value, AppError> {
    let identifier = repository::text(&payload, "id")?;
    let mut tx = repository::begin(pool).await?;
    let status: Option<String> =
        sqlx::query_scalar("SELECT status FROM saas_invite_codes WHERE id=?")
            .bind(&identifier)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db_error)?;
    match status.as_deref() {
        Some("active") | Some("disabled") => (),
        _ => {
            return Err(invalid(
                "saas.invite_not_found",
                "Invite code is unavailable",
            ))
        }
    }
    sqlx::query("UPDATE saas_invite_codes SET status='disabled' WHERE id=? AND status='active'")
        .bind(&identifier)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    repository::audit(
        &mut tx,
        "invites.codes.disable",
        Some(&identifier),
        json!({}),
    )
    .await?;
    tx.commit().await.map_err(db_error)?;
    Ok(json!({"id":identifier,"status":"disabled"}))
}

pub async fn list_codes(pool: &SqlitePool, payload: Value) -> Result<Value, AppError> {
    let (limit, offset) = repository::page(&payload)?;
    let rows:Vec<(String,String,String,String,Option<i64>,i64,Option<i64>,i64)>=sqlx::query_as("SELECT id,prefix,suffix,status,max_uses,used_count,expires_at,created_at FROM saas_invite_codes ORDER BY created_at DESC,id LIMIT ? OFFSET ?").bind(limit).bind(offset).fetch_all(pool).await.map_err(db_error)?;
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM saas_invite_codes")
        .fetch_one(pool)
        .await
        .map_err(db_error)?;
    Ok(
        json!({"items":rows.into_iter().map(|r|json!({"id":r.0,"prefix":r.1,"suffix":r.2,"status":r.3,"maxUses":r.4,"usedCount":r.5,"expiresAt":r.6.map(repository::timestamp),"createdAt":repository::timestamp(r.7)})).collect::<Vec<_>>(),"total":total}),
    )
}

pub(crate) async fn validate_registration_invite(
    connection: &mut SqliteConnection,
    invite: Option<&str>,
) -> Result<Option<String>, AppError> {
    let cfg = config::load_connection(connection).await?;
    if !cfg.invite_enabled
        || (!cfg.invite_registration_required && invite.map(str::trim).unwrap_or("").is_empty())
    {
        return Ok(None);
    }
    let code = invite
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| invalid("saas.invite_required", "An invite code is required"))?;
    if let Some(inviter) = sqlx::query_scalar::<_, String>(
        "SELECT id FROM saas_users WHERE referral_code=? AND status='active'",
    )
    .bind(code.to_ascii_uppercase())
    .fetch_optional(&mut *connection)
    .await
    .map_err(db_error)?
    {
        return Ok(Some(inviter));
    }
    let exists: bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM saas_invite_codes WHERE token_hash=? AND status='active' AND (max_uses IS NULL OR used_count<max_uses) AND (expires_at IS NULL OR expires_at>?))")
        .bind(repository::hash_secret(code)).bind(repository::now()).fetch_one(connection).await.map_err(db_error)?;
    if !exists {
        return Err(invalid(
            "saas.invite_invalid",
            "Invite code is invalid or unavailable",
        ));
    }
    Ok(None)
}

pub(crate) async fn consume_registration_invite(
    connection: &mut SqliteConnection,
    invite: Option<&str>,
) -> Result<Option<String>, AppError> {
    let inviter = validate_registration_invite(connection, invite).await?;
    let Some(code) = invite.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(inviter);
    };
    if inviter.is_none() {
        let changed=sqlx::query("UPDATE saas_invite_codes SET used_count=used_count+1 WHERE token_hash=? AND status='active' AND (max_uses IS NULL OR used_count<max_uses) AND (expires_at IS NULL OR expires_at>?)")
            .bind(repository::hash_secret(code)).bind(repository::now()).execute(connection).await.map_err(db_error)?.rows_affected();
        if changed == 0 {
            return Err(invalid(
                "saas.invite_invalid",
                "Invite code is invalid or unavailable",
            ));
        }
    }
    Ok(inviter)
}

pub(crate) async fn create_signup_reward(
    connection: &mut SqliteConnection,
    inviter_id: Option<&str>,
    invitee_id: &str,
) -> Result<(), AppError> {
    let Some(inviter_id) = inviter_id else {
        return Ok(());
    };
    let cfg = config::load_connection(connection).await?;
    if cfg.invite_signup_reward_micros > 0 {
        sqlx::query("INSERT OR IGNORE INTO saas_invite_rewards(id,inviter_id,invitee_id,kind,source_id,base_micros,amount_micros,created_at) VALUES(?,?,?,'signup',?,0,?,?)")
            .bind(uuid::Uuid::new_v4().to_string()).bind(inviter_id).bind(invitee_id).bind(invitee_id).bind(cfg.invite_signup_reward_micros).bind(repository::now()).execute(connection).await.map_err(db_error)?;
    }
    Ok(())
}

pub(crate) async fn create_recharge_reward(
    connection: &mut SqliteConnection,
    invitee_id: &str,
    source_id: &str,
    amount: i64,
) -> Result<(), AppError> {
    let inviter: Option<String> =
        sqlx::query_scalar("SELECT invited_by FROM saas_users WHERE id=?")
            .bind(invitee_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(db_error)?
            .flatten();
    let Some(inviter) = inviter else {
        return Ok(());
    };
    let cfg = config::load_connection(connection).await?;
    let reward =
        repository::money(amount as i128 * cfg.invite_recharge_rate_micros as i128 / 1_000_000)?;
    if reward > 0 {
        sqlx::query("INSERT OR IGNORE INTO saas_invite_rewards(id,inviter_id,invitee_id,kind,source_id,base_micros,amount_micros,created_at) VALUES(?,?,?,'recharge',?,?,?,?)")
        .bind(uuid::Uuid::new_v4().to_string()).bind(inviter).bind(invitee_id).bind(source_id).bind(amount).bind(reward).bind(repository::now()).execute(connection).await.map_err(db_error)?;
    }
    Ok(())
}

pub async fn rewards(
    pool: &SqlitePool,
    user_id: Option<&str>,
    payload: Value,
) -> Result<Value, AppError> {
    let (limit, offset) = repository::page(&payload)?;
    let status = payload.get("status").and_then(Value::as_str);
    let rows:Vec<(String,String,String,String,String,i64,i64,String,Option<String>,i64)>=sqlx::query_as("SELECT id,inviter_id,invitee_id,kind,source_id,base_micros,amount_micros,status,reason,created_at FROM saas_invite_rewards WHERE (? IS NULL OR inviter_id=?) AND (? IS NULL OR status=?) ORDER BY created_at DESC,id LIMIT ? OFFSET ?")
        .bind(user_id).bind(user_id).bind(status).bind(status).bind(limit).bind(offset).fetch_all(pool).await.map_err(db_error)?;
    let total:i64=sqlx::query_scalar("SELECT COUNT(*) FROM saas_invite_rewards WHERE (? IS NULL OR inviter_id=?) AND (? IS NULL OR status=?)").bind(user_id).bind(user_id).bind(status).bind(status).fetch_one(pool).await.map_err(db_error)?;
    Ok(
        json!({"items":rows.into_iter().map(|r|json!({"id":r.0,"inviterId":r.1,"inviteeId":r.2,"kind":r.3,"sourceId":r.4,"baseMicros":r.5,"amountMicros":r.6,"status":r.7,"reason":r.8,"createdAt":repository::timestamp(r.9)})).collect::<Vec<_>>(),"total":total}),
    )
}

pub async fn review(pool: &SqlitePool, payload: Value) -> Result<Value, AppError> {
    let id = repository::text(&payload, "id")?;
    let status = repository::text(&payload, "status")?;
    let reason = repository::text(&payload, "reason")?;
    if !["approved", "rejected"].contains(&status) {
        return Err(invalid(
            "saas.validation",
            "Invalid invite reward review status",
        ));
    }
    let mut tx = repository::begin(pool).await?;
    let row: (String, i64, String) = sqlx::query_as(
        "SELECT inviter_id,amount_micros,status FROM saas_invite_rewards WHERE id=?",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(db_error)?
    .ok_or_else(|| invalid("saas.reward_not_found", "Invite reward is unavailable"))?;
    if row.2 != "pending" {
        return Err(invalid(
            "saas.reward_final",
            "Invite reward has already been reviewed",
        ));
    }
    sqlx::query("UPDATE saas_invite_rewards SET status=?,reason=?,reviewed_at=? WHERE id=? AND status='pending'").bind(status).bind(reason).bind(repository::now()).bind(id).execute(&mut *tx).await.map_err(db_error)?;
    if status == "approved" {
        operations::credit_wallet(
            &mut tx,
            &row.0,
            row.1,
            "invite_reward",
            id,
            "administrator",
            Some(reason),
        )
        .await?;
    }
    repository::audit(
        &mut tx,
        "invites.rewards.review",
        Some(id),
        json!({"status":status,"reason":reason}),
    )
    .await?;
    tx.commit().await.map_err(db_error)?;
    Ok(json!({"id":id,"status":status}))
}

pub async fn overview(pool: &SqlitePool, user_id: &str) -> Result<Value, AppError> {
    let mut tx = pool.acquire().await.map_err(db_error)?;
    let code = ensure_referral_code(&mut tx, user_id).await?;
    let pending:i64=sqlx::query_scalar("SELECT COALESCE(SUM(amount_micros),0) FROM saas_invite_rewards WHERE inviter_id=? AND status='pending'").bind(user_id).fetch_one(&mut *tx).await.map_err(db_error)?;
    let settled:i64=sqlx::query_scalar("SELECT COALESCE(SUM(amount_micros),0) FROM saas_invite_rewards WHERE inviter_id=? AND status='approved'").bind(user_id).fetch_one(&mut *tx).await.map_err(db_error)?;
    Ok(json!({"referralCode":code,"pendingMicros":pending,"settledMicros":settled}))
}

use crate::error::AppError;
use crate::saas::{
    config,
    repository::{self, db_error, hash_secret, invalid},
};
use crate::security::{KeyringSecretStore, SecretStore};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, SqliteConnection, SqlitePool};

const STATE_LIFETIME_SECONDS: i64 = 600;
const SESSION_LIFETIME_SECONDS: i64 = 30 * 24 * 60 * 60;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SafeUser {
    pub id: String,
    pub github_id: String,
    pub email: Option<String>,
    pub login: String,
    pub avatar_url: Option<String>,
    pub status: String,
    pub balance_micros: i64,
    pub frozen_micros: i64,
    pub available_micros: i64,
    pub debt_micros: i64,
    pub created_at: String,
}

#[derive(FromRow)]
struct UserRow {
    id: String,
    github_id: String,
    email: Option<String>,
    login: String,
    avatar_url: Option<String>,
    status: String,
    balance_micros: i64,
    frozen_micros: i64,
    created_at: i64,
}

pub(crate) async fn safe_user(
    connection: &mut SqliteConnection,
    user_id: &str,
) -> Result<SafeUser, AppError> {
    let row: UserRow = sqlx::query_as("SELECT id,github_id,email,login,avatar_url,status,balance_micros,frozen_micros,created_at FROM saas_users WHERE id=?")
        .bind(user_id).fetch_optional(connection).await.map_err(db_error)?.ok_or_else(|| invalid("saas.unauthorized", "User is unavailable"))?;
    Ok(SafeUser {
        id: row.id,
        github_id: row.github_id,
        email: row.email,
        login: row.login,
        avatar_url: row.avatar_url,
        status: row.status,
        balance_micros: row.balance_micros,
        frozen_micros: row.frozen_micros,
        available_micros: row.balance_micros.saturating_sub(row.frozen_micros).max(0),
        debt_micros: row.balance_micros.saturating_neg().max(0),
        created_at: repository::timestamp(row.created_at),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthStart {
    pub authorize_url: String,
    pub binding: String,
    pub expires_at: String,
}

#[derive(FromRow)]
pub struct OAuthState {
    pub verifier: String,
    pub redirect_uri: String,
    pub client_id: String,
    pub expires_at: i64,
    pub invite_code: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginResult {
    pub session_token: String,
    pub csrf_token: String,
    pub expires_at: String,
    pub user: SafeUser,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPrincipal {
    pub user: SafeUser,
    pub expires_at: String,
    pub csrf_token: String,
}

#[derive(Clone)]
pub struct GitHubClient {
    client: reqwest::Client,
    token_url: String,
    user_url: String,
}

#[derive(Clone, Deserialize)]
struct GitHubProfile {
    id: i64,
    login: String,
    avatar_url: Option<String>,
    created_at: DateTime<Utc>,
}

#[derive(Deserialize)]
struct GitHubToken {
    access_token: Option<String>,
}

impl GitHubClient {
    pub fn new(client: reqwest::Client) -> Self {
        Self {
            client,
            token_url: "https://github.com/login/oauth/access_token".into(),
            user_url: "https://api.github.com/user".into(),
        }
    }

    async fn profile(
        &self,
        code: &str,
        state: &OAuthState,
        secret: &str,
    ) -> Result<GitHubProfile, AppError> {
        let response = self
            .client
            .post(&self.token_url)
            .timeout(std::time::Duration::from_secs(20))
            .header("Accept", "application/json")
            .form(&[
                ("client_id", state.client_id.as_str()),
                ("client_secret", secret),
                ("code", code),
                ("redirect_uri", state.redirect_uri.as_str()),
                ("code_verifier", state.verifier.as_str()),
            ])
            .send()
            .await
            .map_err(|_| {
                invalid(
                    "saas.github_unavailable",
                    "GitHub authentication is unavailable",
                )
            })?;
        if !response.status().is_success() {
            return Err(invalid(
                "saas.github_exchange",
                "GitHub rejected the authorization code",
            ));
        }
        let token: GitHubToken = limited_json(response).await?;
        let access = token
            .access_token
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                invalid(
                    "saas.github_exchange",
                    "GitHub rejected the authorization code",
                )
            })?;
        let response = self
            .client
            .get(&self.user_url)
            .timeout(std::time::Duration::from_secs(20))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "ai-switch-saas")
            .bearer_auth(access)
            .send()
            .await
            .map_err(|_| invalid("saas.github_unavailable", "GitHub identity lookup failed"))?;
        if !response.status().is_success() {
            return Err(invalid(
                "saas.github_identity",
                "Could not verify GitHub identity",
            ));
        }
        limited_json(response).await
    }
}

async fn limited_json<T: serde::de::DeserializeOwned>(
    mut response: reqwest::Response,
) -> Result<T, AppError> {
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| invalid("saas.github_identity", "Invalid GitHub response"))?
    {
        if body.len() + chunk.len() > 256 * 1024 {
            return Err(invalid(
                "saas.github_identity",
                "GitHub response is too large",
            ));
        }
        body.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&body).map_err(|_| {
        invalid(
            "saas.github_identity",
            "GitHub returned incomplete identity information",
        )
    })
}

pub fn eligible_at(created: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    created <= now && now.signed_duration_since(created) >= Duration::days(365)
}

fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn csrf_for_session(token: &str) -> String {
    hash_secret(&format!("ai-switch-saas-csrf:{token}"))
}

fn normalize_email(value: &str) -> Result<String, AppError> {
    let email = value.trim().to_ascii_lowercase();
    let Some((local, domain)) = email.split_once('@') else {
        return Err(invalid("saas.email_invalid", "Enter a valid email address"));
    };
    if email.len() > 254
        || local.is_empty()
        || local.len() > 64
        || domain.is_empty()
        || !domain.contains('.')
        || email.chars().any(char::is_whitespace)
    {
        return Err(invalid("saas.email_invalid", "Enter a valid email address"));
    }
    Ok(email)
}

fn validate_password(password: &str) -> Result<(), AppError> {
    if !(8..=128).contains(&password.chars().count()) {
        return Err(invalid(
            "saas.password_invalid",
            "Password must contain 8 to 128 characters",
        ));
    }
    Ok(())
}

async fn password_hash(password: String) -> Result<String, AppError> {
    tokio::task::spawn_blocking(move || {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(|_| invalid("saas.password_hash", "Could not secure the password"))
    })
    .await
    .map_err(|_| invalid("saas.password_hash", "Could not secure the password"))?
}

async fn password_matches(password: String, encoded: String) -> bool {
    tokio::task::spawn_blocking(move || {
        PasswordHash::new(&encoded).ok().is_some_and(|hash| {
            Argon2::default()
                .verify_password(password.as_bytes(), &hash)
                .is_ok()
        })
    })
    .await
    .unwrap_or(false)
}

pub async fn start_oauth(
    pool: &SqlitePool,
    invite_code: Option<&str>,
) -> Result<OAuthStart, AppError> {
    let mut transaction = repository::begin(pool).await?;
    let config = config::require_enabled(&mut transaction).await?;
    if !config.github_client_secret_configured || config.github_client_id.is_empty() {
        return Err(invalid(
            "saas.oauth_unavailable",
            "GitHub sign-in is not configured",
        ));
    }
    let origin = config::validate_public_base_url(&config.public_base_url)?;
    let redirect_uri = origin
        .join("/api/saas/auth/github/callback")
        .map_err(|_| invalid("saas.config_url", "Invalid OAuth callback"))?
        .to_string();
    let current = repository::now();
    sqlx::query("DELETE FROM saas_oauth_states WHERE expires_at<=?")
        .bind(current)
        .execute(&mut *transaction)
        .await
        .map_err(db_error)?;
    let outstanding: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM saas_oauth_states")
        .fetch_one(&mut *transaction)
        .await
        .map_err(db_error)?;
    if outstanding >= 5_000 {
        return Err(invalid(
            "saas.rate_limited",
            "Too many pending sign-in attempts",
        ));
    }
    let state = repository::random_secret("");
    let binding = repository::random_secret("");
    let verifier = repository::random_secret("");
    let expires_at = current + STATE_LIFETIME_SECONDS;
    sqlx::query("INSERT INTO saas_oauth_states(state_hash,binding_hash,verifier,redirect_uri,client_id,expires_at,created_at,invite_code) VALUES(?,?,?,?,?,?,?,?)")
        .bind(hash_secret(&state)).bind(hash_secret(&binding)).bind(&verifier).bind(&redirect_uri).bind(&config.github_client_id).bind(expires_at).bind(current).bind(invite_code.map(str::trim).filter(|value| !value.is_empty()))
        .execute(&mut *transaction).await.map_err(db_error)?;
    transaction.commit().await.map_err(db_error)?;
    let mut url = url::Url::parse("https://github.com/login/oauth/authorize").map_err(|_| {
        invalid(
            "saas.oauth_unavailable",
            "Invalid GitHub authorization endpoint",
        )
    })?;
    url.query_pairs_mut().extend_pairs(&[
        ("client_id", config.github_client_id.as_str()),
        ("redirect_uri", redirect_uri.as_str()),
        ("state", state.as_str()),
        ("code_challenge", pkce_challenge(&verifier).as_str()),
        ("code_challenge_method", "S256"),
    ]);
    Ok(OAuthStart {
        authorize_url: url.into(),
        binding,
        expires_at: repository::timestamp(expires_at),
    })
}

pub async fn consume_state(
    pool: &SqlitePool,
    state: &str,
    binding: &str,
) -> Result<OAuthState, AppError> {
    if state.len() != 64 || binding.len() != 64 {
        return Err(invalid(
            "saas.oauth_state",
            "Invalid or expired sign-in state",
        ));
    }
    sqlx::query_as("DELETE FROM saas_oauth_states WHERE state_hash=? AND binding_hash=? AND expires_at>? RETURNING verifier,redirect_uri,client_id,expires_at,invite_code")
        .bind(hash_secret(state)).bind(hash_secret(binding)).bind(repository::now()).fetch_optional(pool).await.map_err(db_error)?
        .ok_or_else(|| invalid("saas.oauth_state", "Invalid or expired sign-in state"))
}

pub async fn callback(
    pool: &SqlitePool,
    github: &GitHubClient,
    code: &str,
    state: &str,
    binding: &str,
) -> Result<LoginResult, AppError> {
    callback_with_secret_store(
        pool,
        github,
        code,
        state,
        binding,
        &KeyringSecretStore::new("ai-switch.saas"),
    )
    .await
}

pub async fn callback_with_secret_store(
    pool: &SqlitePool,
    github: &GitHubClient,
    code: &str,
    state: &str,
    binding: &str,
    store: &dyn SecretStore,
) -> Result<LoginResult, AppError> {
    if code.is_empty() || code.len() > 4096 {
        return Err(invalid("saas.oauth_code", "Invalid authorization code"));
    }
    let state = consume_state(pool, state, binding).await?;
    let current_config = config::load(pool).await?;
    let expected_redirect = format!(
        "{}/api/saas/auth/github/callback",
        current_config.public_base_url.trim_end_matches('/')
    );
    if !current_config.enabled
        || current_config.github_client_id != state.client_id
        || state.redirect_uri != expected_redirect
    {
        return Err(invalid(
            "saas.oauth_state",
            "Sign-in configuration changed; please sign in again",
        ));
    }
    let secret = config::github_secret_with_store(pool, store).await?;
    let profile = github.profile(code, &state, &secret).await?;
    complete_login(pool, profile, state.invite_code.as_deref()).await
}

async fn complete_login(
    pool: &SqlitePool,
    profile: GitHubProfile,
    invite_code: Option<&str>,
) -> Result<LoginResult, AppError> {
    if profile.id <= 0
        || profile.login.is_empty()
        || profile.login.len() > 128
        || !eligible_at(profile.created_at, Utc::now())
    {
        return Err(invalid(
            "saas.github_account_age",
            "A verified GitHub account at least 365 days old is required",
        ));
    }
    let avatar = profile.avatar_url.filter(|value| {
        url::Url::parse(value)
            .map(|url| {
                url.scheme() == "https" && url.username().is_empty() && url.password().is_none()
            })
            .unwrap_or(false)
    });
    let mut transaction = repository::begin(pool).await?;
    let config = config::require_enabled(&mut transaction).await?;
    let existing: Option<(String, String)> =
        sqlx::query_as("SELECT id,status FROM saas_users WHERE github_id=?")
            .bind(profile.id.to_string())
            .fetch_optional(&mut *transaction)
            .await
            .map_err(db_error)?;
    let user_id = match existing {
        Some((identifier, status)) if status == "active" => identifier,
        Some(_) => {
            return Err(invalid(
                "saas.user_banned",
                "This user account is suspended",
            ))
        }
        None if !config.registration_enabled => {
            return Err(invalid(
                "saas.registration_closed",
                "New registrations are disabled",
            ))
        }
        None => {
            let inviter = crate::saas::domain::invites::consume_registration_invite(
                &mut transaction,
                invite_code,
            )
            .await?;
            let identifier = uuid::Uuid::new_v4().to_string();
            sqlx::query("INSERT INTO saas_users(id,github_id,login,avatar_url,github_created_at,invited_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?)")
                .bind(&identifier).bind(profile.id.to_string()).bind(&profile.login).bind(&avatar).bind(profile.created_at.to_rfc3339()).bind(inviter.as_deref()).bind(repository::now()).bind(repository::now())
                .execute(&mut *transaction).await.map_err(db_error)?;
            crate::saas::domain::invites::create_signup_reward(
                &mut transaction,
                inviter.as_deref(),
                &identifier,
            )
            .await?;
            identifier
        }
    };
    let current = repository::now();
    sqlx::query("UPDATE saas_users SET login=?,avatar_url=?,updated_at=? WHERE id=?")
        .bind(profile.login)
        .bind(avatar)
        .bind(current)
        .bind(&user_id)
        .execute(&mut *transaction)
        .await
        .map_err(db_error)?;
    let result = issue_session(&mut transaction, &config.public_base_url, &user_id).await?;
    transaction.commit().await.map_err(db_error)?;
    Ok(result)
}

async fn issue_session(
    transaction: &mut SqliteConnection,
    origin: &str,
    user_id: &str,
) -> Result<LoginResult, AppError> {
    let current = repository::now();
    sqlx::query("DELETE FROM saas_sessions WHERE expires_at<=? OR revoked_at IS NOT NULL")
        .bind(current)
        .execute(&mut *transaction)
        .await
        .map_err(db_error)?;
    sqlx::query("DELETE FROM saas_sessions WHERE token_hash IN (SELECT token_hash FROM saas_sessions WHERE user_id=? ORDER BY created_at DESC,token_hash LIMIT -1 OFFSET 9)")
        .bind(user_id).execute(&mut *transaction).await.map_err(db_error)?;
    let session_token = repository::random_secret("ss-saas-");
    let csrf_token = csrf_for_session(&session_token);
    let expires_at = current + SESSION_LIFETIME_SECONDS;
    sqlx::query("INSERT INTO saas_sessions(token_hash,csrf_hash,user_id,origin,expires_at,created_at) VALUES(?,?,?,?,?,?)")
        .bind(hash_secret(&session_token)).bind(hash_secret(&csrf_token)).bind(user_id).bind(origin).bind(expires_at).bind(current).execute(&mut *transaction).await.map_err(db_error)?;
    let user = safe_user(transaction, user_id).await?;
    Ok(LoginResult {
        session_token,
        csrf_token,
        expires_at: repository::timestamp(expires_at),
        user,
    })
}

pub async fn create_password_user(
    pool: &SqlitePool,
    email: &str,
    password: &str,
) -> Result<SafeUser, AppError> {
    let email = normalize_email(email)?;
    validate_password(password)?;
    let encoded = password_hash(password.to_owned()).await?;
    let mut transaction = repository::begin(pool).await?;
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM saas_users WHERE lower(email)=?)")
            .bind(&email)
            .fetch_one(&mut *transaction)
            .await
            .map_err(db_error)?;
    if exists {
        return Err(invalid(
            "saas.user_exists",
            "A user with this email already exists",
        ));
    }
    let identifier = uuid::Uuid::new_v4().to_string();
    let login = email
        .split_once('@')
        .map(|(local, _)| local)
        .unwrap_or("user");
    let current = repository::now();
    sqlx::query("INSERT INTO saas_users(id,github_id,email,password_hash,login,github_created_at,created_at,updated_at) VALUES(?,?,?,?,?,'1970-01-01T00:00:00Z',?,?)")
        .bind(&identifier)
        .bind(format!("local:{email}"))
        .bind(&email)
        .bind(encoded)
        .bind(login)
        .bind(current)
        .bind(current)
        .execute(&mut *transaction)
        .await
        .map_err(db_error)?;
    repository::audit(
        &mut transaction,
        "users.create",
        Some(&identifier),
        serde_json::json!({"email":email}),
    )
    .await?;
    let user = safe_user(&mut transaction, &identifier).await?;
    transaction.commit().await.map_err(db_error)?;
    Ok(user)
}

pub async fn update_user(
    pool: &SqlitePool,
    user_id: &str,
    email: &str,
    password: Option<&str>,
) -> Result<SafeUser, AppError> {
    let email = normalize_email(email)?;
    let encoded = match password {
        Some(password) => {
            validate_password(password)?;
            Some(password_hash(password.to_owned()).await?)
        }
        None => None,
    };
    let mut transaction = repository::begin(pool).await?;
    let duplicate: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM saas_users WHERE lower(email)=? AND id<>?)",
    )
    .bind(&email)
    .bind(user_id)
    .fetch_one(&mut *transaction)
    .await
    .map_err(db_error)?;
    if duplicate {
        return Err(invalid(
            "saas.user_exists",
            "A user with this email already exists",
        ));
    }
    let changed = sqlx::query(
        "UPDATE saas_users SET email=?,password_hash=COALESCE(?,password_hash),updated_at=? WHERE id=?",
    )
    .bind(&email)
    .bind(&encoded)
    .bind(repository::now())
    .bind(user_id)
    .execute(&mut *transaction)
    .await
    .map_err(db_error)?
    .rows_affected();
    if changed == 0 {
        return Err(invalid("saas.user_not_found", "User does not exist"));
    }
    if encoded.is_some() {
        sqlx::query("UPDATE saas_sessions SET revoked_at=? WHERE user_id=? AND revoked_at IS NULL")
            .bind(repository::now())
            .bind(user_id)
            .execute(&mut *transaction)
            .await
            .map_err(db_error)?;
    }
    repository::audit(
        &mut transaction,
        "users.update",
        Some(user_id),
        serde_json::json!({"email":email,"passwordChanged":password.is_some()}),
    )
    .await?;
    let user = safe_user(&mut transaction, user_id).await?;
    transaction.commit().await?;
    Ok(user)
}

pub async fn password_login(
    pool: &SqlitePool,
    email: &str,
    password: &str,
    request_origin: &str,
) -> Result<LoginResult, AppError> {
    let email = normalize_email(email)?;
    if password.is_empty() || password.len() > 512 {
        return Err(invalid(
            "saas.credentials_invalid",
            "Email or password is incorrect",
        ));
    }
    let mut transaction = repository::begin(pool).await?;
    let config = config::require_enabled(&mut transaction).await?;
    if !config.password_login_enabled {
        return Err(invalid(
            "saas.password_login_disabled",
            "Email and password sign-in is disabled",
        ));
    }
    let origin = if config.public_base_url.is_empty() {
        config::validate_public_base_url(request_origin)?
            .origin()
            .ascii_serialization()
    } else {
        if request_origin != config.public_base_url {
            return Err(invalid("saas.origin", "Sign-in origin is invalid"));
        }
        config.public_base_url.clone()
    };
    let row: Option<(String, String, String)> =
        sqlx::query_as("SELECT id,password_hash,status FROM saas_users WHERE lower(email)=?")
            .bind(&email)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(db_error)?;
    let Some((user_id, encoded, status)) = row else {
        return Err(invalid(
            "saas.credentials_invalid",
            "Email or password is incorrect",
        ));
    };
    if status != "active" {
        return Err(invalid(
            "saas.user_banned",
            "This user account is suspended",
        ));
    }
    if !password_matches(password.to_owned(), encoded).await {
        return Err(invalid(
            "saas.credentials_invalid",
            "Email or password is incorrect",
        ));
    }
    let result = issue_session(&mut transaction, &origin, &user_id).await?;
    transaction.commit().await.map_err(db_error)?;
    Ok(result)
}

async fn session(
    pool: &SqlitePool,
    token: &str,
    csrf_origin: Option<(&str, &str)>,
) -> Result<SessionPrincipal, AppError> {
    if !token.starts_with("ss-saas-") || token.len() != 72 {
        return Err(invalid("saas.unauthorized", "Sign-in is required"));
    }
    let mut transaction = pool.begin().await.map_err(db_error)?;
    config::require_enabled(&mut transaction).await?;
    let row: Option<(String,String,String,i64)> = sqlx::query_as("SELECT user_id,csrf_hash,origin,expires_at FROM saas_sessions WHERE token_hash=? AND revoked_at IS NULL AND expires_at>?")
        .bind(hash_secret(token)).bind(repository::now()).fetch_optional(&mut *transaction).await.map_err(db_error)?;
    let (user_id, stored_csrf, origin, expires_at) =
        row.ok_or_else(|| invalid("saas.unauthorized", "Session has expired"))?;
    if let Some((csrf, request_origin)) = csrf_origin {
        if csrf.len() != 64
            || !constant_time_equal(hash_secret(csrf).as_bytes(), stored_csrf.as_bytes())
            || request_origin != origin
        {
            return Err(invalid(
                "saas.csrf",
                "Request origin or CSRF token is invalid",
            ));
        }
    }
    repository::require_user(&mut transaction, &user_id).await?;
    let user = safe_user(&mut transaction, &user_id).await?;
    transaction.commit().await.map_err(db_error)?;
    Ok(SessionPrincipal {
        user,
        expires_at: repository::timestamp(expires_at),
        csrf_token: csrf_for_session(token),
    })
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

pub async fn authenticate_session(
    pool: &SqlitePool,
    token: &str,
) -> Result<SessionPrincipal, AppError> {
    session(pool, token, None).await
}

pub async fn validate_csrf(
    pool: &SqlitePool,
    token: &str,
    csrf: &str,
    origin: &str,
) -> Result<SessionPrincipal, AppError> {
    session(pool, token, Some((csrf, origin))).await
}

pub async fn logout(pool: &SqlitePool, token: &str) -> Result<(), AppError> {
    sqlx::query("UPDATE saas_sessions SET revoked_at=? WHERE token_hash=? AND revoked_at IS NULL")
        .bind(repository::now())
        .bind(hash_secret(token))
        .execute(pool)
        .await
        .map_err(db_error)?;
    Ok(())
}

#[cfg(test)]
mod tests;

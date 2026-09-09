use crate::error::AppError;
use crate::saas::{self, auth, config, repository};
use crate::web::router::WebServerContext;
use axum::extract::{DefaultBodyLimit, Path, Query, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode, Uri};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::atomic::Ordering;

const SESSION_COOKIE: &str = "aiswitch_saas_session";
const OAUTH_COOKIE: &str = "aiswitch_saas_oauth";

pub fn routes() -> Router<WebServerContext> {
    Router::new()
        .route("/", get(root))
        .route("/index.html", get(root))
        .route("/api/saas/public/config", get(public_config))
        .route("/api/saas/auth/github", get(github_start))
        .route("/api/saas/auth/github/callback", get(github_callback))
        .route("/api/saas/auth/session", get(session))
        .route("/api/saas/auth/password", post(password_login))
        .route("/api/saas/auth/logout", post(logout))
        .route("/v1/usage", get(external_usage))
        .route("/v1/subscriptions", get(external_subscriptions))
        .route("/api/saas/user/:operation", post(user))
        .route("/api/saas/admin/:operation", post(admin))
        .layer(DefaultBodyLimit::max(256 * 1024))
        .layer(middleware::from_fn(no_store))
}

async fn no_store(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}

pub fn error_response(error: AppError) -> Response {
    let code = error.code();
    let status = if code.contains("unauthorized")
        || code.contains("key_invalid")
        || code.contains("invalid_key")
        || code.contains("session")
    {
        StatusCode::UNAUTHORIZED
    } else if code.contains("csrf")
        || code.contains("forbidden")
        || code.contains("banned")
        || code.contains("registration")
        || code.contains("password_login_disabled")
        || code.contains("account_age")
    {
        StatusCode::FORBIDDEN
    } else if code.contains("rate_limited") || code.contains("concurrency") {
        StatusCode::TOO_MANY_REQUESTS
    } else if code == "saas.disabled" || code.contains("not_found") || code.contains("operation") {
        StatusCode::NOT_FOUND
    } else if code.contains("balance") || code.contains("quota") {
        StatusCode::PAYMENT_REQUIRED
    } else if code.contains("unavailable") || code.contains("database") {
        StatusCode::SERVICE_UNAVAILABLE
    } else {
        StatusCode::BAD_REQUEST
    };
    (
        status,
        Json(json!({"code":code,"message":error.to_string()})),
    )
        .into_response()
}

fn cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|part| {
            let (key, value) = part.trim().split_once('=')?;
            (key == name && value.len() <= 256).then(|| value.to_string())
        })
}

fn cookie_header(
    name: &str,
    value: &str,
    secure: bool,
    max_age: i64,
) -> Result<HeaderValue, AppError> {
    let secure = if secure { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{name}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure}"
    ))
    .map_err(|_| repository::invalid("saas.session_cookie", "Could not create secure session"))
}

async fn root(State(context): State<WebServerContext>, uri: Uri) -> Response {
    if let Err(error) = context.state.saas.initialize(&context.state.pool).await {
        return error_response(error);
    }
    match config::load(&context.state.pool).await {
        Ok(config) if config.enabled => {
            crate::web::router::static_fallback(State(context), uri).await
        }
        Ok(_) => (StatusCode::FOUND, [(header::LOCATION, "/ai-switch-admin")]).into_response(),
        Err(error) => error_response(error),
    }
}

async fn public_config(State(context): State<WebServerContext>) -> Response {
    if let Err(error) = context.state.saas.initialize(&context.state.pool).await {
        return error_response(error);
    }
    match config::public_config(&context.state.pool).await {
        Ok(config) => Json(config).into_response(),
        Err(error) => error_response(error),
    }
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}

async fn external_usage(State(context): State<WebServerContext>, headers: HeaderMap) -> Response {
    let result = async {
        context.state.saas.initialize(&context.state.pool).await?;
        let key = bearer(&headers).ok_or_else(|| {
            repository::invalid("saas.unauthorized", "Account security key is required")
        })?;
        let user_id = saas::domain::external::authenticate(&context.state.pool, key).await?;
        saas::domain::external::usage(&context.state.pool, &user_id).await
    }
    .await;
    result
        .map(Json)
        .map(IntoResponse::into_response)
        .unwrap_or_else(error_response)
}

async fn external_subscriptions(
    State(context): State<WebServerContext>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        context.state.saas.initialize(&context.state.pool).await?;
        let key = bearer(&headers).ok_or_else(|| {
            repository::invalid("saas.unauthorized", "Account security key is required")
        })?;
        let user_id = saas::domain::external::authenticate(&context.state.pool, key).await?;
        saas::domain::external::subscriptions(&context.state.pool, &user_id).await
    }
    .await;
    result
        .map(Json)
        .map(IntoResponse::into_response)
        .unwrap_or_else(error_response)
}

#[derive(Deserialize)]
struct GitHubStart {
    invite: Option<String>,
}

async fn github_start(
    State(context): State<WebServerContext>,
    Query(query): Query<GitHubStart>,
) -> Response {
    let result = async {
        context.state.saas.initialize(&context.state.pool).await?;
        context.state.saas.allow("oauth:start".into(), 60).await?;
        let config = config::load(&context.state.pool).await?;
        let start = auth::start_oauth(&context.state.pool, query.invite.as_deref()).await?;
        let mut response = Redirect::temporary(&start.authorize_url).into_response();
        response.headers_mut().append(
            header::SET_COOKIE,
            cookie_header(
                OAUTH_COOKIE,
                &start.binding,
                config.public_base_url.starts_with("https://"),
                600,
            )?,
        );
        Ok::<Response, AppError>(response)
    }
    .await;
    result.unwrap_or_else(error_response)
}

#[derive(Deserialize)]
struct Callback {
    code: Option<String>,
    state: Option<String>,
}

async fn github_callback(
    State(context): State<WebServerContext>,
    headers: HeaderMap,
    Query(query): Query<Callback>,
) -> Response {
    let result = async {
        context.state.saas.initialize(&context.state.pool).await?;
        context
            .state
            .saas
            .allow("oauth:callback".into(), 90)
            .await?;
        let binding = cookie(&headers, OAUTH_COOKIE).ok_or_else(|| {
            repository::invalid("saas.oauth_state", "The sign-in request has expired")
        })?;
        let code = query
            .code
            .filter(|value| value.len() <= 2048)
            .ok_or_else(|| {
                repository::invalid("saas.oauth_code", "GitHub sign-in was not completed")
            })?;
        let state = query
            .state
            .filter(|value| value.len() <= 256)
            .ok_or_else(|| {
                repository::invalid("saas.oauth_state", "The sign-in request is invalid")
            })?;
        let client = reqwest::Client::builder()
            .user_agent("ai-switch-saas")
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| {
                repository::invalid("saas.github_unavailable", "Could not connect to GitHub")
            })?;
        let login = auth::callback(
            &context.state.pool,
            &auth::GitHubClient::new(client),
            &code,
            &state,
            &binding,
        )
        .await?;
        let config = config::load(&context.state.pool).await?;
        let secure = config.public_base_url.starts_with("https://");
        let expires = chrono::DateTime::parse_from_rfc3339(&login.expires_at)
            .map_err(|_| repository::invalid("saas.session", "Invalid session expiration"))?;
        let max_age = (expires.timestamp() - chrono::Utc::now().timestamp()).max(0);
        let mut response = Redirect::to("/").into_response();
        response.headers_mut().append(
            header::SET_COOKIE,
            cookie_header(SESSION_COOKIE, &login.session_token, secure, max_age)?,
        );
        response.headers_mut().append(
            header::SET_COOKIE,
            cookie_header(OAUTH_COOKIE, "", secure, 0)?,
        );
        Ok::<Response, AppError>(response)
    }
    .await;
    result.unwrap_or_else(error_response)
}

async fn session(State(context): State<WebServerContext>, headers: HeaderMap) -> Response {
    let result = async {
        context.state.saas.initialize(&context.state.pool).await?;
        let Some(token) = cookie(&headers, SESSION_COOKIE) else {
            return Ok(json!({"user":null,"csrfToken":null}));
        };
        let principal = auth::authenticate_session(&context.state.pool, &token).await?;
        Ok::<Value, AppError>(json!({"user":principal.user,"csrfToken":principal.csrf_token}))
    }
    .await;
    match result {
        Ok(value) => Json(value).into_response(),
        Err(error) => error_response(error),
    }
}

#[derive(Deserialize)]
struct PasswordLogin {
    email: String,
    password: String,
}

async fn password_login(
    State(context): State<WebServerContext>,
    headers: HeaderMap,
    Json(payload): Json<PasswordLogin>,
) -> Response {
    let result = async {
        context.state.saas.initialize(&context.state.pool).await?;
        context
            .state
            .saas
            .allow("password:login".into(), 30)
            .await?;
        let origin = headers
            .get(header::ORIGIN)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
            .or_else(|| {
                headers
                    .get(header::HOST)
                    .and_then(|value| value.to_str().ok())
                    .map(|host| format!("http://{host}"))
            })
            .unwrap_or_default();
        let login = auth::password_login(
            &context.state.pool,
            &payload.email,
            &payload.password,
            &origin,
        )
        .await?;
        let config = config::load(&context.state.pool).await?;
        let expires = chrono::DateTime::parse_from_rfc3339(&login.expires_at)
            .map_err(|_| repository::invalid("saas.session", "Invalid session expiration"))?;
        let max_age = (expires.timestamp() - chrono::Utc::now().timestamp()).max(0);
        let mut response =
            Json(json!({"user":login.user,"csrfToken":login.csrf_token})).into_response();
        response.headers_mut().append(
            header::SET_COOKIE,
            cookie_header(
                SESSION_COOKIE,
                &login.session_token,
                config.public_base_url.starts_with("https://"),
                max_age,
            )?,
        );
        Ok::<Response, AppError>(response)
    }
    .await;
    result.unwrap_or_else(error_response)
}

async fn validated_user(
    context: &WebServerContext,
    headers: &HeaderMap,
) -> Result<(String, auth::SessionPrincipal), AppError> {
    context.state.saas.initialize(&context.state.pool).await?;
    let token = cookie(headers, SESSION_COOKIE)
        .ok_or_else(|| repository::invalid("saas.unauthorized", "Sign in to continue"))?;
    let csrf = headers
        .get("x-saas-csrf")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let principal = auth::validate_csrf(&context.state.pool, &token, csrf, origin).await?;
    Ok((token, principal))
}

async fn logout(State(context): State<WebServerContext>, headers: HeaderMap) -> Response {
    let result = async {
        let (token, _) = validated_user(&context, &headers).await?;
        auth::logout(&context.state.pool, &token).await?;
        let config = config::load(&context.state.pool).await?;
        let mut response = Json(json!({"ok":true})).into_response();
        response.headers_mut().append(
            header::SET_COOKIE,
            cookie_header(
                SESSION_COOKIE,
                "",
                config.public_base_url.starts_with("https://"),
                0,
            )?,
        );
        Ok::<Response, AppError>(response)
    }
    .await;
    result.unwrap_or_else(error_response)
}

async fn user(
    State(context): State<WebServerContext>,
    Path(operation): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let result = async {
        let (_, principal) = validated_user(&context, &headers).await?;
        saas::user_command(&context.state, &principal.user.id, &operation, payload).await
    }
    .await;
    match result {
        Ok(value) => Json(value).into_response(),
        Err(error) => error_response(error),
    }
}

async fn admin(
    State(context): State<WebServerContext>,
    Path(operation): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    if !context.sensitive_command_gate.load(Ordering::Acquire) {
        return StatusCode::NOT_FOUND.into_response();
    }
    if !crate::web::auth::is_authorized(&headers, &context.token) {
        return error_response(repository::invalid(
            "saas.unauthorized",
            "Administrator authentication is required",
        ));
    }
    match saas::admin_command(&context.state, &operation, payload).await {
        Ok(value) => Json(value).into_response(),
        Err(error) => error_response(error),
    }
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn saas_admin(
    state: tauri::State<'_, crate::app_state::AppState>,
    operation: String,
    payload: Option<Value>,
) -> Result<Value, crate::error::ApiError> {
    saas::admin_command(&state, &operation, payload.unwrap_or_else(|| json!({})))
        .await
        .map_err(Into::into)
}

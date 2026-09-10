mod usage;

use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::platform::PlatformId;
use crate::saas::{billing, domain::groups, logs::LogRecord, repository, SaasRuntime};
use crate::services::route_proxy_service::{
    build_proxy_state, extract_inbound_api_key, proxy_handler, ProxyAppState,
};
use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderMap, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures_util::StreamExt;
use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::time::{Duration, Instant};
use usage::UsageObserver;

pub(crate) async fn handle(
    state: &AppState,
    method: Method,
    headers: HeaderMap,
    uri: Uri,
    body: Body,
) -> Response {
    let proxy = build_proxy_state(state.pool.clone(), &state.route_proxy);
    let mut response =
        match execute(&state.pool, &state.saas, proxy, method, headers, uri, body).await {
            Ok(response) => response,
            Err(error) => crate::saas::transport::error_response(error),
        };
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
}

async fn execute(
    pool: &SqlitePool,
    runtime: &SaasRuntime,
    proxy: ProxyAppState,
    method: Method,
    mut headers: HeaderMap,
    uri: Uri,
    body: Body,
) -> Result<Response, AppError> {
    runtime.initialize(pool).await?;
    let key = extract_inbound_api_key(&headers, None)
        .ok_or_else(|| repository::invalid("saas.invalid_key", "An API key is required"))?;
    let principal = billing::authenticate_key(pool, &key).await?;
    runtime
        .allow(format!("inference:{}", principal.user_id), 600)
        .await?;
    let path = uri.path().trim_end_matches('/').to_string();
    if matches!(path.as_str(), "/v1/models" | "/models") && method == Method::GET {
        let models: Vec<String> = sqlx::query_scalar(
            "SELECT model FROM saas_group_models WHERE group_id=? ORDER BY model",
        )
        .bind(&principal.group_id)
        .fetch_all(pool)
        .await
        .map_err(repository::db_error)?;
        let mut available = Vec::new();
        for model in models {
            if !groups::permitted_accounts_for_model(pool, &principal.group_id, &model)
                .await?
                .is_empty()
            {
                available.push(json!({"id":model,"object":"model","owned_by":"ai-switch"}));
            }
        }
        return Ok(Json(json!({"object":"list","data":available})).into_response());
    }
    let anthropic = matches!(path.as_str(), "/v1/messages" | "/messages");
    let responses = matches!(path.as_str(), "/v1/responses" | "/responses");
    let chat = matches!(path.as_str(), "/v1/chat/completions" | "/chat/completions");
    let images = matches!(
        path.as_str(),
        "/v1/images/generations" | "/images/generations"
    );
    if method != Method::POST
        || !(anthropic || responses || chat || images)
        || (principal.platform == "claude" && !anthropic)
        || (principal.platform == "gemini" && !images)
        || (principal.platform == "codex" && anthropic)
    {
        return Err(repository::invalid(
            "saas.endpoint_not_allowed",
            "This endpoint is unavailable for the API key",
        ));
    }
    let status = runtime
        .logs
        .status()
        .await
        .map_err(crate::saas::log_error)?;
    if !status.accepting {
        return Err(repository::invalid(
            "saas.logs_unavailable",
            "Request logging is unavailable; try again later",
        ));
    }
    let body = axum::body::to_bytes(body, 2 * 1024 * 1024)
        .await
        .map_err(|_| {
            repository::invalid("saas.request_limits", "Request body exceeds the limit")
        })?;
    let mut payload: Value = serde_json::from_slice(&body).map_err(|_| {
        repository::invalid("saas.validation", "Request must contain a JSON object")
    })?;
    let object = payload.as_object_mut().ok_or_else(|| {
        repository::invalid("saas.validation", "Request must contain a JSON object")
    })?;
    let model = object
        .get("model")
        .and_then(Value::as_str)
        .filter(|model| !model.is_empty())
        .ok_or_else(|| repository::invalid("saas.model_not_allowed", "A model is required"))?
        .to_string();
    if images {
        return execute_image_request(
            pool, runtime, proxy, principal, headers, uri, payload, model, &path,
        )
        .await;
    }
    let default_output: i64 =
        sqlx::query_scalar("SELECT max_output_tokens FROM saas_group_settings WHERE group_id=?")
            .bind(&principal.group_id)
            .fetch_optional(pool)
            .await
            .map_err(repository::db_error)?
            .ok_or_else(|| {
                repository::invalid("saas.group_unavailable", "Group pricing is not configured")
            })?;
    let output_field = if responses {
        "max_output_tokens"
    } else if chat && object.contains_key("max_completion_tokens") {
        "max_completion_tokens"
    } else {
        "max_tokens"
    };
    let output_limit = match object.get(output_field) {
        Some(value) => value.as_i64().ok_or_else(|| {
            repository::invalid("saas.request_limits", "Output limit must be an integer")
        })?,
        None => default_output,
    };
    let streaming = object
        .get("stream")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    for field in ["max_output_tokens", "max_completion_tokens", "max_tokens"] {
        object.remove(field);
    }
    object.insert(output_field.into(), json!(output_limit));
    if streaming && chat {
        object.insert("stream_options".into(), json!({"include_usage":true}));
    }
    let reservation = billing::reserve(
        pool,
        &principal,
        &model,
        body.len() as i64 + 64,
        output_limit,
    )
    .await?;
    object.insert("model".into(), json!(reservation.upstream_model));
    let deadline =
        tokio::time::Instant::now() + Duration::from_secs(reservation.timeout_seconds as u64);
    let mut completion = Completion {
        pool: pool.clone(),
        runtime: runtime.clone(),
        request_id: Some(reservation.request_id.clone()),
        user_id: reservation.user_id.clone(),
        key_id: reservation.key_id.clone(),
        group_id: reservation.group_id.clone(),
        platform: reservation.platform.clone(),
        model,
        path: path.clone(),
        started: Instant::now(),
        created_at: chrono::Utc::now(),
        status: StatusCode::BAD_GATEWAY.as_u16(),
        observer: UsageObserver::new(streaming, anthropic),
    };
    let proxy = proxy.with_access_scope(
        PlatformId::parse(&reservation.platform)?,
        reservation.credential_ids.into_iter().collect(),
    );
    headers.remove(header::COOKIE);
    headers.remove("x-saas-csrf");
    headers.remove("x-ai-switch-test-trace-id");
    headers.remove(header::CONTENT_LENGTH);
    headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
    let upstream = tokio::time::timeout_at(
        deadline,
        proxy_handler(
            State(proxy),
            method,
            headers,
            uri,
            Body::from(payload.to_string()),
        ),
    )
    .await;
    let response = match upstream {
        Ok(response) => response,
        Err(_) => {
            completion.status = 504;
            completion.complete(false).await;
            return Ok((StatusCode::GATEWAY_TIMEOUT,Json(json!({"error":{"code":"saas.upstream_timeout","message":"Upstream request timed out"}}))).into_response());
        }
    };
    completion.status = response.status().as_u16();
    let (mut parts, body) = response.into_parts();
    parts.headers.remove(header::SET_COOKIE);
    parts
        .headers
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    parts
        .headers
        .insert("x-saas-request-id", reservation.request_id.parse().unwrap());
    let stream = futures_util::stream::unfold(
        (body.into_data_stream(), completion, deadline),
        |(mut source, mut completion, deadline)| async move {
            if completion.request_id.is_none() {
                return None;
            }
            match tokio::time::timeout_at(deadline, source.next()).await {
                Ok(Some(Ok(bytes))) => {
                    completion.observer.observe(&bytes);
                    Some((
                        Ok::<_, std::io::Error>(bytes),
                        (source, completion, deadline),
                    ))
                }
                Ok(None) => {
                    completion.complete(true).await;
                    None
                }
                Ok(Some(Err(_))) | Err(_) => {
                    completion.complete(false).await;
                    Some((
                        Err(std::io::Error::other("Upstream response interrupted")),
                        (source, completion, deadline),
                    ))
                }
            }
        },
    );
    Ok(Response::from_parts(parts, Body::from_stream(stream)))
}

async fn execute_image_request(
    pool: &SqlitePool,
    runtime: &SaasRuntime,
    proxy: ProxyAppState,
    principal: billing::ApiPrincipal,
    mut headers: HeaderMap,
    uri: Uri,
    mut payload: Value,
    model: String,
    path: &str,
) -> Result<Response, AppError> {
    let object = payload.as_object_mut().ok_or_else(|| {
        repository::invalid("saas.validation", "Request must contain a JSON object")
    })?;
    let count = object
        .get("n")
        .map(|value| {
            value.as_i64().ok_or_else(|| {
                repository::invalid("saas.request_limits", "Image count must be an integer")
            })
        })
        .transpose()?
        .unwrap_or(1);
    let reservation = billing::reserve_image(pool, &principal, &model, count).await?;
    object.insert("model".into(), json!(reservation.upstream_model));
    let request_id = reservation.request_id.clone();
    let deadline =
        tokio::time::Instant::now() + Duration::from_secs(reservation.timeout_seconds as u64);
    let proxy = proxy.with_access_scope(
        PlatformId::parse(&reservation.platform)?,
        reservation.credential_ids.into_iter().collect(),
    );
    headers.remove(header::COOKIE);
    headers.remove("x-saas-csrf");
    headers.remove("x-ai-switch-test-trace-id");
    headers.remove(header::CONTENT_LENGTH);
    headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
    let started = Instant::now();
    let created_at = chrono::Utc::now();
    let upstream = tokio::time::timeout_at(
        deadline,
        proxy_handler(
            State(proxy),
            Method::POST,
            headers,
            uri,
            Body::from(payload.to_string()),
        ),
    )
    .await;
    let response = match upstream {
        Ok(response) => response,
        Err(_) => {
            let _ = billing::settle_image(pool, &request_id, None, true).await;
            return Ok((StatusCode::GATEWAY_TIMEOUT, Json(json!({"error":{"code":"saas.upstream_timeout","message":"Upstream request timed out"}}))).into_response());
        }
    };
    let status = response.status();
    let (mut parts, body) = response.into_parts();
    let bytes = axum::body::to_bytes(body, 128 * 1024 * 1024)
        .await
        .map_err(|_| repository::invalid("saas.upstream_body", "Could not read image response"))?;
    let success_count = if status.is_success() {
        serde_json::from_slice::<Value>(&bytes)
            .ok()
            .and_then(|value| {
                value
                    .get("data")
                    .and_then(Value::as_array)
                    .map(|items| items.len() as i64)
            })
    } else {
        Some(0)
    };
    let settlement =
        billing::settle_image(pool, &request_id, success_count, status.is_success()).await?;
    let record = LogRecord {
        request_id: request_id.clone(),
        user_id: reservation.user_id,
        key_id: reservation.key_id,
        group_id: reservation.group_id,
        platform: reservation.platform,
        model,
        endpoint: path.to_string(),
        created_at,
        utc_offset_minutes: chrono::Local::now().offset().local_minus_utc() / 60,
        duration_ms: started.elapsed().as_millis().min(i64::MAX as u128) as i64,
        status: status.as_u16(),
        amount_usd_micros: settlement.price_usd_micros.unwrap_or(0),
        settlement_status: settlement.status,
        error_code: if status.is_success() {
            None
        } else {
            Some("upstream_rejected".into())
        },
        ..Default::default()
    };
    if runtime.logs.enqueue(record).await.is_err() {
        eprintln!("SaaS request log queue rejected a completed image request");
    }
    parts.headers.remove(header::SET_COOKIE);
    parts
        .headers
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    parts
        .headers
        .insert("x-saas-request-id", request_id.parse().unwrap());
    Ok(Response::from_parts(parts, Body::from(bytes)))
}

struct Completion {
    pool: SqlitePool,
    runtime: SaasRuntime,
    request_id: Option<String>,
    user_id: String,
    key_id: String,
    group_id: String,
    platform: String,
    model: String,
    path: String,
    started: Instant,
    created_at: chrono::DateTime<chrono::Utc>,
    status: u16,
    observer: UsageObserver,
}

impl Completion {
    async fn complete(&mut self, eof: bool) {
        let Some(request_id) = self.request_id.take() else {
            return;
        };
        let usage = self.observer.finish(eof);
        let definite_rejection = eof && (400..500).contains(&self.status) && usage.is_none();
        let result =
            billing::settle(&self.pool, &request_id, usage.clone(), !definite_rejection).await;
        let (amount, status) = match result {
            Ok(settlement) => (settlement.price_usd_micros.unwrap_or(0), settlement.status),
            Err(_) => {
                eprintln!("SaaS settlement requires reconciliation: {request_id}");
                (0, "pending_review".into())
            }
        };
        let usage = usage.unwrap_or_default();
        let record = LogRecord {
            request_id,
            user_id: self.user_id.clone(),
            key_id: self.key_id.clone(),
            group_id: self.group_id.clone(),
            platform: self.platform.clone(),
            model: self.model.clone(),
            endpoint: self.path.clone(),
            created_at: self.created_at,
            utc_offset_minutes: chrono::Local::now().offset().local_minus_utc() / 60,
            duration_ms: self.started.elapsed().as_millis().min(i64::MAX as u128) as i64,
            status: self.status,
            input_tokens: usage.input_tokens,
            cache_read_tokens: usage.cache_read_tokens,
            cache_write_tokens: usage.cache_write_tokens,
            output_tokens: usage.output_tokens,
            amount_usd_micros: amount,
            settlement_status: status,
            error_code: if !eof {
                Some("upstream_interrupted".into())
            } else if self.status >= 400 {
                Some("upstream_rejected".into())
            } else {
                None
            },
            ..Default::default()
        };
        if self.runtime.logs.enqueue(record).await.is_err() {
            eprintln!("SaaS request log queue rejected a completed request");
        }
    }
}

impl Drop for Completion {
    fn drop(&mut self) {
        if self.request_id.is_none() {
            return;
        }
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            self.request_id = None;
            return;
        };
        let pending = Self {
            pool: self.pool.clone(),
            runtime: self.runtime.clone(),
            request_id: self.request_id.take(),
            user_id: self.user_id.clone(),
            key_id: self.key_id.clone(),
            group_id: self.group_id.clone(),
            platform: self.platform.clone(),
            model: self.model.clone(),
            path: self.path.clone(),
            started: self.started,
            created_at: self.created_at,
            status: self.status,
            observer: std::mem::replace(&mut self.observer, UsageObserver::new(false, false)),
        };
        handle.spawn(async move {
            let mut pending = pending;
            pending.complete(false).await;
        });
    }
}

#[cfg(test)]
mod tests;

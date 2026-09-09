use super::*;
use crate::saas::{config, repository};
use chrono::Duration;
use serde_json::json;

#[test]
fn github_account_age_has_an_exact_365_day_boundary() {
    let now = Utc::now();
    assert!(!eligible_at(now - Duration::days(364), now));
    assert!(eligible_at(now - Duration::days(365), now));
    assert!(!eligible_at(now + Duration::seconds(1), now));
}

#[tokio::test]
async fn oauth_state_is_browser_bound_single_use_and_uses_s256_pkce() {
    let pool = repository::test_pool().await;
    repository::test_enable(&pool).await;
    let started = start_oauth(&pool, None).await.unwrap();
    let url = url::Url::parse(&started.authorize_url).unwrap();
    let params: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    assert_eq!(params["code_challenge_method"], "S256");
    assert!(consume_state(&pool, &params["state"], "different-browser")
        .await
        .is_err());
    let stored = consume_state(&pool, &params["state"], &started.binding)
        .await
        .unwrap();
    assert_eq!(pkce_challenge(&stored.verifier), params["code_challenge"]);
    assert!(consume_state(&pool, &params["state"], &started.binding)
        .await
        .is_err());
    let expired = start_oauth(&pool, None).await.unwrap();
    let expired_url = url::Url::parse(&expired.authorize_url).unwrap();
    let expired_state = expired_url
        .query_pairs()
        .find(|(key, _)| key == "state")
        .unwrap()
        .1
        .into_owned();
    sqlx::query("UPDATE saas_oauth_states SET expires_at=0")
        .execute(&pool)
        .await
        .unwrap();
    assert!(consume_state(&pool, &expired_state, &expired.binding)
        .await
        .is_err());
}

#[tokio::test]
async fn existing_identity_survives_registration_closure_but_bans_revoke_sessions() {
    let pool = repository::test_pool().await;
    repository::test_enable(&pool).await;
    let profile = GitHubProfile {
        id: 42,
        login: "first-name".into(),
        avatar_url: None,
        created_at: Utc::now() - Duration::days(400),
    };
    let first = complete_login(&pool, profile.clone(), None).await.unwrap();
    let stored: (String, String) = sqlx::query_as("SELECT token_hash,csrf_hash FROM saas_sessions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_ne!(stored.0, first.session_token);
    assert_ne!(stored.1, first.csrf_token);
    assert!(
        validate_csrf(&pool, &first.session_token, "wrong", "https://saas.example")
            .await
            .is_err()
    );
    assert!(validate_csrf(
        &pool,
        &first.session_token,
        &first.csrf_token,
        "https://foreign.example"
    )
    .await
    .is_err());
    assert!(validate_csrf(
        &pool,
        &first.session_token,
        &first.csrf_token,
        "https://saas.example"
    )
    .await
    .is_ok());
    config::save(&pool, serde_json::json!({"registrationEnabled":false}))
        .await
        .unwrap();
    let mut renamed = profile.clone();
    renamed.login = "renamed".into();
    assert_eq!(
        complete_login(&pool, renamed, None).await.unwrap().user.id,
        first.user.id
    );
    let mut new_user = profile;
    new_user.id = 43;
    assert!(complete_login(&pool, new_user, None).await.is_err());
    crate::saas::domain::admin(
        &pool,
        "users.status",
        serde_json::json!({"id":first.user.id,"status":"banned"}),
    )
    .await
    .unwrap();
    assert!(authenticate_session(&pool, &first.session_token)
        .await
        .is_err());
    assert!(complete_login(
        &pool,
        GitHubProfile {
            id: 42,
            login: "renamed".into(),
            avatar_url: None,
            created_at: Utc::now() - Duration::days(400)
        },
        None,
    )
    .await
    .is_err());
}

#[tokio::test]
async fn real_http_oauth_exchange_sends_pkce_and_never_stores_github_tokens() {
    use axum::{
        extract::{Form, State},
        routing::{get, post},
        Json, Router,
    };
    use std::collections::HashMap;
    let pool = repository::test_pool().await;
    let secrets = config::tests::MemorySecrets::default();
    config::unlock(&pool, "ai-switch-ok").await.unwrap();
    config::save_with_secret_store(&pool, serde_json::json!({"enabled":true,"githubClientId":"client-id","githubClientSecret":"github-private-secret","exchangeRateMicros":7_000_000,"publicBaseUrl":"https://saas.example"}), &secrets).await.unwrap();
    let start = start_oauth(&pool, None).await.unwrap();
    let params: HashMap<String, String> = url::Url::parse(&start.authorize_url)
        .unwrap()
        .query_pairs()
        .into_owned()
        .collect();
    async fn token(
        State(challenge): State<String>,
        Form(form): Form<HashMap<String, String>>,
    ) -> Json<serde_json::Value> {
        if form.get("code").map(String::as_str) != Some("valid-code")
            || form.get("client_secret").map(String::as_str) != Some("github-private-secret")
            || form.get("client_id").map(String::as_str) != Some("client-id")
            || form.get("redirect_uri").map(String::as_str)
                != Some("https://saas.example/api/saas/auth/github/callback")
            || form
                .get("code_verifier")
                .map(|verifier| pkce_challenge(verifier))
                != Some(challenge)
        {
            return Json(serde_json::json!({"error":"invalid_request"}));
        }
        Json(serde_json::json!({"access_token":"temporary-github-token","token_type":"bearer"}))
    }
    async fn profile(headers: axum::http::HeaderMap) -> Json<serde_json::Value> {
        if headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            != Some("Bearer temporary-github-token")
        {
            return Json(serde_json::json!({"error":"unauthorized"}));
        }
        Json(
            serde_json::json!({"id":101,"login":"real-http-user","created_at":"2020-01-01T00:00:00Z","avatar_url":"https://avatars.githubusercontent.com/u/101"}),
        )
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let router = Router::new()
        .route("/token", post(token))
        .route("/user", get(profile))
        .with_state(params["code_challenge"].clone());
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let github = GitHubClient {
        client: reqwest::Client::new(),
        token_url: format!("http://{address}/token"),
        user_url: format!("http://{address}/user"),
    };
    let result = callback_with_secret_store(
        &pool,
        &github,
        "valid-code",
        &params["state"],
        &start.binding,
        &secrets,
    )
    .await
    .unwrap();
    assert_eq!(result.user.github_id, "101");
    assert_eq!(result.user.login, "real-http-user");
    assert!(authenticate_session(&pool, &result.session_token)
        .await
        .is_ok());
    assert!(callback_with_secret_store(
        &pool,
        &github,
        "valid-code",
        &params["state"],
        &start.binding,
        &secrets
    )
    .await
    .is_err());
    let persisted: Vec<String> = sqlx::query_scalar("SELECT value_json FROM saas_settings")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(!persisted.join("").contains("temporary-github-token"));
    let session = authenticate_session(&pool, &result.session_token)
        .await
        .unwrap();
    assert_eq!(session.csrf_token, result.csrf_token);
    logout(&pool, &result.session_token).await.unwrap();
    assert!(authenticate_session(&pool, &result.session_token)
        .await
        .is_err());
    server.abort();
}

#[tokio::test]
async fn disabling_saas_revokes_sessions_even_after_reenable() {
    let pool = repository::test_pool().await;
    repository::test_enable(&pool).await;
    let login = complete_login(
        &pool,
        GitHubProfile {
            id: 202,
            login: "returning".into(),
            avatar_url: None,
            created_at: Utc::now() - Duration::days(400),
        },
        None,
    )
    .await
    .unwrap();
    config::save(&pool, serde_json::json!({"enabled":false}))
        .await
        .unwrap();
    assert!(authenticate_session(&pool, &login.session_token)
        .await
        .is_err());
    repository::test_enable(&pool).await;
    assert!(authenticate_session(&pool, &login.session_token)
        .await
        .is_err());
}

#[tokio::test]
async fn invite_codes_are_required_only_for_new_github_registrations() {
    let pool = repository::test_pool().await;
    repository::test_enable(&pool).await;
    config::save(
        &pool,
        json!({
            "inviteEnabled":true,
            "inviteRegistrationRequired":true
        }),
    )
    .await
    .unwrap();
    let profile = GitHubProfile {
        id: 7001,
        login: "invitee".into(),
        avatar_url: None,
        created_at: Utc::now() - Duration::days(400),
    };
    let error = match complete_login(&pool, profile.clone(), None).await {
        Err(error) => error,
        Ok(_) => panic!("GitHub registration should require an invite code"),
    };
    assert_eq!(error.code(), "saas.invite_required");
    let codes = crate::saas::domain::admin(
        &pool,
        "invites.codes.create",
        json!({"count":1,"maxUses":1}),
    )
    .await
    .unwrap();
    let invite = codes["items"][0]["code"].as_str().unwrap();
    let first = complete_login(&pool, profile.clone(), Some(invite))
        .await
        .unwrap();
    let used_count: i64 =
        sqlx::query_scalar("SELECT used_count FROM saas_invite_codes WHERE prefix=?")
            .bind(&invite[..12])
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(used_count, 1);
    let second = complete_login(&pool, profile, None).await.unwrap();
    assert_eq!(second.user.id, first.user.id);
    let used_count: i64 =
        sqlx::query_scalar("SELECT used_count FROM saas_invite_codes WHERE prefix=?")
            .bind(&invite[..12])
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(used_count, 1);
}

#[tokio::test]
async fn administrator_created_password_user_can_sign_in() {
    let pool = repository::test_pool().await;
    repository::test_enable(&pool).await;
    let created = create_password_user(&pool, " Member@Example.com ", "secret-pass")
        .await
        .unwrap();
    assert_eq!(created.email.as_deref(), Some("member@example.com"));
    assert!(created.github_id.starts_with("local:"));
    let login = password_login(
        &pool,
        "member@example.com",
        "secret-pass",
        "https://saas.example",
    )
    .await
    .unwrap();
    assert_eq!(login.user.id, created.id);
    assert!(password_login(
        &pool,
        "member@example.com",
        "wrong-pass",
        "https://saas.example"
    )
    .await
    .is_err());
    let stored: String = sqlx::query_scalar("SELECT password_hash FROM saas_users WHERE id=?")
        .bind(&created.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(stored.starts_with("$argon2"));
    assert!(!stored.contains("secret-pass"));
}

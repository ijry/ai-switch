use crate::database::repositories::updater_repository::UpdaterRepository;
use crate::error::AppError;
use crate::models::updater::{NewUpdateChannel, NewUpdateCheck, UpdateChannel, UpdateCheck};
use serde_json::Value;
use sqlx::SqlitePool;

pub struct UpdaterService;

impl UpdaterService {
    pub async fn list_update_channels(pool: &SqlitePool) -> Result<Vec<UpdateChannel>, AppError> {
        UpdaterRepository::list_channels(pool).await
    }

    pub async fn create_update_channel(
        pool: &SqlitePool,
        input: NewUpdateChannel,
    ) -> Result<UpdateChannel, AppError> {
        let normalized = normalize_update_channel(input)?;
        UpdaterRepository::create_channel(pool, normalized).await
    }

    pub async fn list_update_checks(pool: &SqlitePool) -> Result<Vec<UpdateCheck>, AppError> {
        UpdaterRepository::list_checks(pool).await
    }

    pub async fn create_update_check(
        pool: &SqlitePool,
        input: NewUpdateCheck,
    ) -> Result<UpdateCheck, AppError> {
        let normalized = normalize_update_check(input)?;
        UpdaterRepository::create_check(pool, normalized).await
    }
}

fn normalize_update_channel(input: NewUpdateChannel) -> Result<NewUpdateChannel, AppError> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::Validation {
            code: "validation.update_channel_name_required",
            message: "Update channel name is required".to_string(),
            details: None,
            recoverable: true,
        });
    }

    let channel = normalize_channel(&input.channel)?;
    let feed_url = input
        .feed_url
        .and_then(|url| non_empty_string(url.trim().to_string()));
    if let Some(feed_url) = &feed_url {
        validate_https_url(
            feed_url,
            "validation.update_feed_url_scheme",
            "Update feed URL",
        )?;
    }

    Ok(NewUpdateChannel {
        name,
        channel,
        feed_url,
        enabled: input.enabled,
        notes: input
            .notes
            .and_then(|notes| non_empty_string(notes.trim().to_string())),
    })
}

fn normalize_update_check(input: NewUpdateCheck) -> Result<NewUpdateCheck, AppError> {
    let current_version = input.current_version.trim().to_string();
    if current_version.is_empty() {
        return Err(AppError::Validation {
            code: "validation.update_current_version_required",
            message: "Current version is required".to_string(),
            details: None,
            recoverable: true,
        });
    }

    let status = normalize_update_status(&input.status)?;
    let latest_version = input
        .latest_version
        .and_then(|version| non_empty_string(version.trim().to_string()));
    if status == "available" && latest_version.is_none() {
        return Err(AppError::Validation {
            code: "validation.update_latest_version_required",
            message: "Available updates require a latest version".to_string(),
            details: None,
            recoverable: true,
        });
    }

    let release_notes_url = input
        .release_notes_url
        .and_then(|url| non_empty_string(url.trim().to_string()));
    if let Some(release_notes_url) = &release_notes_url {
        validate_https_url(
            release_notes_url,
            "validation.update_release_notes_url_scheme",
            "Release notes URL",
        )?;
    }

    let details_json = normalize_details_json(&input.details_json)?;

    Ok(NewUpdateCheck {
        channel_id: input
            .channel_id
            .and_then(|id| non_empty_string(id.trim().to_string())),
        current_version,
        latest_version,
        status,
        release_notes_url,
        details_json,
    })
}

fn normalize_channel(channel: &str) -> Result<String, AppError> {
    let normalized = channel.trim().to_lowercase();
    if matches!(normalized.as_str(), "stable" | "beta" | "nightly") {
        return Ok(normalized);
    }

    Err(AppError::Validation {
        code: "validation.update_channel",
        message: "Update channel must be stable, beta, or nightly".to_string(),
        details: Some(channel.to_string()),
        recoverable: true,
    })
}

fn normalize_update_status(status: &str) -> Result<String, AppError> {
    let normalized = status.trim().to_lowercase();
    if matches!(
        normalized.as_str(),
        "unknown" | "up_to_date" | "available" | "error"
    ) {
        return Ok(normalized);
    }

    Err(AppError::Validation {
        code: "validation.update_status",
        message: "Update status must be unknown, up_to_date, available, or error".to_string(),
        details: Some(status.to_string()),
        recoverable: true,
    })
}

fn normalize_details_json(details_json: &str) -> Result<String, AppError> {
    let json = if details_json.trim().is_empty() {
        "{}"
    } else {
        details_json.trim()
    };
    let value = serde_json::from_str::<Value>(json).map_err(|error| AppError::Validation {
        code: "validation.update_details_json",
        message: "Update details JSON is invalid".to_string(),
        details: Some(error.to_string()),
        recoverable: true,
    })?;
    if !value.is_object() {
        return Err(AppError::Validation {
            code: "validation.update_details_object",
            message: "Update details JSON must be an object".to_string(),
            details: None,
            recoverable: true,
        });
    }

    serde_json::to_string(&value).map_err(AppError::from)
}

fn validate_https_url(url: &str, code: &'static str, label: &str) -> Result<(), AppError> {
    if url.starts_with("https://") {
        return Ok(());
    }

    Err(AppError::Validation {
        code,
        message: format!("{label} must start with https://"),
        details: Some(url.to_string()),
        recoverable: true,
    })
}

fn non_empty_string(value: String) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{create_memory_pool, run_migrations};

    #[tokio::test]
    async fn create_update_channel_normalizes_https_feed() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let channel = UpdaterService::create_update_channel(
            &pool,
            NewUpdateChannel {
                name: " Stable ".to_string(),
                channel: "STABLE".to_string(),
                feed_url: Some(" https://updates.example.com/stable.json ".to_string()),
                enabled: true,
                notes: Some(" Main channel ".to_string()),
            },
        )
        .await
        .expect("channel");

        assert_eq!(channel.name, "Stable");
        assert_eq!(channel.channel, "stable");
        assert_eq!(
            channel.feed_url.as_deref(),
            Some("https://updates.example.com/stable.json")
        );
        assert_eq!(channel.notes.as_deref(), Some("Main channel"));
    }

    #[tokio::test]
    async fn create_update_channel_rejects_http_feed() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let error = UpdaterService::create_update_channel(
            &pool,
            NewUpdateChannel {
                name: "Unsafe".to_string(),
                channel: "stable".to_string(),
                feed_url: Some("http://updates.example.com/stable.json".to_string()),
                enabled: true,
                notes: None,
            },
        )
        .await
        .expect_err("error");

        assert_eq!(error.code(), "validation.update_feed_url_scheme");
    }

    #[tokio::test]
    async fn create_update_check_requires_latest_for_available() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let error = UpdaterService::create_update_check(
            &pool,
            NewUpdateCheck {
                channel_id: None,
                current_version: "0.1.0".to_string(),
                latest_version: None,
                status: "available".to_string(),
                release_notes_url: None,
                details_json: "{}".to_string(),
            },
        )
        .await
        .expect_err("error");

        assert_eq!(error.code(), "validation.update_latest_version_required");
    }
}

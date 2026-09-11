use crate::error::AppError;
use crate::models::settings::AppSettings;

const PROXY_ENV_KEYS: [&str; 6] = [
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "http_proxy",
    "https_proxy",
    "all_proxy",
];

/// The proxy URL the user typed, trimmed; `None` when it is missing or blank.
pub fn normalized_proxy_url(settings: &AppSettings) -> Option<&str> {
    settings
        .proxy_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// Rejects an "enabled but no URL" configuration before it is persisted.
pub fn validate(settings: &AppSettings) -> Result<(), AppError> {
    if settings.proxy_enabled && normalized_proxy_url(settings).is_none() {
        return Err(AppError::Validation {
            code: "validation.proxy_url_required",
            message: "Proxy URL is required when the proxy is enabled".to_string(),
            details: None,
            recoverable: true,
        });
    }
    Ok(())
}

/// Applies the persisted choice to this process's environment: sets
/// `HTTP_PROXY` / `HTTPS_PROXY` / `ALL_PROXY` (upper and lower case) to the
/// configured URL while enabled, clears them otherwise. Call after the
/// settings file has landed — an explicit disable is user intent, so it clears
/// the vars even though startup never does (external env must survive boots).
pub fn apply(settings: &AppSettings) {
    if settings.proxy_enabled {
        if let Some(url) = normalized_proxy_url(settings) {
            for key in PROXY_ENV_KEYS {
                // SAFETY: called single-threaded from the settings save path /
                // startup before other threads read the environment.
                unsafe {
                    std::env::set_var(key, url);
                }
            }
        }
    } else {
        clear_proxy_env();
    }
}

/// Startup variant: only writes env vars when the settings explicitly enable
/// the proxy. A fresh install or a disabled setting leaves externally-set
/// `HTTP_PROXY` alone, so docker `-e` and systemd `Environment=` keep working.
pub fn apply_startup(settings: &AppSettings) {
    if settings.proxy_enabled {
        apply(settings);
    }
}

pub fn clear_proxy_env() {
    for key in PROXY_ENV_KEYS {
        // SAFETY: same single-threaded settings path as `apply`.
        unsafe {
            std::env::remove_var(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(enabled: bool, url: Option<&str>) -> AppSettings {
        AppSettings {
            proxy_enabled: enabled,
            proxy_url: url.map(str::to_string),
            ..AppSettings::defaults_for_data_dir("/tmp/ai-switch".to_string())
        }
    }

    #[test]
    fn validate_rejects_enabled_without_url() {
        assert!(validate(&settings(true, None)).is_err());
        assert!(validate(&settings(true, Some("  "))).is_err());
        assert!(validate(&settings(true, Some(" http://127.0.0.1:7890 "))).is_ok());
        assert!(validate(&settings(false, None)).is_ok());
    }

    #[test]
    fn apply_sets_and_clears_proxy_environment() {
        apply(&settings(true, Some("http://127.0.0.1:7890")));
        assert_eq!(std::env::var("HTTPS_PROXY").as_deref(), Ok("http://127.0.0.1:7890"));
        assert_eq!(std::env::var("https_proxy").as_deref(), Ok("http://127.0.0.1:7890"));

        apply(&settings(false, Some("http://127.0.0.1:7890")));
        assert!(std::env::var("HTTPS_PROXY").is_err());
        assert!(std::env::var("https_proxy").is_err());
    }

    #[test]
    fn apply_startup_leaves_environment_alone_when_disabled() {
        std::env::set_var("HTTP_PROXY", "http://external:1");
        apply_startup(&settings(false, None));
        assert_eq!(std::env::var("HTTP_PROXY").as_deref(), Ok("http://external:1"));
        apply_startup(&settings(true, Some("http://127.0.0.1:7890")));
        assert_eq!(std::env::var("HTTP_PROXY").as_deref(), Ok("http://127.0.0.1:7890"));
        clear_proxy_env();
        assert!(std::env::var("HTTP_PROXY").is_err());
    }
}

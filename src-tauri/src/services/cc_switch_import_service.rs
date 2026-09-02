//! Reads provider entries out of cc-switch's own configuration and maps them to
//! AI Switch API accounts.
//!
//! Clean-room: this reads cc-switch's public on-disk formats only — the SQLite
//! `providers` table and the older `config.json` — and no code is shared with it.
//!
//! Two shapes exist in the wild and both are handled:
//!
//! - `~/.cc-switch/cc-switch.db`, table `providers(id, app_type, name,
//!   settings_config, meta, category, sort_index, ...)`.
//! - `~/.cc-switch/config.json`, an object keyed by app type, each holding a
//!   `providers` map of id → entry.
//!
//! `settings_config` is the client's *native* config, so the extraction is
//! per-app: Claude Code keeps `env` vars, Codex keeps an `auth` map plus a
//! `config` TOML string.

use crate::error::AppError;
use crate::models::platform::{ApiDialect, PlatformId};
use crate::models::route_credential::{
    ModelMapping, ANTHROPIC_API_KEY_FIELD, ANTHROPIC_AUTH_TOKEN_FIELD, CLAUDE_ONE_M_SUFFIX,
    CLAUDE_SUBAGENT_MODEL_ALIAS, FALLBACK_MODEL_ALIAS,
};
use serde_json::{Map, Value};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::path::{Path, PathBuf};

/// One provider entry as it exists in cc-switch, before any mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalClientProvider {
    /// cc-switch's own record id, namespaced by app type because its primary
    /// key is the `(id, app_type)` pair — the same id may appear under two apps.
    pub source_id: String,
    pub app_type: String,
    pub display_name: String,
    pub category: Option<String>,
    pub sort_index: i64,
    pub settings_config: Value,
    pub meta: Value,
}

/// A cc-switch provider translated into the fields `create_api` needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedApiCredential {
    pub platform: PlatformId,
    pub display_name: String,
    pub api_key: String,
    pub base_url: String,
    pub interface_format: ApiDialect,
    pub api_key_field: Option<&'static str>,
    pub model_mappings: Vec<ModelMapping>,
    pub user_agent: Option<String>,
}

/// Claude Code env keys that carry a per-role upstream model, paired with the
/// AI Switch alias the proxy rewrites and the env key holding the menu label.
const CLAUDE_ROLE_MODEL_KEYS: &[(&str, &str, Option<&str>, &str)] = &[
    (
        "ANTHROPIC_DEFAULT_SONNET_MODEL",
        "claude-sonnet-alias",
        Some("ANTHROPIC_DEFAULT_SONNET_MODEL_NAME"),
        "Sonnet",
    ),
    (
        "ANTHROPIC_DEFAULT_OPUS_MODEL",
        "claude-opus-alias",
        Some("ANTHROPIC_DEFAULT_OPUS_MODEL_NAME"),
        "Opus",
    ),
    (
        "ANTHROPIC_DEFAULT_FABLE_MODEL",
        "claude-fable-alias",
        Some("ANTHROPIC_DEFAULT_FABLE_MODEL_NAME"),
        "Fable",
    ),
    (
        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
        "claude-haiku-alias",
        Some("ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME"),
        "Haiku",
    ),
];

/// Haiku is the one menu role with no 1M context tier, matching the role table
/// the mapping editor uses.
fn alias_supports_one_m(alias: &str) -> bool {
    alias != "claude-haiku-alias"
}

const GENERIC_API_KEY_ENV_KEYS: &[&str] = &[
    "GEMINI_API_KEY",
    "GOOGLE_API_KEY",
    "GOOGLE_GENAI_API_KEY",
    "XAI_API_KEY",
    "GROK_API_KEY",
    "OPENAI_API_KEY",
    "API_KEY",
];

const GENERIC_BASE_URL_ENV_KEYS: &[&str] = &[
    "GOOGLE_GEMINI_BASE_URL",
    "GEMINI_BASE_URL",
    "GOOGLE_BASE_URL",
    "XAI_BASE_URL",
    "GROK_BASE_URL",
    "OPENAI_BASE_URL",
    "BASE_URL",
];

/// Default cc-switch data directory. `CC_SWITCH_HOME` wins when set, so a user
/// who moved the directory for cloud sync can still import.
fn cc_switch_home() -> PathBuf {
    match std::env::var("CC_SWITCH_HOME")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        Some(value) if value == "~" => home_dir(),
        Some(value) => match value.strip_prefix("~/") {
            Some(rest) => home_dir().join(rest),
            None => PathBuf::from(value),
        },
        None => home_dir().join(".cc-switch"),
    }
}

fn home_dir() -> PathBuf {
    directories::BaseDirs::new()
        .map(|base| base.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Config files cc-switch may have written, newest format first.
pub fn default_source_paths() -> Vec<PathBuf> {
    let home = cc_switch_home();
    vec![home.join("cc-switch.db"), home.join("config.json")]
}

/// Picks the config to read: the caller's explicit choice, else the first
/// default path that exists.
pub fn resolve_source_path(source_path: Option<&str>) -> Result<PathBuf, AppError> {
    if let Some(source_path) = source_path.map(str::trim).filter(|path| !path.is_empty()) {
        let path = PathBuf::from(source_path);
        if !path.is_file() {
            return Err(AppError::Validation {
                code: "external_import.source_missing",
                message: "The selected cc-switch config file does not exist".to_string(),
                details: Some(source_path.to_string()),
                recoverable: true,
            });
        }
        return Ok(path);
    }

    default_source_paths()
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| AppError::Validation {
            code: "external_import.source_not_found",
            message: "Could not find a cc-switch configuration on this machine".to_string(),
            details: Some(cc_switch_home().display().to_string()),
            recoverable: true,
        })
}

/// Loads every provider entry from `path`, dispatching on file shape.
pub async fn read_providers(path: &Path) -> Result<Vec<ExternalClientProvider>, AppError> {
    let providers = if is_sqlite_path(path) {
        read_providers_from_database(path).await?
    } else {
        read_providers_from_json(path).await?
    };

    if providers.len() > crate::models::external_client_import::EXTERNAL_CLIENT_MAX_ITEMS {
        return Err(AppError::Validation {
            code: "external_import.too_many_items",
            message: "The cc-switch configuration contains too many providers".to_string(),
            details: Some(providers.len().to_string()),
            recoverable: true,
        });
    }
    Ok(providers)
}

fn is_sqlite_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("db"))
}

/// Reads the `providers` table over a read-only, immutable connection.
///
/// `immutable` matters: cc-switch may be running and holding the database, and
/// opening it read-write would either block or create `-wal` files next to
/// someone else's data. We only ever read, so declaring that is both safer and
/// enough to sidestep locking.
async fn read_providers_from_database(
    path: &Path,
) -> Result<Vec<ExternalClientProvider>, AppError> {
    type ProviderRow = (
        String,
        String,
        String,
        Option<String>,
        Option<i64>,
        String,
        String,
    );

    let options = SqliteConnectOptions::new()
        .filename(path)
        .read_only(true)
        .immutable(true)
        .create_if_missing(false);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|error| read_error("Could not open the cc-switch database", error))?;

    let with_meta = sqlx::query_as::<_, ProviderRow>(
        "SELECT id, app_type, name, category, sort_index, settings_config, meta FROM providers",
    )
    .fetch_all(&pool)
    .await;
    // `meta` arrived in a later cc-switch version, so a long-untouched database
    // may not have the column. Retry without it rather than failing the import.
    let rows = match with_meta {
        Ok(rows) => Ok(rows),
        Err(_) => {
            sqlx::query_as::<_, (String, String, String, Option<String>, Option<i64>, String)>(
                "SELECT id, app_type, name, category, sort_index, settings_config FROM providers",
            )
            .fetch_all(&pool)
            .await
            .map(|rows| {
                rows.into_iter()
                    .map(
                        |(id, app_type, name, category, sort_index, settings_config)| {
                            (
                                id,
                                app_type,
                                name,
                                category,
                                sort_index,
                                settings_config,
                                "{}".to_string(),
                            )
                        },
                    )
                    .collect()
            })
            .map_err(|error| read_error("Could not read cc-switch providers", error))
        }
    };
    pool.close().await;

    Ok(rows?
        .into_iter()
        .map(
            |(id, app_type, name, category, sort_index, settings_config, meta)| {
                ExternalClientProvider {
                    source_id: namespaced_source_id(&app_type, &id),
                    app_type,
                    display_name: name,
                    category: nonempty(category),
                    sort_index: sort_index.unwrap_or(0),
                    settings_config: serde_json::from_str::<Value>(&settings_config)
                        .unwrap_or(Value::Null),
                    meta: serde_json::from_str::<Value>(&meta).unwrap_or(Value::Null),
                }
            },
        )
        .collect())
}

/// Reads the legacy `config.json`: `{ "<app>": { "providers": { "<id>": {...} } } }`.
async fn read_providers_from_json(path: &Path) -> Result<Vec<ExternalClientProvider>, AppError> {
    let text = tokio::fs::read_to_string(path)
        .await
        .map_err(|error| read_error("Could not read the cc-switch config file", error))?;
    let root: Value = serde_json::from_str(&text).map_err(|error| AppError::Validation {
        code: "external_import.source_invalid_json",
        message: "The cc-switch config file is not valid JSON".to_string(),
        details: Some(error.to_string()),
        recoverable: true,
    })?;
    let Some(apps) = root.as_object() else {
        return Err(AppError::Validation {
            code: "external_import.source_unexpected_shape",
            message: "The cc-switch config file has an unexpected shape".to_string(),
            details: None,
            recoverable: true,
        });
    };

    let mut providers = Vec::new();
    for (app_type, app_value) in apps {
        let Some(entries) = app_value.get("providers").and_then(Value::as_object) else {
            continue;
        };
        for (id, entry) in entries {
            let object = entry.as_object().cloned().unwrap_or_default();
            providers.push(ExternalClientProvider {
                source_id: namespaced_source_id(app_type, id),
                app_type: app_type.clone(),
                display_name: string_field(&object, "name").unwrap_or_else(|| id.clone()),
                category: string_field(&object, "category"),
                sort_index: object
                    .get("sortIndex")
                    .or_else(|| object.get("sort_index"))
                    .and_then(Value::as_i64)
                    .unwrap_or(0),
                settings_config: object
                    .get("settingsConfig")
                    .or_else(|| object.get("settings_config"))
                    .cloned()
                    .unwrap_or(Value::Null),
                meta: object.get("meta").cloned().unwrap_or(Value::Null),
            });
        }
    }
    Ok(providers)
}

/// cc-switch keys providers by `(id, app_type)`, so the app type has to be part
/// of our source id or two apps sharing an id would fight over one local row.
fn namespaced_source_id(app_type: &str, id: &str) -> String {
    format!("{}:{}", app_type.trim(), id.trim())
}

fn read_error(message: &str, error: impl ToString) -> AppError {
    AppError::Filesystem {
        code: "external_import.source_read_failed",
        message: message.to_string(),
        details: Some(error.to_string()),
        recoverable: true,
    }
}

fn nonempty(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn string_field(object: &Map<String, Value>, key: &str) -> Option<String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Why one provider entry cannot become an AI Switch account.
///
/// Codes travel to the UI as stable strings; the frontend owns the wording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtractIssue {
    PlatformUnsupported,
    OfficialLogin,
    ApiKeyMissing,
    BaseUrlMissing,
    BaseUrlInvalid,
}

impl ExtractIssue {
    pub const fn code(self) -> &'static str {
        match self {
            Self::PlatformUnsupported => "external_import.platform_unsupported",
            Self::OfficialLogin => "external_import.official_login_unsupported",
            Self::ApiKeyMissing => "external_import.api_key_missing",
            Self::BaseUrlMissing => "external_import.base_url_missing",
            Self::BaseUrlInvalid => "external_import.base_url_invalid",
        }
    }
}

/// AI Switch platform for a cc-switch app type.
///
/// `claude-desktop` is intentionally *not* mapped even though `PlatformId::parse`
/// accepts the alias: those entries describe the Claude desktop app's own login,
/// never an API route, so treating them as Claude accounts would offer an import
/// that cannot work.
pub fn platform_for_app_type(app_type: &str) -> Option<PlatformId> {
    match app_type.trim().to_ascii_lowercase().as_str() {
        "claude" | "claude-code" | "claude_code" => Some(PlatformId::Claude),
        "codex" => Some(PlatformId::Codex),
        "gemini" | "gemini-cli" => Some(PlatformId::Gemini),
        "grok" | "xai" => Some(PlatformId::Grok),
        "opencode" => Some(PlatformId::OpenCode),
        "openclaw" => Some(PlatformId::OpenClaw),
        "hermes" => Some(PlatformId::Hermes),
        _ => None,
    }
}

/// Translates one cc-switch provider into API-account fields.
pub fn extract_api_credential(
    provider: &ExternalClientProvider,
) -> Result<ExtractedApiCredential, ExtractIssue> {
    let platform =
        platform_for_app_type(&provider.app_type).ok_or(ExtractIssue::PlatformUnsupported)?;
    if provider
        .category
        .as_deref()
        .is_some_and(|category| category.eq_ignore_ascii_case("official"))
    {
        return Err(ExtractIssue::OfficialLogin);
    }

    let settings = provider
        .settings_config
        .as_object()
        .cloned()
        .unwrap_or_default();
    let env = settings
        .get("env")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let (api_key, api_key_field) = extract_api_key(platform, &settings, &env)?;
    let codex_config = (platform == PlatformId::Codex)
        .then(|| {
            settings
                .get("config")
                .and_then(Value::as_str)
                .and_then(|config| config.parse::<toml::Value>().ok())
        })
        .flatten();
    let base_url = extract_base_url(platform, &env, codex_config.as_ref())?;
    let interface_format =
        extract_interface_format(platform, &provider.meta, codex_config.as_ref());
    let model_mappings = extract_model_mappings(platform, &env, &settings, codex_config.as_ref());
    let user_agent = provider
        .meta
        .get("customUserAgent")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let display_name = provider.display_name.trim();
    let display_name = if display_name.is_empty() {
        provider.source_id.clone()
    } else {
        display_name.to_string()
    };

    Ok(ExtractedApiCredential {
        platform,
        display_name,
        api_key,
        base_url,
        interface_format,
        api_key_field,
        model_mappings,
        user_agent,
    })
}

fn extract_api_key(
    platform: PlatformId,
    settings: &Map<String, Value>,
    env: &Map<String, Value>,
) -> Result<(String, Option<&'static str>), ExtractIssue> {
    if platform == PlatformId::Claude {
        // Which env key held the token decides how AI Switch must send it:
        // ANTHROPIC_AUTH_TOKEN is a bearer header, ANTHROPIC_API_KEY is x-api-key.
        // Guessing here would authenticate against the wrong header.
        if let Some(token) = string_field(env, ANTHROPIC_AUTH_TOKEN_FIELD) {
            return Ok((token, Some(ANTHROPIC_AUTH_TOKEN_FIELD)));
        }
        if let Some(token) = string_field(env, ANTHROPIC_API_KEY_FIELD) {
            return Ok((token, Some(ANTHROPIC_API_KEY_FIELD)));
        }
        return Err(ExtractIssue::ApiKeyMissing);
    }

    if platform == PlatformId::Codex {
        if let Some(auth) = settings.get("auth").and_then(Value::as_object) {
            if let Some(key) = string_field(auth, "OPENAI_API_KEY") {
                return Ok((key, None));
            }
            // Some relays store the token under their own key name.
            if let Some(key) = auth
                .values()
                .filter_map(Value::as_str)
                .map(str::trim)
                .find(|value| !value.is_empty())
            {
                return Ok((key.to_string(), None));
            }
        }
    }

    GENERIC_API_KEY_ENV_KEYS
        .iter()
        .find_map(|key| string_field(env, key))
        .map(|key| (key, None))
        .ok_or(ExtractIssue::ApiKeyMissing)
}

fn extract_base_url(
    platform: PlatformId,
    env: &Map<String, Value>,
    codex_config: Option<&toml::Value>,
) -> Result<String, ExtractIssue> {
    let candidate = match platform {
        PlatformId::Claude => string_field(env, "ANTHROPIC_BASE_URL"),
        PlatformId::Codex => codex_config
            .and_then(active_codex_provider)
            .and_then(|provider| provider.get("base_url")?.as_str().map(str::to_string))
            .or_else(|| {
                GENERIC_BASE_URL_ENV_KEYS
                    .iter()
                    .find_map(|key| string_field(env, key))
            }),
        _ => GENERIC_BASE_URL_ENV_KEYS
            .iter()
            .find_map(|key| string_field(env, key)),
    };

    let base_url = candidate
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or(ExtractIssue::BaseUrlMissing)?;
    let parsed = url::Url::parse(&base_url).map_err(|_| ExtractIssue::BaseUrlInvalid)?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(ExtractIssue::BaseUrlInvalid);
    }
    Ok(base_url)
}

/// The `[model_providers.X]` table the Codex config actually selects.
///
/// Falls back to the sole entry when `model_provider` is missing or dangling —
/// a config with one provider block is unambiguous even if the pointer is stale.
fn active_codex_provider(config: &toml::Value) -> Option<&toml::value::Table> {
    let providers = config.get("model_providers")?.as_table()?;
    if let Some(selected) = config
        .get("model_provider")
        .and_then(toml::Value::as_str)
        .and_then(|name| providers.get(name))
        .and_then(toml::Value::as_table)
    {
        return Some(selected);
    }
    if providers.len() == 1 {
        return providers.values().next().and_then(toml::Value::as_table);
    }
    None
}

fn extract_interface_format(
    platform: PlatformId,
    meta: &Value,
    codex_config: Option<&toml::Value>,
) -> ApiDialect {
    let default = platform
        .default_api_credential_dialect()
        .unwrap_or(ApiDialect::OpenAi);
    if platform != PlatformId::Codex {
        // Claude/Gemini/Grok each have exactly one dialect in cc-switch, and its
        // `apiFormat` for them only ever restates that.
        return default;
    }

    if let Some(format) = meta
        .get("apiFormat")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        // cc-switch spells these `openai_responses` / `openai_chat`.
        match format.to_ascii_lowercase().as_str() {
            "openai_responses" | "openai-responses" | "responses" => {
                return ApiDialect::OpenAiResponses
            }
            "openai_chat" | "openai-chat" | "chat" | "openai" => return ApiDialect::OpenAi,
            "anthropic" => return ApiDialect::Anthropic,
            "gemini" => return ApiDialect::Gemini,
            _ => {}
        }
    }

    match codex_config
        .and_then(active_codex_provider)
        .and_then(|provider| provider.get("wire_api")?.as_str())
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("responses") => ApiDialect::OpenAiResponses,
        Some("chat") => ApiDialect::OpenAi,
        // Codex talks Responses natively, so an unstated wire API is Responses.
        _ => ApiDialect::OpenAiResponses,
    }
}

/// Rebuilds AI Switch model mappings from the external client's config.
///
/// The direction is inverted relative to how the config was written: cc-switch
/// stores the *upstream* model directly under the client's env key (Claude Code
/// asks for exactly what the key says), whereas AI Switch stores `alias →
/// upstream` and re-advertises the alias. So each env key becomes one mapping
/// whose `from` is the AI Switch alias for that role.
fn extract_model_mappings(
    platform: PlatformId,
    env: &Map<String, Value>,
    settings: &Map<String, Value>,
    codex_config: Option<&toml::Value>,
) -> Vec<ModelMapping> {
    match platform {
        PlatformId::Claude => extract_claude_model_mappings(env),
        PlatformId::Codex => extract_codex_model_mappings(settings, codex_config),
        // Gemini and Grok providers carry no per-role model keys in cc-switch,
        // and an empty mapping list means "accept the platform baseline" — the
        // same default a hand-made account starts with.
        _ => Vec::new(),
    }
}

fn extract_claude_model_mappings(env: &Map<String, Value>) -> Vec<ModelMapping> {
    let mut mappings = Vec::new();
    for (model_key, alias, name_key, role_label) in CLAUDE_ROLE_MODEL_KEYS {
        let Some(raw) = string_field(env, model_key) else {
            continue;
        };
        let (upstream, one_m) = split_one_m_suffix(&raw);
        if upstream.is_empty() {
            continue;
        }
        let label = name_key
            .and_then(|name_key| string_field(env, name_key))
            .unwrap_or_else(|| (*role_label).to_string());
        mappings.push(ModelMapping {
            from: (*alias).to_string(),
            to: upstream,
            label: Some(label),
            // Only roles with a 1M tier may declare it, matching the editor's
            // own rule — Haiku has no 1M variant to declare.
            supports_1m: (one_m && alias_supports_one_m(alias)).then_some(true),
        });
    }

    // The generic aliases carry no display name: neither shows up in the
    // `/model` menu, so a label would never be rendered.
    for (env_key, alias) in [
        ("CLAUDE_CODE_SUBAGENT_MODEL", CLAUDE_SUBAGENT_MODEL_ALIAS),
        ("ANTHROPIC_MODEL", FALLBACK_MODEL_ALIAS),
    ] {
        let Some(raw) = string_field(env, env_key) else {
            continue;
        };
        let (upstream, one_m) = split_one_m_suffix(&raw);
        if upstream.is_empty() {
            continue;
        }
        mappings.push(ModelMapping {
            from: alias.to_string(),
            to: upstream,
            label: None,
            supports_1m: one_m.then_some(true),
        });
    }
    mappings
}

/// Codex is a pass-through: cc-switch points the CLI straight at the relay's own
/// model ids, so each id maps to itself. Keeping `from == to` means the proxy
/// advertises exactly the models the relay serves and forwards them unchanged.
fn extract_codex_model_mappings(
    settings: &Map<String, Value>,
    codex_config: Option<&toml::Value>,
) -> Vec<ModelMapping> {
    let mut mappings: Vec<ModelMapping> = Vec::new();
    let mut push = |model: &str, label: Option<String>| {
        let model = model.trim();
        if model.is_empty() || mappings.iter().any(|mapping| mapping.from == model) {
            return;
        }
        mappings.push(ModelMapping {
            from: model.to_string(),
            to: model.to_string(),
            label: label.filter(|label| label != model),
            supports_1m: None,
        });
    };

    if let Some(models) = settings
        .get("modelCatalog")
        .and_then(|catalog| catalog.get("models"))
        .and_then(Value::as_array)
    {
        for entry in models {
            let Some(object) = entry.as_object() else {
                continue;
            };
            let Some(model) = string_field(object, "model") else {
                continue;
            };
            push(&model, string_field(object, "displayName"));
        }
    }

    if let Some(model) = codex_config
        .and_then(|config| config.get("model")?.as_str())
        .map(str::trim)
        .filter(|model| !model.is_empty())
    {
        push(model, None);
    }
    mappings
}

/// Splits Claude Code's `[1M]` context marker off a model value.
///
/// Case-insensitive because the suffix is written both ways in the wild.
fn split_one_m_suffix(value: &str) -> (String, bool) {
    let trimmed = value.trim();
    let suffix = CLAUDE_ONE_M_SUFFIX.to_ascii_lowercase();
    if trimmed.to_ascii_lowercase().ends_with(&suffix) {
        let base = trimmed[..trimmed.len() - suffix.len()].trim_end();
        return (base.to_string(), true);
    }
    (trimmed.to_string(), false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn provider(app_type: &str, name: &str, settings_config: Value) -> ExternalClientProvider {
        ExternalClientProvider {
            source_id: namespaced_source_id(app_type, "provider-1"),
            app_type: app_type.to_string(),
            display_name: name.to_string(),
            category: None,
            sort_index: 0,
            settings_config,
            meta: Value::Null,
        }
    }

    #[test]
    fn source_id_is_namespaced_by_app_type() {
        // cc-switch's primary key is (id, app_type): the same id can appear under
        // two apps, and collapsing them would make one import overwrite the other.
        assert_eq!(namespaced_source_id("claude", "abc"), "claude:abc");
        assert_ne!(
            namespaced_source_id("claude", "abc"),
            namespaced_source_id("codex", "abc")
        );
    }

    #[test]
    fn extracts_claude_provider_with_roles_and_auth_token_field() {
        let entry = provider(
            "claude",
            "goRouter",
            json!({
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": "sk-secret",
                    "ANTHROPIC_BASE_URL": "https://gorouter.app",
                    "ANTHROPIC_MODEL": "claude-opus-5[1M]",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "claude-opus-5[1M]",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME": "Opus as Sonnet",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "claude-haiku-4-5[1M]",
                    "CLAUDE_CODE_SUBAGENT_MODEL": "claude-opus-5",
                },
                "includeCoAuthoredBy": false
            }),
        );

        let extracted = extract_api_credential(&entry).expect("claude provider");
        assert_eq!(extracted.platform, PlatformId::Claude);
        assert_eq!(extracted.display_name, "goRouter");
        assert_eq!(extracted.api_key, "sk-secret");
        assert_eq!(extracted.base_url, "https://gorouter.app");
        assert_eq!(extracted.interface_format, ApiDialect::Anthropic);
        assert_eq!(extracted.api_key_field, Some(ANTHROPIC_AUTH_TOKEN_FIELD));
        assert_eq!(extracted.user_agent, None);

        let sonnet = extracted
            .model_mappings
            .iter()
            .find(|mapping| mapping.from == "claude-sonnet-alias")
            .expect("sonnet mapping");
        assert_eq!(sonnet.to, "claude-opus-5");
        assert_eq!(sonnet.label.as_deref(), Some("Opus as Sonnet"));
        assert_eq!(sonnet.supports_1m, Some(true));

        // Haiku has no 1M tier, so the suffix is stripped without declaring 1M.
        let haiku = extracted
            .model_mappings
            .iter()
            .find(|mapping| mapping.from == "claude-haiku-alias")
            .expect("haiku mapping");
        assert_eq!(haiku.to, "claude-haiku-4-5");
        assert_eq!(haiku.supports_1m, None);

        let fallback = extracted
            .model_mappings
            .iter()
            .find(|mapping| mapping.from == FALLBACK_MODEL_ALIAS)
            .expect("fallback mapping");
        assert_eq!(fallback.to, "claude-opus-5");
        assert_eq!(fallback.label, None);
        let subagent = extracted
            .model_mappings
            .iter()
            .find(|mapping| mapping.from == CLAUDE_SUBAGENT_MODEL_ALIAS)
            .expect("subagent mapping");
        assert_eq!(subagent.to, "claude-opus-5");
        assert_eq!(subagent.supports_1m, None);
        // Unset roles contribute nothing rather than empty mappings.
        assert!(!extracted
            .model_mappings
            .iter()
            .any(|mapping| mapping.from == "claude-fable-alias"));
    }

    #[test]
    fn claude_api_key_env_key_decides_the_auth_header() {
        let entry = provider(
            "claude",
            "x-api-key relay",
            json!({
                "env": {
                    "ANTHROPIC_API_KEY": "sk-header",
                    "ANTHROPIC_BASE_URL": "https://relay.example",
                }
            }),
        );
        let extracted = extract_api_credential(&entry).expect("claude provider");
        assert_eq!(extracted.api_key, "sk-header");
        assert_eq!(extracted.api_key_field, Some(ANTHROPIC_API_KEY_FIELD));
    }

    #[test]
    fn extracts_codex_provider_from_auth_and_toml_config() {
        let mut entry = provider(
            "codex",
            "kktoken",
            json!({
                "auth": { "OPENAI_API_KEY": "sk-codex" },
                "config": "model_provider = \"custom\"\nmodel = \"claude-opus-5\"\n\n[model_providers.custom]\nname = \"custom\"\nwire_api = \"responses\"\nbase_url = \"https://kktoken.cc/v1\"\n",
                "modelCatalog": { "models": [{ "model": "claude-opus-5", "displayName": "Opus 5" }] }
            }),
        );
        entry.meta = json!({ "apiFormat": "openai_chat" });

        let extracted = extract_api_credential(&entry).expect("codex provider");
        assert_eq!(extracted.platform, PlatformId::Codex);
        assert_eq!(extracted.api_key, "sk-codex");
        assert_eq!(extracted.base_url, "https://kktoken.cc/v1");
        // `apiFormat` is cc-switch's own record of the dialect and outranks the
        // TOML `wire_api`, which stays "responses" even when the relay is Chat.
        assert_eq!(extracted.interface_format, ApiDialect::OpenAi);
        assert_eq!(extracted.api_key_field, None);
        assert_eq!(
            extracted.model_mappings,
            vec![ModelMapping {
                from: "claude-opus-5".to_string(),
                to: "claude-opus-5".to_string(),
                label: Some("Opus 5".to_string()),
                supports_1m: None,
            }]
        );
    }

    #[test]
    fn codex_wire_api_is_the_fallback_when_meta_is_silent() {
        let entry = provider(
            "codex",
            "chat relay",
            json!({
                "auth": { "OPENAI_API_KEY": "sk-codex" },
                "config": "model_provider = \"relay\"\n\n[model_providers.relay]\nwire_api = \"chat\"\nbase_url = \"https://relay.example/v1\"\n"
            }),
        );
        let extracted = extract_api_credential(&entry).expect("codex provider");
        assert_eq!(extracted.interface_format, ApiDialect::OpenAi);
        assert_eq!(extracted.base_url, "https://relay.example/v1");

        let unstated = provider(
            "codex",
            "responses relay",
            json!({
                "auth": { "OPENAI_API_KEY": "sk-codex" },
                "config": "[model_providers.relay]\nbase_url = \"https://relay.example/v1\"\n"
            }),
        );
        assert_eq!(
            extract_api_credential(&unstated)
                .expect("codex provider")
                .interface_format,
            ApiDialect::OpenAiResponses
        );
    }

    #[test]
    fn codex_base_url_follows_the_selected_provider_block() {
        // Several `[model_providers.*]` blocks are normal; picking the wrong one
        // silently points the account at an endpoint the user is not using.
        let entry = provider(
            "codex",
            "multi",
            json!({
                "auth": { "OPENAI_API_KEY": "sk-codex" },
                "config": "model_provider = \"second\"\n\n[model_providers.first]\nbase_url = \"https://first.example/v1\"\n\n[model_providers.second]\nbase_url = \"https://second.example/v1\"\n"
            }),
        );
        assert_eq!(
            extract_api_credential(&entry).expect("codex").base_url,
            "https://second.example/v1"
        );
    }

    #[test]
    fn reports_actionable_issues_instead_of_importing_broken_entries() {
        let official = ExternalClientProvider {
            category: Some("official".to_string()),
            ..provider("claude", "Claude Official", json!({ "env": {} }))
        };
        assert_eq!(
            extract_api_credential(&official).unwrap_err(),
            ExtractIssue::OfficialLogin
        );

        let desktop = provider("claude-desktop", "Claude Desktop", json!({ "env": {} }));
        assert_eq!(
            extract_api_credential(&desktop).unwrap_err(),
            ExtractIssue::PlatformUnsupported
        );

        let no_key = provider(
            "claude",
            "No key",
            json!({ "env": { "ANTHROPIC_BASE_URL": "https://relay.example" } }),
        );
        assert_eq!(
            extract_api_credential(&no_key).unwrap_err(),
            ExtractIssue::ApiKeyMissing
        );

        let no_url = provider(
            "claude",
            "No URL",
            json!({ "env": { "ANTHROPIC_AUTH_TOKEN": "sk-x" } }),
        );
        assert_eq!(
            extract_api_credential(&no_url).unwrap_err(),
            ExtractIssue::BaseUrlMissing
        );

        let bad_url = provider(
            "claude",
            "Bad URL",
            json!({
                "env": { "ANTHROPIC_AUTH_TOKEN": "sk-x", "ANTHROPIC_BASE_URL": "file:///etc/passwd" }
            }),
        );
        assert_eq!(
            extract_api_credential(&bad_url).unwrap_err(),
            ExtractIssue::BaseUrlInvalid
        );
    }

    #[test]
    fn carries_over_a_custom_user_agent() {
        let mut entry = provider(
            "claude",
            "Any",
            json!({
                "env": { "ANTHROPIC_AUTH_TOKEN": "sk-x", "ANTHROPIC_BASE_URL": "https://any.example" }
            }),
        );
        entry.meta = json!({ "customUserAgent": "claude-cli/2.1.161 (external, cli)" });
        assert_eq!(
            extract_api_credential(&entry).expect("claude").user_agent,
            Some("claude-cli/2.1.161 (external, cli)".to_string())
        );
    }

    #[tokio::test]
    async fn reads_providers_from_a_sqlite_config() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("cc-switch.db");
        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .expect("create source db");
        sqlx::query(
            "CREATE TABLE providers (
                id TEXT NOT NULL, app_type TEXT NOT NULL, name TEXT NOT NULL,
                settings_config TEXT NOT NULL, website_url TEXT, category TEXT,
                created_at INTEGER, sort_index INTEGER, notes TEXT, icon TEXT,
                icon_color TEXT, meta TEXT NOT NULL DEFAULT '{}',
                is_current BOOLEAN NOT NULL DEFAULT 0,
                PRIMARY KEY (id, app_type)
            )",
        )
        .execute(&pool)
        .await
        .expect("create table");
        sqlx::query(
            "INSERT INTO providers (id, app_type, name, settings_config, category, sort_index, meta)
             VALUES ('p1', 'claude', 'goRouter', ?, NULL, 2, ?)",
        )
        .bind(
            json!({
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": "sk-secret",
                    "ANTHROPIC_BASE_URL": "https://gorouter.app"
                }
            })
            .to_string(),
        )
        .bind(json!({ "apiFormat": "anthropic" }).to_string())
        .execute(&pool)
        .await
        .expect("insert provider");
        pool.close().await;

        let providers = read_providers(&path).await.expect("read providers");
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].source_id, "claude:p1");
        assert_eq!(providers[0].display_name, "goRouter");
        assert_eq!(providers[0].sort_index, 2);
        assert_eq!(providers[0].meta["apiFormat"], "anthropic");
        assert_eq!(
            extract_api_credential(&providers[0])
                .expect("extract")
                .api_key,
            "sk-secret"
        );
        // The source stays untouched and unlocked: no journal beside it.
        assert!(!dir.path().join("cc-switch.db-wal").exists());
    }

    #[tokio::test]
    async fn reads_providers_from_a_legacy_json_config() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("config.json");
        tokio::fs::write(
            &path,
            json!({
                "claude": {
                    "providers": {
                        "p1": {
                            "name": "goRouter",
                            "settingsConfig": {
                                "env": {
                                    "ANTHROPIC_AUTH_TOKEN": "sk-secret",
                                    "ANTHROPIC_BASE_URL": "https://gorouter.app"
                                }
                            },
                            "meta": { "apiFormat": "anthropic" }
                        }
                    }
                },
                "codex": { "providers": {} }
            })
            .to_string(),
        )
        .await
        .expect("write config");

        let providers = read_providers(&path).await.expect("read providers");
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].source_id, "claude:p1");
        assert_eq!(
            extract_api_credential(&providers[0])
                .expect("extract")
                .base_url,
            "https://gorouter.app"
        );
    }

    #[tokio::test]
    async fn reads_providers_from_a_database_without_the_meta_column() {
        // cc-switch added `meta` in a later version; an untouched old database
        // must still import instead of failing on the missing column.
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("cc-switch.db");
        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .expect("create source db");
        sqlx::query(
            "CREATE TABLE providers (
                id TEXT NOT NULL, app_type TEXT NOT NULL, name TEXT NOT NULL,
                settings_config TEXT NOT NULL, category TEXT, sort_index INTEGER,
                PRIMARY KEY (id, app_type)
            )",
        )
        .execute(&pool)
        .await
        .expect("create table");
        sqlx::query(
            "INSERT INTO providers (id, app_type, name, settings_config, category, sort_index)
             VALUES ('p1', 'claude', 'Legacy', ?, NULL, 0)",
        )
        .bind(
            json!({
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": "sk-legacy",
                    "ANTHROPIC_BASE_URL": "https://legacy.example"
                }
            })
            .to_string(),
        )
        .execute(&pool)
        .await
        .expect("insert provider");
        pool.close().await;

        let providers = read_providers(&path).await.expect("read providers");
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].meta, json!({}));
        let extracted = extract_api_credential(&providers[0]).expect("extract");
        assert_eq!(extracted.api_key, "sk-legacy");
        assert_eq!(extracted.interface_format, ApiDialect::Anthropic);
    }

    #[test]
    fn resolve_source_path_rejects_a_missing_explicit_file() {
        let error = resolve_source_path(Some("C:/definitely/not/here.json"))
            .expect_err("missing file must be reported");
        assert!(matches!(
            error,
            AppError::Validation {
                code: "external_import.source_missing",
                ..
            }
        ));
    }
}

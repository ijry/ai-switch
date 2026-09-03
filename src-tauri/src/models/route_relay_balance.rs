use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Key under `route_credentials.config_json` holding the query settings.
pub const RELAY_BALANCE_CONFIG_KEY: &str = "relay_balance";
/// Key under `route_credentials.config_json` holding the last queried snapshot.
pub const RELAY_BALANCE_SNAPSHOT_KEY: &str = "relay_balance_snapshot";
/// new-api reports quota as an integer that has to be divided by the panel's
/// `QuotaPerUnit` to become dollars. This is the shipped default; panels may
/// change it, and `GET /api/status` reports whatever the panel actually uses.
pub const DEFAULT_NEW_API_QUOTA_PER_UNIT: f64 = 500_000.0;
const MAX_RELAY_BALANCE_ENDPOINT_CHARS: usize = 2048;
const MAX_RELAY_BALANCE_PATH_CHARS: usize = 256;

/// Which relay panel dialect an account's balance is read with.
///
/// The absence of the whole config block means "off"; there is deliberately no
/// `None` variant, so an enabled config cannot be spelled two ways.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RelayBalanceProvider {
    /// `GET <panel>/api/usage/token/`, authenticated with the account's own key.
    #[serde(rename = "new_api", alias = "newapi", alias = "new-api")]
    NewApi,
    /// `GET <panel>/v1/usage`, authenticated with the account's own key.
    #[serde(rename = "sub2api", alias = "sub_2_api")]
    Sub2Api,
    /// User-declared endpoint plus dotted paths to the numbers.
    #[serde(rename = "custom")]
    Custom,
}

impl RelayBalanceProvider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NewApi => "new_api",
            Self::Sub2Api => "sub2api",
            Self::Custom => "custom",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::NewApi => "new-api",
            Self::Sub2Api => "sub2api",
            Self::Custom => "自定义",
        }
    }

    /// The other zero-config dialect, used to cross-check when the selected one
    /// finds no endpoint at all.
    ///
    /// A relay's Base URL does not say which panel software is behind it, so the
    /// setting is a guess the user makes from the outside — and guessing wrong is
    /// the most common reason a healthy account reads as "查询失败". Both built-ins
    /// reuse the same Base URL and the same key, so trying the other one asks
    /// nothing of the user. `Custom` has no counterpart: the user named that URL,
    /// and second-guessing it would query somewhere they did not ask for.
    pub fn other_built_in(self) -> Option<Self> {
        match self {
            Self::NewApi => Some(Self::Sub2Api),
            Self::Sub2Api => Some(Self::NewApi),
            Self::Custom => None,
        }
    }
}

/// Per-account balance query settings, stored under `config_json.relay_balance`.
///
/// Both built-in providers are zero-config: they reuse the account's own base
/// URL and API key, so only `provider` is set. The remaining fields exist for
/// `Custom`, which is the escape hatch for panels neither built-in covers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RelayBalanceConfig {
    pub provider: RelayBalanceProvider,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub endpoint: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub remaining_path: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub used_path: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub limit_path: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub plan_path: String,
    /// What the custom endpoint's numbers are denominated in. Built-in providers
    /// report USD and ignore this.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub unit: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub divisor: Option<f64>,
}

impl RelayBalanceConfig {
    pub fn new(provider: RelayBalanceProvider) -> Self {
        Self {
            provider,
            endpoint: String::new(),
            remaining_path: String::new(),
            used_path: String::new(),
            limit_path: String::new(),
            plan_path: String::new(),
            unit: String::new(),
            divisor: None,
        }
    }

    /// The unit a snapshot should be labelled with. Empty means the user did not
    /// say, and "USD" is the safe guess for a relay station.
    pub fn display_unit(&self) -> String {
        let unit = self.unit.trim();
        if unit.is_empty() {
            "USD".to_string()
        } else {
            unit.to_string()
        }
    }

    /// Reads the block out of a credential's `config_json`. Returns `Ok(None)`
    /// when balance querying is off, which is also what a malformed config
    /// degrades to — a broken query setting must never make an account
    /// unreadable.
    pub fn from_config_json(config_json: &str) -> Option<Self> {
        serde_json::from_str::<Value>(config_json)
            .ok()
            .and_then(|config| Self::from_config_value(&config).ok())
            .flatten()
    }

    pub fn from_config_value(config: &Value) -> Result<Option<Self>, String> {
        let Some(raw) = config.get(RELAY_BALANCE_CONFIG_KEY) else {
            return Ok(None);
        };
        if raw.is_null() {
            return Ok(None);
        }
        // The UI drops the key when the user turns querying off, but an
        // explicit "none" arrives from hand-edited config JSON and imports.
        if let Some(provider) = raw.get("provider").and_then(Value::as_str) {
            let provider = provider.trim();
            if provider.is_empty() || provider.eq_ignore_ascii_case("none") {
                return Ok(None);
            }
        }
        let config = serde_json::from_value::<Self>(raw.clone())
            .map_err(|error| format!("relay_balance is not a valid query setting: {error}"))?;
        config.validate()?;
        Ok(Some(config))
    }

    pub fn validate(&self) -> Result<(), String> {
        if let Some(divisor) = self.divisor {
            if !divisor.is_finite() || divisor <= 0.0 {
                return Err("relay_balance.divisor must be a positive number".to_string());
            }
        }
        if self.provider != RelayBalanceProvider::Custom {
            // Leftovers from a previous custom setup are ignored, not rejected:
            // switching providers back and forth must not fail validation.
            return Ok(());
        }

        let endpoint = self.endpoint.trim();
        if endpoint.is_empty() {
            return Err("自定义余额查询需要填写请求 URL".to_string());
        }
        if endpoint.chars().count() > MAX_RELAY_BALANCE_ENDPOINT_CHARS {
            return Err(format!(
                "relay_balance.endpoint must be at most {MAX_RELAY_BALANCE_ENDPOINT_CHARS} characters"
            ));
        }
        if !endpoint.starts_with("http://") && !endpoint.starts_with("https://") {
            return Err("自定义余额查询的请求 URL 必须以 http:// 或 https:// 开头".to_string());
        }
        if self.remaining_path.trim().is_empty() {
            return Err("自定义余额查询需要填写剩余额度的取值路径".to_string());
        }
        for (label, path) in [
            ("remaining_path", &self.remaining_path),
            ("used_path", &self.used_path),
            ("limit_path", &self.limit_path),
            ("plan_path", &self.plan_path),
        ] {
            validate_json_path(label, path)?;
        }
        Ok(())
    }
}

/// One balance reading, stored under `config_json.relay_balance_snapshot`.
///
/// Money is `f64` on purpose. The `quota_*` SQL columns are `i64` (see
/// `quota_columns_from_config_json`), so reusing them would round $12.34 down
/// to 12 and would also conflate relay balance with official-account quota,
/// which the account row and the scheduler both read.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RelayBalanceSnapshot {
    pub provider: RelayBalanceProvider,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remaining: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub used: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<f64>,
    pub unit: String,
    /// The panel says this key has no cap, which makes the numbers meaningless.
    #[serde(default)]
    pub unlimited: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    pub source_url: String,
    pub checked_at: String,
    /// Extra human-readable lines (subscription windows, rate-limit windows).
    /// A free-form list beats modelling every panel's shape.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

impl RelayBalanceSnapshot {
    pub fn from_config_json(config_json: &str) -> Option<Self> {
        let config = serde_json::from_str::<Value>(config_json).ok()?;
        let raw = config.get(RELAY_BALANCE_SNAPSHOT_KEY)?;
        serde_json::from_value::<Self>(raw.clone()).ok()
    }

    /// Whether the panel reported a spent key. `unlimited` keys never are.
    pub fn is_exhausted(&self) -> bool {
        !self.unlimited && self.remaining.is_some_and(|remaining| remaining <= 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn missing_block_means_querying_is_off() {
        let config = json!({"base_url": "https://panel.example.com/v1"});
        assert_eq!(RelayBalanceConfig::from_config_value(&config), Ok(None));
    }

    #[test]
    fn explicit_none_provider_means_querying_is_off() {
        for raw in [json!({"provider": "none"}), json!({"provider": "  "})] {
            let config = json!({"relay_balance": raw});
            assert_eq!(RelayBalanceConfig::from_config_value(&config), Ok(None));
        }
    }

    #[test]
    fn built_in_providers_need_nothing_but_the_provider_name() {
        let config = json!({"relay_balance": {"provider": "new_api"}});
        let parsed = RelayBalanceConfig::from_config_value(&config)
            .expect("parses")
            .expect("enabled");
        assert_eq!(parsed.provider, RelayBalanceProvider::NewApi);
        assert_eq!(
            parsed,
            RelayBalanceConfig::new(RelayBalanceProvider::NewApi)
        );
    }

    #[test]
    fn provider_name_aliases_are_accepted() {
        for (raw, expected) in [
            ("new_api", RelayBalanceProvider::NewApi),
            ("newapi", RelayBalanceProvider::NewApi),
            ("new-api", RelayBalanceProvider::NewApi),
            ("sub2api", RelayBalanceProvider::Sub2Api),
        ] {
            let config = json!({"relay_balance": {"provider": raw}});
            let parsed = RelayBalanceConfig::from_config_value(&config)
                .expect("parses")
                .expect("enabled");
            assert_eq!(parsed.provider, expected, "provider {raw}");
        }
    }

    #[test]
    fn custom_provider_requires_endpoint_and_remaining_path() {
        let missing_endpoint = json!({"relay_balance": {"provider": "custom"}});
        let error = RelayBalanceConfig::from_config_value(&missing_endpoint)
            .expect_err("endpoint is required");
        assert!(error.contains("请求 URL"), "{error}");

        let missing_path = json!({
            "relay_balance": {"provider": "custom", "endpoint": "https://panel.example.com/usage"}
        });
        let error =
            RelayBalanceConfig::from_config_value(&missing_path).expect_err("path is required");
        assert!(error.contains("取值路径"), "{error}");
    }

    #[test]
    fn custom_endpoint_must_be_http() {
        let config = json!({
            "relay_balance": {
                "provider": "custom",
                "endpoint": "panel.example.com/usage",
                "remaining_path": "data.remaining",
            }
        });
        let error = RelayBalanceConfig::from_config_value(&config).expect_err("scheme is required");
        assert!(error.contains("http://"), "{error}");
    }

    #[test]
    fn dotted_paths_reject_empty_segments() {
        let config = json!({
            "relay_balance": {
                "provider": "custom",
                "endpoint": "https://panel.example.com/usage",
                "remaining_path": "data..remaining",
            }
        });
        let error = RelayBalanceConfig::from_config_value(&config).expect_err("path is malformed");
        assert!(error.contains("不合法"), "{error}");
    }

    #[test]
    fn divisor_must_be_positive() {
        for divisor in [0.0, -1.0] {
            let config = json!({"relay_balance": {"provider": "new_api", "divisor": divisor}});
            let error =
                RelayBalanceConfig::from_config_value(&config).expect_err("divisor is invalid");
            assert!(error.contains("divisor"), "{error}");
        }
    }

    #[test]
    fn stale_custom_fields_do_not_block_a_built_in_provider() {
        let config = json!({
            "relay_balance": {"provider": "sub2api", "endpoint": "not-a-url", "remaining_path": ""}
        });
        let parsed = RelayBalanceConfig::from_config_value(&config)
            .expect("parses")
            .expect("enabled");
        assert_eq!(parsed.provider, RelayBalanceProvider::Sub2Api);
    }

    #[test]
    fn a_broken_block_reads_as_off_instead_of_failing_the_row() {
        let config_json = r#"{"relay_balance": {"provider": "nope"}}"#;
        assert_eq!(RelayBalanceConfig::from_config_json(config_json), None);
        assert_eq!(RelayBalanceConfig::from_config_json("not json"), None);
    }

    #[test]
    fn snapshot_round_trips_and_omits_empty_fields() {
        let snapshot = RelayBalanceSnapshot {
            provider: RelayBalanceProvider::NewApi,
            plan_name: Some("default".to_string()),
            remaining: Some(37.7),
            used: Some(12.3),
            limit: Some(50.0),
            unit: "USD".to_string(),
            unlimited: false,
            expires_at: None,
            source_url: "https://panel.example.com/api/usage/token/".to_string(),
            checked_at: "2026-09-02T12:00:00Z".to_string(),
            notes: Vec::new(),
        };
        let value = serde_json::to_value(&snapshot).expect("serializes");
        assert_eq!(
            value.get("provider").and_then(Value::as_str),
            Some("new_api")
        );
        assert!(
            value.get("expires_at").is_none(),
            "empty fields are omitted"
        );
        assert!(value.get("notes").is_none(), "empty notes are omitted");

        let config_json = json!({"relay_balance_snapshot": value}).to_string();
        assert_eq!(
            RelayBalanceSnapshot::from_config_json(&config_json),
            Some(snapshot)
        );
    }

    #[test]
    fn exhaustion_needs_a_capped_key() {
        let base = RelayBalanceSnapshot {
            provider: RelayBalanceProvider::Sub2Api,
            plan_name: None,
            remaining: Some(0.0),
            used: None,
            limit: None,
            unit: "USD".to_string(),
            unlimited: false,
            expires_at: None,
            source_url: "https://panel.example.com/v1/usage".to_string(),
            checked_at: "2026-09-02T12:00:00Z".to_string(),
            notes: Vec::new(),
        };
        assert!(base.is_exhausted());
        assert!(!RelayBalanceSnapshot {
            unlimited: true,
            ..base.clone()
        }
        .is_exhausted());
        assert!(!RelayBalanceSnapshot {
            remaining: None,
            ..base
        }
        .is_exhausted());
    }
}

fn validate_json_path(label: &str, path: &str) -> Result<(), String> {
    let path = path.trim();
    if path.is_empty() {
        return Ok(());
    }
    if path.chars().count() > MAX_RELAY_BALANCE_PATH_CHARS {
        return Err(format!(
            "relay_balance.{label} must be at most {MAX_RELAY_BALANCE_PATH_CHARS} characters"
        ));
    }
    if path.split('.').any(|segment| segment.trim().is_empty()) {
        return Err(format!(
            "取值路径 {path} 不合法：用点号分隔字段名，例如 data.total_available"
        ));
    }
    Ok(())
}

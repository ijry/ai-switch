use crate::error::AppError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum PlatformId {
    Codex,
    Claude,
    Gemini,
    Grok,
    OpenCode,
    OpenClaw,
    Hermes,
}

impl PlatformId {
    pub const ALL: [Self; 7] = [
        Self::Codex,
        Self::Claude,
        Self::Gemini,
        Self::Grok,
        Self::OpenCode,
        Self::OpenClaw,
        Self::Hermes,
    ];

    pub fn parse(value: &str) -> Result<Self, AppError> {
        match normalize_identifier(value).as_str() {
            "codex" | "openai" | "chatgpt" => Ok(Self::Codex),
            "claude" | "anthropic" | "claude_code" | "claude_desktop" => Ok(Self::Claude),
            "gemini" | "google" | "gemini_cli" => Ok(Self::Gemini),
            "grok" | "xai" | "x_ai" | "x.ai" => Ok(Self::Grok),
            "opencode" | "open_code" => Ok(Self::OpenCode),
            "openclaw" | "open_claw" => Ok(Self::OpenClaw),
            "hermes" => Ok(Self::Hermes),
            _ => Err(AppError::Validation {
                code: "platform.unknown",
                message: "Platform is not recognized".to_string(),
                details: Some(value.trim().to_string()),
                recoverable: true,
            }),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::Gemini => "gemini",
            Self::Grok => "grok",
            Self::OpenCode => "opencode",
            Self::OpenClaw => "openclaw",
            Self::Hermes => "hermes",
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::Claude => "Claude",
            Self::Gemini => "Gemini",
            Self::Grok => "Grok",
            Self::OpenCode => "OpenCode",
            Self::OpenClaw => "OpenClaw",
            Self::Hermes => "Hermes",
        }
    }

    pub const fn default_api_credential_dialect(self) -> Option<ApiDialect> {
        match self {
            Self::Codex | Self::Grok => Some(ApiDialect::OpenAi),
            Self::Claude => Some(ApiDialect::Anthropic),
            Self::Gemini => Some(ApiDialect::Gemini),
            Self::OpenCode | Self::OpenClaw | Self::Hermes => None,
        }
    }
}

impl std::fmt::Display for PlatformId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl std::str::FromStr for PlatformId {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ApiDialect {
    #[serde(rename = "openai")]
    OpenAi,
    #[serde(rename = "openai-responses")]
    OpenAiResponses,
    #[serde(rename = "anthropic")]
    Anthropic,
    #[serde(rename = "gemini")]
    Gemini,
}

impl ApiDialect {
    pub fn parse(value: &str) -> Result<Self, AppError> {
        match normalize_identifier(value).as_str() {
            "openai" => Ok(Self::OpenAi),
            "openai_responses" => Ok(Self::OpenAiResponses),
            "anthropic" => Ok(Self::Anthropic),
            "gemini" => Ok(Self::Gemini),
            _ => Err(AppError::Validation {
                code: "validation.api_dialect",
                message: "API dialect is not recognized".to_string(),
                details: Some(value.trim().to_string()),
                recoverable: true,
            }),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OpenAi => "openai",
            Self::OpenAiResponses => "openai-responses",
            Self::Anthropic => "anthropic",
            Self::Gemini => "gemini",
        }
    }
}

impl std::fmt::Display for ApiDialect {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl std::str::FromStr for ApiDialect {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PlatformOperation {
    RouteCredentials,
    GenericApiRouting,
    ConfigWrite,
    OfficialImport,
    OfficialAccountRouting,
    DeeplinkImport,
    OfficialQuota,
    ModelTest,
    TerminalLaunch,
    SessionResume,
}

impl PlatformOperation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RouteCredentials => "route_credentials",
            Self::GenericApiRouting => "generic_api_routing",
            Self::ConfigWrite => "config_write",
            Self::OfficialImport => "official_import",
            Self::OfficialAccountRouting => "official_account_routing",
            Self::DeeplinkImport => "deeplink_import",
            Self::OfficialQuota => "official_quota",
            Self::ModelTest => "model_test",
            Self::TerminalLaunch => "terminal_launch",
            Self::SessionResume => "session_resume",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityAvailability {
    Supported,
    Partial,
    Unavailable,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SupportLevel {
    Supported,
    Partial,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityRule {
    pub availability: CapabilityAvailability,
    pub reason_code: Option<String>,
    pub credential_kinds: Vec<String>,
    pub requires_base_url: bool,
    pub requires_api_dialect: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlatformOperations {
    pub route_credentials: CapabilityRule,
    pub generic_api_routing: CapabilityRule,
    pub config_write: CapabilityRule,
    pub official_import: CapabilityRule,
    pub official_account_routing: CapabilityRule,
    pub deeplink_import: CapabilityRule,
    pub official_quota: CapabilityRule,
    pub model_test: CapabilityRule,
    pub terminal_launch: CapabilityRule,
    pub session_resume: CapabilityRule,
}

impl PlatformOperations {
    pub fn get(&self, operation: PlatformOperation) -> &CapabilityRule {
        match operation {
            PlatformOperation::RouteCredentials => &self.route_credentials,
            PlatformOperation::GenericApiRouting => &self.generic_api_routing,
            PlatformOperation::ConfigWrite => &self.config_write,
            PlatformOperation::OfficialImport => &self.official_import,
            PlatformOperation::OfficialAccountRouting => &self.official_account_routing,
            PlatformOperation::DeeplinkImport => &self.deeplink_import,
            PlatformOperation::OfficialQuota => &self.official_quota,
            PlatformOperation::ModelTest => &self.model_test,
            PlatformOperation::TerminalLaunch => &self.terminal_launch,
            PlatformOperation::SessionResume => &self.session_resume,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlatformCapability {
    pub platform: PlatformId,
    pub display_name: String,
    pub support_level: SupportLevel,
    pub operations: PlatformOperations,
}

fn normalize_identifier(value: &str) -> String {
    value.trim().to_lowercase().replace([' ', '-'], "_")
}

#[cfg(test)]
mod tests {
    use super::{ApiDialect, PlatformId};

    #[test]
    fn parses_only_explicit_platform_aliases() {
        assert_eq!(
            PlatformId::parse("claude-code").unwrap(),
            PlatformId::Claude
        );
        assert_eq!(PlatformId::parse("x.ai").unwrap(), PlatformId::Grok);
        assert_eq!(PlatformId::parse("OpenClaw").unwrap(), PlatformId::OpenClaw);
        assert!(PlatformId::parse("my-claude-wrapper").is_err());
        assert!(PlatformId::parse("unknown-provider").is_err());
    }

    #[test]
    fn parses_supported_api_dialect_aliases() {
        assert_eq!(
            ApiDialect::parse("openai-responses").unwrap(),
            ApiDialect::OpenAiResponses
        );
        assert_eq!(ApiDialect::parse("anthropic").unwrap(), ApiDialect::Anthropic);
        let legacy_dash = ["anthropic", "messages"].join("-");
        let legacy_underscore = ["anthropic", "messages"].join("_");
        assert!(ApiDialect::parse(&legacy_dash).is_err());
        assert!(ApiDialect::parse(&legacy_underscore).is_err());
        assert!(ApiDialect::parse("automatic").is_err());
    }

    #[test]
    fn platform_serialization_uses_canonical_ids() {
        assert_eq!(
            serde_json::to_string(&PlatformId::OpenCode).unwrap(),
            "\"opencode\""
        );
        assert_eq!(
            serde_json::to_string(&ApiDialect::OpenAiResponses).unwrap(),
            "\"openai-responses\""
        );
    }

    #[test]
    fn partial_platforms_have_no_default_api_dialect() {
        assert_eq!(
            PlatformId::Codex.default_api_credential_dialect(),
            Some(ApiDialect::OpenAi)
        );
        assert_eq!(PlatformId::Hermes.default_api_credential_dialect(), None);
    }
}

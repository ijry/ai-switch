//! Launch metadata for the Vibe screen: which agent CLIs are installed, how to
//! install the missing ones, and the real model / reasoning choices each agent
//! can be started with.
//!
//! Model ids come from the same source the proxy advertises on `/v1/models`
//! (the pool credentials' `model_mappings`), so the dropdowns never show a model
//! the router would reject.

use crate::database::repositories::route_credential_repository::RouteCredentialRepository;
use crate::database::repositories::route_pool_repository::RoutePoolRepository;
use crate::error::AppError;
use crate::models::platform::PlatformId;
use crate::models::route_credential::RouteCredentialPoolScope;
use crate::models::route_credential_transfer::RouteCredentialSelectionContext;
use crate::services::route_model_capability::{
    advertised_model_ids, codex_reasoning_metadata, parse_model_capability,
};
use crate::terminal_manager::{
    agent_program_name, agent_supports_model_flag, agent_supports_reasoning, find_program_in_path,
};
use serde::Serialize;
use serde_json::Value;
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentReasoningLevel {
    pub effort: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentLaunchModel {
    pub id: String,
    pub reasoning_levels: Vec<AgentReasoningLevel>,
    pub default_reasoning_level: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentLaunchOption {
    pub platform: String,
    pub display_name: String,
    pub program: String,
    pub installed: bool,
    pub npm_package: String,
    pub install_command: String,
    pub supports_model_selection: bool,
    pub supports_reasoning: bool,
    pub models: Vec<AgentLaunchModel>,
}

struct AgentDescriptor {
    platform: PlatformId,
    display_name: &'static str,
    npm_package: &'static str,
}

/// Verified `npm` packages whose bin name matches what `resolve_launch_command`
/// spawns. Two lookalikes are deliberately avoided: bare `grok-cli` is an
/// unrelated proxy wrapper and bare `hermes` is a 2014 chat bot.
const AGENT_DESCRIPTORS: &[AgentDescriptor] = &[
    AgentDescriptor {
        platform: PlatformId::Codex,
        display_name: "Codex",
        npm_package: "@openai/codex",
    },
    AgentDescriptor {
        platform: PlatformId::Claude,
        display_name: "Claude",
        npm_package: "@anthropic-ai/claude-code",
    },
    AgentDescriptor {
        platform: PlatformId::Grok,
        display_name: "Grok",
        npm_package: "@vibe-kit/grok-cli",
    },
    AgentDescriptor {
        platform: PlatformId::Gemini,
        display_name: "Gemini",
        npm_package: "@google/gemini-cli",
    },
    AgentDescriptor {
        platform: PlatformId::OpenCode,
        display_name: "OpenCode",
        npm_package: "opencode-ai",
    },
    AgentDescriptor {
        platform: PlatformId::OpenClaw,
        display_name: "OpenClaw",
        npm_package: "openclaw",
    },
    AgentDescriptor {
        platform: PlatformId::Hermes,
        display_name: "Hermes",
        npm_package: "hermes-agent",
    },
];

pub struct AgentLaunchService;

impl AgentLaunchService {
    pub async fn list_options(pool: &SqlitePool) -> Result<Vec<AgentLaunchOption>, AppError> {
        let mut options = Vec::with_capacity(AGENT_DESCRIPTORS.len());
        for descriptor in AGENT_DESCRIPTORS {
            let platform = descriptor.platform.as_str();
            let program = agent_program_name(platform).unwrap_or(platform);
            let models = Self::platform_models(pool, descriptor.platform).await?;
            options.push(AgentLaunchOption {
                platform: platform.to_string(),
                display_name: descriptor.display_name.to_string(),
                program: program.to_string(),
                installed: find_program_in_path(program).is_some(),
                npm_package: descriptor.npm_package.to_string(),
                install_command: format!("npm install -g {}", descriptor.npm_package),
                supports_model_selection: agent_supports_model_flag(platform),
                supports_reasoning: agent_supports_reasoning(platform),
                models,
            });
        }
        Ok(options)
    }

    async fn platform_models(
        pool: &SqlitePool,
        platform: PlatformId,
    ) -> Result<Vec<AgentLaunchModel>, AppError> {
        if !agent_supports_model_flag(platform.as_str()) {
            return Ok(Vec::new());
        }

        let ids = RoutePoolRepository::list_member_ids(pool, platform.as_str()).await?;
        let credentials = RouteCredentialRepository::list_by_ids(
            pool,
            &ids,
            &RouteCredentialSelectionContext {
                platform: platform.as_str().to_string(),
                pool_scope: RouteCredentialPoolScope::InPool,
            },
        )
        .await?;
        let capabilities = credentials
            .iter()
            .map(|credential| parse_model_capability(&credential.config_json))
            .collect::<Vec<_>>();

        let supports_reasoning = agent_supports_reasoning(platform.as_str());
        Ok(advertised_model_ids(platform.as_str(), &capabilities)
            .into_iter()
            .map(|id| {
                if !supports_reasoning {
                    return AgentLaunchModel {
                        id,
                        reasoning_levels: Vec::new(),
                        default_reasoning_level: None,
                    };
                }
                let (levels, default_level) = codex_reasoning_metadata(&id);
                AgentLaunchModel {
                    id,
                    reasoning_levels: levels.iter().filter_map(reasoning_level).collect(),
                    default_reasoning_level: Some(default_level.to_string()),
                }
            })
            .collect())
    }
}

fn reasoning_level(value: &Value) -> Option<AgentReasoningLevel> {
    let effort = value.get("effort").and_then(Value::as_str)?.trim();
    if effort.is_empty() {
        return None;
    }
    Some(AgentReasoningLevel {
        effort: effort.to_string(),
        description: value
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_platform_has_a_launch_descriptor() {
        for platform in PlatformId::ALL {
            assert!(
                AGENT_DESCRIPTORS
                    .iter()
                    .any(|descriptor| descriptor.platform == platform),
                "missing launch descriptor for {}",
                platform.as_str()
            );
        }
    }

    #[test]
    fn every_descriptor_names_an_npm_package() {
        for descriptor in AGENT_DESCRIPTORS {
            assert!(!descriptor.npm_package.trim().is_empty());
        }
    }

    #[test]
    fn reasoning_level_requires_a_non_empty_effort() {
        assert!(reasoning_level(&json!({ "effort": " " })).is_none());
        assert_eq!(
            reasoning_level(&json!({ "effort": "high", "description": "deep" })),
            Some(AgentReasoningLevel {
                effort: "high".to_string(),
                description: "deep".to_string(),
            })
        );
    }
}

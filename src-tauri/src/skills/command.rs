//! Tauri command boundary for Skills settings.

use super::model::{
    SkillAgentInfo, SkillAgentType, SkillContent, SkillItem, SkillLayout, SkillScope,
    SkillsListResult,
};
use super::service;
use crate::error::ApiError;

#[tauri::command]
pub async fn skills_list_agents() -> Result<Vec<SkillAgentInfo>, ApiError> {
    Ok(service::list_agents())
}

#[tauri::command]
pub async fn skills_list(
    agent_type: SkillAgentType,
    scope: SkillScope,
    workspace_path: Option<String>,
) -> Result<SkillsListResult, ApiError> {
    service::list_skills(
        agent_type,
        scope,
        workspace_path.as_deref().map(std::path::Path::new),
    )
    .map_err(ApiError::from)
}

#[tauri::command]
pub async fn skills_read(
    agent_type: SkillAgentType,
    scope: SkillScope,
    skill_id: String,
    workspace_path: Option<String>,
) -> Result<SkillContent, ApiError> {
    service::read_skill(
        agent_type,
        scope,
        &skill_id,
        workspace_path.as_deref().map(std::path::Path::new),
    )
    .map_err(ApiError::from)
}

#[tauri::command]
pub async fn skills_save(
    agent_type: SkillAgentType,
    scope: SkillScope,
    skill_id: String,
    content: String,
    layout: Option<SkillLayout>,
    workspace_path: Option<String>,
) -> Result<SkillItem, ApiError> {
    service::save_skill(
        agent_type,
        scope,
        skill_id,
        content,
        layout,
        workspace_path.as_deref().map(std::path::Path::new),
    )
    .map_err(ApiError::from)
}

#[tauri::command]
pub async fn skills_delete(
    agent_type: SkillAgentType,
    scope: SkillScope,
    skill_id: String,
    workspace_path: Option<String>,
) -> Result<bool, ApiError> {
    service::delete_skill(
        agent_type,
        scope,
        skill_id,
        workspace_path.as_deref().map(std::path::Path::new),
    )
    .map_err(ApiError::from)
}

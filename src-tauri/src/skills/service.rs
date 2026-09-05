//! Skill list/read/save/delete operations.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::error::AppError;

use super::frontmatter::parse_skill_metadata;
use super::model::{
    SkillAgentInfo, SkillAgentType, SkillContent, SkillItem, SkillLayout, SkillLocation,
    SkillScope, SkillSource, SkillsListResult,
};
use super::packages::builtin_package_index;
use super::paths::{
    content_path, resolve_skill_path, scoped_dirs, skill_storage_spec, validate_skill_id,
    SkillStorageKind,
};

fn io_error(message: impl Into<String>, details: Option<String>) -> AppError {
    AppError::Filesystem {
        code: "skills.config_io",
        message: message.into(),
        details,
        recoverable: true,
    }
}

fn list_skills_from_dir(
    scope: SkillScope,
    dir: &Path,
    kind: SkillStorageKind,
    read_only: bool,
    source: SkillSource,
    target_client: SkillAgentType,
) -> Result<Vec<SkillItem>, AppError> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut values = BTreeMap::new();
    for entry in fs::read_dir(dir)
        .map_err(|error| io_error("Could not read Skills directory", Some(error.to_string())))?
    {
        let entry = entry.map_err(|error| {
            io_error(
                "Could not read Skills directory entry",
                Some(error.to_string()),
            )
        })?;
        let path = entry.path();
        if path.is_dir() && path.join("SKILL.md").is_file() {
            let id = entry.file_name().to_string_lossy().to_string();
            let content = fs::read_to_string(path.join("SKILL.md")).map_err(|error| {
                io_error("Could not read Skill content", Some(error.to_string()))
            })?;
            let metadata = parse_skill_metadata(&content)?.unwrap_or_default();
            values.insert(
                id.clone(),
                SkillItem {
                    id: id.clone(),
                    name: metadata
                        .display_name
                        .clone()
                        .or_else(|| metadata.name.clone())
                        .unwrap_or_else(|| id.clone()),
                    scope,
                    layout: SkillLayout::SkillDirectory,
                    path: path.display().to_string(),
                    description: metadata.description.clone(),
                    read_only,
                    package_id: None,
                    package_name: None,
                    category: metadata.category.clone(),
                    tags: metadata.tags.clone(),
                    language: metadata.language.clone(),
                    source,
                    version: None,
                    installed_at: None,
                    target_clients: vec![target_client],
                },
            );
            continue;
        }
        if matches!(kind, SkillStorageKind::SkillDirectoryOrMarkdownFile)
            && path.extension().and_then(|value| value.to_str()) == Some("md")
            && path.is_file()
        {
            let id = path
                .file_stem()
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_default();
            let content = fs::read_to_string(&path).map_err(|error| {
                io_error("Could not read Skill content", Some(error.to_string()))
            })?;
            let metadata = parse_skill_metadata(&content)?.unwrap_or_default();
            values.insert(
                id.clone(),
                SkillItem {
                    id: id.clone(),
                    name: metadata
                        .display_name
                        .clone()
                        .or_else(|| metadata.name.clone())
                        .unwrap_or_else(|| id.clone()),
                    scope,
                    layout: SkillLayout::MarkdownFile,
                    path: path.display().to_string(),
                    description: metadata.description.clone(),
                    read_only,
                    package_id: None,
                    package_name: None,
                    category: metadata.category.clone(),
                    tags: metadata.tags.clone(),
                    language: metadata.language.clone(),
                    source,
                    version: None,
                    installed_at: None,
                    target_clients: vec![target_client],
                },
            );
        }
    }
    Ok(values.into_values().collect())
}

pub fn list_agents() -> Vec<SkillAgentInfo> {
    SkillAgentType::ALL
        .into_iter()
        .map(|agent_type| SkillAgentInfo {
            agent_type,
            display_name: agent_type.display_name().to_string(),
            skills_capable: true,
        })
        .collect()
}

pub fn list_skills(
    agent: SkillAgentType,
    scope: SkillScope,
    workspace_path: Option<&Path>,
) -> Result<SkillsListResult, AppError> {
    let spec = skill_storage_spec(agent);
    let dirs = scoped_dirs(agent, scope, workspace_path)?;
    let package_index = builtin_package_index();
    let mut skills = BTreeMap::new();
    let mut locations = Vec::new();
    for dir in dirs {
        let source = if matches!(scope, SkillScope::Project) {
            SkillSource::Project
        } else {
            spec.source_for_path(&dir)
        };
        locations.push(SkillLocation {
            scope,
            path: dir.display().to_string(),
            exists: dir.is_dir(),
        });
        for mut item in list_skills_from_dir(
            scope,
            &dir,
            spec.kind,
            spec.is_read_only_path(&dir),
            source,
            agent,
        )? {
            if let Some(package) = package_index.by_skill.get(&item.id) {
                item.package_id = Some(package.package_id.clone());
                item.package_name = Some(package.package_name.clone());
            }
            skills.entry(item.id.clone()).or_insert(item);
        }
    }
    Ok(SkillsListResult {
        supported: true,
        message: None,
        locations,
        skills: skills.into_values().collect(),
    })
}

pub fn read_skill(
    agent: SkillAgentType,
    scope: SkillScope,
    skill_id: &str,
    workspace_path: Option<&Path>,
) -> Result<SkillContent, AppError> {
    let id = validate_skill_id(skill_id)?;
    let listed = list_skills(agent, scope, workspace_path)?.skills;
    let skill = listed
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| AppError::Validation {
            code: "skills.not_found",
            message: "Skill was not found".to_string(),
            details: Some(id.clone()),
            recoverable: true,
        })?;
    let content = fs::read_to_string(content_path(skill.layout, Path::new(&skill.path)))
        .map_err(|error| io_error("Could not read Skill content", Some(error.to_string())))?;
    Ok(SkillContent { skill, content })
}

pub fn save_skill(
    agent: SkillAgentType,
    scope: SkillScope,
    skill_id: String,
    content: String,
    layout: Option<SkillLayout>,
    workspace_path: Option<&Path>,
) -> Result<SkillItem, AppError> {
    let id = validate_skill_id(&skill_id)?;
    let spec = skill_storage_spec(agent);
    let dirs = scoped_dirs(agent, scope, workspace_path)?;
    let existing = list_skills(agent, scope, workspace_path)?
        .skills
        .into_iter()
        .find(|item| item.id == id);
    if existing.as_ref().is_some_and(|item| item.read_only) {
        return Err(AppError::Validation {
            code: "skills.read_only",
            message: "Built-in Skill is read-only".to_string(),
            details: None,
            recoverable: true,
        });
    }
    let chosen_layout = existing
        .as_ref()
        .map(|item| item.layout)
        .or(layout)
        .unwrap_or(match spec.kind {
            SkillStorageKind::SkillDirectoryOnly => SkillLayout::SkillDirectory,
            SkillStorageKind::SkillDirectoryOrMarkdownFile => SkillLayout::SkillDirectory,
        });
    let root = dirs.first().ok_or_else(|| AppError::Validation {
        code: "skills.directory_missing",
        message: "No Skill directory is configured".to_string(),
        details: None,
        recoverable: true,
    })?;
    let target = resolve_skill_path(root, &id, chosen_layout)?;
    if spec.is_read_only_path(&target) {
        return Err(AppError::Validation {
            code: "skills.read_only",
            message: "Built-in Skill is read-only".to_string(),
            details: None,
            recoverable: true,
        });
    }
    let content_file = content_path(chosen_layout, &target);
    if let Ok(canonical_content) = content_file.canonicalize() {
        let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        if canonical_content != canonical_root && !canonical_content.starts_with(&canonical_root) {
            return Err(AppError::Validation {
                code: "skills.path_invalid",
                message: "Skill path escapes its storage directory".to_string(),
                details: None,
                recoverable: true,
            });
        }
    }
    if let Some(parent) = content_file.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            io_error("Could not create Skill directory", Some(error.to_string()))
        })?;
    }
    let temp = content_file.with_extension(format!("md.ai-switch-{}", uuid::Uuid::new_v4()));
    fs::write(&temp, content)
        .map_err(|error| io_error("Could not write Skill content", Some(error.to_string())))?;
    if let Err(error) = fs::rename(&temp, &content_file) {
        if content_file.exists() {
            fs::remove_file(&content_file).map_err(|remove_error| {
                io_error(
                    "Could not replace Skill content",
                    Some(remove_error.to_string()),
                )
            })?;
            fs::rename(&temp, &content_file).map_err(|rename_error| {
                let _ = fs::remove_file(&temp);
                io_error(
                    "Could not replace Skill content",
                    Some(rename_error.to_string()),
                )
            })?;
        } else {
            let _ = fs::remove_file(&temp);
            return Err(io_error(
                "Could not replace Skill content",
                Some(error.to_string()),
            ));
        }
    }
    list_skills(agent, scope, workspace_path)?
        .skills
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| AppError::Validation {
            code: "skills.config_invalid",
            message: "Skill was saved but could not be reloaded".to_string(),
            details: None,
            recoverable: true,
        })
}

/// Deletes a Skill that was already resolved from a listing. Package uninstall
/// resolves many members from one listing, so it must not re-scan per member.
pub(super) fn remove_skill_item(skill: &SkillItem) -> Result<(), AppError> {
    let target = Path::new(&skill.path);
    match skill.layout {
        SkillLayout::SkillDirectory => fs::remove_dir_all(target),
        SkillLayout::MarkdownFile => fs::remove_file(target),
    }
    .map_err(|error| io_error("Could not delete Skill", Some(error.to_string())))
}

pub fn delete_skill(
    agent: SkillAgentType,
    scope: SkillScope,
    skill_id: String,
    workspace_path: Option<&Path>,
) -> Result<bool, AppError> {
    let id = validate_skill_id(&skill_id)?;
    let Some(skill) = list_skills(agent, scope, workspace_path)?
        .skills
        .into_iter()
        .find(|item| item.id == id)
    else {
        return Ok(false);
    };
    if skill.read_only {
        return Err(AppError::Validation {
            code: "skills.read_only",
            message: "Built-in Skill is read-only".to_string(),
            details: None,
            recoverable: true,
        });
    }
    remove_skill_item(&skill).map(|_| true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(path: &Path, layout: SkillLayout) -> SkillItem {
        SkillItem {
            id: "demo".to_string(),
            name: "demo".to_string(),
            scope: SkillScope::Global,
            layout,
            path: path.display().to_string(),
            description: None,
            read_only: false,
            package_id: None,
            package_name: None,
            category: None,
            tags: Vec::new(),
            language: None,
            source: SkillSource::Codex,
            version: None,
            installed_at: None,
            target_clients: vec![SkillAgentType::Codex],
        }
    }

    #[test]
    fn removes_both_skill_layouts_from_a_resolved_listing() {
        let root = tempfile::tempdir().unwrap();
        let directory_skill = root.path().join("demo");
        fs::create_dir_all(&directory_skill).unwrap();
        fs::write(directory_skill.join("SKILL.md"), "---\nname: demo\n---\n").unwrap();
        let markdown_skill = root.path().join("solo.md");
        fs::write(&markdown_skill, "---\nname: solo\n---\n").unwrap();

        remove_skill_item(&item(&directory_skill, SkillLayout::SkillDirectory)).unwrap();
        remove_skill_item(&item(&markdown_skill, SkillLayout::MarkdownFile)).unwrap();

        assert!(!directory_skill.exists());
        assert!(!markdown_skill.exists());
    }

    #[test]
    fn reports_a_filesystem_code_when_the_skill_is_already_gone() {
        let root = tempfile::tempdir().unwrap();
        let missing = root.path().join("missing.md");

        let error = remove_skill_item(&item(&missing, SkillLayout::MarkdownFile)).unwrap_err();

        assert_eq!(error.code(), "skills.config_io");
    }
}

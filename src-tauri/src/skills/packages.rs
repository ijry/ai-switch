//! Read-only aggregation of AI Switch owned Skill packages.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::AppError;

use super::model::{
    SkillAgentType, SkillItem, SkillPackage, SkillPackageDetail, SkillPackageInstallResult,
    SkillPackageMember, SkillScope, SkillSource, SkillsPackageListResult,
};
use super::paths::{scoped_dirs, skill_storage_spec, validate_skill_id};
use super::service::list_skills;

const CORE_SKILL_IDS: &[&str] = &[
    "brainstorming",
    "dispatching-parallel-agents",
    "executing-plans",
    "finishing-a-development-branch",
    "receiving-code-review",
    "requesting-code-review",
    "subagent-driven-development",
    "systematic-debugging",
    "test-driven-development",
    "using-git-worktrees",
    "using-superpowers",
    "verification-before-completion",
    "writing-plans",
    "writing-skills",
];

const SCIENCE_SKILL_IDS: &[&str] = &[
    "citation-management",
    "experimental-design",
    "exploratory-data-analysis",
    "hypothesis-generation",
    "paper-lookup",
    "peer-review",
    "scholar-evaluation",
    "scientific-brainstorming",
    "scientific-critical-thinking",
    "scientific-schematics",
    "scientific-visualization",
    "statistical-analysis",
    "statistical-power",
];

#[derive(Debug, Clone, Copy)]
struct BuiltinPackageSpec {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    skill_ids: &'static [&'static str],
}

#[derive(Debug, Clone)]
pub(crate) struct PackageSkillMetadata {
    pub package_id: String,
    pub package_name: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PackageMetadataIndex {
    pub by_skill: BTreeMap<String, PackageSkillMetadata>,
}

fn package_specs() -> &'static [BuiltinPackageSpec] {
    &[
        BuiltinPackageSpec {
            id: "ai-switch.core",
            name: "AI Switch Core Skill Pack",
            description: "Core agent workflow Skills bundled by AI Switch.",
            skill_ids: CORE_SKILL_IDS,
        },
        BuiltinPackageSpec {
            id: "ai-switch.science",
            name: "AI Switch Science Skill Pack",
            description: "Scientific research and analysis Skills bundled by AI Switch.",
            skill_ids: SCIENCE_SKILL_IDS,
        },
    ]
}

pub(crate) fn builtin_package_index() -> PackageMetadataIndex {
    let mut index = PackageMetadataIndex::default();
    for package in package_specs() {
        for skill_id in package.skill_ids {
            index.by_skill.insert(
                (*skill_id).to_string(),
                PackageSkillMetadata {
                    package_id: package.id.to_string(),
                    package_name: package.name.to_string(),
                },
            );
        }
    }
    index
}

fn package_not_found(package_id: &str) -> AppError {
    AppError::Validation {
        code: "skills.package_not_found",
        message: "Skill package was not found".to_string(),
        details: Some(package_id.to_string()),
        recoverable: true,
    }
}

fn package_operation_unsupported(message: impl Into<String>, details: Option<String>) -> AppError {
    AppError::Validation {
        code: "skills.package_operation_unsupported",
        message: message.into(),
        details,
        recoverable: true,
    }
}

fn package_for_spec(
    spec: BuiltinPackageSpec,
    installed_by_id: &BTreeMap<String, SkillItem>,
) -> SkillPackage {
    let skill_ids = spec
        .skill_ids
        .iter()
        .map(|skill_id| (*skill_id).to_string())
        .collect::<Vec<_>>();
    let installed_skill_ids = spec
        .skill_ids
        .iter()
        .filter(|skill_id| installed_by_id.contains_key(**skill_id))
        .map(|skill_id| (*skill_id).to_string())
        .collect::<Vec<_>>();

    SkillPackage {
        id: spec.id.to_string(),
        name: spec.name.to_string(),
        description: Some(spec.description.to_string()),
        source: SkillSource::Builtin,
        version: None,
        manifest_path: None,
        skill_count: skill_ids.len(),
        skill_ids,
        installed_count: installed_skill_ids.len(),
        installed_skill_ids,
        installed_at: None,
        read_only: true,
        target_clients: vec![SkillAgentType::Codex],
    }
}

fn member_name(skill_id: &str, installed: Option<&SkillItem>) -> String {
    installed
        .map(|skill| skill.name.clone())
        .unwrap_or_else(|| skill_id.to_string())
}

fn member_for_skill(skill_id: &str, installed: Option<&SkillItem>) -> SkillPackageMember {
    SkillPackageMember {
        id: skill_id.to_string(),
        name: member_name(skill_id, installed),
        description: installed.and_then(|skill| skill.description.clone()),
        category: installed.and_then(|skill| skill.category.clone()),
        tags: installed
            .map(|skill| skill.tags.clone())
            .unwrap_or_default(),
        language: installed.and_then(|skill| skill.language.clone()),
        installed: installed.is_some(),
        skill: installed.cloned(),
    }
}

fn installed_skill_index(
    agent: SkillAgentType,
    scope: SkillScope,
    workspace_path: Option<&Path>,
) -> Result<BTreeMap<String, SkillItem>, AppError> {
    Ok(list_skills(agent, scope, workspace_path)?
        .skills
        .into_iter()
        .map(|skill| (skill.id.clone(), skill))
        .collect())
}

fn candidate_resource_roots() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(value) = std::env::var("AI_SWITCH_SKILL_PACKAGES_DIR") {
        candidates.push(PathBuf::from(value));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            candidates.extend([
                exe_dir.join("skill-packages"),
                exe_dir.join("resources").join("skill-packages"),
                exe_dir.join("_up_").join("skill-packages"),
                exe_dir.join("..").join("skill-packages"),
                exe_dir.join("..").join("resources").join("skill-packages"),
            ]);
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.extend([
            cwd.join("src-tauri")
                .join("resources")
                .join("skill-packages"),
            cwd.join("resources").join("skill-packages"),
            cwd.join("..")
                .join("src-tauri")
                .join("resources")
                .join("skill-packages"),
        ]);
    }
    candidates
}

fn resolve_resource_root() -> Result<PathBuf, AppError> {
    for candidate in candidate_resource_roots() {
        if package_specs()
            .iter()
            .all(|package| candidate.join(package.id).is_dir())
        {
            return Ok(candidate);
        }
    }
    Err(package_operation_unsupported(
        "AI Switch Skill package resources were not found",
        None,
    ))
}

fn copy_dir_missing_only(source: &Path, target: &Path) -> Result<(), AppError> {
    if target.exists() && !target.is_dir() {
        return Err(AppError::Validation {
            code: "skills.path_invalid",
            message: "Skill install target is not a directory".to_string(),
            details: Some(target.display().to_string()),
            recoverable: true,
        });
    }
    if !target.exists() {
        fs::create_dir_all(target).map_err(|error| AppError::Filesystem {
            code: "skills.config_io",
            message: "Could not create Skill directory".to_string(),
            details: Some(error.to_string()),
            recoverable: true,
        })?;
    }
    for entry in fs::read_dir(source).map_err(|error| AppError::Filesystem {
        code: "skills.config_io",
        message: "Could not read bundled Skill resource".to_string(),
        details: Some(error.to_string()),
        recoverable: true,
    })? {
        let entry = entry.map_err(|error| AppError::Filesystem {
            code: "skills.config_io",
            message: "Could not read bundled Skill resource entry".to_string(),
            details: Some(error.to_string()),
            recoverable: true,
        })?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        if source_path.is_dir() {
            copy_dir_missing_only(&source_path, &target_path)?;
        } else if source_path.is_file() && !target_path.exists() {
            fs::copy(&source_path, &target_path).map_err(|error| AppError::Filesystem {
                code: "skills.config_io",
                message: "Could not copy bundled Skill resource".to_string(),
                details: Some(error.to_string()),
                recoverable: true,
            })?;
        }
    }
    Ok(())
}

fn aggregate_from_installed(
    installed_by_id: BTreeMap<String, SkillItem>,
) -> SkillsPackageListResult {
    let package_index = builtin_package_index();
    let package_ids = package_index
        .by_skill
        .values()
        .map(|metadata| metadata.package_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut skills = installed_by_id
        .values()
        .filter(|skill| {
            skill
                .package_id
                .as_deref()
                .is_some_and(|package_id| package_ids.contains(package_id))
        })
        .cloned()
        .collect::<Vec<_>>();
    skills.sort_by(|left, right| left.id.cmp(&right.id));

    let mut packages = package_specs()
        .iter()
        .map(|spec| package_for_spec(*spec, &installed_by_id))
        .collect::<Vec<_>>();
    packages.sort_by(|left, right| left.name.cmp(&right.name));

    SkillsPackageListResult {
        packages,
        skills,
        warnings: Vec::new(),
    }
}

pub fn list_skill_packages(
    agent: Option<SkillAgentType>,
    scope: Option<SkillScope>,
    workspace_path: Option<&Path>,
) -> Result<SkillsPackageListResult, AppError> {
    let agent = agent.unwrap_or(SkillAgentType::Codex);
    let scope = scope.unwrap_or(SkillScope::Global);
    if agent != SkillAgentType::Codex {
        return Ok(SkillsPackageListResult {
            packages: Vec::new(),
            skills: Vec::new(),
            warnings: Vec::new(),
        });
    }

    Ok(aggregate_from_installed(installed_skill_index(
        agent,
        scope,
        workspace_path,
    )?))
}

pub fn read_skill_package(
    package_id: &str,
    agent: Option<SkillAgentType>,
    scope: Option<SkillScope>,
    workspace_path: Option<&Path>,
) -> Result<SkillPackageDetail, AppError> {
    let agent = agent.unwrap_or(SkillAgentType::Codex);
    let scope = scope.unwrap_or(SkillScope::Global);
    if agent != SkillAgentType::Codex {
        return Err(package_not_found(package_id));
    }

    let installed_by_id = installed_skill_index(agent, scope, workspace_path)?;
    let spec = package_specs()
        .iter()
        .copied()
        .find(|item| item.id == package_id)
        .ok_or_else(|| package_not_found(package_id))?;
    let package = package_for_spec(spec, &installed_by_id);
    let members = spec
        .skill_ids
        .iter()
        .map(|skill_id| member_for_skill(skill_id, installed_by_id.get(*skill_id)))
        .collect::<Vec<_>>();
    let skills = members
        .iter()
        .filter_map(|member| member.skill.clone())
        .collect::<Vec<_>>();

    Ok(SkillPackageDetail {
        package,
        skills,
        members,
    })
}

pub fn install_skill_package(
    package_id: &str,
    agent: Option<SkillAgentType>,
    scope: Option<SkillScope>,
    workspace_path: Option<&Path>,
) -> Result<SkillPackageInstallResult, AppError> {
    let agent = agent.unwrap_or(SkillAgentType::Codex);
    let scope = scope.unwrap_or(SkillScope::Global);
    if agent != SkillAgentType::Codex {
        return Err(package_operation_unsupported(
            "AI Switch Skill packages can currently be installed for Codex CLI only",
            Some(agent.as_str().to_string()),
        ));
    }
    let spec = package_specs()
        .iter()
        .copied()
        .find(|item| item.id == package_id)
        .ok_or_else(|| package_not_found(package_id))?;

    let storage_spec = skill_storage_spec(agent);
    let dirs = scoped_dirs(agent, scope, workspace_path)?;
    let target_root = dirs
        .iter()
        .find(|dir| !storage_spec.is_read_only_path(dir))
        .ok_or_else(|| {
            package_operation_unsupported(
                "No writable Skill directory is available for package installation",
                None,
            )
        })?;
    if storage_spec.is_read_only_path(target_root) {
        return Err(package_operation_unsupported(
            "Cannot install a Skill package into a read-only Skill directory",
            Some(target_root.display().to_string()),
        ));
    }

    let installed = installed_skill_index(agent, scope, workspace_path)?;
    let resource_root = resolve_resource_root()?;
    let package_resource_root = resource_root.join(spec.id);
    let mut installed_skill_ids = Vec::new();
    let mut skipped_skill_ids = Vec::new();
    for skill_id in spec.skill_ids {
        let safe_id = validate_skill_id(skill_id)?;
        if installed.contains_key(&safe_id) {
            skipped_skill_ids.push(safe_id);
            continue;
        }
        let source = package_resource_root.join(&safe_id);
        if !source.join("SKILL.md").is_file() {
            return Err(package_operation_unsupported(
                "Bundled Skill resource is missing SKILL.md",
                Some(source.display().to_string()),
            ));
        }
        let target = target_root.join(&safe_id);
        copy_dir_missing_only(&source, &target)?;
        installed_skill_ids.push(safe_id);
    }

    Ok(SkillPackageInstallResult {
        package_id: spec.id.to_string(),
        installed_skill_ids,
        skipped_skill_ids,
    })
}

#[cfg(test)]
fn package_result_from_installed(skills: Vec<SkillItem>) -> SkillsPackageListResult {
    aggregate_from_installed(
        skills
            .into_iter()
            .map(|skill| (skill.id.clone(), skill))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::super::model::{SkillLayout, SkillScope};
    use super::*;

    fn skill(id: &str) -> SkillItem {
        SkillItem {
            id: id.to_string(),
            name: id.to_string(),
            scope: SkillScope::Global,
            layout: SkillLayout::SkillDirectory,
            path: format!("/skills/{id}"),
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
    fn exposes_ai_switch_owned_packages() {
        let result = package_result_from_installed(Vec::new());

        assert!(result
            .packages
            .iter()
            .any(|package| package.id == "ai-switch.core"));
        assert!(result
            .packages
            .iter()
            .any(|package| package.id == "ai-switch.science"));
        assert!(result
            .packages
            .iter()
            .all(|package| package.source == SkillSource::Builtin));
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn marks_same_skill_id_as_installed_without_source_distinction() {
        let mut brainstorming = skill("brainstorming");
        brainstorming.source = SkillSource::Agents;
        let result = package_result_from_installed(vec![brainstorming]);
        let core = result
            .packages
            .iter()
            .find(|package| package.id == "ai-switch.core")
            .unwrap();

        assert_eq!(core.installed_skill_ids, vec!["brainstorming"]);
        assert_eq!(core.installed_count, 1);
        assert_eq!(core.skill_count, CORE_SKILL_IDS.len());
    }

    #[test]
    fn builtin_index_annotates_members_with_ai_switch_package_ids() {
        let index = builtin_package_index();

        assert_eq!(index.by_skill["brainstorming"].package_id, "ai-switch.core");
        assert_eq!(
            index.by_skill["statistical-analysis"].package_id,
            "ai-switch.science"
        );
    }

    #[test]
    fn install_package_copies_only_missing_skill_ids() {
        let resource_dir = tempfile::tempdir().unwrap();
        let target_dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(
            resource_dir
                .path()
                .join("ai-switch.core")
                .join("brainstorming"),
        )
        .unwrap();
        fs::write(
            resource_dir
                .path()
                .join("ai-switch.core")
                .join("brainstorming")
                .join("SKILL.md"),
            "---\nname: bundled\n---\n# bundled\n",
        )
        .unwrap();
        fs::create_dir_all(target_dir.path().join("brainstorming")).unwrap();
        fs::write(
            target_dir.path().join("brainstorming").join("SKILL.md"),
            "---\nname: existing\n---\n# existing\n",
        )
        .unwrap();
        copy_dir_missing_only(
            &resource_dir
                .path()
                .join("ai-switch.core")
                .join("brainstorming"),
            &target_dir.path().join("brainstorming"),
        )
        .unwrap();

        let content =
            fs::read_to_string(target_dir.path().join("brainstorming").join("SKILL.md")).unwrap();
        assert!(content.contains("existing"));
        assert!(!content.contains("bundled"));
    }
}

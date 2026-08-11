//! Skill directory resolution and path validation.

use std::path::{Path, PathBuf};

use super::model::{SkillAgentType, SkillLayout, SkillScope};
use crate::error::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillStorageKind {
    SkillDirectoryOnly,
    SkillDirectoryOrMarkdownFile,
}

#[derive(Debug, Clone)]
pub struct SkillStorageSpec {
    pub kind: SkillStorageKind,
    pub global_dirs: Vec<PathBuf>,
    pub project_rel_dirs: Vec<&'static str>,
    pub read_only_roots: Vec<PathBuf>,
}

fn home_dir() -> PathBuf {
    directories::BaseDirs::new()
        .map(|base| base.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

fn env_path(name: &str, fallback: PathBuf) -> PathBuf {
    let raw = std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string());
    let Some(value) = raw.filter(|value| !value.is_empty()) else {
        return fallback;
    };
    if value == "~" {
        return home_dir();
    }
    if let Some(rest) = value.strip_prefix("~/") {
        return home_dir().join(rest);
    }
    PathBuf::from(value)
}

pub fn skill_storage_spec(agent: SkillAgentType) -> SkillStorageSpec {
    let home = home_dir();
    match agent {
        SkillAgentType::ClaudeCode => SkillStorageSpec::directory_only(
            vec![home.join(".claude/skills")],
            vec![".claude/skills"],
            vec![],
        ),
        SkillAgentType::Codex => {
            let root = env_path("CODEX_HOME", home.join(".codex"));
            SkillStorageSpec::new(
                SkillStorageKind::SkillDirectoryOrMarkdownFile,
                vec![
                    root.join("skills"),
                    root.join("skills/.system"),
                    home.join(".agents/skills"),
                ],
                vec![".codex/skills", ".agents/skills"],
                vec![root.join("skills/.system")],
            )
        }
        SkillAgentType::Gemini => SkillStorageSpec::directory_only(
            vec![home.join(".gemini/skills"), home.join(".agents/skills")],
            vec![".gemini/skills", ".agents/skills"],
            vec![],
        ),
        SkillAgentType::Grok => {
            let root = env_path("GROK_HOME", home.join(".grok"));
            SkillStorageSpec::directory_only(
                vec![root.join("skills")],
                vec![".grok/skills"],
                vec![],
            )
        }
        SkillAgentType::OpenCode => SkillStorageSpec::directory_only(
            vec![
                home.join(".config/opencode/skills"),
                home.join(".agents/skills"),
            ],
            vec![".agents/skills", ".opencode/skills"],
            vec![],
        ),
        SkillAgentType::OpenClaw => SkillStorageSpec::directory_only(
            vec![home.join(".openclaw/skills")],
            vec!["skills"],
            vec![],
        ),
        SkillAgentType::Hermes => {
            SkillStorageSpec::directory_only(vec![home.join(".hermes/skills")], vec![], vec![])
        }
        SkillAgentType::Cline => SkillStorageSpec::directory_only(
            vec![home.join(".agents/skills"), home.join(".cline/skills")],
            vec![
                ".agents/skills",
                ".cline/skills",
                ".clinerules/skills",
                ".claude/skills",
            ],
            vec![],
        ),
        SkillAgentType::Cursor => SkillStorageSpec::directory_only(
            vec![
                home.join(".cursor/skills"),
                home.join(".agents/skills"),
                home.join(".cursor/skills-cursor"),
            ],
            vec![".cursor/skills", ".agents/skills"],
            vec![home.join(".cursor/skills-cursor")],
        ),
        SkillAgentType::KimiCode => {
            let root = env_path("KIMI_CODE_HOME", home.join(".kimi-code"));
            SkillStorageSpec::directory_only(
                vec![root.join("skills")],
                vec![".kimi-code/skills"],
                vec![],
            )
        }
        SkillAgentType::CodeBuddy => SkillStorageSpec::directory_only(
            vec![home.join(".codebuddy/skills")],
            vec![".codebuddy/skills"],
            vec![],
        ),
    }
}

impl SkillStorageSpec {
    fn new(
        kind: SkillStorageKind,
        global_dirs: Vec<PathBuf>,
        project_rel_dirs: Vec<&'static str>,
        read_only_roots: Vec<PathBuf>,
    ) -> Self {
        Self {
            kind,
            global_dirs,
            project_rel_dirs,
            read_only_roots,
        }
    }

    fn directory_only(
        global_dirs: Vec<PathBuf>,
        project_rel_dirs: Vec<&'static str>,
        read_only_roots: Vec<PathBuf>,
    ) -> Self {
        Self::new(
            SkillStorageKind::SkillDirectoryOnly,
            global_dirs,
            project_rel_dirs,
            read_only_roots,
        )
    }

    pub fn is_read_only_path(&self, path: &Path) -> bool {
        let candidate = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.read_only_roots.iter().any(|root| {
            let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
            candidate == root || candidate.starts_with(root)
        })
    }
}

pub fn validate_skill_id(id: &str) -> Result<String, AppError> {
    let trimmed = id.trim();
    if trimmed.is_empty()
        || trimmed == "."
        || trimmed == ".."
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed.chars().any(|char| char.is_control())
    {
        return Err(AppError::Validation {
            code: "skills.invalid_id",
            message: "Skill id is not a safe file name".to_string(),
            details: None,
            recoverable: true,
        });
    }
    Ok(trimmed.to_string())
}

pub fn scoped_dirs(
    agent: SkillAgentType,
    scope: SkillScope,
    workspace_path: Option<&Path>,
) -> Result<Vec<PathBuf>, AppError> {
    let spec = skill_storage_spec(agent);
    match scope {
        SkillScope::Global => Ok(spec.global_dirs),
        SkillScope::Project => {
            let root = workspace_path.ok_or_else(|| AppError::Validation {
                code: "skills.path_invalid",
                message: "Project directory is required".to_string(),
                details: None,
                recoverable: true,
            })?;
            if !root.is_dir() {
                return Err(AppError::Validation {
                    code: "skills.directory_missing",
                    message: "Project directory does not exist".to_string(),
                    details: Some(root.display().to_string()),
                    recoverable: true,
                });
            }
            Ok(spec
                .project_rel_dirs
                .into_iter()
                .map(|relative| root.join(relative))
                .collect())
        }
    }
}

pub fn content_path(layout: SkillLayout, path: &Path) -> PathBuf {
    match layout {
        SkillLayout::SkillDirectory => path.join("SKILL.md"),
        SkillLayout::MarkdownFile => path.to_path_buf(),
    }
}

pub fn resolve_skill_path(root: &Path, id: &str, layout: SkillLayout) -> Result<PathBuf, AppError> {
    let id = validate_skill_id(id)?;
    let path = match layout {
        SkillLayout::SkillDirectory => root.join(&id),
        SkillLayout::MarkdownFile => root.join(format!("{id}.md")),
    };
    let parent = path.parent().unwrap_or(root);
    let parent_canonical = parent
        .canonicalize()
        .unwrap_or_else(|_| parent.to_path_buf());
    let root_canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    if parent_canonical != root_canonical && !parent_canonical.starts_with(&root_canonical) {
        return Err(AppError::Validation {
            code: "skills.path_invalid",
            message: "Skill path escapes its storage directory".to_string(),
            details: None,
            recoverable: true,
        });
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsafe_ids() {
        assert!(validate_skill_id("../outside").is_err());
        assert!(validate_skill_id("nested/skill").is_err());
        assert_eq!(validate_skill_id("demo-skill").unwrap(), "demo-skill");
    }

    #[test]
    fn codex_marks_system_directory_read_only() {
        let spec = skill_storage_spec(SkillAgentType::Codex);
        let path = spec.read_only_roots[0].join("imagegen");
        assert!(spec.is_read_only_path(&path));
    }
}

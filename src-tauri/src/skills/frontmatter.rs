//! Skill Markdown front matter parsing.

use serde::Deserialize;

use crate::error::AppError;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SkillMetadata {
    pub name: Option<String>,
    pub description: Option<String>,
}

pub fn parse_skill_metadata(content: &str) -> Result<Option<SkillMetadata>, AppError> {
    let Some(rest) = content.strip_prefix("---") else {
        return Ok(None);
    };
    let Some(end) = rest.find("\n---") else {
        return Ok(None);
    };
    let raw = &rest[..end];
    serde_yaml::from_str(raw)
        .map(Some)
        .map_err(|error| AppError::Validation {
            code: "skills.config_invalid",
            message: "Skill front matter is invalid".to_string(),
            details: Some(error.to_string()),
            recoverable: true,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_name_and_description() {
        let value = parse_skill_metadata("---\nname: demo\ndescription: hello\n---\n# Body")
            .unwrap()
            .unwrap();
        assert_eq!(value.name.as_deref(), Some("demo"));
        assert_eq!(value.description.as_deref(), Some("hello"));
    }
}

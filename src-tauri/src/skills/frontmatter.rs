//! Skill Markdown front matter parsing.

use serde::Deserialize;

use crate::error::AppError;

#[derive(Debug, Clone, Default)]
pub struct SkillMetadata {
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub language: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RawSkillMetadata {
    name: Option<String>,
    display_name: Option<String>,
    description: Option<String>,
    category: Option<String>,
    tags: Option<RawTags>,
    language: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum RawTags {
    List(Vec<String>),
    Text(String),
}

impl RawSkillMetadata {
    fn into_metadata(self) -> SkillMetadata {
        let tags = match self.tags {
            Some(RawTags::List(values)) => values,
            Some(RawTags::Text(value)) => value.split(',').map(str::to_string).collect(),
            None => Vec::new(),
        };
        let mut unique_tags = Vec::new();
        for tag in tags
            .into_iter()
            .map(|tag| tag.trim().to_string())
            .filter(|tag| !tag.is_empty())
        {
            if !unique_tags.contains(&tag) {
                unique_tags.push(tag);
            }
        }
        SkillMetadata {
            name: self.name,
            display_name: self.display_name,
            description: self.description,
            category: self.category,
            tags: unique_tags,
            language: self.language,
        }
    }
}

pub fn parse_skill_metadata(content: &str) -> Result<Option<SkillMetadata>, AppError> {
    let Some(rest) = content.strip_prefix("---") else {
        return Ok(None);
    };
    let Some(end) = rest.find("\n---") else {
        return Ok(None);
    };
    let raw = &rest[..end];
    serde_yaml::from_str::<RawSkillMetadata>(raw)
        .map(|value| Some(value.into_metadata()))
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

    #[test]
    fn parses_optional_metadata_and_comma_separated_tags() {
        let value = parse_skill_metadata(
            "---\nname: demo\ndisplay_name: Demo\ncategory: tools\ntags: filesystem, io, filesystem\nlanguage: en\n---\n# Body",
        )
        .unwrap()
        .unwrap();
        assert_eq!(value.display_name.as_deref(), Some("Demo"));
        assert_eq!(value.category.as_deref(), Some("tools"));
        assert_eq!(value.tags, vec!["filesystem", "io"]);
        assert_eq!(value.language.as_deref(), Some("en"));
    }
}

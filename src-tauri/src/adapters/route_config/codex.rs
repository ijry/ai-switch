use super::{
    existing_text, generated_invalid, invalid_existing_config, RouteConfigInput, TargetAdapter,
    TargetInspection,
};
use crate::{error::AppError, models::platform::PlatformId};
use std::path::{Path, PathBuf};
use toml_edit::{value, Document, Item, Table};

pub(super) struct CodexAdapter;

pub(crate) const CODEX_MODEL_CATALOG_FILENAME: &str = "ai-switch-model-catalog.json";

pub(crate) fn codex_model_catalog_path(home: &Path) -> PathBuf {
    home.join(".codex").join(CODEX_MODEL_CATALOG_FILENAME)
}

impl TargetAdapter for CodexAdapter {
    fn target_key(&self) -> &'static str {
        "codex"
    }

    fn client_key(&self) -> &'static str {
        "codex"
    }

    fn client_display_name(&self) -> &'static str {
        "Codex CLI"
    }

    fn native(&self) -> bool {
        true
    }

    fn restart_required(&self) -> bool {
        false
    }

    fn requires_client_models(&self) -> bool {
        false
    }

    fn platform(&self) -> PlatformId {
        PlatformId::Codex
    }

    fn resolve_path(&self, home: &Path) -> PathBuf {
        home.join(".codex").join("config.toml")
    }

    fn render(
        &self,
        path: &Path,
        existing: Option<&[u8]>,
        input: &RouteConfigInput,
    ) -> Result<Vec<u8>, AppError> {
        let mut document = match existing {
            Some(bytes) => {
                let content = existing_text(path, "TOML", bytes)?;
                if content.trim().is_empty() {
                    Document::new()
                } else {
                    content
                        .parse::<Document>()
                        .map_err(|_| invalid_existing_config(path, "TOML", "syntax is invalid"))?
                }
            }
            None => Document::new(),
        };

        apply_managed_config(&mut document, path, input)?;
        let rendered = document.to_string();
        rendered
            .parse::<Document>()
            .map_err(|_| generated_invalid(path, "TOML"))?;
        Ok(rendered.into_bytes())
    }

    fn inspect(&self, _path: &Path, existing: Option<&[u8]>) -> TargetInspection {
        let Some(bytes) = existing else {
            return TargetInspection::missing();
        };
        let Ok(content) = std::str::from_utf8(bytes) else {
            return TargetInspection::invalid();
        };
        let Ok(document) = content.parse::<Document>() else {
            return TargetInspection::invalid();
        };

        let provider_selected = document
            .get("model_provider")
            .and_then(Item::as_str)
            .is_some_and(|value| value == "ai-switch");
        let provider_defined = document
            .get("model_providers")
            .and_then(Item::as_table)
            .and_then(|providers| providers.get("ai-switch"))
            .and_then(Item::as_table)
            .is_some();

        TargetInspection::valid(provider_selected && provider_defined)
    }
}

fn apply_managed_config(
    document: &mut Document,
    path: &Path,
    input: &RouteConfigInput,
) -> Result<(), AppError> {
    document["model_provider"] = value("ai-switch");
    document["model_catalog_json"] = value(CODEX_MODEL_CATALOG_FILENAME);
    if document.get("model_providers").is_none() {
        document["model_providers"] = Item::Table(Table::new());
    }
    let providers = document["model_providers"].as_table_mut().ok_or_else(|| {
        invalid_existing_config(
            path,
            "TOML",
            "model_providers must be a table to add the ai-switch provider",
        )
    })?;
    if !providers.contains_key("ai-switch") {
        providers.insert("ai-switch", Item::Table(Table::new()));
    }
    let provider = providers["ai-switch"].as_table_mut().ok_or_else(|| {
        invalid_existing_config(path, "TOML", "model_providers.ai-switch must be a table")
    })?;
    provider["name"] = value("AI Switch Route Proxy");
    provider["base_url"] = value(codex_route_proxy_base_url(&input.base_url));
    provider["wire_api"] = value("responses");
    provider["experimental_bearer_token"] = value(&input.route_proxy_key);
    provider.remove("api_key");
    Ok(())
}

fn codex_route_proxy_base_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if base_last_path_segment(trimmed).is_some_and(|segment| segment.eq_ignore_ascii_case("v1")) {
        trimmed.to_string()
    } else {
        format!("{trimmed}/v1")
    }
}

fn base_last_path_segment(base_url: &str) -> Option<&str> {
    let after_scheme = base_url
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(base_url);
    let path = after_scheme.split_once('/').map(|(_, path)| path)?;
    path.trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .next_back()
}

#[cfg(test)]
mod tests {
    use super::codex_route_proxy_base_url;

    #[test]
    fn route_proxy_base_url_adds_v1_once() {
        assert_eq!(
            codex_route_proxy_base_url("http://127.0.0.1:43111"),
            "http://127.0.0.1:43111/v1"
        );
        assert_eq!(
            codex_route_proxy_base_url("http://127.0.0.1:43111/v1/"),
            "http://127.0.0.1:43111/v1"
        );
    }
}

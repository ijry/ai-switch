use crate::database::repositories::batch_repository::BatchRepository;
use crate::error::AppError;
use crate::models::batch::NewBatch;
use crate::models::provider::NewProvider;
use crate::models::provider_preset::{
    CreateProviderFromPresetOutcome, CreateProviderFromPresetRequest, ProviderPreset,
};
use crate::services::batch_service::BatchService;
use sqlx::SqlitePool;

pub struct ProviderPresetService;

impl ProviderPresetService {
    pub fn list_presets() -> Vec<ProviderPreset> {
        provider_presets()
    }

    pub async fn create_provider_from_preset(
        pool: &SqlitePool,
        request: CreateProviderFromPresetRequest,
    ) -> Result<CreateProviderFromPresetOutcome, AppError> {
        let preset = provider_presets()
            .into_iter()
            .find(|preset| preset.id == request.preset_id)
            .ok_or_else(|| AppError::Validation {
                code: "validation.provider_preset_not_found",
                message: "Provider preset was not found".to_string(),
                details: Some(request.preset_id.clone()),
                recoverable: true,
            })?;

        let batch_id = match request.batch_name.as_deref().map(str::trim) {
            Some(batch_name) if !batch_name.is_empty() => {
                let batch = BatchRepository::create(
                    pool,
                    NewBatch {
                        name: batch_name.to_string(),
                        source: "provider_preset".to_string(),
                        notes: Some(preset.name.clone()),
                    },
                )
                .await?;
                Some(batch.id)
            }
            _ => None,
        };

        let secret_ref = preset
            .secret_env_key
            .as_deref()
            .map(|env_key| format!("env://{env_key}"));
        let provider = BatchService::create_provider(
            pool,
            NewProvider {
                name: preset.name,
                kind: preset.kind,
                base_url: preset.base_url,
                model_config_json: preset.model_config_json,
                target_options_json: preset.target_options_json,
                secret_ref,
            },
            batch_id.clone(),
        )
        .await?;

        Ok(CreateProviderFromPresetOutcome { provider, batch_id })
    }
}

fn provider_presets() -> Vec<ProviderPreset> {
    vec![
        ProviderPreset {
            id: "openai-compatible".to_string(),
            name: "OpenAI Compatible".to_string(),
            description: "Generic OpenAI-compatible API using OPENAI_API_KEY.".to_string(),
            kind: "openai_compatible".to_string(),
            base_url: Some("https://api.openai.com/v1".to_string()),
            model_config_json: r#"{"default":"gpt-4.1","model_name":"GPT 4.1"}"#.to_string(),
            target_options_json: r#"{"env_key":"OPENAI_API_KEY","codex":{"env_key":"OPENAI_API_KEY"},"opencode":{"env_key":"OPENAI_API_KEY","model":"gpt-4.1","model_name":"GPT 4.1"}}"#.to_string(),
            secret_env_key: Some("OPENAI_API_KEY".to_string()),
        },
        ProviderPreset {
            id: "local-openai-compatible".to_string(),
            name: "Local OpenAI Compatible".to_string(),
            description: "Local OpenAI-compatible endpoint for tools like Ollama, LM Studio, or a proxy.".to_string(),
            kind: "openai_compatible".to_string(),
            base_url: Some("http://127.0.0.1:11434/v1".to_string()),
            model_config_json: r#"{"default":"qwen2.5-coder","model_name":"Qwen Coder"}"#.to_string(),
            target_options_json: r#"{"env_key":"OPENAI_API_KEY","codex":{"env_key":"OPENAI_API_KEY"},"opencode":{"env_key":"OPENAI_API_KEY","model":"qwen2.5-coder","model_name":"Qwen Coder"}}"#.to_string(),
            secret_env_key: Some("OPENAI_API_KEY".to_string()),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::repositories::batch_repository::BatchRepository;
    use crate::database::{create_memory_pool, run_migrations};

    #[test]
    fn list_presets_returns_builtin_presets_without_raw_secrets() {
        let presets = ProviderPresetService::list_presets();

        assert!(presets
            .iter()
            .any(|preset| preset.id == "openai-compatible"));
        assert!(presets
            .iter()
            .all(|preset| !preset.target_options_json.contains("sk-")));
    }

    #[tokio::test]
    async fn create_provider_from_preset_adds_provider_to_named_batch() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let outcome = ProviderPresetService::create_provider_from_preset(
            &pool,
            CreateProviderFromPresetRequest {
                preset_id: "openai-compatible".to_string(),
                batch_name: Some("Preset Batch".to_string()),
            },
        )
        .await
        .expect("preset");
        let groups = BatchRepository::list_groups(&pool, Some("Preset Batch"))
            .await
            .expect("groups");

        assert_eq!(outcome.provider.name, "OpenAI Compatible");
        assert_eq!(
            outcome.provider.secret_ref.as_deref(),
            Some("env://OPENAI_API_KEY")
        );
        assert!(outcome.batch_id.is_some());
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].children.len(), 1);
        assert_eq!(groups[0].children[0].title, "OpenAI Compatible");
    }

    #[tokio::test]
    async fn create_provider_from_preset_rejects_unknown_preset() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let error = ProviderPresetService::create_provider_from_preset(
            &pool,
            CreateProviderFromPresetRequest {
                preset_id: "missing".to_string(),
                batch_name: None,
            },
        )
        .await
        .expect_err("error");

        assert_eq!(error.code(), "validation.provider_preset_not_found");
    }
}

use super::models::*;
use super::protocol::{openai_images_body, parse_openai_images};
use super::repository::ImageGenerationRepository;
use super::storage::store_image;
use crate::app_state::AppState;
use crate::error::AppError;
use crate::services::route_model_capability::parse_model_capability;
use crate::services::route_proxy_service::{
    build_proxy_state, forward_request, load_pool_candidates,
};
use axum::body::{to_bytes, Body};
use axum::http::{HeaderMap, HeaderValue, Method, Uri};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GenerateImageInput {
    pub session_id: String,
    pub model: String,
    pub prompt: String,
    #[serde(default = "default_count")]
    pub count: u32,
    #[serde(default = "default_size")]
    pub size: String,
    #[serde(default = "default_quality")]
    pub quality: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GenerateImageOutcome {
    pub message: ImageMessage,
    pub assets: Vec<ImageAsset>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImageAssetContent {
    pub asset: ImageAsset,
    pub data_base64: String,
}

pub struct ImageGenerationService;

impl ImageGenerationService {
    pub async fn list_models(
        state: &AppState,
        platform: &str,
    ) -> Result<Vec<ImageModelOption>, AppError> {
        if !matches!(platform, "codex" | "gemini") {
            return Err(validation(
                "validation.image_platform",
                "Image generation supports Codex and Gemini groups",
            ));
        }
        let candidates = load_pool_candidates(&state.pool, platform).await?;
        let mut models = BTreeMap::<String, ImageModelOption>::new();
        for candidate in candidates {
            for mapping in parse_model_capability(&candidate.credential.config_json).mappings {
                let capabilities = mapping
                    .capabilities
                    .iter()
                    .map(|item| item.trim().to_string())
                    .filter(|item| item.starts_with("image."))
                    .collect::<Vec<_>>();
                if capabilities.is_empty() {
                    continue;
                }
                models
                    .entry(mapping.from.to_ascii_lowercase())
                    .or_insert(ImageModelOption {
                        id: mapping.from,
                        upstream_model: mapping.to,
                        capabilities,
                    });
            }
        }
        Ok(models.into_values().collect())
    }

    pub async fn generate(
        state: &AppState,
        input: GenerateImageInput,
    ) -> Result<GenerateImageOutcome, AppError> {
        let session =
            ImageGenerationRepository::get_session(&state.pool, &input.session_id).await?;
        let prompt = input.prompt.trim();
        let model = input.model.trim();
        if prompt.is_empty() || model.is_empty() || !(1..=10).contains(&input.count) {
            return Err(validation(
                "validation.image_request",
                "Prompt, model, and an image count from 1 to 10 are required",
            ));
        }
        let available = Self::list_models(state, &session.platform).await?;
        if !available.iter().any(|item| {
            item.id.eq_ignore_ascii_case(model)
                && item.capabilities.iter().any(|cap| cap == "image.generate")
        }) {
            return Err(validation(
                "imagegen.model_unavailable",
                "The selected image model is not available in the active group",
            ));
        }
        let request_json = serde_json::to_string(&input)?;
        ImageGenerationRepository::add_message(
            &state.pool,
            NewImageMessage {
                session_id: session.id.clone(),
                role: "user".into(),
                prompt: prompt.into(),
                request_json: request_json.clone(),
                status: "completed".into(),
                model: Some(model.into()),
            },
        )
        .await?;
        let assistant = ImageGenerationRepository::add_message(
            &state.pool,
            NewImageMessage {
                session_id: session.id.clone(),
                role: "assistant".into(),
                prompt: String::new(),
                request_json,
                status: "running".into(),
                model: Some(model.into()),
            },
        )
        .await?;
        match Self::execute(
            state,
            &session.platform,
            model,
            prompt,
            input.count,
            &input.size,
            &input.quality,
        )
        .await
        {
            Ok((response_id, images)) => {
                let mut assets = Vec::with_capacity(images.len());
                for (index, image) in images.iter().enumerate() {
                    let pending = store_image(
                        &state.paths.imagegen_dir,
                        &session.id,
                        &assistant.id,
                        index,
                        image,
                    )
                    .await?;
                    assets.push(ImageGenerationRepository::add_asset(&state.pool, pending).await?);
                }
                let message = ImageGenerationRepository::finish_message(
                    &state.pool,
                    &assistant.id,
                    "completed",
                    None,
                    None,
                    response_id.as_deref(),
                    None,
                    assets.len() as i64,
                )
                .await?;
                Ok(GenerateImageOutcome { message, assets })
            }
            Err(error) => {
                let _ = ImageGenerationRepository::finish_message(
                    &state.pool,
                    &assistant.id,
                    "failed",
                    None,
                    None,
                    None,
                    Some(&error),
                    0,
                )
                .await;
                Err(validation("imagegen.generate_failed", &error))
            }
        }
    }

    async fn execute(
        state: &AppState,
        platform: &str,
        model: &str,
        prompt: &str,
        count: u32,
        size: &str,
        quality: &str,
    ) -> Result<(Option<String>, Vec<super::protocol::GeneratedImage>), String> {
        let proxy = build_proxy_state(state.pool.clone(), &state.route_proxy);
        let mut headers = HeaderMap::new();
        headers.insert("content-type", HeaderValue::from_static("application/json"));
        headers.insert(
            "x-ai-switch-platform",
            HeaderValue::from_str(platform).map_err(|error| error.to_string())?,
        );
        let body = openai_images_body(model, prompt, count, size, quality);
        let response = forward_request(
            &proxy,
            Method::POST,
            headers,
            Uri::from_static("/v1/images/generations"),
            Body::from(body),
        )
        .await?;
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 128 * 1024 * 1024)
            .await
            .map_err(|error| error.to_string())?;
        if !status.is_success() {
            return Err(format!(
                "Image upstream returned {}: {}",
                status.as_u16(),
                String::from_utf8_lossy(&bytes)
                    .chars()
                    .take(500)
                    .collect::<String>()
            ));
        }
        let value: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        let response_id = value
            .get("id")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned);
        let images = parse_openai_images(&bytes)?;
        if images.is_empty() {
            return Err("Image upstream returned no images".to_string());
        }
        Ok((response_id, images))
    }

    pub async fn read_asset(
        state: &AppState,
        asset_id: &str,
    ) -> Result<ImageAssetContent, AppError> {
        let asset = ImageGenerationRepository::get_asset(&state.pool, asset_id).await?;
        let path =
            super::storage::resolve_asset_path(&state.paths.imagegen_dir, &asset.relative_path)?;
        let bytes = tokio::fs::read(path).await?;
        Ok(ImageAssetContent {
            asset,
            data_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
        })
    }
}

fn default_count() -> u32 {
    1
}
fn default_size() -> String {
    "1024x1024".to_string()
}
fn default_quality() -> String {
    "auto".to_string()
}
fn validation(code: &'static str, message: &str) -> AppError {
    AppError::Validation {
        code,
        message: message.to_string(),
        details: None,
        recoverable: true,
    }
}

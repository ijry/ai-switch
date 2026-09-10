use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GeneratedImage {
    pub bytes: Vec<u8>,
    pub mime_type: String,
    pub revised_prompt: Option<String>,
}

pub fn openai_images_body(
    model: &str,
    prompt: &str,
    count: u32,
    size: &str,
    quality: &str,
) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "model": model,
        "prompt": prompt,
        "n": count,
        "size": size,
        "quality": quality,
        "response_format": "b64_json"
    }))
    .expect("static image request serializes")
}

pub fn responses_image_body(
    model: &str,
    prompt: &str,
    count: u32,
    size: &str,
    quality: &str,
) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "model": model,
        "input": prompt,
        "tools": [{"type":"image_generation","size":size,"quality":quality,"output_format":"png"}],
        "tool_choice": {"type":"image_generation"},
        "metadata": {"requested_images": count}
    }))
    .expect("static Responses image request serializes")
}

pub fn gemini_image_body(prompt: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "contents": [{"role":"user","parts":[{"text":prompt}]}],
        "generationConfig": {"responseModalities":["TEXT","IMAGE"]}
    }))
    .expect("static Gemini image request serializes")
}

pub fn openai_request_to_responses(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|error| format!("Invalid OpenAI image request: {error}"))?;
    let model = required_request_string(&value, "model")?;
    let prompt = required_request_string(&value, "prompt")?;
    let count = value
        .get("n")
        .and_then(Value::as_u64)
        .unwrap_or(1)
        .clamp(1, 10) as u32;
    let size = value
        .get("size")
        .and_then(Value::as_str)
        .unwrap_or("1024x1024");
    let quality = value
        .get("quality")
        .and_then(Value::as_str)
        .unwrap_or("auto");
    Ok(responses_image_body(model, prompt, count, size, quality))
}

pub fn openai_request_to_gemini(bytes: &[u8]) -> Result<(String, Vec<u8>), String> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|error| format!("Invalid OpenAI image request: {error}"))?;
    let model = required_request_string(&value, "model")?.to_string();
    let prompt = required_request_string(&value, "prompt")?;
    Ok((model, gemini_image_body(prompt)))
}

pub fn responses_to_openai_images(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|error| format!("Invalid Responses image response: {error}"))?;
    let id = value.get("id").cloned().unwrap_or(Value::Null);
    let images = parse_responses_images(bytes)?;
    let data = images
        .into_iter()
        .map(|image| {
            json!({
                "b64_json": base64::engine::general_purpose::STANDARD.encode(image.bytes),
                "mime_type": image.mime_type,
                "revised_prompt": image.revised_prompt
            })
        })
        .collect::<Vec<_>>();
    serde_json::to_vec(&json!({"created": chrono::Utc::now().timestamp(), "id": id, "data": data}))
        .map_err(|error| format!("Could not serialize OpenAI image response: {error}"))
}

pub fn gemini_to_openai_images(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|error| format!("Invalid Gemini image response: {error}"))?;
    let id = value.get("responseId").cloned().unwrap_or(Value::Null);
    let images = parse_gemini_images(bytes)?;
    let data = images
        .into_iter()
        .map(|image| {
            json!({
                "b64_json": base64::engine::general_purpose::STANDARD.encode(image.bytes),
                "mime_type": image.mime_type
            })
        })
        .collect::<Vec<_>>();
    serde_json::to_vec(&json!({"created": chrono::Utc::now().timestamp(), "id": id, "data": data}))
        .map_err(|error| format!("Could not serialize OpenAI image response: {error}"))
}

fn required_request_string<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .ok_or_else(|| format!("Image request is missing {key}"))
}

pub fn parse_openai_images(bytes: &[u8]) -> Result<Vec<GeneratedImage>, String> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|error| format!("Invalid OpenAI image response: {error}"))?;
    let entries = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| "OpenAI image response is missing data".to_string())?;
    entries
        .iter()
        .map(|entry| {
            let encoded = entry
                .get("b64_json")
                .and_then(Value::as_str)
                .ok_or_else(|| "OpenAI image response did not contain b64_json".to_string())?;
            Ok(GeneratedImage {
                bytes: decode_base64(encoded)?,
                mime_type: entry
                    .get("mime_type")
                    .and_then(Value::as_str)
                    .unwrap_or("image/png")
                    .to_string(),
                revised_prompt: entry
                    .get("revised_prompt")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
            })
        })
        .collect()
}

pub fn parse_responses_images(bytes: &[u8]) -> Result<Vec<GeneratedImage>, String> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|error| format!("Invalid Responses image response: {error}"))?;
    let output = value
        .get("output")
        .and_then(Value::as_array)
        .ok_or_else(|| "Responses image response is missing output".to_string())?;
    output
        .iter()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("image_generation_call"))
        .map(|item| {
            let encoded = item
                .get("result")
                .and_then(Value::as_str)
                .ok_or_else(|| "Responses image result is missing".to_string())?;
            Ok(GeneratedImage {
                bytes: decode_base64(encoded)?,
                mime_type: "image/png".to_string(),
                revised_prompt: None,
            })
        })
        .collect()
}

pub fn parse_gemini_images(bytes: &[u8]) -> Result<Vec<GeneratedImage>, String> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|error| format!("Invalid Gemini image response: {error}"))?;
    let parts = value
        .pointer("/candidates/0/content/parts")
        .and_then(Value::as_array)
        .ok_or_else(|| "Gemini image response is missing candidate parts".to_string())?;
    parts
        .iter()
        .filter_map(|part| part.get("inlineData").or_else(|| part.get("inline_data")))
        .map(|inline| {
            let encoded = inline
                .get("data")
                .and_then(Value::as_str)
                .ok_or_else(|| "Gemini inline image is missing data".to_string())?;
            Ok(GeneratedImage {
                bytes: decode_base64(encoded)?,
                mime_type: inline
                    .get("mimeType")
                    .or_else(|| inline.get("mime_type"))
                    .and_then(Value::as_str)
                    .unwrap_or("image/png")
                    .to_string(),
                revised_prompt: None,
            })
        })
        .collect()
}

fn decode_base64(value: &str) -> Result<Vec<u8>, String> {
    base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|error| format!("Image base64 is invalid: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_openai_and_gemini_inline_images() {
        let openai =
            parse_openai_images(br#"{"data":[{"b64_json":"aGVsbG8=","revised_prompt":"fox"}]}"#)
                .expect("openai");
        assert_eq!(openai[0].bytes, b"hello");
        let gemini = parse_gemini_images(br#"{"candidates":[{"content":{"parts":[{"inlineData":{"mimeType":"image/webp","data":"aGVsbG8="}}]}}]}"#).expect("gemini");
        assert_eq!(gemini[0].mime_type, "image/webp");
    }
}

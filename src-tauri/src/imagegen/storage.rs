use super::models::NewImageAsset;
use super::protocol::GeneratedImage;
use crate::error::AppError;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

const MAX_IMAGE_BYTES: usize = 32 * 1024 * 1024;

pub async fn store_image(
    root: &Path,
    session_id: &str,
    message_id: &str,
    index: usize,
    image: &GeneratedImage,
) -> Result<NewImageAsset, AppError> {
    if image.bytes.is_empty()
        || image.bytes.len() > MAX_IMAGE_BYTES
        || !image.mime_type.starts_with("image/")
    {
        return Err(AppError::Validation {
            code: "validation.image_asset",
            message: "Generated image is empty, too large, or has an invalid MIME type".to_string(),
            details: None,
            recoverable: true,
        });
    }
    let extension = extension_for_mime(&image.mime_type).ok_or_else(|| AppError::Validation {
        code: "validation.image_mime",
        message: "Generated image format is not supported".to_string(),
        details: Some(image.mime_type.clone()),
        recoverable: true,
    })?;
    let directory = root.join(session_id).join(message_id);
    tokio::fs::create_dir_all(&directory).await?;
    let name = format!("{:03}.{}", index + 1, extension);
    let path = directory.join(&name);
    tokio::fs::write(&path, &image.bytes).await?;
    let relative_path = PathBuf::from(session_id)
        .join(message_id)
        .join(name)
        .to_string_lossy()
        .replace('\\', "/");
    Ok(NewImageAsset {
        session_id: session_id.to_string(),
        message_id: message_id.to_string(),
        relative_path,
        mime_type: image.mime_type.clone(),
        sha256: format!("{:x}", Sha256::digest(&image.bytes)),
        width: None,
        height: None,
        byte_size: image.bytes.len() as i64,
    })
}

pub async fn delete_session_assets(root: &Path, session_id: &str) -> Result<(), AppError> {
    let directory = root.join(session_id);
    match tokio::fs::remove_dir_all(directory).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

pub fn resolve_asset_path(root: &Path, relative_path: &str) -> Result<PathBuf, AppError> {
    let path = Path::new(relative_path);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(AppError::Validation {
            code: "validation.image_asset_path",
            message: "Image asset path is invalid".to_string(),
            details: None,
            recoverable: false,
        });
    }
    Ok(root.join(path))
}

fn extension_for_mime(mime: &str) -> Option<&'static str> {
    match mime {
        "image/png" => Some("png"),
        "image/jpeg" => Some("jpg"),
        "image/webp" => Some("webp"),
        "image/gif" => Some("gif"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_parent_traversal() {
        assert!(resolve_asset_path(Path::new("images"), "../secret").is_err());
    }
}

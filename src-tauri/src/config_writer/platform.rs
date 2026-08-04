use std::{
    fs::{Metadata, Permissions},
    path::Path,
};

pub(super) fn metadata_is_link_or_reparse_point(metadata: &Metadata) -> bool {
    #[cfg(unix)]
    {
        metadata.file_type().is_symlink()
    }

    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }

    #[cfg(not(any(unix, windows)))]
    {
        metadata.file_type().is_symlink()
    }
}

pub(super) async fn apply_config_permissions(
    path: &Path,
    existing: Option<&Permissions>,
) -> std::io::Result<()> {
    if let Some(permissions) = existing {
        return tokio::fs::set_permissions(path, permissions.clone()).await;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(path, Permissions::from_mode(0o600)).await?;
    }

    Ok(())
}

pub(super) async fn apply_private_permissions(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(path, Permissions::from_mode(0o600)).await?;
    }

    #[cfg(not(unix))]
    let _ = path;

    Ok(())
}

pub(super) async fn replace_temp_file(
    temp_path: &Path,
    target_path: &Path,
    target_existed: bool,
) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        replace_temp_file_windows(temp_path, target_path, target_existed)?;
    }

    #[cfg(not(windows))]
    {
        let _ = target_existed;
        tokio::fs::rename(temp_path, target_path).await?;
        let parent = target_path.parent().unwrap_or_else(|| Path::new("."));
        sync_parent(parent).await?;
    }

    Ok(())
}

pub(super) async fn sync_parent(parent: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        tokio::fs::File::open(parent).await?.sync_all().await?;
    }

    #[cfg(not(unix))]
    let _ = parent;

    Ok(())
}

#[cfg(windows)]
fn replace_temp_file_windows(
    temp_path: &Path,
    target_path: &Path,
    target_existed: bool,
) -> std::io::Result<()> {
    use std::{os::windows::ffi::OsStrExt, ptr};
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, ReplaceFileW, MOVEFILE_WRITE_THROUGH,
    };

    let temp: Vec<u16> = temp_path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let target: Vec<u16> = target_path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let replaced = unsafe {
        if target_existed {
            ReplaceFileW(
                target.as_ptr(),
                temp.as_ptr(),
                ptr::null(),
                0,
                ptr::null(),
                ptr::null(),
            )
        } else {
            MoveFileExW(temp.as_ptr(), target.as_ptr(), MOVEFILE_WRITE_THROUGH)
        }
    };
    if replaced == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

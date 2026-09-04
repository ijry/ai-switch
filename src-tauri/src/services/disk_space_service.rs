use crate::models::disk_space::{DiskSpaceStatus, DiskVolumeSpace, LOW_DISK_SPACE_THRESHOLD_BYTES};
use std::path::{Path, PathBuf};

/// Reads free space on the volumes the app cannot work without.
///
/// Deliberately read-only and infallible at the command boundary: a machine that
/// refuses to report disk space should show no warning, not an error dialog.
pub struct DiskSpaceService;

impl DiskSpaceService {
    /// Free space on every monitored volume, flagged against the shipped 1 GiB
    /// threshold.
    pub fn status(data_dir: &Path) -> DiskSpaceStatus {
        Self::status_with_threshold(data_dir, LOW_DISK_SPACE_THRESHOLD_BYTES)
    }

    /// `status` with the threshold injected, so tests can force both verdicts
    /// without depending on how full the machine running them happens to be.
    pub fn status_with_threshold(data_dir: &Path, threshold_bytes: u64) -> DiskSpaceStatus {
        let mut volumes: Vec<DiskVolumeSpace> = Vec::new();
        let mut seen: Vec<String> = Vec::new();

        // The system drive is what the user thinks of as "the disk"; the data dir
        // is what actually breaks the app. They are the same volume on a standard
        // Windows or macOS install, but `/home` is frequently its own partition,
        // so probing only one of them would miss the other.
        for candidate in [system_root(), data_dir.to_path_buf()] {
            // A first run has not created `~/.ai-switch` yet, and both platforms
            // refuse to report space for a path that does not exist.
            let Some(probe) = existing_ancestor(&candidate) else {
                continue;
            };
            let Some(key) = volume_key(&probe) else {
                continue;
            };
            if seen.iter().any(|entry| entry == &key) {
                continue;
            }

            let (total_bytes, available_bytes) = match volume_space(&probe) {
                Ok(space) => space,
                Err(error) => {
                    eprintln!("could not read free space for {}: {error}", probe.display());
                    continue;
                }
            };

            seen.push(key);
            volumes.push(DiskVolumeSpace {
                label: volume_label(&probe),
                path: probe.display().to_string(),
                total_bytes,
                available_bytes,
                low: available_bytes < threshold_bytes,
            });
        }

        DiskSpaceStatus {
            threshold_bytes,
            low: volumes.iter().any(|volume| volume.low),
            volumes,
        }
    }
}

/// The root of the volume the OS itself is installed on.
fn system_root() -> PathBuf {
    #[cfg(windows)]
    {
        // `%SystemDrive%` is the bare letter and colon ("C:"), which names the
        // process' current directory on that drive rather than its root, so the
        // separator has to be appended.
        let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
        let drive = drive.trim().trim_end_matches(['\\', '/']);
        let drive = if drive.is_empty() { "C:" } else { drive };
        PathBuf::from(format!("{drive}\\"))
    }

    #[cfg(not(windows))]
    {
        PathBuf::from("/")
    }
}

/// The nearest ancestor of `path` that exists, `path` itself included.
fn existing_ancestor(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|candidate| !candidate.as_os_str().is_empty() && candidate.exists())
        .map(Path::to_path_buf)
}

/// `(total_bytes, available_bytes)` for the volume holding `path`.
#[cfg(windows)]
fn volume_space(path: &Path) -> std::io::Result<(u64, u64)> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let directory: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut available_to_caller: u64 = 0;
    let mut total: u64 = 0;
    let queried = unsafe {
        GetDiskFreeSpaceExW(
            directory.as_ptr(),
            &mut available_to_caller,
            &mut total,
            std::ptr::null_mut(),
        )
    };
    if queried == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok((total, available_to_caller))
}

#[cfg(unix)]
fn volume_space(path: &Path) -> std::io::Result<(u64, u64)> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let directory = CString::new(path.as_os_str().as_bytes()).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "path contains an interior NUL byte",
        )
    })?;

    let mut stats: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(directory.as_ptr(), &mut stats) } != 0 {
        return Err(std::io::Error::last_os_error());
    }

    // `f_frsize` is the fragment size blocks are counted in; a few filesystems
    // report it as 0, where `f_bsize` is the documented fallback.
    let block_size = if stats.f_frsize > 0 {
        stats.f_frsize as u64
    } else {
        stats.f_bsize as u64
    };
    // `f_bavail` excludes the blocks reserved for root, so it is what an
    // unprivileged app can actually still write; `f_bfree` would overstate it.
    Ok((
        (stats.f_blocks as u64).saturating_mul(block_size),
        (stats.f_bavail as u64).saturating_mul(block_size),
    ))
}

#[cfg(not(any(windows, unix)))]
fn volume_space(_path: &Path) -> std::io::Result<(u64, u64)> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "free space reporting is not implemented for this platform",
    ))
}

/// A value that is equal for two paths exactly when they share a volume.
#[cfg(windows)]
fn volume_key(path: &Path) -> Option<String> {
    use std::path::{Component, Prefix};

    let Some(Component::Prefix(prefix)) = path.components().next() else {
        return None;
    };
    Some(match prefix.kind() {
        Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => {
            format!("{}:", (letter as char).to_ascii_uppercase())
        }
        // UNC and device paths have no drive letter; their own text is the best
        // available identity.
        _ => prefix.as_os_str().to_string_lossy().to_uppercase(),
    })
}

#[cfg(not(windows))]
fn volume_key(path: &Path) -> Option<String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;

        // The kernel's device id, rather than a path prefix: it is what actually
        // decides whether two directories share a filesystem.
        Some(format!("dev:{}", std::fs::metadata(path).ok()?.dev()))
    }

    #[cfg(not(unix))]
    {
        Some(path.display().to_string())
    }
}

/// How the volume is named to the user.
#[cfg(windows)]
fn volume_label(path: &Path) -> String {
    volume_key(path).unwrap_or_else(|| path.display().to_string())
}

#[cfg(not(windows))]
fn volume_label(path: &Path) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;

        let Ok(metadata) = std::fs::metadata(path) else {
            return path.display().to_string();
        };
        let device = metadata.dev();
        // Climb while the device id holds; the last path that still matches is
        // the mount point, which is the name a user recognizes ("/", "/home").
        let mut mount_point = path;
        for ancestor in path.ancestors().skip(1) {
            match std::fs::metadata(ancestor) {
                Ok(parent) if parent.dev() == device => mount_point = ancestor,
                _ => break,
            }
        }
        mount_point.display().to_string()
    }

    #[cfg(not(unix))]
    {
        path.display().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{existing_ancestor, system_root, DiskSpaceService, LOW_DISK_SPACE_THRESHOLD_BYTES};

    #[test]
    fn threshold_ships_as_one_gibibyte() {
        assert_eq!(LOW_DISK_SPACE_THRESHOLD_BYTES, 1_073_741_824);
    }

    #[test]
    fn reports_plausible_space_and_no_warning_at_a_zero_threshold() {
        let temp = tempfile::tempdir().expect("temp dir");

        let status = DiskSpaceService::status_with_threshold(temp.path(), 0);

        assert!(
            !status.volumes.is_empty(),
            "no volume could be probed on this platform"
        );
        assert_eq!(status.threshold_bytes, 0);
        assert!(!status.low);
        for volume in &status.volumes {
            assert!(volume.total_bytes > 0, "{volume:?}");
            assert!(volume.available_bytes <= volume.total_bytes, "{volume:?}");
            assert!(!volume.label.is_empty(), "{volume:?}");
            assert!(!volume.low, "{volume:?}");
        }
    }

    #[test]
    fn every_volume_warns_once_the_threshold_exceeds_any_disk() {
        let temp = tempfile::tempdir().expect("temp dir");

        let status = DiskSpaceService::status_with_threshold(temp.path(), u64::MAX);

        assert!(status.low);
        assert!(status.volumes.iter().all(|volume| volume.low));
    }

    #[test]
    fn volumes_are_deduplicated() {
        let temp = tempfile::tempdir().expect("temp dir");

        let status = DiskSpaceService::status_with_threshold(temp.path(), 0);

        let mut labels: Vec<&str> = status
            .volumes
            .iter()
            .map(|volume| volume.label.as_str())
            .collect();
        let probed = labels.len();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), probed, "{:?}", status.volumes);
    }

    #[test]
    fn a_data_dir_on_the_system_volume_is_reported_once() {
        let status = DiskSpaceService::status_with_threshold(&system_root(), 0);

        assert_eq!(status.volumes.len(), 1, "{:?}", status.volumes);
    }

    #[test]
    fn a_data_dir_that_does_not_exist_yet_still_resolves_to_its_volume() {
        let temp = tempfile::tempdir().expect("temp dir");
        let unborn = temp.path().join("ai-switch").join("nested").join("deeper");

        assert_eq!(
            existing_ancestor(&unborn).as_deref(),
            Some(temp.path()),
            "should fall back to the nearest existing ancestor"
        );
        let status = DiskSpaceService::status_with_threshold(&unborn, 0);
        assert!(!status.volumes.is_empty());
    }
}

use serde::{Deserialize, Serialize};

/// Warn once a monitored volume has less room left than this.
///
/// A full volume is the failure mode behind a whole class of confusing errors —
/// SQLite write failures, config snapshots that never land, a migration backup
/// that cannot be copied — so the warning has to arrive while there is still
/// room to act, not once writes have already started failing.
pub const LOW_DISK_SPACE_THRESHOLD_BYTES: u64 = 1024 * 1024 * 1024;

/// Free space on one of the volumes the app depends on.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiskVolumeSpace {
    /// What the user sees: the drive prefix on Windows (`C:`), the mount point
    /// on Unix (`/`, `/home`).
    pub label: String,
    /// The path actually probed. Kept so a surprising reading can be traced back
    /// to the directory it came from.
    pub path: String,
    pub total_bytes: u64,
    /// Space the current user can still write, not counting reserved blocks.
    pub available_bytes: u64,
    /// `available_bytes < DiskSpaceStatus::threshold_bytes`.
    pub low: bool,
}

/// Free space across every volume the app writes to.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiskSpaceStatus {
    pub threshold_bytes: u64,
    /// True when any monitored volume is below the threshold.
    pub low: bool,
    /// One entry per distinct volume. Empty when no volume could be probed,
    /// which is reported as "not low" rather than as an error.
    pub volumes: Vec<DiskVolumeSpace>,
}

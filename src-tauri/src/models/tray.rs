use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrayMenuStatus {
    pub provider_count: i64,
    pub target_count: i64,
    pub switch_item_count: i64,
}

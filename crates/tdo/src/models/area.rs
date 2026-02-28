use jiff::Timestamp;
use saku_storage::entity::Entity;
use saku_storage::timestamp::HybridTimestamp;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Area {
    pub name: String,
    pub deleted_at: Option<Timestamp>,
    /// Hybrid logical clock timestamp for sync conflict resolution
    pub modified_at: HybridTimestamp,
    /// If this entry was renamed, points to the new key (tombstone field)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub renamed_to: Option<String>,
    /// If this entry was created by a rename, points to the old key
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_key: Option<String>,
}

impl Entity for Area {
    fn entity_type() -> &'static str {
        "area"
    }

    fn natural_key(&self) -> String {
        self.name.to_lowercase()
    }
}

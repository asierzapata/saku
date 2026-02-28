use jiff::Timestamp;
use jiff::civil::Date;
use saku_storage::entity::Entity;
use saku_storage::timestamp::HybridTimestamp;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Project {
    /// Name of the project
    pub name: String,
    /// Area key (e.g., "area/work") if the project belongs to an area
    pub area_key: Option<String>,
    /// Notes of the project
    pub notes: Option<String>,
    /// Deadline of the project
    pub deadline: Option<Date>,
    /// Completed at timestamp of the project
    pub completed_at: Option<Timestamp>,
    /// Deleted at timestamp of the project
    pub deleted_at: Option<Timestamp>,
    /// Created at timestamp of the project
    pub created_at: Timestamp,
    /// Hybrid logical clock timestamp for sync conflict resolution
    pub modified_at: HybridTimestamp,
    /// If this entry was renamed, points to the new key (tombstone field)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub renamed_to: Option<String>,
    /// If this entry was created by a rename, points to the old key
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_key: Option<String>,
}

impl Entity for Project {
    fn entity_type() -> &'static str {
        "project"
    }

    fn natural_key(&self) -> String {
        self.name.to_lowercase()
    }
}

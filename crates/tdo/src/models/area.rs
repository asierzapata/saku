use jiff::Timestamp;
use saku_storage::timestamp::HybridTimestamp;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Area {
    pub id: Uuid,
    pub name: String,
    pub deleted_at: Option<Timestamp>,
    /// Hybrid logical clock timestamp for sync conflict resolution
    pub modified_at: HybridTimestamp,
}

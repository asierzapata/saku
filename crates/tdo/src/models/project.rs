use jiff::Timestamp;
use jiff::civil::Date;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Project {
    /// UUID of the project
    pub id: Uuid,
    /// Name of the project
    pub name: String,
    /// Slug of the project
    pub slug: String,
    /// Area ID of the project
    pub area_id: Option<Uuid>,
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
}

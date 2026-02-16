use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Area {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub deleted_at: Option<Timestamp>,
}

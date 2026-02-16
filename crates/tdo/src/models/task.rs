// src/model.rs

use jiff::Timestamp;
use jiff::civil::Date;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Task {
    /// UUID to identify the task
    pub id: Uuid,
    /// User-facing auto-incremental task number
    pub task_number: u64,
    /// Title of the task
    pub title: String,
    /// Notes of the task
    pub notes: Option<String>,
    /// The project of this task if it belongs to any
    pub project_id: Option<Uuid>,
    /// The area of this task if it belongs to any (and no project)
    pub area_id: Option<Uuid>,
    /// Tags of the task
    pub tags: Vec<String>,
    /// When the user wants do to this task
    pub when: When,
    /// Deadline for this task
    pub deadline: Option<Date>,
    /// Defered date when to surface again the task
    pub defer_until: Option<Date>,
    /// Sub tasks of the main task - Modeled as a lighter task called ChecklistItem
    pub checklist: Vec<ChecklistItem>,
    /// When the task was completed
    pub completed_at: Option<Timestamp>,
    /// When the task was deleted
    pub deleted_at: Option<Timestamp>,
    /// When the task was created
    pub created_at: Timestamp,
}

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(tag = "type")]
pub enum When {
    #[default]
    Inbox,
    Today {
        evening: bool,
    },
    Someday,
    Anytime,
    Scheduled {
        date: Date,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum WhenInstantiationError {
    #[error("Invalid schedule date format: {0}")]
    ScheduleAtIncorrect(String),

    #[error("Conflicting scheduling flags: {}", .0.join(", "))]
    ConflictingFlags(Vec<String>),

    #[error("The --evening flag can only be used with --today")]
    EveningWithoutToday,
}

impl When {
    pub fn from_command_flags(
        today: bool,
        evening: bool,
        someday: bool,
        anytime: bool,
        schedule_at: Option<String>,
    ) -> Result<When, WhenInstantiationError> {
        // Collect provided scheduling flags
        let mut provided_flags = Vec::new();
        if today {
            provided_flags.push("--today");
        }
        if someday {
            provided_flags.push("--someday");
        }
        if anytime {
            provided_flags.push("--anytime");
        }
        if schedule_at.is_some() {
            provided_flags.push("--when");
        }

        // Detect mutually exclusive flag conflicts
        if provided_flags.len() > 1 {
            return Err(WhenInstantiationError::ConflictingFlags(
                provided_flags.into_iter().map(String::from).collect(),
            ));
        }

        // Validate --evening usage
        if evening && !today {
            return Err(WhenInstantiationError::EveningWithoutToday);
        }

        // Process the valid flag (existing logic)
        if today {
            Ok(When::Today { evening })
        } else if someday {
            Ok(When::Someday)
        } else if anytime {
            Ok(When::Anytime)
        } else if let Some(string_date) = schedule_at {
            string_date
                .parse()
                .map(|date| When::Scheduled { date })
                .map_err(|_| WhenInstantiationError::ScheduleAtIncorrect(string_date))
        } else {
            Ok(When::Inbox)
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ChecklistItem {
    pub id: Uuid,
    pub title: String,
    pub completed: bool,
}

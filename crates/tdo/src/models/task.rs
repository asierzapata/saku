// src/model.rs

use std::cmp::Ordering;

use jiff::civil::Date;
use jiff::Timestamp;
use saku_storage::timestamp::HybridTimestamp;
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
    /// Hybrid logical clock timestamp for sync conflict resolution
    pub modified_at: HybridTimestamp,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "PascalCase")]
pub enum When {
    #[default]
    Inbox,
    Scheduled {
        date: Date,
    },
    Someday,
    // Legacy variants for migration - deserialize only
    #[serde(rename = "Today", skip_serializing)]
    LegacyToday {
        evening: bool,
    },
    #[serde(rename = "Anytime", skip_serializing)]
    LegacyAnytime,
}

#[derive(Debug, thiserror::Error)]
pub enum WhenInstantiationError {
    #[error("Invalid schedule date format: {0} - {1}")]
    ScheduleAtIncorrect(String, String),

    #[error("Conflicting scheduling flags: {}", .0.join(", "))]
    ConflictingFlags(Vec<String>),
}

impl When {
    /// Normalize legacy variants after deserialization
    pub fn normalize(self) -> Self {
        use jiff::Zoned;
        match self {
            When::LegacyToday { evening: _ } => {
                // Convert old Today variant to Scheduled with today's date
                let today = Zoned::now().date();
                When::Scheduled { date: today }
            }
            When::LegacyAnytime => When::Someday,
            other => other,
        }
    }

    pub fn from_command_flags(
        today: bool,
        tomorrow: bool,
        next_week: bool,
        someday: bool,
        on: Option<String>,
    ) -> Result<When, WhenInstantiationError> {
        use crate::date_parser::parse_natural_date;
        use jiff::Zoned;

        // Collect provided scheduling flags
        let mut provided_flags = Vec::new();
        if today {
            provided_flags.push("--today");
        }
        if tomorrow {
            provided_flags.push("--tomorrow");
        }
        if next_week {
            provided_flags.push("--next-week");
        }
        if someday {
            provided_flags.push("--someday");
        }
        if on.is_some() {
            provided_flags.push("--on");
        }

        // Detect mutually exclusive flag conflicts
        if provided_flags.len() > 1 {
            return Err(WhenInstantiationError::ConflictingFlags(
                provided_flags.into_iter().map(String::from).collect(),
            ));
        }

        // Process the valid flag
        if today {
            let today_date = Zoned::now().date();
            Ok(When::Scheduled { date: today_date })
        } else if tomorrow {
            let tomorrow_date = Zoned::now()
                .date()
                .checked_add(jiff::Span::new().days(1))
                .expect("Failed to calculate tomorrow");
            Ok(When::Scheduled {
                date: tomorrow_date,
            })
        } else if next_week {
            let today_date = Zoned::now().date();
            let days_until_next_monday = match today_date.weekday() {
                jiff::civil::Weekday::Monday => 7,
                jiff::civil::Weekday::Tuesday => 6,
                jiff::civil::Weekday::Wednesday => 5,
                jiff::civil::Weekday::Thursday => 4,
                jiff::civil::Weekday::Friday => 3,
                jiff::civil::Weekday::Saturday => 2,
                jiff::civil::Weekday::Sunday => 1,
            };
            let next_monday = today_date
                .checked_add(jiff::Span::new().days(days_until_next_monday))
                .expect("Failed to calculate next week");
            Ok(When::Scheduled { date: next_monday })
        } else if someday {
            Ok(When::Someday)
        } else if let Some(date_string) = on {
            let date = parse_natural_date(&date_string).map_err(|e| {
                WhenInstantiationError::ScheduleAtIncorrect(date_string, e.to_string())
            })?;
            Ok(When::Scheduled { date })
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

pub fn order_tasks(tasks: Vec<&Task>) -> Vec<&Task> {
    let mut ordered = tasks;
    ordered.sort_by(|a, b| {
        // 1. Deadline urgency: sooner deadlines first, no deadline last
        let deadline_ord = match (a.deadline, b.deadline) {
            (None, None) => Ordering::Equal,
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (Some(da), Some(db)) => da.cmp(&db),
        };
        if deadline_ord != Ordering::Equal {
            return deadline_ord;
        }
        // 2. Project grouping: same project adjacent, no project last
        let project_ord = match (a.project_id, b.project_id) {
            (None, None) => Ordering::Equal,
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (Some(pa), Some(pb)) => pa.cmp(&pb),
        };
        if project_ord != Ordering::Equal {
            return project_ord;
        }
        // 3. Area grouping: same area adjacent, no area last
        let area_ord = match (a.area_id, b.area_id) {
            (None, None) => Ordering::Equal,
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (Some(aa), Some(ab)) => aa.cmp(&ab),
        };
        if area_ord != Ordering::Equal {
            return area_ord;
        }
        // 4. Task number within each group
        a.task_number.cmp(&b.task_number)
    });
    ordered
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    fn make_task(
        task_number: u64,
        deadline: Option<Date>,
        project_id: Option<Uuid>,
        area_id: Option<Uuid>,
    ) -> Task {
        Task {
            task_number,
            deadline,
            project_id,
            area_id,
            ..Task::default()
        }
    }

    #[test]
    fn deadline_sorts_before_no_deadline() {
        let with_deadline = make_task(2, Some(date(2025, 6, 1)), None, None);
        let no_deadline = make_task(1, None, None, None);

        let tasks = vec![&no_deadline, &with_deadline];
        let ordered = order_tasks(tasks);

        assert_eq!(
            ordered[0].task_number, 2,
            "task with deadline should be first"
        );
        assert_eq!(
            ordered[1].task_number, 1,
            "task without deadline should be last"
        );
    }

    #[test]
    fn earlier_deadline_sorts_first() {
        let earlier = make_task(2, Some(date(2025, 1, 10)), None, None);
        let later = make_task(1, Some(date(2025, 1, 20)), None, None);

        let tasks = vec![&later, &earlier];
        let ordered = order_tasks(tasks);

        assert_eq!(
            ordered[0].task_number, 2,
            "earlier deadline should be first"
        );
        assert_eq!(ordered[1].task_number, 1, "later deadline should be second");
    }

    #[test]
    fn same_deadline_groups_by_project() {
        let deadline = Some(date(2025, 6, 1));
        let project_a = Uuid::new_v4();
        let with_project = make_task(2, deadline, Some(project_a), None);
        let no_project = make_task(1, deadline, None, None);

        let tasks = vec![&no_project, &with_project];
        let ordered = order_tasks(tasks);

        assert_eq!(
            ordered[0].task_number, 2,
            "task with project should be first"
        );
        assert_eq!(
            ordered[1].task_number, 1,
            "task without project should be last"
        );
    }

    #[test]
    fn same_project_groups_by_area() {
        let project_id = Some(Uuid::new_v4());
        let area_a = Uuid::new_v4();
        let with_area = make_task(2, None, project_id, Some(area_a));
        let no_area = make_task(1, None, project_id, None);

        let tasks = vec![&no_area, &with_area];
        let ordered = order_tasks(tasks);

        assert_eq!(ordered[0].task_number, 2, "task with area should be first");
        assert_eq!(
            ordered[1].task_number, 1,
            "task without area should be last"
        );
    }

    #[test]
    fn same_group_orders_by_task_number() {
        let project_id = Some(Uuid::new_v4());
        let t1 = make_task(1, None, project_id, None);
        let t3 = make_task(3, None, project_id, None);
        let t2 = make_task(2, None, project_id, None);

        let tasks = vec![&t3, &t1, &t2];
        let ordered = order_tasks(tasks);

        assert_eq!(ordered[0].task_number, 1);
        assert_eq!(ordered[1].task_number, 2);
        assert_eq!(ordered[2].task_number, 3);
    }

    #[test]
    fn normalize_legacy_today_to_scheduled() {
        let legacy = When::LegacyToday { evening: false };
        let normalized = legacy.normalize();

        match normalized {
            When::Scheduled { date } => {
                // Should be today's date
                let today = jiff::Zoned::now().date();
                assert_eq!(date, today);
            }
            _ => panic!("Expected Scheduled variant"),
        }
    }

    #[test]
    fn normalize_legacy_anytime_to_someday() {
        let legacy = When::LegacyAnytime;
        let normalized = legacy.normalize();
        assert_eq!(normalized, When::Someday);
    }

    #[test]
    fn normalize_preserves_non_legacy_variants() {
        let inbox = When::Inbox;
        assert_eq!(inbox.clone().normalize(), inbox);

        let someday = When::Someday;
        assert_eq!(someday.clone().normalize(), someday);

        let scheduled = When::Scheduled {
            date: date(2026, 3, 15),
        };
        assert_eq!(scheduled.clone().normalize(), scheduled);
    }

    #[test]
    fn when_from_command_flags_today() {
        let when = When::from_command_flags(true, false, false, false, None).unwrap();
        let today = jiff::Zoned::now().date();

        match when {
            When::Scheduled { date } => {
                assert_eq!(date, today);
            }
            _ => panic!("Expected Scheduled variant"),
        }
    }

    #[test]
    fn when_from_command_flags_on_date() {
        let when =
            When::from_command_flags(false, false, false, false, Some("2026-03-15".to_string()))
                .unwrap();

        match when {
            When::Scheduled {
                date: schedule_date,
            } => {
                assert_eq!(schedule_date, date(2026, 3, 15));
            }
            _ => panic!("Expected Scheduled variant"),
        }
    }

    #[test]
    fn when_from_command_flags_conflicting() {
        let result = When::from_command_flags(true, true, false, false, None);
        assert!(result.is_err());

        match result {
            Err(WhenInstantiationError::ConflictingFlags(_)) => {}
            _ => panic!("Expected ConflictingFlags error"),
        }
    }

    #[test]
    fn when_from_command_flags_natural_language() {
        // Test tomorrow
        let when = When::from_command_flags(false, true, false, false, None).unwrap();
        let tomorrow = jiff::Zoned::now()
            .date()
            .checked_add(jiff::Span::new().days(1))
            .unwrap();

        match when {
            When::Scheduled { date, .. } => {
                assert_eq!(date, tomorrow);
            }
            _ => panic!("Expected Scheduled variant"),
        }
    }
}

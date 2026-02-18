// src/model.rs

use std::cmp::Ordering;

use jiff::civil::Date;
use jiff::Timestamp;
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

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
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
}

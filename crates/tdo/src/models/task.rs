// src/model.rs

use std::cmp::Ordering;

use jiff::Timestamp;
use jiff::civil::Date;
use saku_storage::entity::Entity;
use saku_storage::timestamp::HybridTimestamp;
use serde::{Deserialize, Serialize};

// ============================================================================
// Recurrence types
// ============================================================================

/// Serializable weekday (jiff::civil::Weekday does not implement Serialize/Deserialize).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SerdeWeekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

impl SerdeWeekday {
    pub fn short_name(self) -> &'static str {
        match self {
            SerdeWeekday::Monday => "Mon",
            SerdeWeekday::Tuesday => "Tue",
            SerdeWeekday::Wednesday => "Wed",
            SerdeWeekday::Thursday => "Thu",
            SerdeWeekday::Friday => "Fri",
            SerdeWeekday::Saturday => "Sat",
            SerdeWeekday::Sunday => "Sun",
        }
    }
}

impl From<SerdeWeekday> for jiff::civil::Weekday {
    fn from(w: SerdeWeekday) -> Self {
        match w {
            SerdeWeekday::Monday => jiff::civil::Weekday::Monday,
            SerdeWeekday::Tuesday => jiff::civil::Weekday::Tuesday,
            SerdeWeekday::Wednesday => jiff::civil::Weekday::Wednesday,
            SerdeWeekday::Thursday => jiff::civil::Weekday::Thursday,
            SerdeWeekday::Friday => jiff::civil::Weekday::Friday,
            SerdeWeekday::Saturday => jiff::civil::Weekday::Saturday,
            SerdeWeekday::Sunday => jiff::civil::Weekday::Sunday,
        }
    }
}

impl From<jiff::civil::Weekday> for SerdeWeekday {
    fn from(w: jiff::civil::Weekday) -> Self {
        match w {
            jiff::civil::Weekday::Monday => SerdeWeekday::Monday,
            jiff::civil::Weekday::Tuesday => SerdeWeekday::Tuesday,
            jiff::civil::Weekday::Wednesday => SerdeWeekday::Wednesday,
            jiff::civil::Weekday::Thursday => SerdeWeekday::Thursday,
            jiff::civil::Weekday::Friday => SerdeWeekday::Friday,
            jiff::civil::Weekday::Saturday => SerdeWeekday::Saturday,
            jiff::civil::Weekday::Sunday => SerdeWeekday::Sunday,
        }
    }
}

/// Base frequency of a recurrence.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Freq {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

/// Monthly recurrence anchor — the three cases are mutually exclusive.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MonthlyAnchor {
    /// e.g. "1st of month" (day=1) or "monthly" (day=dtstart.day())
    DayOfMonth { day: u8 },
    /// e.g. "1st monday of month" (nth=1, weekday=Monday)
    NthWeekday { nth: u8, weekday: SerdeWeekday },
    /// e.g. "last friday of month"
    LastWeekday { weekday: SerdeWeekday },
}

/// RRULE-style recurrence rule stored on the task.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Recurrence {
    pub freq: Freq,
    /// For Weekly: weekdays to recur on (empty = same weekday as dtstart).
    pub weekdays: Vec<SerdeWeekday>,
    /// For Monthly: how to anchor within the month.
    pub monthly_anchor: Option<MonthlyAnchor>,
    /// End date (inclusive). None = repeat forever.
    pub until: Option<Date>,
    /// First occurrence / recurrence anchor date.
    pub dtstart: Date,
}

impl std::fmt::Display for Recurrence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.freq {
            Freq::Daily => write!(f, "every day"),
            Freq::Weekly => {
                if self.weekdays.is_empty() {
                    write!(f, "every week")
                } else {
                    let days: Vec<&str> = self.weekdays.iter().map(|w| w.short_name()).collect();
                    write!(f, "{}", days.join(", "))
                }
            }
            Freq::Monthly => match &self.monthly_anchor {
                Some(MonthlyAnchor::DayOfMonth { day }) => {
                    let suffix = ordinal_suffix(*day);
                    write!(f, "{}{} of month", day, suffix)
                }
                Some(MonthlyAnchor::NthWeekday { nth, weekday }) => {
                    let suffix = ordinal_suffix(*nth);
                    write!(f, "{}{} {} of month", nth, suffix, weekday.short_name())
                }
                Some(MonthlyAnchor::LastWeekday { weekday }) => {
                    write!(f, "last {} of month", weekday.short_name())
                }
                None => write!(f, "every month"),
            },
            Freq::Yearly => write!(f, "every year"),
        }
    }
}

fn ordinal_suffix(n: u8) -> &'static str {
    match (n % 100, n % 10) {
        (11..=13, _) => "th",
        (_, 1) => "st",
        (_, 2) => "nd",
        (_, 3) => "rd",
        _ => "th",
    }
}

// ============================================================================
// Task struct
// ============================================================================

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Task {
    /// Short hash suffix used as the storage key (e.g., "k7m2a3x9")
    pub storage_key_suffix: String,
    /// User-facing auto-incremental task number
    pub task_number: u64,
    /// Title of the task
    pub title: String,
    /// Notes of the task
    pub notes: Option<String>,
    /// Project storage key (e.g., "project/website")
    pub project_key: Option<String>,
    /// Area storage key (e.g., "area/work") — used when no project
    pub area_key: Option<String>,
    /// Tags of the task
    pub tags: Vec<String>,
    /// When the user wants do to this task
    pub when: When,
    /// Deadline for this task
    pub deadline: Option<Date>,
    /// Defered date when to surface again the task
    pub defer_until: Option<Date>,
    /// Storage keys of tasks that must be completed before this task can start
    pub depends_on: Vec<String>,
    /// Parent task storage key if this is a subtask (one level deep only)
    pub parent_task_key: Option<String>,
    /// When the task was completed
    pub completed_at: Option<Timestamp>,
    /// When the task was deleted
    pub deleted_at: Option<Timestamp>,
    /// When the task was created
    pub created_at: Timestamp,
    /// Hybrid logical clock timestamp for sync conflict resolution
    pub modified_at: HybridTimestamp,
    /// Who this task is assigned to (e.g., "agent", "wrk", or a username).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assigned_to: Option<String>,
    /// Recurrence rule. None = one-off task.
    #[serde(default)]
    pub recurrence: Option<Recurrence>,
    /// Dates of occurrences that have already been completed.
    #[serde(default)]
    pub completed_occurrences: Vec<Date>,
}

impl Entity for Task {
    fn entity_type() -> &'static str {
        "task"
    }

    fn natural_key(&self) -> String {
        self.storage_key_suffix.clone()
    }
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

// ============================================================================
// Recurrence helpers
// ============================================================================

/// Number of days in a given month.
fn days_in_month(year: i16, month: i8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if year % 400 == 0 || (year % 4 == 0 && year % 100 != 0) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

/// Date of the nth occurrence of `weekday` in (year, month), or None if it doesn't exist.
fn nth_weekday_of_month(year: i16, month: i8, nth: u8, weekday: SerdeWeekday) -> Option<Date> {
    let first = Date::new(year, month, 1).ok()?;
    let target = jiff::civil::Weekday::from(weekday);
    let first_wd = first.weekday();
    let days_to_first = ((target as i64 - first_wd as i64 + 7) % 7) as i8;
    let day = 1 + days_to_first + 7 * (nth as i8 - 1);
    let last = days_in_month(year, month) as i8;
    if day < 1 || day > last {
        return None;
    }
    Date::new(year, month, day).ok()
}

/// Date of the last occurrence of `weekday` in (year, month).
fn last_weekday_of_month(year: i16, month: i8, weekday: SerdeWeekday) -> Option<Date> {
    let last_day = days_in_month(year, month) as i8;
    let last = Date::new(year, month, last_day).ok()?;
    let target = jiff::civil::Weekday::from(weekday);
    let last_wd = last.weekday();
    let days_back = ((last_wd as i64 - target as i64 + 7) % 7) as i8;
    let day = last_day - days_back;
    if day < 1 {
        return None;
    }
    Date::new(year, month, day).ok()
}

/// Returns true if `date` is the nth occurrence of `weekday` in its month.
fn is_nth_weekday_of_month(date: Date, nth: u8, weekday: SerdeWeekday) -> bool {
    if date.weekday() != jiff::civil::Weekday::from(weekday) {
        return false;
    }
    // Count = ceil(day / 7) = (day - 1) / 7 + 1
    let count = (date.day() as u8 - 1) / 7 + 1;
    count == nth
}

/// Returns true if `date` is the last occurrence of `weekday` in its month.
fn is_last_weekday_of_month(date: Date, weekday: SerdeWeekday) -> bool {
    if date.weekday() != jiff::civil::Weekday::from(weekday) {
        return false;
    }
    let last_day = days_in_month(date.year(), date.month());
    date.day() as u8 + 7 > last_day
}

/// Returns true if `date` is a pending (not yet completed) occurrence of the task's recurrence.
pub fn is_pending_on(task: &Task, date: Date) -> bool {
    let Some(rule) = &task.recurrence else {
        return false;
    };
    if task.completed_at.is_some() {
        return false;
    }
    if date < rule.dtstart {
        return false;
    }
    if rule.until.is_some_and(|u| date > u) {
        return false;
    }

    let is_occurrence = match &rule.freq {
        Freq::Daily => true,
        Freq::Weekly => {
            if rule.weekdays.is_empty() {
                date.weekday() == rule.dtstart.weekday()
            } else {
                rule.weekdays
                    .iter()
                    .any(|w| date.weekday() == jiff::civil::Weekday::from(*w))
            }
        }
        Freq::Monthly => match rule.monthly_anchor.as_ref() {
            Some(MonthlyAnchor::DayOfMonth { day }) => date.day() as u8 == *day,
            Some(MonthlyAnchor::NthWeekday { nth, weekday }) => {
                is_nth_weekday_of_month(date, *nth, *weekday)
            }
            Some(MonthlyAnchor::LastWeekday { weekday }) => {
                is_last_weekday_of_month(date, *weekday)
            }
            None => date.day() == rule.dtstart.day(),
        },
        Freq::Yearly => {
            date.month() == rule.dtstart.month() && date.day() == rule.dtstart.day()
        }
    };

    is_occurrence && !task.completed_occurrences.contains(&date)
}

/// Returns all pending (not yet completed) occurrence dates of the task's recurrence up to `up_to`.
pub fn pending_occurrences_up_to(task: &Task, up_to: Date) -> Vec<Date> {
    let Some(rule) = &task.recurrence else {
        return vec![];
    };
    if task.completed_at.is_some() {
        return vec![];
    }

    let end = if let Some(until) = rule.until {
        up_to.min(until)
    } else {
        up_to
    };

    if rule.dtstart > end {
        return vec![];
    }

    let mut dates = Vec::new();

    match &rule.freq {
        Freq::Daily => {
            let mut d = rule.dtstart;
            while d <= end {
                if !task.completed_occurrences.contains(&d) {
                    dates.push(d);
                }
                d = d.checked_add(jiff::Span::new().days(1)).unwrap();
            }
        }
        Freq::Weekly => {
            if rule.weekdays.is_empty() {
                let mut d = rule.dtstart;
                while d <= end {
                    if !task.completed_occurrences.contains(&d) {
                        dates.push(d);
                    }
                    d = d.checked_add(jiff::Span::new().days(7)).unwrap();
                }
            } else {
                let mut d = rule.dtstart;
                while d <= end {
                    if rule
                        .weekdays
                        .iter()
                        .any(|w| d.weekday() == jiff::civil::Weekday::from(*w))
                        && !task.completed_occurrences.contains(&d)
                    {
                        dates.push(d);
                    }
                    d = d.checked_add(jiff::Span::new().days(1)).unwrap();
                }
            }
        }
        Freq::Monthly => {
            let mut year = rule.dtstart.year();
            let mut month = rule.dtstart.month();
            loop {
                let candidate = match rule.monthly_anchor.as_ref() {
                    Some(MonthlyAnchor::DayOfMonth { day }) => {
                        let last = days_in_month(year, month) as i8;
                        let d = (*day as i8).min(last);
                        Date::new(year, month, d).ok()
                    }
                    Some(MonthlyAnchor::NthWeekday { nth, weekday }) => {
                        nth_weekday_of_month(year, month, *nth, *weekday)
                    }
                    Some(MonthlyAnchor::LastWeekday { weekday }) => {
                        last_weekday_of_month(year, month, *weekday)
                    }
                    None => {
                        let last = days_in_month(year, month) as i8;
                        let d = rule.dtstart.day().min(last);
                        Date::new(year, month, d).ok()
                    }
                };

                if let Some(d) = candidate {
                    if d > end {
                        break;
                    }
                    if d >= rule.dtstart && !task.completed_occurrences.contains(&d) {
                        dates.push(d);
                    }
                }

                if month == 12 {
                    month = 1;
                    year += 1;
                } else {
                    month += 1;
                }
                // Stop as soon as the current month/year is past the end date.
                if year > end.year() || (year == end.year() && i16::from(month) > end.month() as i16) {
                    break;
                }
            }
        }
        Freq::Yearly => {
            let mut year = rule.dtstart.year();
            while year <= end.year() {
                if let Ok(d) =
                    Date::new(year, rule.dtstart.month(), rule.dtstart.day())
                {
                    if d > end {
                        break;
                    }
                    if d >= rule.dtstart && !task.completed_occurrences.contains(&d) {
                        dates.push(d);
                    }
                }
                year += 1;
            }
        }
    }

    dates
}

/// Returns the first pending occurrence on or after `from`, within a 1-year look-ahead.
/// More efficient than `pending_occurrences_up_to` for the "what's next?" use-case because
/// it stops as soon as it finds one result and starts iteration from `from`, not `dtstart`.
pub fn next_pending_occurrence(task: &Task, from: Date) -> Option<Date> {
    let look_ahead = from
        .checked_add(jiff::Span::new().years(1))
        .unwrap_or(from);
    pending_occurrences_up_to(task, look_ahead)
        .into_iter()
        .find(|&d| d >= from)
}

pub fn order_tasks(tasks: Vec<&Task>) -> Vec<&Task> {
    order_tasks_impl(tasks, None)
}

pub fn order_tasks_with_store<'a>(
    tasks: Vec<&'a Task>,
    store: &crate::models::store::Store,
) -> Vec<&'a Task> {
    order_tasks_impl(tasks, Some(store))
}

fn order_tasks_impl<'a>(
    tasks: Vec<&'a Task>,
    store: Option<&crate::models::store::Store>,
) -> Vec<&'a Task> {
    let mut ordered = tasks;
    ordered.sort_by(|a, b| {
        // 0. Blocked tasks sort to the bottom
        let a_blocked = store.is_some_and(|s| s.is_task_blocked(a));
        let b_blocked = store.is_some_and(|s| s.is_task_blocked(b));
        let blocked_ord = match (a_blocked, b_blocked) {
            (true, false) => return Ordering::Greater,
            (false, true) => return Ordering::Less,
            _ => Ordering::Equal,
        };
        if blocked_ord != Ordering::Equal {
            return blocked_ord;
        }

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
        let project_ord = match (&a.project_key, &b.project_key) {
            (None, None) => Ordering::Equal,
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (Some(pa), Some(pb)) => pa.cmp(pb),
        };
        if project_ord != Ordering::Equal {
            return project_ord;
        }
        // 3. Area grouping: same area adjacent, no area last
        let area_ord = match (&a.area_key, &b.area_key) {
            (None, None) => Ordering::Equal,
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (Some(aa), Some(ab)) => aa.cmp(ab),
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
        project_key: Option<String>,
        area_key: Option<String>,
    ) -> Task {
        Task {
            task_number,
            deadline,
            project_key,
            area_key,
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
        let with_project = make_task(2, deadline, Some("project/alpha".into()), None);
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
        let project_key = Some("project/alpha".into());
        let with_area = make_task(2, None, project_key.clone(), Some("area/work".into()));
        let no_area = make_task(1, None, project_key, None);

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
        let project_key = Some("project/alpha".into());
        let t1 = make_task(1, None, project_key.clone(), None);
        let t3 = make_task(3, None, project_key.clone(), None);
        let t2 = make_task(2, None, project_key, None);

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

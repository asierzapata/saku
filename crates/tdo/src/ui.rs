use colored::*;
use jiff::civil::Date;

use crate::models::{store::Store, task::Task};

const MAX_CONTENT_WIDTH: usize = 100;

/// Represents the urgency level of a task based on its deadline or scheduled date
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TaskUrgency {
    Completed,           // Task is done
    OnTrack,             // Deadline >3 days away
    ApproachingDeadline, // Deadline 1-3 days away
    DueToday,            // Deadline is today
    Overdue,             // Past deadline or scheduled date
}

/// Get the terminal width, defaulting to 80 if unavailable
fn get_terminal_width() -> usize {
    term_size::dimensions().map(|(w, _)| w).unwrap_or(80)
}

/// Calculate urgency based on a date relative to today
fn calculate_urgency_from_date(date: Date, today: Date) -> TaskUrgency {
    if date < today {
        return TaskUrgency::Overdue;
    }
    if date == today {
        return TaskUrgency::DueToday;
    }

    let days_until = today.until(date).expect("valid date range").get_days();
    if days_until <= 3 {
        return TaskUrgency::ApproachingDeadline;
    }

    TaskUrgency::OnTrack
}

/// Determine the urgency level of a task based on its deadline or scheduled date
pub fn calculate_task_urgency(task: &Task) -> TaskUrgency {
    // Completed tasks have their own category
    if task.completed_at.is_some() {
        return TaskUrgency::Completed;
    }

    let today = jiff::Zoned::now().date();

    // Use the earliest date between deadline and scheduled for urgency calculation
    let scheduled_date = match task.when {
        crate::models::task::When::Scheduled { date, .. } => Some(date),
        _ => None,
    };

    let earliest_date = match (task.deadline, scheduled_date) {
        (Some(d1), Some(d2)) => Some(d1.min(d2)),
        (Some(d), None) | (None, Some(d)) => Some(d),
        (None, None) => None,
    };

    if let Some(date) = earliest_date {
        return calculate_urgency_from_date(date, today);
    }

    // Default: On track (no specific urgency)
    TaskUrgency::OnTrack
}

/// Format a date compactly based on proximity to today
/// is_completion: true for completed tasks (always show full date)
fn format_date_compact(date: Date, today: Date, is_completion: bool) -> String {
    // Completed tasks: always show full date format (Feb 19)
    if is_completion {
        let formatted = date.strftime("%b %-d").to_string();
        return format!("{:<6}", formatted); // Pad to 6 chars
    }

    // Active tasks: use proximity-based formatting
    if date == today {
        return "Today ".to_string(); // 6 chars
    }

    let days_diff = if date > today {
        today.until(date).expect("valid date range").get_days()
    } else {
        -(date.until(today).expect("valid date range").get_days())
    };

    // Within ±7 days: show day name (Mon, Tue, etc.)
    if days_diff.abs() <= 7 {
        let day_name = date.strftime("%a").to_string(); // e.g., "Mon"
        return format!("{:<6}", day_name); // Pad to 6 chars: "Mon   "
    }

    // Beyond 7 days: show "Feb 18" format
    let formatted = date.strftime("%b %-d").to_string();
    format!("{:<6}", formatted) // Pad to 6 chars
}

/// Get the date badge string and urgency for a task
/// Always returns a badge - either a date or dot placeholder
/// Format: "Mar 15" (scheduled only), "⚑ Mar 20" (deadline only), "Mar 15 | ⚑ Mar 20" (both)
pub fn get_task_date_badge(task: &Task) -> (String, TaskUrgency) {
    let today = jiff::Zoned::now().date();

    // Completed tasks: show completion date
    if let Some(completed_at) = task.completed_at {
        let completion_date = jiff::Zoned::new(completed_at, jiff::tz::TimeZone::system()).date();
        let formatted = format_date_compact(completion_date, today, true);
        return (formatted, TaskUrgency::Completed);
    }

    let scheduled_date = match task.when {
        crate::models::task::When::Scheduled { date, .. } => Some(date),
        _ => None,
    };
    let deadline = task.deadline;

    // Calculate urgency based on earliest date
    let urgency = calculate_task_urgency(task);

    // Format badge based on what dates we have
    match (scheduled_date, deadline) {
        (Some(sched), Some(dead)) => {
            // Both dates: "Mar 15 | ⚑ Mar 20"
            let sched_str = format_date_compact(sched, today, false).trim().to_string();
            let dead_str = format_date_compact(dead, today, false).trim().to_string();
            (format!("{} | ⚑ {}", sched_str, dead_str), urgency)
        }
        (Some(sched), None) => {
            // Scheduled only: "Mar 15"
            let formatted = format_date_compact(sched, today, false);
            (formatted, urgency)
        }
        (None, Some(dead)) => {
            // Deadline only: "⚑ Mar 20"
            let formatted = format_date_compact(dead, today, false).trim().to_string();
            (format!("⚑ {}", formatted), urgency)
        }
        (None, None) => {
            // No dates
            if let crate::models::task::When::Someday = task.when {
                ("······".to_string(), TaskUrgency::OnTrack)
            } else {
                // Inbox or other: show placeholder
                ("······".to_string(), TaskUrgency::OnTrack)
            }
        }
    }
}

/// Apply styling to date badge - just dimmed text, no background
fn style_date_badge(text: &str, _urgency: TaskUrgency) -> ColoredString {
    text.dimmed()
}

/// Get the appropriate status glyph for a task based on urgency
pub fn get_status_glyph(urgency: TaskUrgency) -> ColoredString {
    match urgency {
        TaskUrgency::Completed => "✔".dimmed(), // U+2714 - Heavy Check Mark
        TaskUrgency::OnTrack => "▢".green(),    // U+25A2 - Empty square
        TaskUrgency::ApproachingDeadline => "▢".yellow(), // U+25A2 - Empty square
        TaskUrgency::DueToday => "▢".truecolor(255, 140, 0), // U+25A2 - Orange square
        TaskUrgency::Overdue => "▢".red(),      // U+25A2 - Empty square
    }
}

/// Build the context string for a task (Area/Project hierarchy)
/// Returns None if task has no area or project associations
pub fn get_task_context(task: &Task, store: &Store) -> Option<String> {
    if let Some(project_id) = task.project_id
        && let Some(project) = store.get_project(project_id)
    {
        if let Some(area_id) = project.area_id
            && let Some(area) = store.get_area(area_id)
        {
            // Rule A: {Area Name} / {Project Name}
            return Some(format!("{} / {}", area.name, project.name));
        }
        return Some(project.name.clone());
    }

    if let Some(area_id) = task.area_id
        && let Some(area) = store.get_area(area_id)
    {
        return Some(area.name.clone());
    }

    None
}

/// Render a single task line with ID, glyph, title, and right-aligned context
pub fn render_task_line(task: &Task, store: &Store) {
    render_task_line_with_options(task, store, false, false);
}

/// Render a task line with optional completion date display
pub fn render_task_line_with_completion_date(task: &Task, store: &Store) {
    render_task_line_with_options(task, store, false, true);
}

/// Internal function to render a task line with various options
fn render_task_line_with_options(
    task: &Task,
    store: &Store,
    _is_overdue: bool,
    _show_completion_date: bool,
) {
    let terminal_width = get_terminal_width();

    let id_str = format!("{:>3}", task.task_number);

    // Calculate urgency
    let urgency = calculate_task_urgency(task);

    // Get status glyph
    let glyph = get_status_glyph(urgency);

    // Get date badge (always present now)
    let (date_str, badge_urgency) = get_task_date_badge(task);

    // Style task title
    let styled_title = if task.completed_at.is_some() {
        task.title.dimmed()
    } else {
        task.title.white()
    };

    // Build left section with date badge
    let styled_badge = style_date_badge(&date_str, badge_urgency);
    let left_section = format!(
        " {}  {}  {}  {}",
        id_str.italic().dimmed(),
        glyph,
        styled_badge,
        styled_title
    );

    let styled_left = left_section;

    // Get context for right-aligned section
    let context = get_task_context(task, store);

    // Calculate visible lengths
    // ID (3) + spaces (2) + glyph (1) + spaces (2) + badge (6) + spaces (2) + title
    let left_visible_len = format!("  {}    {}  {}", id_str, "      ", task.title).len();

    let effective_width = terminal_width.min(MAX_CONTENT_WIDTH);

    // Render with dotted separator
    if let Some(right_section) = context {
        let right_dimmed = right_section.dimmed();
        let right_visible_len = right_section.len();
        let total_content = left_visible_len + right_visible_len;

        if total_content + 4 < effective_width {
            let gap = effective_width - total_content - 2;
            let dots = format!(" {}{}", "·".repeat(gap - 2), " ");
            println!("{}{}{}", styled_left, dots.dimmed(), right_dimmed);
        } else {
            // Fallback: compact inline separator
            println!("{}  {}  {}", styled_left, "·".dimmed(), right_dimmed);
        }
    } else {
        if left_visible_len + 2 < effective_width {
            let gap = effective_width - left_visible_len - 1;
            let dots = format!(" {}", "·".repeat(gap - 2));
            println!("{}{}", styled_left, dots.dimmed());
        } else {
            println!("{}", styled_left);
        }
    }
}

/// Render a view header with title and count
pub fn render_view_header(title: &str, count: usize) {
    let task_word = if count == 1 { "task" } else { "tasks" };
    let count_str = format!("({} {})", count, task_word);
    println!(
        "\n  {} {} {}\n",
        "▌".cyan().bold(),
        title.cyan().bold(),
        count_str.dimmed().italic()
    );
}

/// Render a section header (e.g., "Evening", "Tomorrow")
pub fn render_section_header(title: &str) {
    // Pastel green using truecolor
    println!(
        "\n  {} {}\n",
        "▌".truecolor(144, 238, 144).bold(), // Light green
        title.truecolor(144, 238, 144).bold()
    );
}

/// Render a section separator
pub fn render_section_separator() {
    println!();
}

/// Check if a task is overdue
pub fn is_overdue(task: &Task) -> bool {
    if task.completed_at.is_some() || task.deleted_at.is_some() {
        return false;
    }

    if let crate::models::task::When::Scheduled { date, .. } = task.when {
        let today = jiff::Zoned::now().date();
        return date < today;
    }

    false
}

/// Check if a timestamp is within the last N days
pub fn is_within_days(timestamp: jiff::Timestamp, days: i64) -> bool {
    let now = jiff::Timestamp::now();
    let duration = jiff::SignedDuration::from_hours(days * 24);

    if let Ok(threshold) = now.checked_sub(duration) {
        timestamp >= threshold
    } else {
        false
    }
}

/// Format a date as a human-readable header (e.g., "Tomorrow", "Monday, Feb 17")
pub fn format_date_header(date: Date) -> String {
    let today = jiff::Zoned::now().date();

    if date == today {
        "Today".to_string()
    } else if date == today.tomorrow().expect("tomorrow should be valid") {
        "Tomorrow".to_string()
    } else {
        // Format as "Monday, Feb 17"
        date.strftime("%A, %b %d").to_string()
    }
}

/// Extract year and month from a timestamp for grouping purposes
pub fn get_year_month(timestamp: jiff::Timestamp) -> (i16, i8) {
    let zoned = jiff::Zoned::new(timestamp, jiff::tz::TimeZone::system());
    let date = zoned.date();
    (date.year(), date.month())
}

/// Format a timestamp as a month header (e.g., "February 2026")
pub fn format_month_header(timestamp: jiff::Timestamp) -> String {
    let zoned = jiff::Zoned::new(timestamp, jiff::tz::TimeZone::system());
    zoned.strftime("%B %Y").to_string()
}

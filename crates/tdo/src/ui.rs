use std::{fmt::format, str::MatchIndices};

use colored::*;
use jiff::civil::Date;

use crate::models::{store::Store, task::Task};

/// Get the terminal width, defaulting to 80 if unavailable
fn get_terminal_width() -> usize {
    term_size::dimensions().map(|(w, _)| w).unwrap_or(80)
}

/// Get the appropriate status glyph for a task
pub fn get_status_glyph(task: &Task, is_overdue: bool) -> ColoredString {
    if task.completed_at.is_some() {
        "✓".dimmed()
    } else if is_overdue {
        "●".red()
    } else {
        "○".normal()
    }
}

/// Build the context string for a task (Area/Project hierarchy)
/// Returns None if task has no area or project associations
pub fn get_task_context(task: &Task, store: &Store) -> Option<String> {
    if let Some(project_id) = task.project_id {
        if let Some(project) = store.get_project(project_id) {
            if let Some(area_id) = project.area_id {
                if let Some(area) = store.get_area(area_id) {
                    // Rule A: {Area Name} / {Project Name}
                    return Some(format!("{} / {}", area.name, project.name));
                }
            }
            return Some(project.name.clone());
        }
    }

    if let Some(area_id) = task.area_id {
        if let Some(area) = store.get_area(area_id) {
            return Some(area.name.clone());
        }
    }

    None
}

/// Render a single task line with ID, glyph, title, and right-aligned context
pub fn render_task_line(task: &Task, store: &Store, is_overdue: bool) {
    render_task_line_with_options(task, store, is_overdue, false);
}

/// Render a task line with optional completion date display
pub fn render_task_line_with_completion_date(task: &Task, store: &Store, is_overdue: bool) {
    render_task_line_with_options(task, store, is_overdue, true);
}

/// Internal function to render a task line with various options
fn render_task_line_with_options(
    task: &Task,
    store: &Store,
    is_overdue: bool,
    show_completion_date: bool,
) {
    let terminal_width = get_terminal_width();

    let id_str = format!("{:>3}", task.task_number);
    let glyph = get_status_glyph(task, is_overdue);
    let title = &task.title;

    let styled_title = if task.completed_at.is_some() {
        title.dimmed()
    } else {
        title.white()
    };

    let styled_glyph = if task.completed_at.is_some() {
        glyph.dimmed()
    } else {
        glyph.white()
    };

    let context = get_task_context(task, store);

    let left_section = format!(
        " {}  {}  {}",
        id_str.italic().dimmed(),
        styled_glyph,
        styled_title
    );

    let styled_left = left_section;

    // Build right-aligned section with completion date and/or context
    let right_section = if show_completion_date && task.completed_at.is_some() {
        let completion_date = format_completion_date(task.completed_at.unwrap());
        if let Some(ctx) = context {
            format!("{}  ·  {}", completion_date, ctx)
        } else {
            completion_date
        }
    } else if let Some(ctx) = context {
        ctx
    } else {
        String::new()
    };

    if !right_section.is_empty() {
        let right_dimmed = right_section.dimmed();

        let left_visible_len = format!("  {}  {}  {}", id_str, " ", title).len();
        let right_visible_len = if show_completion_date && task.completed_at.is_some() {
            // Account for the visible length without ANSI codes
            right_section.chars().count()
        } else {
            right_section.len()
        };

        let total_content = left_visible_len + right_visible_len;

        if total_content + 4 < terminal_width {
            let padding = terminal_width - total_content - 2;
            println!("{}{}{}", styled_left, " ".repeat(padding), right_dimmed);
        } else {
            // Not enough space for right alignment, just print normally
            println!("{}", styled_left);
        }
    } else {
        println!("{}", styled_left);
    }
}

/// Format a completion date for display (e.g., "Feb 15", "Today", "Yesterday")
fn format_completion_date(timestamp: jiff::Timestamp) -> String {
    let zoned = jiff::Zoned::new(timestamp, jiff::tz::TimeZone::system());
    let date = zoned.date();
    let today = jiff::Zoned::now().date();

    if date == today {
        "Today".to_string()
    } else if date == today.yesterday().expect("yesterday should be valid") {
        "Yesterday".to_string()
    } else {
        // Format as "Feb 15"
        date.strftime("%b %d").to_string()
    }
}

/// Render a view header with title and count
pub fn render_view_header(title: &str, count: usize) {
    let task_word = if count == 1 { "task" } else { "tasks" };
    let count_str = format!("({} {})", count, task_word);
    println!(
        "\n  {} {}\n",
        title.cyan().bold(),
        count_str.dimmed().italic()
    );
}

/// Render a section header (e.g., "Evening", "Tomorrow")
pub fn render_section_header(title: &str) {
    println!("\n  ─── {} ───\n", title.bold());
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

    if let crate::models::task::When::Scheduled { date } = task.when {
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

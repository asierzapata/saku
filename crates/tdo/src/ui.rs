use colored::*;
use jiff::civil::Date;

use crate::models::{store::Store, task::Task};

const MAX_CONTENT_WIDTH: usize = 100;

/// Represents the urgency level of a task based on its deadline or scheduled date
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TaskUrgency {
    Completed,              // Task is done
    OnTrack,                // Deadline >3 days away
    ApproachingDeadline,    // Deadline 1-3 days away
    DueToday,               // Deadline is today
    Overdue,                // Past deadline or scheduled date
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

    // Priority 1: Check deadline (if exists)
    if let Some(deadline) = task.deadline {
        return calculate_urgency_from_date(deadline, today);
    }

    // Priority 2: Check scheduled date
    if let crate::models::task::When::Scheduled { date } = task.when {
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
/// Returns None if task has no date to display
pub fn get_task_date_badge(task: &Task) -> Option<(String, TaskUrgency)> {
    let today = jiff::Zoned::now().date();

    // Completed tasks: show completion date
    if let Some(completed_at) = task.completed_at {
        let completion_date = jiff::Zoned::new(completed_at, jiff::tz::TimeZone::system()).date();
        let formatted = format_date_compact(completion_date, today, true);
        return Some((formatted, TaskUrgency::Completed));
    }

    // Priority 1: Deadline
    if let Some(deadline) = task.deadline {
        let urgency = calculate_urgency_from_date(deadline, today);
        let formatted = format_date_compact(deadline, today, false);
        return Some((formatted, urgency));
    }

    // Priority 2: Scheduled date
    if let crate::models::task::When::Scheduled { date } = task.when {
        let urgency = calculate_urgency_from_date(date, today);
        let formatted = format_date_compact(date, today, false);
        return Some((formatted, urgency));
    }

    // Priority 3: Someday tasks (show placeholder)
    if let crate::models::task::When::Someday = task.when {
        return Some(("······".to_string(), TaskUrgency::OnTrack));
    }

    // No date to show
    None
}

/// Apply colored background and text styling to date badge
fn style_date_badge(text: &str, urgency: TaskUrgency) -> ColoredString {
    match urgency {
        TaskUrgency::Completed => {
            text.truecolor(180, 180, 180) // Light gray text
                .on_truecolor(60, 60, 60)   // Dark gray background
                .dimmed()
        }
        _ => {
            // All active tasks use consistent gray badge
            text.truecolor(200, 200, 200)   // Light gray text
                .on_truecolor(70, 70, 70)   // Dark gray background
        }
    }
}

/// Get the appropriate status glyph for a task based on urgency
pub fn get_status_glyph(urgency: TaskUrgency) -> ColoredString {
    match urgency {
        TaskUrgency::Completed => "✔".dimmed(),  // U+2714 - Heavy Check Mark
        TaskUrgency::OnTrack => "▢".green(),  // U+25A2 - Empty square
        TaskUrgency::ApproachingDeadline => "▢".yellow(),  // U+25A2 - Empty square
        TaskUrgency::DueToday => "▢".truecolor(255, 140, 0),  // U+25A2 - Orange square
        TaskUrgency::Overdue => "▢".red(),  // U+25A2 - Empty square
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
    
    // Get date badge (if applicable)
    let date_badge = get_task_date_badge(task);
    
    // Style task title
    let styled_title = if task.completed_at.is_some() {
        task.title.dimmed()
    } else {
        task.title.white()
    };
    
    // Build left section with optional date badge
    let has_date_badge = date_badge.is_some();
    let left_section = if let Some((date_str, badge_urgency)) = date_badge {
        let styled_badge = style_date_badge(&date_str, badge_urgency);
        format!(
            " {}  {}  {}  {}",
            id_str.italic().dimmed(),
            glyph,
            styled_badge,
            styled_title
        )
    } else {
        format!(
            " {}  {}  {}",
            id_str.italic().dimmed(),
            glyph,
            styled_title
        )
    };
    
    let styled_left = left_section;
    
    // Get context for right-aligned section
    let context = get_task_context(task, store);
    let right_section = context.unwrap_or_default();
    
    // Render with dotted separator
    if !right_section.is_empty() {
        let right_dimmed = right_section.dimmed();
        
        // Calculate visible lengths
        let left_visible_len = if has_date_badge {
            // ID (3) + spaces (2) + glyph (1) + spaces (2) + badge (6) + spaces (2) + title
            format!("  {}    {}  {}", id_str, "      ", task.title).len()
        } else {
            // ID (3) + spaces (2) + glyph (1) + spaces (2) + title
            format!("  {}    {}", id_str, task.title).len()
        };
        
        let right_visible_len = right_section.len();
        let effective_width = terminal_width.min(MAX_CONTENT_WIDTH);
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

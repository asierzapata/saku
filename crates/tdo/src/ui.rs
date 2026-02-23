use colored::*;
use jiff::civil::Date;
use uuid::Uuid;

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

/// Build an abbreviated context string for a task (first 2 chars of each part)
/// Used as a fallback when the full context doesn't fit on the line
fn get_task_context_abbreviated(task: &Task, store: &Store) -> Option<String> {
    if let Some(project_id) = task.project_id
        && let Some(project) = store.get_project(project_id)
    {
        let proj_abbrev: String = project.name.chars().take(2).collect();
        if let Some(area_id) = project.area_id
            && let Some(area) = store.get_area(area_id)
        {
            let area_abbrev: String = area.name.chars().take(2).collect();
            return Some(format!("{} / {}", area_abbrev, proj_abbrev));
        }
        return Some(proj_abbrev);
    }

    if let Some(area_id) = task.area_id
        && let Some(area) = store.get_area(area_id)
    {
        let area_abbrev: String = area.name.chars().take(2).collect();
        return Some(area_abbrev);
    }

    None
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
    render_task_line_with_options(task, store, false, false, None);
}

/// Render a task line with optional completion date display
pub fn render_task_line_with_completion_date(task: &Task, store: &Store) {
    render_task_line_with_options(task, store, false, true, None);
}

/// Render a task line for the Recurring view, showing the next occurrence date.
pub fn render_task_line_with_next_occurrence(
    task: &Task,
    store: &Store,
    next_date: jiff::civil::Date,
) {
    render_task_line_with_options(task, store, false, false, Some(next_date));
}

/// Build a compact blocker badge string: "[blocked: #11]" or "[blocked: #11 +2]"
fn blocker_badge(blockers: &[&crate::models::task::Task]) -> String {
    match blockers.len() {
        0 => String::new(),
        1 => format!("[blocked: #{}]", blockers[0].task_number),
        n => format!("[blocked: #{} +{}]", blockers[0].task_number, n - 1),
    }
}

/// Internal function to render a task line with various options
fn render_task_line_with_options(
    task: &Task,
    store: &Store,
    _is_overdue: bool,
    _show_completion_date: bool,
    next_occurrence: Option<jiff::civil::Date>,
) {
    let terminal_width = get_terminal_width();
    let effective_width = terminal_width.min(MAX_CONTENT_WIDTH);

    let id_str = format!("{:>3}", task.task_number);
    let urgency = calculate_task_urgency(task);
    let glyph = get_status_glyph(urgency);
    let (date_str, badge_urgency) = get_task_date_badge(task);

    // Fixed overhead: " {id}  {glyph}  {badge}  " visible chars
    // 1 (leading space) + 3 (id) + 2 (spaces) + 1 (glyph) + 2 (spaces) + badge_len + 2 (spaces)
    let badge_visible_len = date_str.chars().count();
    let fixed_overhead = 11 + badge_visible_len;
    let available = effective_width.saturating_sub(fixed_overhead);

    let title_chars: Vec<char> = task.title.chars().collect();
    let title_len = title_chars.len();

    // Recurrence badge suffix (e.g. "↻ Mon, Wed, Fri")
    let recurrence_badge: Option<String> = task.recurrence.as_ref().map(|r| {
        if let Some(next) = next_occurrence {
            let today = jiff::Zoned::now().date();
            let next_str = format_date_compact(next, today, false);
            format!("↻ {}  {}", r, next_str.trim())
        } else {
            format!("↻ {}", r)
        }
    });

    // Context: full and abbreviated fallback
    let context_full = get_task_context(task, store);
    // Append recurrence badge to context if both exist, or use badge alone
    let context_full = match (context_full, recurrence_badge.clone()) {
        (Some(ctx), Some(badge)) => Some(format!("{}  {}", ctx, badge)),
        (Some(ctx), None) => Some(ctx),
        (None, Some(badge)) => Some(badge),
        (None, None) => None,
    };
    let context_abbrev = context_full
        .as_ref()
        .and_then(|_| {
            let abbrev = get_task_context_abbreviated(task, store);
            match (abbrev, recurrence_badge.as_ref()) {
                (Some(a), Some(b)) => Some(format!("{}  {}", a, b)),
                (Some(a), None) => Some(a),
                (None, Some(b)) => Some(b.clone()),
                (None, None) => None,
            }
        });

    const ELLIPSIS: &str = "[…]";
    const ELLIPSIS_LEN: usize = 3;
    const MIN_SEP: usize = 3; // minimum " · " separator

    // Decide what to display, in priority order:
    //   1. Full title + full context
    //   2. Full title + abbreviated context (first 2 letters of each part)
    //   3. Truncated title[…] + abbreviated context
    let (display_title, display_context): (String, Option<String>) =
        if let Some(ref ctx_full) = context_full {
            let ctx_full_len = ctx_full.chars().count();
            let ctx_abbrev = context_abbrev.as_deref().unwrap_or("");
            let ctx_abbrev_len = ctx_abbrev.chars().count();

            if title_len + MIN_SEP + ctx_full_len <= available {
                // Full title + full context fits
                (task.title.clone(), Some(ctx_full.clone()))
            } else if title_len + MIN_SEP + ctx_abbrev_len <= available {
                // Full title + abbreviated context fits
                (task.title.clone(), Some(ctx_abbrev.to_string()))
            } else {
                // Truncate title to fit alongside abbreviated context
                let title_space = available.saturating_sub(MIN_SEP + ctx_abbrev_len);
                let truncated = if title_space >= ELLIPSIS_LEN {
                    title_chars[..title_space - ELLIPSIS_LEN]
                        .iter()
                        .collect::<String>()
                        + ELLIPSIS
                } else {
                    title_chars[..title_space.min(title_len)].iter().collect()
                };
                (truncated, Some(ctx_abbrev.to_string()))
            }
        } else {
            // No context: show full title or truncate if needed
            if title_len <= available {
                (task.title.clone(), None)
            } else if available >= ELLIPSIS_LEN {
                let truncated =
                    title_chars[..available - ELLIPSIS_LEN].iter().collect::<String>() + ELLIPSIS;
                (truncated, None)
            } else {
                (title_chars[..available.min(title_len)].iter().collect(), None)
            }
        };

    // Determine blocked state
    let blockers = store.get_blockers(task);
    let is_blocked = !blockers.is_empty();

    // Build styled left section
    let styled_title = if task.completed_at.is_some() || is_blocked {
        display_title.as_str().dimmed()
    } else {
        display_title.as_str().white()
    };
    let styled_badge = style_date_badge(&date_str, badge_urgency);
    let left_section = format!(
        " {}  {}  {}  {}",
        id_str.italic().dimmed(),
        glyph,
        styled_badge,
        styled_title
    );

    let display_title_len = display_title.chars().count();
    let display_ctx_len = display_context
        .as_ref()
        .map(|c| c.chars().count())
        .unwrap_or(0);

    let badge_str = if is_blocked {
        blocker_badge(&blockers)
    } else {
        String::new()
    };
    let badge_str_len = badge_str.chars().count();

    if let Some(ref ctx) = display_context {
        // Fill remaining space with dots between title and context (+ blocker badge after context)
        let right_len = display_ctx_len + if badge_str_len > 0 { 1 + badge_str_len } else { 0 };
        let sep_space =
            effective_width.saturating_sub(fixed_overhead + display_title_len + right_len);
        let dots_count = sep_space.saturating_sub(2);
        let separator = format!(" {}{}", "·".repeat(dots_count), " ");
        if badge_str_len > 0 {
            println!(
                "{}{}{} {}",
                left_section,
                separator.dimmed(),
                ctx.as_str().dimmed(),
                badge_str.dimmed()
            );
        } else {
            println!(
                "{}{}{}",
                left_section,
                separator.dimmed(),
                ctx.as_str().dimmed()
            );
        }
    } else {
        // No context: fill remaining space with dots, then blocker badge
        let right_len = badge_str_len;
        let fill_space =
            effective_width.saturating_sub(fixed_overhead + display_title_len + right_len);
        let fill = if fill_space >= 2 {
            format!(" {}", "·".repeat(fill_space - 1))
        } else if fill_space == 1 {
            " ".to_string()
        } else {
            String::new()
        };
        if badge_str_len > 0 {
            println!("{}{}{}", left_section, fill.dimmed(), badge_str.dimmed());
        } else {
            println!("{}{}", left_section, fill.dimmed());
        }
    }
}

/// Render a single subtask line, indented under its parent
pub fn render_subtask_line(task: &Task, store: &Store) {
    let terminal_width = get_terminal_width();
    let effective_width = terminal_width.min(MAX_CONTENT_WIDTH);

    // Indent prefix: 6 chars visible ("    └─"), shown dimmed
    let prefix = "    └─";
    let prefix_len = 6usize;

    let id_str = format!("{:>3}", task.task_number);
    let urgency = calculate_task_urgency(task);
    let glyph = get_status_glyph(urgency);
    let (date_str, badge_urgency) = get_task_date_badge(task);

    // Fixed overhead: prefix + " {id}  {glyph}  {badge}  "
    // prefix_len + 1 (space) + 3 (id) + 2 (spaces) + 1 (glyph) + 2 (spaces) + badge_len + 2 (spaces)
    let badge_visible_len = date_str.chars().count();
    let fixed_overhead = prefix_len + 11 + badge_visible_len;
    let available = effective_width.saturating_sub(fixed_overhead);

    let title_chars: Vec<char> = task.title.chars().collect();
    let title_len = title_chars.len();

    const ELLIPSIS: &str = "[…]";
    const ELLIPSIS_LEN: usize = 3;

    let display_title = if title_len <= available {
        task.title.clone()
    } else if available >= ELLIPSIS_LEN {
        title_chars[..available - ELLIPSIS_LEN].iter().collect::<String>() + ELLIPSIS
    } else {
        title_chars[..available.min(title_len)].iter().collect()
    };

    // Determine blocked state
    let blockers = store.get_blockers(task);
    let is_blocked = !blockers.is_empty();

    let styled_title = if task.completed_at.is_some() || is_blocked {
        display_title.as_str().dimmed()
    } else {
        display_title.as_str().white()
    };
    let styled_badge = style_date_badge(&date_str, badge_urgency);

    let display_title_len = display_title.chars().count();
    let badge_str = if is_blocked {
        blocker_badge(&blockers)
    } else {
        String::new()
    };
    let badge_str_len = badge_str.chars().count();

    // Fill remaining space with dots
    let right_len = badge_str_len;
    let fill_space = effective_width.saturating_sub(fixed_overhead + display_title_len + right_len);
    let fill = if fill_space >= 2 {
        format!(" {}", "·".repeat(fill_space - 1))
    } else if fill_space == 1 {
        " ".to_string()
    } else {
        String::new()
    };

    let left_section = format!(
        "{}  {}  {}  {}",
        id_str.italic().dimmed(),
        glyph,
        styled_badge,
        styled_title
    );

    if badge_str_len > 0 {
        println!(
            "{} {}{}{}",
            prefix.dimmed(),
            left_section,
            fill.dimmed(),
            badge_str.dimmed()
        );
    } else {
        println!("{} {}{}", prefix.dimmed(), left_section, fill.dimmed());
    }
}

/// Render all non-deleted subtasks of a parent task, ordered by task_number
pub fn render_subtask_children(parent_id: Uuid, store: &Store) {
    let mut subtasks: Vec<&Task> = store
        .tasks
        .values()
        .filter(|t| t.parent_task_id == Some(parent_id) && t.deleted_at.is_none())
        .collect();
    subtasks.sort_by_key(|t| t.task_number);
    for subtask in subtasks {
        render_subtask_line(subtask, store);
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

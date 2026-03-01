use colored::*;
use unicode_width::UnicodeWidthStr;

use crate::models::{
    entry::{Entry, EntryKind},
    store::Store,
};

/// Get the terminal width, defaulting to 80 if unavailable
fn get_terminal_width() -> usize {
    term_size::dimensions().map(|(w, _)| w).unwrap_or(80)
}

/// Truncate a string to fit within a maximum display width, adding ellipsis if needed
fn truncate_to_width(s: &str, max_width: usize, ellipsis: &str) -> String {
    use unicode_width::UnicodeWidthChar;

    let current_width = s.width();
    if current_width <= max_width {
        return s.to_string();
    }

    let ellipsis_width = ellipsis.width();
    if max_width < ellipsis_width {
        let mut result = String::new();
        let mut current = 0;
        for ch in s.chars() {
            let ch_width = ch.width().unwrap_or(0);
            if current + ch_width > max_width {
                break;
            }
            result.push(ch);
            current += ch_width;
        }
        return result;
    }

    let target_width = max_width - ellipsis_width;
    let mut result = String::new();
    let mut current = 0;
    for ch in s.chars() {
        let ch_width = ch.width().unwrap_or(0);
        if current + ch_width > target_width {
            break;
        }
        result.push(ch);
        current += ch_width;
    }
    result.push_str(ellipsis);
    result
}

/// Format project name from project_key (strip "project/" prefix, title-case)
fn format_project_name(project_key: &str) -> String {
    let name = project_key
        .strip_prefix("project/")
        .unwrap_or(project_key);
    // Title case: capitalize first letter of each word
    name.split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => {
                    let mut s = c.to_uppercase().to_string();
                    s.extend(chars);
                    s
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Format a date string for display ("2026-03-01" -> "Mar 01, 2026")
fn format_date_display(date_str: &str) -> String {
    if let Ok(date) = jiff::civil::Date::strptime("%Y-%m-%d", date_str) {
        date.strftime("%b %d, %Y").to_string()
    } else {
        date_str.to_string()
    }
}

/// Format a date string for header ("2026-03-01" -> "Mar 01")
fn format_date_short(date_str: &str) -> String {
    if let Ok(date) = jiff::civil::Date::strptime("%Y-%m-%d", date_str) {
        date.strftime("%b %d").to_string()
    } else {
        date_str.to_string()
    }
}

/// Format time for display ("14:30:00" -> "14:30")
fn format_time_short(time_str: &str) -> String {
    // Just take HH:MM from HH:MM:SS
    if time_str.len() >= 5 {
        time_str[..5].to_string()
    } else {
        time_str.to_string()
    }
}

/// Render the "Today" view showing all entries for today
///
/// ```text
///   Today (Mar 01)                                4 entries
///
///   10:14  #1  Reviewed PR #42, left comments     website
///   17:30  #4  ★ Auth fix on staging...           website
/// ```
pub fn render_today_view(store: &Store) {
    let today = jiff::Zoned::now();
    let today_str = today.strftime("%Y-%m-%d").to_string();
    let entries = store.get_entries_for_date(&today_str);
    let total = entries.len();

    let date_short = format_date_short(&today_str);
    let title = format!("Today ({})", date_short);
    render_view_header(&title, total, "entry", "entries");

    if entries.is_empty() {
        println!("    {}", "No entries yet.".dimmed());
        println!();
        return;
    }

    let term_width = get_terminal_width();

    for entry in &entries {
        render_entry_line(entry, term_width);
    }

    println!();
}

/// Render a view header matching tdo's style
fn render_view_header(title: &str, count: usize, singular: &str, plural: &str) {
    let word = if count == 1 { singular } else { plural };
    let count_str = format!("({} {})", count, word);
    println!(
        "\n  {} {} {}\n",
        "▌".cyan().bold(),
        title.cyan().bold(),
        count_str.dimmed().italic()
    );
}

/// Render a single entry line in the today view
fn render_entry_line(entry: &Entry, term_width: usize) {
    let time = format_time_short(&entry.time);
    let number = format!("#{}", entry.entry_number);

    let body_first_line = entry.body.lines().next().unwrap_or("");

    // Build project context
    let context = entry
        .project_key
        .as_ref()
        .map(|pk| format_project_name(pk));

    // Layout: "  {time}  {number}  {body}     {context}"
    // Widths: 2 + 5 + 2 + number_width + 2 + body + 2 + context
    let prefix_len = 2 + 5 + 2 + number.width() + 2; // "  10:14  #1  "
    let context_len = context.as_ref().map(|c| c.width() + 4).unwrap_or(0); // "    website"
    let available_body = term_width.saturating_sub(prefix_len + context_len);

    let body_truncated = if entry.kind == EntryKind::Handoff {
        // Account for "★ " prefix (2 display chars) in the available width
        let plain_body = truncate_to_width(body_first_line, available_body.saturating_sub(2), "...");
        format!("{} {}", "★".yellow().bold(), plain_body)
    } else {
        truncate_to_width(body_first_line, available_body, "...")
    };

    // Calculate padding for right-aligned context
    let body_plain_width = if entry.kind == EntryKind::Handoff {
        2 + truncate_to_width(body_first_line, available_body.saturating_sub(2), "...").width()
    } else {
        truncate_to_width(body_first_line, available_body, "...").width()
    };

    let padding = available_body.saturating_sub(body_plain_width);

    match context {
        Some(ctx) => {
            println!(
                "  {}  {}  {}{:>pad$}    {}",
                time.dimmed(),
                number.dimmed(),
                body_truncated,
                "",
                ctx.dimmed(),
                pad = padding,
            );
        }
        None => {
            println!(
                "  {}  {}  {}",
                time.dimmed(),
                number.dimmed(),
                body_truncated,
            );
        }
    }
}

/// Render the detail view for a single entry (`jrn show <id>`)
///
/// ```text
///   ▌ #4  ★ Auth fix on staging...
///
///     Kind        Handoff
///     Author      human
///     Date        Mar 01, 2026
///     Time        17:30
///     Project     Website
///     Tags        #deploy  #auth
///     Refs        tdo:42 · tdo:43
///
///   Auth fix on staging. Needs prod deploy tomorrow.
/// ```
pub fn render_entry_detail(entry: &Entry) {
    let number_str = format!("#{}", entry.entry_number);

    let title_prefix = if entry.kind == EntryKind::Handoff {
        format!("{} ", "★".yellow().bold())
    } else {
        String::new()
    };

    let body_first_line = entry.body.lines().next().unwrap_or("");

    println!(
        "\n  {} {}  {}{}\n",
        "▌".cyan().bold(),
        number_str.italic().dimmed(),
        title_prefix,
        body_first_line.white(),
    );

    // Helper closure: print a labeled field line
    let field = |label: &str, value: ColoredString| {
        println!("    {:<12}{}", label.dimmed(), value);
    };

    // Kind
    field("Kind", entry.kind.to_string().white());

    // Author
    field("Author", entry.author.as_str().white());

    // Date
    field("Date", format_date_display(&entry.date).white());

    // Time
    field("Time", format_time_short(&entry.time).white());

    // Project
    if let Some(ref pk) = entry.project_key {
        field("Project", format_project_name(pk).white());
    }

    // Tags
    if !entry.tags.is_empty() {
        let tags_str = entry
            .tags
            .iter()
            .map(|t| format!("#{}", t))
            .collect::<Vec<_>>()
            .join("  ");
        field("Tags", tags_str.white());
    }

    // Refs
    if !entry.refs.is_empty() {
        let refs_str = entry.refs.join(" · ");
        field("Refs", refs_str.white());
    }

    // Full body (if multi-line)
    let body_lines: Vec<&str> = entry.body.lines().collect();
    if body_lines.len() > 1 || !entry.body.is_empty() {
        println!();
        for line in &body_lines {
            println!("  {}", line);
        }
    }

    println!();
}

/// Render the handoff read view (`jrn handoff --read`)
///
/// ```text
///   ★  Handoff #11 · Mar 01 17:30 · human          Website
///
///   Auth fix on staging. Needs prod deploy tomorrow.
///
///   Refs  tdo:42 · tdo:43
/// ```
pub fn render_handoff_read(entry: &Entry) {
    let number_str = format!("Handoff #{}", entry.entry_number);
    let date_short = format_date_short(&entry.date);
    let time_short = format_time_short(&entry.time);

    let context = entry
        .project_key
        .as_ref()
        .map(|pk| format_project_name(pk));

    let header = format!(
        "{} · {} {} · {}",
        number_str, date_short, time_short, entry.author
    );

    match context {
        Some(ctx) => {
            println!(
                "\n  {}  {}    {}",
                "★".yellow().bold(),
                header.white().bold(),
                ctx.dimmed(),
            );
        }
        None => {
            println!(
                "\n  {}  {}",
                "★".yellow().bold(),
                header.white().bold(),
            );
        }
    }

    println!();
    for line in entry.body.lines() {
        println!("  {}", line);
    }

    if !entry.refs.is_empty() {
        let refs_str = entry.refs.join(" · ");
        println!();
        println!("  {}  {}", "Refs".dimmed(), refs_str);
    }

    println!();
}

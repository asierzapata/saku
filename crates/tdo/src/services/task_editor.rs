use std::env;
use std::io::{Read as IoRead, Write as IoWrite};
use std::process::Command;
use tempfile::NamedTempFile;

use crate::models::{
    store::Store,
    task::{Task, When},
};

/// Get the editor from environment variables or platform-specific fallback
pub fn get_editor() -> String {
    // Try $VISUAL first, then $EDITOR, then platform defaults
    env::var("VISUAL")
        .or_else(|_| env::var("EDITOR"))
        .unwrap_or_else(|_| {
            // Platform-specific fallbacks
            if cfg!(target_os = "windows") {
                "notepad".to_string()
            } else {
                "vi".to_string()
            }
        })
}

/// Serialize a task to the human-friendly editor format
pub fn serialize_task_for_edit(task: &Task, store: &Store) -> String {
    let mut output = String::new();

    // Header
    output.push_str(&format!("# Edit Task #{}\n", task.task_number));
    output.push_str("# Lines starting with # are ignored\n");
    output.push_str("# Save and close to apply changes\n\n");

    // Title
    output.push_str(&format!("Title: {}\n\n", task.title));

    // Notes
    output.push_str("Notes:\n");
    if let Some(notes) = &task.notes {
        for line in notes.lines() {
            output.push_str(line);
            output.push('\n');
        }
    }
    output.push('\n');

    // Project
    output.push_str("Project: ");
    if let Some(project_id) = task.project_id
        && let Some(project) = store.get_project(project_id)
    {
        output.push_str(&project.name);
    }
    output.push_str("\n\n");

    // Area
    output.push_str("Area: ");
    if let Some(area_id) = task.area_id
        && let Some(area) = store.get_area(area_id)
    {
        output.push_str(&area.name);
    }
    output.push_str("\n\n");

    // Tags
    output.push_str("Tags: ");
    output.push_str(&task.tags.join(", "));
    output.push_str("\n\n");

    // When
    output.push_str("When: ");
    output.push_str(&when_to_string(&task.when));
    output.push('\n');
    output.push_str("# Options: inbox, today, today-evening, anytime, someday, or YYYY-MM-DD\n\n");

    // Deadline
    output.push_str("Deadline: ");
    if let Some(deadline) = task.deadline {
        output.push_str(&deadline.to_string());
    }
    output.push('\n');
    output.push_str("# Format: YYYY-MM-DD or leave empty\n\n");

    // Defer Until
    output.push_str("Defer Until: ");
    if let Some(defer_until) = task.defer_until {
        output.push_str(&defer_until.to_string());
    }
    output.push('\n');
    output.push_str("# Format: YYYY-MM-DD or leave empty\n\n");

    // Checklist
    output.push_str("Checklist:\n");
    for item in &task.checklist {
        let checkbox = if item.completed { "[x]" } else { "[ ]" };
        output.push_str(&format!("{} {}\n", checkbox, item.title));
    }

    output
}

/// Convert When enum to string representation for editor
fn when_to_string(when: &When) -> String {
    match when {
        When::Inbox => "inbox".to_string(),
        When::Someday => "someday".to_string(),
        When::Scheduled { date, .. } => date.to_string(),
        When::LegacyToday { .. } => "today".to_string(), // Should not appear after migration
        When::LegacyAnytime => "someday".to_string(), // Converted to someday
    }
}

/// Open content in editor, wait for user to edit, and return the modified content
pub fn open_in_editor(content: &str) -> Result<String, String> {
    let mut temp_file =
        NamedTempFile::new().map_err(|e| format!("Failed to create temporary file: {}", e))?;

    temp_file
        .write_all(content.as_bytes())
        .map_err(|e| format!("Failed to write to temporary file: {}", e))?;

    temp_file
        .flush()
        .map_err(|e| format!("Failed to flush temporary file: {}", e))?;

    let editor = get_editor();
    let temp_path = temp_file.path();

    let parts: Vec<&str> = editor.split_whitespace().collect();
    if parts.is_empty() {
        return Err("Editor command is empty".to_string());
    }

    let (cmd, args) = (parts[0], &parts[1..]);

    let mut command = Command::new(cmd);
    command.args(args).arg(temp_path);

    let status = command
        .status()
        .map_err(|e| format!("Failed to execute editor '{}': {}", editor, e))?;

    if !status.success() {
        return Err(format!("Editor '{}' exited with non-zero status", editor));
    }

    let mut modified_content = String::new();
    temp_file
        .reopen()
        .map_err(|e| format!("Failed to reopen temporary file: {}", e))?
        .read_to_string(&mut modified_content)
        .map_err(|e| format!("Failed to read modified content: {}", e))?;

    Ok(modified_content)
}

/// Parsed task data from editor
pub struct ParsedTaskEdit {
    pub title: String,
    pub notes: Option<String>,
    pub project: Option<String>,
    pub area: Option<String>,
    pub tags: Vec<String>,
    pub when: String,
    pub deadline: Option<String>,
    pub defer_until: Option<String>,
    pub checklist: Vec<(String, bool)>, // (title, completed)
}

/// Parse the edited content back into structured fields
pub fn parse_edited_task(content: &str) -> Result<ParsedTaskEdit, String> {
    let mut title: Option<String> = None;
    let mut notes_lines: Vec<String> = Vec::new();
    let mut project: Option<String> = None;
    let mut area: Option<String> = None;
    let mut tags: Vec<String> = Vec::new();
    let mut when: Option<String> = None;
    let mut deadline: Option<String> = None;
    let mut defer_until: Option<String> = None;
    let mut checklist: Vec<(String, bool)> = Vec::new();

    let mut in_notes = false;
    let mut in_checklist = false;

    for line in content.lines() {
        // Skip comment lines
        if line.trim().starts_with('#') {
            continue;
        }

        // First, check if this is a field header (before checking section state)
        let is_field_header = line.starts_with("Title:")
            || line.starts_with("Notes:")
            || line.starts_with("Project:")
            || line.starts_with("Area:")
            || line.starts_with("Tags:")
            || line.starts_with("When:")
            || line.starts_with("Deadline:")
            || line.starts_with("Defer Until:")
            || line.starts_with("Checklist:");

        if is_field_header {
            // Process field header
            if line.starts_with("Title: ") {
                title = Some(line.trim_start_matches("Title: ").trim().to_string());
                in_notes = false;
                in_checklist = false;
            } else if line.starts_with("Notes:") {
                in_notes = true;
                in_checklist = false;
                notes_lines.clear();
            } else if line.starts_with("Project: ") {
                project = Some(line.trim_start_matches("Project: ").trim().to_string());
                in_notes = false;
                in_checklist = false;
            } else if line.starts_with("Area: ") {
                area = Some(line.trim_start_matches("Area: ").trim().to_string());
                in_notes = false;
                in_checklist = false;
            } else if line.starts_with("Tags: ") {
                let tags_str = line.trim_start_matches("Tags: ").trim();
                tags = tags_str
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                in_notes = false;
                in_checklist = false;
            } else if line.starts_with("When: ") {
                when = Some(line.trim_start_matches("When: ").trim().to_string());
                in_notes = false;
                in_checklist = false;
            } else if line.starts_with("Deadline: ") {
                deadline = Some(line.trim_start_matches("Deadline: ").trim().to_string());
                in_notes = false;
                in_checklist = false;
            } else if line.starts_with("Defer Until: ") {
                defer_until = Some(line.trim_start_matches("Defer Until: ").trim().to_string());
                in_notes = false;
                in_checklist = false;
            } else if line.starts_with("Checklist:") {
                in_checklist = true;
                in_notes = false;
            }
        } else if in_notes {
            notes_lines.push(line.to_string());
        } else if in_checklist {
            let trimmed = line.trim();
            if trimmed.starts_with("[ ]") || trimmed.starts_with("[x]") {
                let completed = trimmed.starts_with("[x]");
                let item_title = trimmed[3..].trim().to_string();
                if !item_title.is_empty() {
                    checklist.push((item_title, completed));
                }
            }
        }
    }

    let title = title.ok_or("Title is required")?;
    if title.is_empty() {
        return Err("Title cannot be empty".to_string());
    }

    // Process notes
    let notes = if notes_lines.is_empty() {
        None
    } else {
        let notes_text = notes_lines.join("\n").trim_end().to_string();
        if notes_text.is_empty() {
            None
        } else {
            Some(notes_text)
        }
    };

    let project = project.and_then(|s| if s.is_empty() { None } else { Some(s) });
    let area = area.and_then(|s| if s.is_empty() { None } else { Some(s) });
    let when = when.unwrap_or_else(|| "inbox".to_string());
    let deadline = deadline.and_then(|s| if s.is_empty() { None } else { Some(s) });
    let defer_until = defer_until.and_then(|s| if s.is_empty() { None } else { Some(s) });

    Ok(ParsedTaskEdit {
        title,
        notes,
        project,
        area,
        tags,
        when,
        deadline,
        defer_until,
        checklist,
    })
}

/// Detect if any fields were actually modified
pub fn has_changes(original_task: &Task, parsed: &ParsedTaskEdit, store: &Store) -> bool {
    if original_task.title != parsed.title {
        return true;
    }

    if original_task.notes != parsed.notes {
        return true;
    }

    let original_project = original_task
        .project_id
        .and_then(|id| store.get_project(id))
        .map(|p| p.name.clone());
    if original_project.as_deref() != parsed.project.as_deref() {
        return true;
    }

    let original_area = original_task
        .area_id
        .and_then(|id| store.get_area(id))
        .map(|a| a.name.clone());
    if original_area.as_deref() != parsed.area.as_deref() {
        return true;
    }

    if original_task.tags != parsed.tags {
        return true;
    }

    if when_to_string(&original_task.when) != parsed.when {
        return true;
    }

    let original_deadline = original_task.deadline.map(|d| d.to_string());
    if original_deadline.as_deref() != parsed.deadline.as_deref() {
        return true;
    }

    let original_defer = original_task.defer_until.map(|d| d.to_string());
    if original_defer.as_deref() != parsed.defer_until.as_deref() {
        return true;
    }

    if original_task.checklist.len() != parsed.checklist.len() {
        return true;
    }
    for (i, original_item) in original_task.checklist.iter().enumerate() {
        if let Some((parsed_title, parsed_completed)) = parsed.checklist.get(i)
            && (original_item.title != *parsed_title || original_item.completed != *parsed_completed)
        {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        area::Area,
        project::Project,
        task::{ChecklistItem, Task},
    };
    use jiff::civil::Date;
    use uuid::Uuid;

    fn create_test_store() -> Store {
        Store::default()
    }

    #[test]
    fn test_serialize_minimal_task() {
        let store = create_test_store();
        let task = Task {
            id: Uuid::new_v4(),
            task_number: 42,
            title: "Test task".to_string(),
            notes: None,
            project_id: None,
            area_id: None,
            tags: vec![],
            when: When::Inbox,
            deadline: None,
            defer_until: None,
            checklist: vec![],
            completed_at: None,
            deleted_at: None,
            created_at: jiff::Timestamp::now(),
            modified_at: saku_storage::timestamp::HybridTimestamp::default(),
        };

        let serialized = serialize_task_for_edit(&task, &store);

        assert!(serialized.contains("# Edit Task #42"));
        assert!(serialized.contains("Title: Test task"));
        assert!(serialized.contains("When: inbox"));
    }

    #[test]
    fn test_serialize_full_task() {
        let mut store = create_test_store();

        // Create area and project
        let area = Area {
            id: Uuid::new_v4(),
            name: "Work".to_string(),
            deleted_at: None,
            modified_at: saku_storage::timestamp::HybridTimestamp::default(),
        };
        store.add_area(area.clone());

        let project = Project {
            id: Uuid::new_v4(),
            name: "Backend API".to_string(),
            area_id: Some(area.id),
            notes: None,
            deadline: None,
            created_at: jiff::Timestamp::now(),
            completed_at: None,
            deleted_at: None,
            modified_at: saku_storage::timestamp::HybridTimestamp::default(),
        };
        store.add_project(project.clone());

        let task = Task {
            id: Uuid::new_v4(),
            task_number: 1,
            title: "Fix bug".to_string(),
            notes: Some("This is important\nMulti-line notes".to_string()),
            project_id: Some(project.id),
            area_id: Some(area.id),
            tags: vec!["urgent".to_string(), "bug".to_string()],
            when: When::Scheduled {
                date: jiff::Zoned::now().date(),
                evening: None,
            },
            deadline: Some("2026-03-15".parse::<Date>().unwrap()),
            defer_until: None,
            checklist: vec![
                ChecklistItem {
                    id: Uuid::new_v4(),
                    title: "Step 1".to_string(),
                    completed: false,
                },
                ChecklistItem {
                    id: Uuid::new_v4(),
                    title: "Step 2".to_string(),
                    completed: true,
                },
            ],
            completed_at: None,
            deleted_at: None,
            created_at: jiff::Timestamp::now(),
            modified_at: saku_storage::timestamp::HybridTimestamp::default(),
        };

        let serialized = serialize_task_for_edit(&task, &store);

        assert!(serialized.contains("Title: Fix bug"));
        assert!(serialized.contains("This is important"));
        assert!(serialized.contains("Multi-line notes"));
        assert!(serialized.contains("Project: Backend API"));
        assert!(serialized.contains("Area: Work"));
        assert!(serialized.contains("Tags: urgent, bug"));
        // When should show the date (which is today's date in ISO format)
        let today_str = jiff::Zoned::now().date().to_string();
        assert!(serialized.contains(&format!("When: {}", today_str)));
        assert!(serialized.contains("Deadline: 2026-03-15"));
        assert!(serialized.contains("[ ] Step 1"));
        assert!(serialized.contains("[x] Step 2"));
    }

    #[test]
    fn test_parse_simple_edit() {
        let content = r#"# Edit Task #1
Title: Updated title

Notes:
Some notes here

Project: MyProject

Area: Work

Tags: tag1, tag2

When: today

Deadline: 2026-03-15

Defer Until:

Checklist:
"#;

        let parsed = parse_edited_task(content).unwrap();
        assert_eq!(parsed.title, "Updated title");
        assert_eq!(parsed.notes, Some("Some notes here".to_string()));
        assert_eq!(parsed.project, Some("MyProject".to_string()));
        assert_eq!(parsed.area, Some("Work".to_string()));
        assert_eq!(parsed.tags, vec!["tag1", "tag2"]);
        assert_eq!(parsed.when, "today");
        assert_eq!(parsed.deadline, Some("2026-03-15".to_string()));
        assert_eq!(parsed.defer_until, None);
    }

    #[test]
    fn test_parse_multiline_notes() {
        let content = r#"Title: Task title

Notes:
Line 1
Line 2
Line 3

Project:

Area:

Tags:

When: inbox

Deadline:

Defer Until:

Checklist:
"#;

        let parsed = parse_edited_task(content).unwrap();
        assert_eq!(parsed.notes, Some("Line 1\nLine 2\nLine 3".to_string()));
    }

    #[test]
    fn test_parse_checklist() {
        let content = r#"Title: Task with checklist

Notes:

Project:

Area:

Tags:

When: inbox

Deadline:

Defer Until:

Checklist:
[ ] Item 1
[x] Item 2
[ ] Item 3
"#;

        let parsed = parse_edited_task(content).unwrap();
        assert_eq!(parsed.checklist.len(), 3);
        assert_eq!(parsed.checklist[0], ("Item 1".to_string(), false));
        assert_eq!(parsed.checklist[1], ("Item 2".to_string(), true));
        assert_eq!(parsed.checklist[2], ("Item 3".to_string(), false));
    }

    #[test]
    fn test_parse_empty_fields() {
        let content = r#"Title: Task title

Notes:

Project:

Area:

Tags:

When: inbox

Deadline:

Defer Until:

Checklist:
"#;

        let parsed = parse_edited_task(content).unwrap();
        assert_eq!(parsed.project, None);
        assert_eq!(parsed.area, None);
        assert_eq!(parsed.tags, Vec::<String>::new());
        assert_eq!(parsed.deadline, None);
        assert_eq!(parsed.defer_until, None);
    }

    #[test]
    fn test_has_changes_detects_change() {
        let store = create_test_store();
        let task = Task {
            id: Uuid::new_v4(),
            task_number: 1,
            title: "Original title".to_string(),
            notes: None,
            project_id: None,
            area_id: None,
            tags: vec![],
            when: When::Inbox,
            deadline: None,
            defer_until: None,
            checklist: vec![],
            completed_at: None,
            deleted_at: None,
            created_at: jiff::Timestamp::now(),
            modified_at: saku_storage::timestamp::HybridTimestamp::default(),
        };

        let parsed = ParsedTaskEdit {
            title: "Updated title".to_string(),
            notes: None,
            project: None,
            area: None,
            tags: vec![],
            when: "inbox".to_string(),
            deadline: None,
            defer_until: None,
            checklist: vec![],
        };

        assert!(has_changes(&task, &parsed, &store));
    }

    #[test]
    fn test_has_changes_no_change() {
        let store = create_test_store();
        let task = Task {
            id: Uuid::new_v4(),
            task_number: 1,
            title: "Same title".to_string(),
            notes: None,
            project_id: None,
            area_id: None,
            tags: vec![],
            when: When::Inbox,
            deadline: None,
            defer_until: None,
            checklist: vec![],
            completed_at: None,
            deleted_at: None,
            created_at: jiff::Timestamp::now(),
            modified_at: saku_storage::timestamp::HybridTimestamp::default(),
        };

        let parsed = ParsedTaskEdit {
            title: "Same title".to_string(),
            notes: None,
            project: None,
            area: None,
            tags: vec![],
            when: "inbox".to_string(),
            deadline: None,
            defer_until: None,
            checklist: vec![],
        };

        assert!(!has_changes(&task, &parsed, &store));
    }
}

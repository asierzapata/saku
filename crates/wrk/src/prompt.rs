use saku_storage::entity::Entity;
use saku_tdo::models::store::Store;
use saku_tdo::models::task::Task;

/// Assembles a prompt for `claude --print` from task data.
///
/// Sections:
/// 1. Context header (project, area, date)
/// 2. Task title
/// 3. Task notes (the primary instruction)
/// 4. Subtask checklist
/// 5. Blocker status
/// 6. Project CLAUDE.md (if provided)
pub fn build_prompt(task: &Task, store: &Store, project_claude_md: Option<&str>) -> String {
    let mut sections: Vec<String> = Vec::new();

    // 1. Context header
    let today = jiff::Zoned::now().date();
    let mut context_lines = vec![format!("Date: {}", today)];

    if let Some(ref project_key) = task.project_key {
        if let Some(project) = store.get_project(project_key) {
            context_lines.push(format!("Project: {}", project.name));
            if let Some(ref area_key) = project.area_key {
                if let Some(area) = store.get_area(area_key) {
                    context_lines.push(format!("Area: {}", area.name));
                }
            }
        }
    } else if let Some(ref area_key) = task.area_key {
        if let Some(area) = store.get_area(area_key) {
            context_lines.push(format!("Area: {}", area.name));
        }
    }

    if let Some(deadline) = task.deadline {
        context_lines.push(format!("Deadline: {}", deadline));
    }

    sections.push(context_lines.join("\n"));

    // 2. Task title
    sections.push(format!("# Task: {}", task.title));

    // 3. Task notes (primary instruction)
    if let Some(ref notes) = task.notes {
        sections.push(notes.clone());
    }

    // 4. Subtasks
    let subtasks: Vec<_> = store.get_subtasks(&task.storage_key()).collect();
    if !subtasks.is_empty() {
        let mut checklist = String::from("## Subtasks\n");
        for sub in &subtasks {
            let marker = if sub.completed_at.is_some() {
                "[x]"
            } else {
                "[ ]"
            };
            checklist.push_str(&format!("- {} {}\n", marker, sub.title));
            if let Some(ref notes) = sub.notes {
                for line in notes.lines() {
                    checklist.push_str(&format!("  {}\n", line));
                }
            }
        }
        sections.push(checklist.trim_end().to_string());
    }

    // 5. Blocker status
    let blockers = store.get_blockers(task);
    if !blockers.is_empty() {
        let mut status = String::from("## Blocked by\n");
        for blocker in &blockers {
            status.push_str(&format!("- #{} {}\n", blocker.task_number, blocker.title));
        }
        sections.push(status.trim_end().to_string());
    }

    // 6. Project CLAUDE.md
    if let Some(claude_md) = project_claude_md {
        sections.push(format!(
            "## Project instructions (CLAUDE.md)\n\n{}",
            claude_md
        ));
    }

    sections.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use saku_tdo::models::task::When;

    fn make_store() -> Store {
        Store::default()
    }

    fn make_task(title: &str) -> Task {
        Task {
            storage_key_suffix: "test123".to_string(),
            title: title.to_string(),
            ..Task::default()
        }
    }

    #[test]
    fn basic_prompt_includes_title() {
        let store = make_store();
        let task = make_task("Fix the bug");
        let prompt = build_prompt(&task, &store, None);
        assert!(prompt.contains("# Task: Fix the bug"));
    }

    #[test]
    fn prompt_includes_notes() {
        let store = make_store();
        let mut task = make_task("Fix the bug");
        task.notes = Some("Look at main.rs line 42".to_string());
        let prompt = build_prompt(&task, &store, None);
        assert!(prompt.contains("Look at main.rs line 42"));
    }

    #[test]
    fn prompt_includes_claude_md() {
        let store = make_store();
        let task = make_task("Fix the bug");
        let prompt = build_prompt(&task, &store, Some("Always run tests"));
        assert!(prompt.contains("## Project instructions (CLAUDE.md)"));
        assert!(prompt.contains("Always run tests"));
    }

    #[test]
    fn prompt_includes_subtasks() {
        let mut store = make_store();
        let parent = Task {
            storage_key_suffix: "parent1".to_string(),
            title: "Main task".to_string(),
            ..Task::default()
        };
        store.add_task(parent);
        let parent_key = "task/parent1".to_string();

        let sub = Task {
            storage_key_suffix: "sub1".to_string(),
            title: "Sub task A".to_string(),
            parent_task_key: Some(parent_key.clone()),
            ..Task::default()
        };
        store.add_task(sub);

        let parent = store.get_task(&parent_key).unwrap();
        let prompt = build_prompt(parent, &store, None);
        assert!(prompt.contains("## Subtasks"));
        assert!(prompt.contains("[ ] Sub task A"));
    }
}

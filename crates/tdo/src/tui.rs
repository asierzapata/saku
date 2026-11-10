use std::io::{self, stdout};
use std::time::{Duration, Instant};

use crossterm::{
    cursor::{Hide, Show},
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers,
    },
    execute,
    terminal::{EnterAlternateScreen, enable_raw_mode},
};
use jiff::civil::Date;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, ListState, Paragraph, Wrap},
};
use saku_storage::entity::Entity;
use saku_tdo::{
    date_parser::parse_natural_date,
    models::{
        store::Store,
        task::{Task, When, is_pending_on, order_tasks_with_store},
    },
    services::{
        projects::{CreateProjectParameters, create_project},
        tasks::{
            AddTaskParameters, CompleteTaskParameters, DeleteTaskParameters, EditTaskError,
            EditTaskParameters, MoveTaskParameters, add_task, complete_task, delete_task,
            edit_task, move_task,
        },
    },
    storage::{
        Storage,
        sqlite::{CommandMode, SqliteStorage},
    },
    ui::{
        TaskUrgency, calculate_task_urgency, format_date_header, format_estimate,
        get_task_context, get_task_date_badge,
    },
};
use unicode_width::UnicodeWidthStr;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Clone, Debug, PartialEq, Eq)]
enum View {
    Today,
    Inbox,
    Upcoming,
    Someday,
    Projects,
    Project(String),
    All,
}

enum InputKind {
    Add,
    AddSubtask {
        parent_key: String,
        parent_number: u64,
    },
    AddProject,
    Schedule(String),
    Project(String),
    Search,
}

struct Input {
    kind: InputKind,
    text: String,
    cursor: usize,
}

impl Input {
    fn new(kind: InputKind, text: String) -> Self {
        let cursor = text.len();
        Self { kind, text, cursor }
    }

    fn set_text(&mut self, text: String) {
        self.cursor = text.len();
        self.text = text;
    }

    fn insert_char(&mut self, c: char) {
        let idx = self.cursor.min(self.text.len());
        self.text.insert(idx, c);
        self.cursor = idx + c.len_utf8();
    }

    fn insert_str(&mut self, s: &str) {
        for c in s.chars() {
            if c.is_whitespace() {
                self.insert_char(' ');
            } else if !c.is_control() {
                self.insert_char(c);
            }
        }
    }

    fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let prev = self.text[..self.cursor]
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0);
        self.text.drain(prev..self.cursor);
        self.cursor = prev;
    }

    fn delete_forward(&mut self) {
        if self.cursor >= self.text.len() {
            return;
        }
        let next = self.text[self.cursor..]
            .chars()
            .next()
            .map(|c| self.cursor + c.len_utf8())
            .unwrap_or(self.text.len());
        self.text.drain(self.cursor..next);
    }

    fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor = self.text[..self.cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    fn move_right(&mut self) {
        if self.cursor < self.text.len() {
            self.cursor = self.text[self.cursor..]
                .chars()
                .next()
                .map(|c| self.cursor + c.len_utf8())
                .unwrap_or(self.text.len());
        }
    }

    fn move_home(&mut self) {
        self.cursor = 0;
    }

    fn move_end(&mut self) {
        self.cursor = self.text.len();
    }

    fn delete_word_back(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let before = &self.text[..self.cursor];
        let trimmed = before.trim_end();
        let word_start = trimmed
            .rfind(char::is_whitespace)
            .map(|i| i + 1)
            .unwrap_or(0);
        self.text.drain(word_start..self.cursor);
        self.cursor = word_start;
    }

    fn clear_before_cursor(&mut self) {
        if self.cursor > 0 {
            self.text.drain(..self.cursor);
            self.cursor = 0;
        } else {
            self.text.clear();
        }
    }
}

#[derive(Clone, Debug)]
enum UndoAction {
    Uncomplete {
        key: String,
        occurrence: Option<Date>,
    },
    RestoreDeleted {
        keys: Vec<String>,
    },
}

enum Mutation {
    Add(String),
    AddSubtask {
        parent_key: String,
        title: String,
    },
    AddProject(String),
    Complete(String),
    Schedule {
        key: String,
        when: When,
    },
    AssignProject {
        key: String,
        project: Option<String>,
    },
    Delete(String),
    Undo(UndoAction),
}

#[cfg(test)]
fn mutate(db: &mut SqliteStorage, mutation: Mutation) -> Result<String> {
    mutate_with_undo(db, mutation).map(|(status, _)| status)
}

fn mutate_with_undo(
    db: &mut SqliteStorage,
    mutation: Mutation,
) -> Result<(String, Option<UndoAction>)> {
    db.transaction::<_, Box<dyn std::error::Error>>(|store, storage| match mutation {
        Mutation::Add(title) => {
            let title = title.trim();
            if title.is_empty() {
                return Err("Title cannot be empty".into());
            }
            let task = add_task(
                store,
                storage,
                AddTaskParameters {
                    title: title.to_string(),
                    notes: None,
                    when: When::Inbox,
                    deadline: None,
                    defer_until: None,
                    project: None,
                    area: None,
                    tags: vec![],
                    recurrence: None,
                    parent_task_number: None,
                    estimate_minutes: None,
                },
            )?;
            Ok((format!("Added #{} to Inbox", task.task_number), None))
        }
        Mutation::AddSubtask { parent_key, title } => {
            let title = title.trim();
            if title.is_empty() {
                return Err("Title cannot be empty".into());
            }
            let parent_number = available_task(store, &parent_key)?.task_number;
            let task = add_task(
                store,
                storage,
                AddTaskParameters {
                    title: title.to_string(),
                    notes: None,
                    when: When::Inbox,
                    deadline: None,
                    defer_until: None,
                    project: None,
                    area: None,
                    tags: vec![],
                    recurrence: None,
                    parent_task_number: Some(parent_number),
                    estimate_minutes: None,
                },
            )?;
            Ok((
                format!(
                    "Added subtask #{} under #{parent_number}",
                    task.task_number
                ),
                None,
            ))
        }
        Mutation::AddProject(name) => {
            let name = name.trim();
            if name.is_empty() {
                return Err("Project name cannot be empty".into());
            }
            let project = create_project(
                store,
                storage,
                CreateProjectParameters {
                    name: name.to_string(),
                    area: None,
                },
            )?;
            Ok((format!("Created project '{}'", project.name), None))
        }
        Mutation::Complete(key) => {
            if let Some(done_task) = store
                .get_task(&key)
                .filter(|t| t.deleted_at.is_none() && t.completed_at.is_some())
            {
                let number = done_task.task_number;
                let stamp = storage.next_modified_at()?;
                let task = store.get_task_mut(&key).unwrap();
                task.completed_at = None;
                task.modified_at = stamp;
                storage.save(store)?;
                return Ok((format!("Reopened #{number}"), None));
            }
            let current = available_task(store, &key)?;
            let today = jiff::Zoned::now().date();
            let is_recurring = current.recurrence.is_some();
            if is_recurring && current.completed_occurrences.contains(&today) {
                return Err("Today's occurrence is already complete".into());
            }
            let number = current.task_number;
            complete_task(
                store,
                storage,
                CompleteTaskParameters {
                    task_number_or_fuzzy_name: number.to_string(),
                    stop: false,
                    note: None,
                },
            )?;
            let undo = UndoAction::Uncomplete {
                key,
                occurrence: is_recurring.then_some(today),
            };
            Ok((format!("Completed #{number} (u to undo)"), Some(undo)))
        }
        Mutation::Schedule { key, when } => {
            let current = available_task(store, &key)?.clone();
            let parameters = MoveTaskParameters {
                task_number: current.task_number,
                notes: current.notes.clone(),
                tags: current.tags.clone(),
                when: Some(when),
                deadline: None,
                clear_schedule: false,
                clear_deadline: false,
                project: None,
                area: None,
                clear_project: false,
                clear_area: false,
                defer_until: None,
                clear_defer: false,
                recurrence: None,
                clear_recurrence: false,
                estimate_minutes: None,
                clear_estimate: false,
            };
            move_task(store, storage, parameters)?;
            Ok((format!("Scheduled #{}", current.task_number), None))
        }
        Mutation::AssignProject { key, project } => {
            let current = available_task(store, &key)?.clone();
            if current.parent_task_key.is_some() {
                return Err("Subtasks inherit project from their parent".into());
            }
            let clear_project = project.is_none();
            let parameters = MoveTaskParameters {
                task_number: current.task_number,
                notes: current.notes.clone(),
                tags: current.tags.clone(),
                when: None,
                deadline: None,
                clear_schedule: false,
                clear_deadline: false,
                project,
                area: None,
                clear_project,
                clear_area: false,
                defer_until: None,
                clear_defer: false,
                recurrence: None,
                clear_recurrence: false,
                estimate_minutes: None,
                clear_estimate: false,
            };
            let updated = move_task(store, storage, parameters)?;
            let label = if let Some(pkey) = &updated.project_key
                && let Some(p) = store.get_project(pkey)
            {
                format!("Moved #{} to {}", updated.task_number, p.name)
            } else {
                format!("Cleared project on #{}", updated.task_number)
            };
            Ok((label, None))
        }
        Mutation::Delete(key) => {
            let current = available_task(store, &key)?;
            let number = current.task_number;
            let mut deleted_keys = vec![key.clone()];
            deleted_keys.extend(store.get_subtasks(&key).map(Entity::storage_key));
            delete_task(
                store,
                storage,
                DeleteTaskParameters {
                    task_number_or_fuzzy_name: number.to_string(),
                },
            )?;
            let undo = UndoAction::RestoreDeleted { keys: deleted_keys };
            Ok((format!("Deleted #{number} (u to undo)"), Some(undo)))
        }
        Mutation::Undo(action) => match action {
            UndoAction::Uncomplete { key, occurrence } => {
                let stamp = storage.next_modified_at()?;
                let task = store
                    .get_task_mut(&key)
                    .filter(|t| t.deleted_at.is_none())
                    .ok_or_else(|| -> Box<dyn std::error::Error> {
                        "Task is no longer available".into()
                    })?;
                let number = task.task_number;
                if let Some(date) = occurrence {
                    task.completed_occurrences.retain(|d| *d != date);
                } else {
                    task.completed_at = None;
                }
                task.modified_at = stamp;
                storage.save(store)?;
                Ok((format!("Restored #{number}"), None))
            }
            UndoAction::RestoreDeleted { keys } => {
                let mut restored_number = None;
                for key in keys {
                    let stamp = storage.next_modified_at()?;
                    if let Some(task) = store.get_task_mut(&key) {
                        if restored_number.is_none() {
                            restored_number = Some(task.task_number);
                        }
                        task.deleted_at = None;
                        task.modified_at = stamp;
                    }
                }
                let Some(number) = restored_number else {
                    return Err("Task is no longer available".into());
                };
                storage.save(store)?;
                Ok((format!("Restored #{number}"), None))
            }
        },
    })
}

fn available_task<'a>(store: &'a Store, key: &str) -> Result<&'a Task> {
    store
        .get_task(key)
        .filter(|task| task.deleted_at.is_none() && task.completed_at.is_none())
        .ok_or_else(|| "Task is no longer available".into())
}

fn edit_selected(db: &mut SqliteStorage, key: &str) -> Result<String> {
    let command = db.command(CommandMode::Edit)?;
    let mut store = command.load()?;
    let number = available_task(&store, key)?.task_number;
    match edit_task(
        &mut store,
        &command,
        EditTaskParameters {
            task_number_or_fuzzy_name: number.to_string(),
        },
    ) {
        Ok(_) => {
            command.commit()?;
            Ok(format!("Edited #{number}"))
        }
        Err(EditTaskError::NoChanges) => Ok("No changes".into()),
        Err(error) => Err(error.into()),
    }
}

fn parse_schedule(text: &str) -> Result<When> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Enter a date, inbox, or someday".into());
    }
    match text.to_lowercase().as_str() {
        "inbox" => Ok(When::Inbox),
        "someday" => Ok(When::Someday),
        _ => Ok(When::Scheduled {
            date: parse_natural_date(text)?,
        }),
    }
}

fn is_task_overdue_on(task: &Task, today: Date) -> bool {
    if task.completed_at.is_some() || task.deleted_at.is_some() || task.recurrence.is_some() {
        return false;
    }
    let scheduled_overdue = matches!(task.when, When::Scheduled { date } if date < today);
    let deadline_overdue = task.deadline.is_some_and(|date| date < today);
    scheduled_overdue || deadline_overdue
}

fn upcoming_date(task: &Task, today: Date) -> Option<Date> {
    let scheduled = match task.when {
        When::Scheduled { date } if date > today => Some(date),
        _ => None,
    };
    let deadline = task.deadline.filter(|&date| date > today);
    match (scheduled, deadline) {
        (Some(d1), Some(d2)) => Some(d1.min(d2)),
        (Some(d), None) | (None, Some(d)) => Some(d),
        (None, None) => None,
    }
}

/// Orders tasks using `order_tasks_with_store`, then groups any subtasks present
/// in the slice immediately after their parent task so tree indentation (`└─`) works.
fn order_with_subtasks<'a>(tasks: Vec<&'a Task>, store: &Store) -> Vec<&'a Task> {
    let ordered = order_tasks_with_store(tasks, store);
    let keys_in_bucket: std::collections::HashSet<String> =
        ordered.iter().map(|t| t.storage_key()).collect();
    let mut result = Vec::with_capacity(ordered.len());
    let mut emitted = std::collections::HashSet::new();

    for task in &ordered {
        let key = task.storage_key();
        if emitted.contains(&key) {
            continue;
        }
        // Defer subtasks whose parent is also in this bucket until the parent is emitted
        if let Some(parent_key) = &task.parent_task_key
            && keys_in_bucket.contains(parent_key)
        {
            continue;
        }
        emitted.insert(key.clone());
        result.push(*task);
        for candidate in &ordered {
            let c_key = candidate.storage_key();
            if !emitted.contains(&c_key)
                && candidate.parent_task_key.as_deref() == Some(key.as_str())
            {
                emitted.insert(c_key);
                result.push(*candidate);
            }
        }
    }
    // Emit any remaining tasks (e.g. multi-level subtasks)
    for task in ordered {
        if emitted.insert(task.storage_key()) {
            result.push(task);
        }
    }
    result
}

fn task_section(task: &Task, view: &View, today: Date) -> Option<String> {
    match view {
        View::Today => {
            if is_task_overdue_on(task, today) {
                Some("Overdue".into())
            } else {
                Some("Today".into())
            }
        }
        View::Upcoming => upcoming_date(task, today).map(format_date_header),
        View::All => Some(
            match task.when {
                When::Inbox => "Inbox",
                When::Scheduled { .. } | When::LegacyToday { .. } => "Scheduled",
                When::Someday | When::LegacyAnytime => "Someday",
            }
            .into(),
        ),
        View::Inbox | View::Someday | View::Project(_) | View::Projects => None,
    }
}

fn project_section(project_key: &str, store: &Store) -> Option<String> {
    let project = store.get_project(project_key)?;
    if let Some(area_key) = &project.area_key
        && let Some(area) = store.get_area(area_key)
    {
        return Some(clean(&area.name, false));
    }
    if store.get_active_areas().next().is_some() {
        Some("Other Projects".into())
    } else {
        None
    }
}

#[cfg(test)]
fn task_keys(store: &Store, view: &View, query: &str, today: Date) -> Vec<String> {
    task_keys_filtered(store, view, query, today, false, false)
}

fn task_keys_filtered(
    store: &Store,
    view: &View,
    query: &str,
    today: Date,
    show_completed: bool,
    ready_only: bool,
) -> Vec<String> {
    let query = query.to_lowercase();
    let matching: Vec<&Task> = store
        .get_active_tasks()
        .filter(|task| {
            if !show_completed && task.completed_at.is_some() {
                return false;
            }
            if ready_only && store.is_task_blocked(task) {
                return false;
            }
            let visible = match view {
                View::Today => {
                    task.defer_until.is_none_or(|date| date <= today)
                        && if task.recurrence.is_some() {
                            is_pending_on(task, today)
                                || (show_completed && task.completed_occurrences.contains(&today))
                        } else {
                            matches!(task.when, When::Scheduled { date } if date <= today)
                                || task.deadline.is_some_and(|date| date <= today)
                        }
                }
                View::Inbox => {
                    matches!(task.when, When::Inbox)
                        && task.defer_until.is_none_or(|date| date <= today)
                }
                View::Upcoming => upcoming_date(task, today).is_some(),
                View::Someday => matches!(task.when, When::Someday | When::LegacyAnytime),
                View::Project(key) => task.project_key.as_deref() == Some(key.as_str()),
                View::All => true,
                View::Projects => false,
            };
            visible
                && (query.is_empty()
                    || task.title.to_lowercase().contains(&query)
                    || task
                        .notes
                        .as_ref()
                        .is_some_and(|notes| notes.to_lowercase().contains(&query))
                    || task
                        .project_key
                        .as_deref()
                        .and_then(|key| store.get_project(key))
                        .is_some_and(|project| project.name.to_lowercase().contains(&query))
                    || task
                        .tags
                        .iter()
                        .any(|tag| tag.to_lowercase().contains(&query)))
        })
        .collect();

    match view {
        View::Today => {
            let (overdue, due_today): (Vec<_>, Vec<_>) = matching
                .into_iter()
                .partition(|task| is_task_overdue_on(task, today));
            let mut result = order_with_subtasks(overdue, store);
            result.extend(order_with_subtasks(due_today, store));
            result.into_iter().map(Entity::storage_key).collect()
        }
        View::Upcoming => {
            let mut by_date: std::collections::BTreeMap<Date, Vec<&Task>> =
                std::collections::BTreeMap::new();
            for task in matching {
                if let Some(date) = upcoming_date(task, today) {
                    by_date.entry(date).or_default().push(task);
                }
            }
            let mut result = Vec::new();
            for (_date, bucket) in by_date {
                result.extend(
                    order_with_subtasks(bucket, store)
                        .into_iter()
                        .map(Entity::storage_key),
                );
            }
            result
        }
        View::All => {
            let mut inbox = Vec::new();
            let mut scheduled = Vec::new();
            let mut someday = Vec::new();
            for task in matching {
                match task.when {
                    When::Inbox => inbox.push(task),
                    When::Scheduled { .. } | When::LegacyToday { .. } => scheduled.push(task),
                    When::Someday | When::LegacyAnytime => someday.push(task),
                }
            }
            let mut result = order_with_subtasks(inbox, store);
            result.extend(order_with_subtasks(scheduled, store));
            result.extend(order_with_subtasks(someday, store));
            result.into_iter().map(Entity::storage_key).collect()
        }
        _ => order_with_subtasks(matching, store)
            .into_iter()
            .map(Entity::storage_key)
            .collect(),
    }
}

struct App {
    store: Store,
    view: View,
    list: ListState,
    rows: Vec<String>,
    input: Option<Input>,
    query: String,
    status: String,
    status_error: bool,
    help: bool,
    show_inspector: bool,
    show_completed: bool,
    ready_only: bool,
    last_undo: Option<UndoAction>,
    detail: bool,
    detail_scroll: u16,
    stream_scroll: usize,
    page_size: usize,
}

impl App {
    fn new(store: Store) -> Self {
        let mut app = Self {
            store,
            view: View::Today,
            list: ListState::default(),
            rows: vec![],
            input: None,
            query: String::new(),
            status: String::new(),
            status_error: false,
            help: false,
            show_inspector: true,
            show_completed: false,
            ready_only: false,
            last_undo: None,
            detail: false,
            detail_scroll: 0,
            stream_scroll: 0,
            page_size: 1,
        };
        app.rebuild();
        app
    }

    fn set_ok(&mut self, msg: impl Into<String>) {
        self.status = msg.into();
        self.status_error = false;
    }

    fn set_err(&mut self, msg: impl Into<String>) {
        self.status = msg.into();
        self.status_error = true;
    }

    fn selected(&self) -> Option<&str> {
        self.list
            .selected()
            .and_then(|i| self.rows.get(i))
            .map(String::as_str)
    }

    fn rebuild(&mut self) {
        let previous = self.selected().map(str::to_owned);
        let previous_index = self.list.selected().unwrap_or(0);
        if let View::Project(key) = &self.view
            && !self.store.get_project(key).is_some_and(|project| {
                project.deleted_at.is_none() && project.completed_at.is_none()
            })
        {
            self.view = View::Projects;
            self.set_err("Project is no longer available");
            self.detail = false;
        }
        self.rows = if self.view == View::Projects {
            let mut projects: Vec<_> = self
                .store
                .get_active_projects()
                .filter(|project| project.completed_at.is_none())
                .collect();
            projects.sort_by(|a, b| {
                let area_a = a
                    .area_key
                    .as_deref()
                    .and_then(|k| self.store.get_area(k))
                    .map(|ar| ar.name.to_lowercase());
                let area_b = b
                    .area_key
                    .as_deref()
                    .and_then(|k| self.store.get_area(k))
                    .map(|ar| ar.name.to_lowercase());
                match (&area_a, &area_b) {
                    (Some(aa), Some(ab)) => aa.cmp(ab),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                }
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                .then_with(|| a.storage_key().cmp(&b.storage_key()))
            });
            projects.into_iter().map(Entity::storage_key).collect()
        } else {
            task_keys_filtered(
                &self.store,
                &self.view,
                &self.query,
                jiff::Zoned::now().date(),
                self.show_completed,
                self.ready_only,
            )
        };
        let index = previous
            .and_then(|key| self.rows.iter().position(|row| *row == key))
            .or_else(|| (!self.rows.is_empty()).then(|| previous_index.min(self.rows.len() - 1)));
        self.list.select(index);
        if self.rows.is_empty() {
            self.detail = false;
            self.stream_scroll = 0;
        }
    }

    fn reload(&mut self, db: &SqliteStorage) -> Result<()> {
        self.store = db.load()?;
        self.rebuild();
        Ok(())
    }

    fn switch(&mut self, view: View) {
        self.view = view;
        self.query.clear();
        self.list.select(None);
        self.rows.clear();
        self.detail = false;
        self.detail_scroll = 0;
        self.stream_scroll = 0;
        self.rebuild();
    }

    fn select_delta(&mut self, delta: isize) {
        if self.rows.is_empty() {
            self.list.select(None);
            return;
        }
        let i = self.list.selected().unwrap_or(0);
        let next = i.saturating_add_signed(delta).min(self.rows.len() - 1);
        if next != i {
            self.detail_scroll = 0;
        }
        self.list.select(Some(next));
    }

    fn row_section(&self, idx: usize, today: Date) -> Option<String> {
        let key = self.rows.get(idx)?;
        if self.view == View::Projects {
            project_section(key, &self.store)
        } else {
            self.store
                .get_task(key)
                .and_then(|task| task_section(task, &self.view, today))
        }
    }

    fn jump_section(&mut self, forward: bool) {
        if self.rows.is_empty() {
            return;
        }
        let today = jiff::Zoned::now().date();
        let current_idx = self.list.selected().unwrap_or(0);
        let current_sec = self.row_section(current_idx, today);

        if forward {
            for idx in (current_idx + 1)..self.rows.len() {
                if self.row_section(idx, today) != current_sec {
                    self.list.select(Some(idx));
                    self.detail_scroll = 0;
                    return;
                }
            }
        } else {
            // Find the start of the current section first; if already at the start, find the start of the previous section
            let mut section_start = current_idx;
            while section_start > 0 && self.row_section(section_start - 1, today) == current_sec {
                section_start -= 1;
            }
            if section_start < current_idx {
                self.list.select(Some(section_start));
                self.detail_scroll = 0;
                return;
            }
            if section_start > 0 {
                let prev_sec = self.row_section(section_start - 1, today);
                let mut target = section_start - 1;
                while target > 0 && self.row_section(target - 1, today) == prev_sec {
                    target -= 1;
                }
                self.list.select(Some(target));
                self.detail_scroll = 0;
            }
        }
    }

    fn task_key(&self) -> Option<String> {
        self.selected()
            .filter(|key| self.store.get_task(key).is_some())
            .map(str::to_owned)
    }

    fn apply_mutation(&mut self, db: &mut SqliteStorage, mutation: Mutation) -> Result<()> {
        match mutate_with_undo(db, mutation) {
            Ok((status, undo)) => {
                if undo.is_some() {
                    self.last_undo = undo;
                }
                self.set_ok(status);
                self.reload(db)?;
            }
            Err(error) => {
                self.set_err(error.to_string());
            }
        }
        Ok(())
    }
}

fn tab_complete_input(input: &mut Input, store: &Store) {
    let query = input.text.trim().to_lowercase();
    match &input.kind {
        InputKind::Project(_) => {
            let mut projects: Vec<&str> = store
                .get_active_projects()
                .filter(|p| p.completed_at.is_none())
                .map(|p| p.name.as_str())
                .collect();
            projects.sort_by_key(|name| name.to_lowercase());
            if let Some(matched) = projects
                .iter()
                .find(|name| name.to_lowercase().starts_with(&query))
                .or_else(|| {
                    projects
                        .iter()
                        .find(|name| name.to_lowercase().contains(&query))
                })
            {
                input.set_text((*matched).to_string());
            }
        }
        InputKind::Schedule(_) => {
            const CANDIDATES: &[&str] = &[
                "today",
                "tomorrow",
                "someday",
                "inbox",
                "next week",
                "monday",
                "tuesday",
                "wednesday",
                "thursday",
                "friday",
                "saturday",
                "sunday",
            ];
            if !query.is_empty()
                && let Some(matched) = CANDIDATES.iter().find(|c| c.starts_with(&query))
            {
                input.set_text((*matched).to_string());
            }
        }
        _ => {}
    }
}

struct TerminalGuard;

impl TerminalGuard {
    fn suspend(&self) -> io::Result<()> {
        execute!(stdout(), DisableBracketedPaste, Show)?;
        ratatui::try_restore()
    }

    fn resume(&self, terminal: &mut ratatui::DefaultTerminal) -> io::Result<()> {
        enable_raw_mode()?;
        execute!(stdout(), EnterAlternateScreen, Hide, EnableBracketedPaste)?;
        terminal.clear()
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(stdout(), DisableBracketedPaste, Show);
        ratatui::restore();
    }
}

pub fn run(db: &mut SqliteStorage) -> Result<()> {
    let mut app = App::new(db.load()?);
    let guard = TerminalGuard;
    let mut terminal = ratatui::try_init()?;
    execute!(stdout(), EnableBracketedPaste)?;
    let mut refresh = Instant::now();
    loop {
        terminal.draw(|frame| render(frame, &mut app))?;
        if refresh.elapsed() >= Duration::from_millis(750) {
            app.reload(db)?;
            refresh = Instant::now();
        }
        if !event::poll(Duration::from_millis(150))? {
            continue;
        }
        match event::read()? {
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                    break;
                }
                if handle_key(&mut app, db, &guard, &mut terminal, key)? {
                    break;
                }
            }
            Event::Paste(text) => {
                if let Some(input) = &mut app.input {
                    input.insert_str(&text);
                    if matches!(input.kind, InputKind::Search) {
                        app.query = input.text.trim().to_string();
                        app.rebuild();
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn handle_key(
    app: &mut App,
    db: &mut SqliteStorage,
    guard: &TerminalGuard,
    terminal: &mut ratatui::DefaultTerminal,
    key: KeyEvent,
) -> Result<bool> {
    if let Some(input) = &mut app.input {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        match key.code {
            KeyCode::Esc => {
                app.input = None;
            }
            KeyCode::Left => input.move_left(),
            KeyCode::Right => input.move_right(),
            KeyCode::Home => input.move_home(),
            KeyCode::End => input.move_end(),
            KeyCode::Char('b') if ctrl => input.move_left(),
            KeyCode::Char('f') if ctrl => input.move_right(),
            KeyCode::Char('a') if ctrl => input.move_home(),
            KeyCode::Char('e') if ctrl => input.move_end(),
            KeyCode::Char('w') if ctrl => {
                input.delete_word_back();
                if matches!(input.kind, InputKind::Search) {
                    app.query = input.text.trim().to_string();
                    app.rebuild();
                }
            }
            KeyCode::Char('u') if ctrl => {
                input.clear_before_cursor();
                if matches!(input.kind, InputKind::Search) {
                    app.query = input.text.trim().to_string();
                    app.rebuild();
                }
            }
            KeyCode::Backspace if alt => {
                input.delete_word_back();
                if matches!(input.kind, InputKind::Search) {
                    app.query = input.text.trim().to_string();
                    app.rebuild();
                }
            }
            KeyCode::Backspace => {
                input.backspace();
                if matches!(input.kind, InputKind::Search) {
                    app.query = input.text.trim().to_string();
                    app.rebuild();
                }
            }
            KeyCode::Delete => {
                input.delete_forward();
                if matches!(input.kind, InputKind::Search) {
                    app.query = input.text.trim().to_string();
                    app.rebuild();
                }
            }
            KeyCode::Tab => {
                tab_complete_input(input, &app.store);
            }
            KeyCode::Char(c) if !ctrl && !alt && !c.is_control() => {
                input.insert_char(c);
                if matches!(input.kind, InputKind::Search) {
                    app.query = input.text.trim().to_string();
                    app.rebuild();
                }
            }
            KeyCode::Enter if key.kind == KeyEventKind::Press => {
                let input = app.input.take().unwrap();
                let result = match &input.kind {
                    InputKind::Add => mutate_with_undo(db, Mutation::Add(input.text.clone())),
                    InputKind::AddSubtask { parent_key, .. } => mutate_with_undo(
                        db,
                        Mutation::AddSubtask {
                            parent_key: parent_key.clone(),
                            title: input.text.clone(),
                        },
                    ),
                    InputKind::AddProject => {
                        mutate_with_undo(db, Mutation::AddProject(input.text.clone()))
                    }
                    InputKind::Schedule(key) => parse_schedule(&input.text).and_then(|when| {
                        mutate_with_undo(
                            db,
                            Mutation::Schedule {
                                key: key.clone(),
                                when,
                            },
                        )
                    }),
                    InputKind::Project(key) => {
                        let trimmed = input.text.trim();
                        let project = if trimmed.is_empty()
                            || trimmed.eq_ignore_ascii_case("none")
                            || trimmed.eq_ignore_ascii_case("clear")
                        {
                            None
                        } else {
                            Some(trimmed.to_string())
                        };
                        mutate_with_undo(
                            db,
                            Mutation::AssignProject {
                                key: key.clone(),
                                project,
                            },
                        )
                    }
                    InputKind::Search => {
                        if app.view == View::Projects {
                            app.view = View::All;
                        }
                        app.query = input.text.trim().to_string();
                        app.detail = false;
                        app.detail_scroll = 0;
                        app.rebuild();
                        Ok(("Search applied".into(), None))
                    }
                };
                match result {
                    Ok((status, undo)) => {
                        if undo.is_some() {
                            app.last_undo = undo;
                        }
                        app.set_ok(status);
                        app.reload(db)?;
                    }
                    Err(error) => {
                        app.set_err(error.to_string());
                        app.input = Some(input);
                    }
                }
            }
            _ => {}
        }
        return Ok(false);
    }
    if app.help {
        if matches!(key.code, KeyCode::Esc | KeyCode::Char('?')) {
            app.help = false;
        }
        return Ok(false);
    }
    if app.detail {
        match key.code {
            KeyCode::Esc | KeyCode::Enter => {
                app.detail = false;
                return Ok(false);
            }
            KeyCode::Char('d') if key.kind == KeyEventKind::Press => {
                app.detail = false;
                app.show_inspector = false;
                return Ok(false);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                app.detail_scroll = app.detail_scroll.saturating_sub(1);
                return Ok(false);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.detail_scroll = app.detail_scroll.saturating_add(1);
                return Ok(false);
            }
            KeyCode::PageUp => {
                app.detail_scroll = app.detail_scroll.saturating_sub(app.page_size as u16);
                return Ok(false);
            }
            KeyCode::PageDown => {
                app.detail_scroll = app.detail_scroll.saturating_add(app.page_size as u16);
                return Ok(false);
            }
            _ => {}
        }
    }
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char('q') => return Ok(true),
        KeyCode::Char('?') => app.help = true,
        KeyCode::Char('d') if ctrl => {
            let half = (app.page_size / 2).max(1) as isize;
            app.select_delta(half);
        }
        KeyCode::Char('u') if ctrl => {
            let half = (app.page_size / 2).max(1) as isize;
            app.select_delta(-half);
        }
        KeyCode::Char('d') if key.kind == KeyEventKind::Press => {
            app.show_inspector = !app.show_inspector;
            if !app.show_inspector {
                app.detail = false;
            }
        }
        KeyCode::Char('c') if key.kind == KeyEventKind::Press => {
            app.show_completed = !app.show_completed;
            app.rebuild();
            app.set_ok(if app.show_completed {
                "Showing completed tasks"
            } else {
                "Hiding completed tasks"
            });
        }
        KeyCode::Char('b') if key.kind == KeyEventKind::Press => {
            app.ready_only = !app.ready_only;
            app.rebuild();
            app.set_ok(if app.ready_only {
                "Showing ready (unblocked) tasks only"
            } else {
                "Showing all tasks (including blocked)"
            });
        }
        KeyCode::Char('1') => app.switch(View::Today),
        KeyCode::Char('2') => app.switch(View::Inbox),
        KeyCode::Char('3') => app.switch(View::Upcoming),
        KeyCode::Char('4') => app.switch(View::Someday),
        KeyCode::Char('5') => app.switch(View::Projects),
        KeyCode::Char('6') => app.switch(View::All),
        KeyCode::Tab => app.switch(match app.view {
            View::Today => View::Inbox,
            View::Inbox => View::Upcoming,
            View::Upcoming => View::Someday,
            View::Someday => View::Projects,
            View::Projects | View::Project(_) => View::All,
            View::All => View::Today,
        }),
        KeyCode::BackTab => app.switch(match app.view {
            View::Today => View::All,
            View::Inbox => View::Today,
            View::Upcoming => View::Inbox,
            View::Someday => View::Upcoming,
            View::Projects | View::Project(_) => View::Someday,
            View::All => View::Projects,
        }),
        KeyCode::Down | KeyCode::Char('j') => app.select_delta(1),
        KeyCode::Up | KeyCode::Char('k') => app.select_delta(-1),
        KeyCode::Home | KeyCode::Char('g') => {
            if !app.rows.is_empty() {
                app.list.select(Some(0));
                app.detail_scroll = 0;
            }
        }
        KeyCode::End | KeyCode::Char('G') => {
            if !app.rows.is_empty() {
                app.list.select(Some(app.rows.len() - 1));
                app.detail_scroll = 0;
            }
        }
        KeyCode::Char(']') => app.jump_section(true),
        KeyCode::Char('[') => app.jump_section(false),
        KeyCode::PageUp => app.select_delta(-(app.page_size as isize)),
        KeyCode::PageDown => app.select_delta(app.page_size as isize),
        KeyCode::Enter => {
            if let Some(key) = app.selected().map(str::to_owned) {
                if app.view == View::Projects {
                    app.switch(View::Project(key));
                } else {
                    app.detail = true;
                    app.detail_scroll = 0;
                }
            }
        }
        KeyCode::Esc if !app.query.is_empty() => {
            app.query.clear();
            app.rebuild();
        }
        KeyCode::Esc if matches!(app.view, View::Project(_)) => app.switch(View::Projects),
        KeyCode::Char('r') => {
            app.reload(db)?;
            app.set_ok("Refreshed");
        }
        KeyCode::Char('a') if key.kind == KeyEventKind::Press => {
            app.input = Some(Input::new(InputKind::Add, String::new()));
        }
        KeyCode::Char('A' | 'n') if key.kind == KeyEventKind::Press => {
            if app.view == View::Projects {
                app.input = Some(Input::new(InputKind::AddProject, String::new()));
            } else if let Some(key) = app.task_key()
                && let Some(task) = app.store.get_task(&key)
            {
                let (parent_key, parent_number) = if let Some(pk) = &task.parent_task_key
                    && let Some(parent) = app.store.get_task(pk)
                {
                    (pk.clone(), parent.task_number)
                } else {
                    (key, task.task_number)
                };
                app.input = Some(Input::new(
                    InputKind::AddSubtask {
                        parent_key,
                        parent_number,
                    },
                    String::new(),
                ));
            }
        }
        KeyCode::Char('/') if key.kind == KeyEventKind::Press => {
            if app.view == View::Projects {
                app.view = View::All;
                app.rebuild();
            }
            app.input = Some(Input::new(InputKind::Search, app.query.clone()));
        }
        KeyCode::Char('s') if key.kind == KeyEventKind::Press => {
            if let Some(key) = app.task_key() {
                app.input = Some(Input::new(InputKind::Schedule(key), String::new()));
            }
        }
        KeyCode::Char('t') if key.kind == KeyEventKind::Press => {
            if let Some(key) = app.task_key() {
                let today = jiff::Zoned::now().date();
                app.apply_mutation(
                    db,
                    Mutation::Schedule {
                        key,
                        when: When::Scheduled { date: today },
                    },
                )?;
            }
        }
        KeyCode::Char('S') if key.kind == KeyEventKind::Press => {
            if let Some(key) = app.task_key() {
                app.apply_mutation(
                    db,
                    Mutation::Schedule {
                        key,
                        when: When::Someday,
                    },
                )?;
            }
        }
        KeyCode::Char('i') if key.kind == KeyEventKind::Press => {
            if let Some(key) = app.task_key() {
                app.apply_mutation(
                    db,
                    Mutation::Schedule {
                        key,
                        when: When::Inbox,
                    },
                )?;
            }
        }
        KeyCode::Char('p') if key.kind == KeyEventKind::Press => {
            if let Some(key) = app.task_key() {
                let current_proj = app
                    .store
                    .get_task(&key)
                    .and_then(|t| t.project_key.as_deref())
                    .and_then(|pk| app.store.get_project(pk))
                    .map(|p| p.name.clone())
                    .unwrap_or_default();
                app.input = Some(Input::new(InputKind::Project(key), current_proj));
            }
        }
        KeyCode::Char('D') | KeyCode::Delete if key.kind == KeyEventKind::Press => {
            if let Some(key) = app.task_key() {
                app.apply_mutation(db, Mutation::Delete(key))?;
            }
        }
        KeyCode::Char('u') if key.kind == KeyEventKind::Press => {
            if let Some(action) = app.last_undo.take() {
                app.apply_mutation(db, Mutation::Undo(action))?;
            } else {
                app.set_err("Nothing to undo");
            }
        }
        KeyCode::Char('x' | ' ') if key.kind == KeyEventKind::Press => {
            if let Some(key) = app.task_key() {
                app.apply_mutation(db, Mutation::Complete(key))?;
            }
        }
        KeyCode::Char('e') if key.kind == KeyEventKind::Press => {
            if let Some(key) = app.task_key() {
                guard.suspend()?;
                let result = edit_selected(db, &key);
                guard.resume(terminal)?;
                match result {
                    Ok(status) => app.set_ok(status),
                    Err(error) => app.set_err(error.to_string()),
                }
                app.reload(db)?;
            }
        }
        _ => {}
    }
    Ok(false)
}

fn clean(text: &str, multiline: bool) -> String {
    text.chars()
        .map(|c| {
            if c == '\n' && multiline {
                '\n'
            } else if c.is_control() {
                ' '
            } else {
                c
            }
        })
        .collect()
}

fn schedule(when: &When) -> String {
    match when {
        When::Inbox => "Inbox".into(),
        When::Someday => "Someday".into(),
        When::Scheduled { date } => date.to_string(),
        When::LegacyToday { .. } => "Today".into(),
        When::LegacyAnytime => "Someday".into(),
    }
}

fn truncate_str(s: &str, max_width: usize) -> String {
    use unicode_width::UnicodeWidthChar;
    if UnicodeWidthStr::width(s) <= max_width {
        return s.to_string();
    }
    let ellipsis = "…";
    if max_width <= 1 {
        return ellipsis.chars().take(max_width).collect();
    }
    let target = max_width - 1;
    let mut out = String::new();
    let mut w = 0;
    for ch in s.chars() {
        let cw = ch.width().unwrap_or(0);
        if w + cw > target {
            break;
        }
        out.push(ch);
        w += cw;
    }
    out.push('…');
    out
}

fn urgency_color(urgency: TaskUrgency) -> Color {
    match urgency {
        TaskUrgency::Completed => Color::DarkGray,
        TaskUrgency::OnTrack => Color::Green,
        TaskUrgency::ApproachingDeadline => Color::Yellow,
        TaskUrgency::DueToday => Color::Rgb(255, 140, 0),
        TaskUrgency::Overdue => Color::Red,
    }
}

fn status_glyph_span(task: &Task, is_blocked: bool, row_bg: Option<Color>) -> Span<'static> {
    let urgency = calculate_task_urgency(task);
    let (glyph, color) = if task.completed_at.is_some() {
        ("✔", Color::DarkGray)
    } else if is_blocked {
        ("▢", Color::DarkGray)
    } else {
        ("▢", urgency_color(urgency))
    };
    let mut style = Style::default().fg(color);
    if matches!(urgency, TaskUrgency::Overdue | TaskUrgency::DueToday) && !is_blocked {
        style = style.add_modifier(Modifier::BOLD);
    }
    if let Some(bg) = row_bg {
        style = style.bg(bg);
    }
    Span::styled(glyph, style)
}

fn with_bg(mut style: Style, row_bg: Option<Color>) -> Style {
    if let Some(bg) = row_bg {
        style = style.bg(bg);
    }
    style
}

/// Renders a single task row in the Saku Classic layout:
/// ` ›  12  ▢  Today   Title ·················· ↻ weekly  Work / Project `
fn render_task_row(
    task: &Task,
    store: &Store,
    width: usize,
    is_selected: bool,
    parent_in_view: bool,
) -> Line<'static> {
    let row_bg = is_selected.then_some(Color::Indexed(236));
    let is_blocked = store.is_task_blocked(task);
    let (date_raw, badge_urgency) = get_task_date_badge(task);
    let date_clean = clean(&date_raw, false);

    let prefix_str = if is_selected { " › " } else { "   " };
    let indent_str = if parent_in_view { "   └─ " } else { "" };
    let id_str = format!("{:>3}", task.task_number);

    let badge_color = match badge_urgency {
        TaskUrgency::Overdue => Color::Red,
        TaskUrgency::DueToday => Color::Rgb(255, 140, 0),
        TaskUrgency::ApproachingDeadline => Color::Yellow,
        _ => Color::DarkGray,
    };

    // Build right-aligned metadata tokens
    let mut right_parts: Vec<(String, Style)> = Vec::new();
    let blockers = store.get_blockers(task);
    if !blockers.is_empty() {
        let label = if blockers.len() == 1 {
            format!("[blocked: #{}]", blockers[0].task_number)
        } else {
            format!(
                "[blocked: #{} +{}]",
                blockers[0].task_number,
                blockers.len() - 1
            )
        };
        right_parts.push((label, Style::default().fg(Color::Red)));
    }
    if !parent_in_view
        && let Some(parent) = task
            .parent_task_key
            .as_deref()
            .and_then(|key| store.get_task(key))
    {
        right_parts.push((
            format!("↳ #{}", parent.task_number),
            Style::default().fg(Color::DarkGray),
        ));
    }
    if let Some(est) = task.estimate_minutes {
        right_parts.push((
            format!("⏱ {}", format_estimate(est)),
            Style::default().fg(Color::DarkGray),
        ));
    }
    if let Some(rec) = &task.recurrence {
        right_parts.push((
            format!("↻ {}", clean(&rec.to_string(), false)),
            Style::default().fg(Color::Cyan),
        ));
    }
    if let Some(ctx) = get_task_context(task, store) {
        right_parts.push((clean(&ctx, false), Style::default().fg(Color::DarkGray)));
    }

    // Fixed left width: prefix(3) + indent + id(3) + "  " + glyph(1) + "  " + date + "  "
    let left_fixed_width = 3
        + UnicodeWidthStr::width(indent_str)
        + 3
        + 2
        + 1
        + 2
        + UnicodeWidthStr::width(date_clean.as_str())
        + 2;

    let right_total_width: usize = if right_parts.is_empty() {
        0
    } else {
        right_parts
            .iter()
            .map(|(s, _)| UnicodeWidthStr::width(s.as_str()))
            .sum::<usize>()
            + (right_parts.len() - 1) * 2
            + 1 // trailing space
    };

    let raw_title = clean(&task.title, false);
    let min_dots = 3usize;
    let show_right = !right_parts.is_empty()
        && width > left_fixed_width + 8 + min_dots + right_total_width;
    let effective_right_width = if show_right { right_total_width } else { 1 };
    let max_title_width = width
        .saturating_sub(left_fixed_width + effective_right_width + min_dots)
        .max(4);
    let display_title = truncate_str(&raw_title, max_title_width);
    let title_width = UnicodeWidthStr::width(display_title.as_str());

    let mut title_style = if is_selected {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else if is_blocked || task.completed_at.is_some() {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::White)
    };
    if task.completed_at.is_some() {
        title_style = title_style.add_modifier(Modifier::CROSSED_OUT);
    }

    let mut spans = vec![
        Span::styled(
            prefix_str,
            with_bg(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
                row_bg,
            ),
        ),
        Span::styled(
            indent_str,
            with_bg(Style::default().fg(Color::DarkGray), row_bg),
        ),
        Span::styled(
            id_str,
            with_bg(
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
                row_bg,
            ),
        ),
        Span::styled("  ", with_bg(Style::default(), row_bg)),
        status_glyph_span(task, is_blocked, row_bg),
        Span::styled("  ", with_bg(Style::default(), row_bg)),
        Span::styled(
            date_clean,
            with_bg(Style::default().fg(badge_color), row_bg),
        ),
        Span::styled("  ", with_bg(Style::default(), row_bg)),
        Span::styled(display_title, with_bg(title_style, row_bg)),
    ];

    let used_width = left_fixed_width + title_width + if show_right { right_total_width } else { 0 };
    let fill_space = width.saturating_sub(used_width);
    let dots = if fill_space >= 3 {
        format!(" {} ", "·".repeat(fill_space - 2))
    } else {
        " ".repeat(fill_space)
    };
    spans.push(Span::styled(
        dots,
        with_bg(Style::default().fg(Color::DarkGray), row_bg),
    ));

    if show_right {
        for (idx, (part, style)) in right_parts.into_iter().enumerate() {
            if idx > 0 {
                spans.push(Span::styled(
                    "  ",
                    with_bg(Style::default().fg(Color::DarkGray), row_bg),
                ));
            }
            spans.push(Span::styled(part, with_bg(style, row_bg)));
        }
        spans.push(Span::styled(" ", with_bg(Style::default(), row_bg)));
    }

    Line::from(spans)
}

fn render_project_row(
    project_key: &str,
    store: &Store,
    width: usize,
    is_selected: bool,
) -> Option<Line<'static>> {
    let project = store.get_project(project_key)?;
    let row_bg = is_selected.then_some(Color::Indexed(236));
    let active_count = store
        .get_tasks_for_project(project_key)
        .filter(|t| t.deleted_at.is_none() && t.completed_at.is_none())
        .count();
    let prefix_str = if is_selected { " › " } else { "   " };
    let name = clean(&project.name, false);
    let count_label = if active_count == 1 {
        "1 task".to_string()
    } else {
        format!("{active_count} tasks")
    };

    let left_len = 3 + UnicodeWidthStr::width(name.as_str());
    let right_len = UnicodeWidthStr::width(count_label.as_str()) + 1;
    let fill_space = width.saturating_sub(left_len + right_len);
    let dots = if fill_space >= 3 {
        format!(" {} ", "·".repeat(fill_space - 2))
    } else {
        " ".repeat(fill_space)
    };

    let name_style = if is_selected {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };
    let count_style = if active_count > 0 {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    Some(Line::from(vec![
        Span::styled(
            prefix_str,
            with_bg(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
                row_bg,
            ),
        ),
        Span::styled(name, with_bg(name_style, row_bg)),
        Span::styled(
            dots,
            with_bg(Style::default().fg(Color::DarkGray), row_bg),
        ),
        Span::styled(count_label, with_bg(count_style, row_bg)),
        Span::styled(" ", with_bg(Style::default(), row_bg)),
    ]))
}

fn section_header_line(title: &str) -> Line<'static> {
    let color = if title == "Overdue" {
        Color::Red
    } else {
        Color::Rgb(144, 238, 144)
    };
    Line::from(vec![
        Span::raw("  "),
        Span::styled(
            "▌ ",
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            clean(title, false),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
    ])
}

fn detail_section_header_line(label: &str, width: u16) -> Line<'static> {
    let green = Color::Rgb(144, 238, 144);
    let label_clean = clean(label, false);
    let dashes = usize::from(width)
        .saturating_sub(5 + UnicodeWidthStr::width(label_clean.as_str()) + 1);
    Line::from(vec![
        Span::raw("  "),
        Span::styled("▌ ", Style::default().fg(green).add_modifier(Modifier::BOLD)),
        Span::styled(
            format!("{label_clean} "),
            Style::default().fg(green).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "─".repeat(dashes),
            Style::default().fg(Color::DarkGray),
        ),
    ])
}

fn detail_field_line(label: &str, value: String, value_style: Style) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("    {:<12}", label),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(value, value_style),
    ])
}

fn detail_lines(task: &Task, store: &Store, width: u16) -> Vec<Line<'static>> {
    let is_blocked = store.is_task_blocked(task);
    let mut lines = vec![
        Line::from(vec![
            Span::raw("  "),
            Span::styled(
                "▌ ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("#{}  ", task.task_number),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            ),
            status_glyph_span(task, is_blocked, None),
            Span::raw("  "),
            Span::styled(
                clean(&task.title, false),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::default(),
    ];

    let (status_label, status_color) = if task.completed_at.is_some() {
        ("Completed", Color::DarkGray)
    } else if is_blocked {
        ("Blocked", Color::Red)
    } else {
        ("Active", Color::Green)
    };
    lines.push(detail_field_line(
        "Status",
        status_label.into(),
        Style::default().fg(status_color),
    ));

    lines.push(detail_field_line(
        "Schedule",
        schedule(&task.when),
        Style::default().fg(Color::White),
    ));

    if let Some(date) = task.deadline {
        let today = jiff::Zoned::now().date();
        let dl_color = if date < today {
            Color::Red
        } else if date == today {
            Color::Rgb(255, 140, 0)
        } else {
            Color::Yellow
        };
        lines.push(detail_field_line(
            "Deadline",
            format!("⚑ {date}"),
            Style::default().fg(dl_color),
        ));
    }

    if let Some(date) = task.defer_until {
        lines.push(detail_field_line(
            "Defer until",
            date.to_string(),
            Style::default().fg(Color::White),
        ));
    }

    if let Some(ctx) = get_task_context(task, store) {
        lines.push(detail_field_line(
            "Project",
            clean(&ctx, false),
            Style::default().fg(Color::Cyan),
        ));
    }

    if let Some(est) = task.estimate_minutes {
        lines.push(detail_field_line(
            "Estimate",
            format!("⏱ {}", format_estimate(est)),
            Style::default().fg(Color::Yellow),
        ));
    }

    if let Some(assignee) = &task.assigned_to {
        lines.push(detail_field_line(
            "Assigned to",
            clean(assignee, false),
            Style::default().fg(Color::White),
        ));
    }

    if !task.tags.is_empty() {
        let tags_str = task
            .tags
            .iter()
            .map(|t| format!("#{}", clean(t, false)))
            .collect::<Vec<_>>()
            .join("  ");
        lines.push(detail_field_line(
            "Tags",
            tags_str,
            Style::default().fg(Color::Cyan),
        ));
    }

    if let Some(recurrence) = &task.recurrence {
        lines.push(detail_field_line(
            "Repeats",
            format!("↻ {}", clean(&recurrence.to_string(), false)),
            Style::default().fg(Color::Cyan),
        ));
    }

    let blockers = store.get_blockers(task);
    if !blockers.is_empty() {
        let blocker_str = blockers
            .iter()
            .map(|t| format!("#{} {}", t.task_number, clean(&t.title, false)))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(detail_field_line(
            "Blocked by",
            blocker_str,
            Style::default().fg(Color::Red),
        ));
    }

    if let Some(parent) = task
        .parent_task_key
        .as_deref()
        .and_then(|key| store.get_task(key))
    {
        lines.push(detail_field_line(
            "Parent",
            format!("#{} {}", parent.task_number, clean(&parent.title, false)),
            Style::default().fg(Color::White),
        ));
    }

    if let Some(notes) = &task.notes {
        lines.push(Line::default());
        lines.push(detail_section_header_line("Notes", width));
        for note_line in clean(notes, true).split('\n') {
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled(note_line.to_string(), Style::default().fg(Color::Gray)),
            ]));
        }
    }

    let mut subtasks: Vec<&Task> = store.get_subtasks(&task.storage_key()).collect();
    subtasks.sort_by_key(|t| (t.completed_at.is_some(), t.task_number));
    if !subtasks.is_empty() {
        lines.push(Line::default());
        lines.push(detail_section_header_line(
            &format!("Subtasks ({})", subtasks.len()),
            width,
        ));
        for sub in subtasks {
            let sub_blocked = store.is_task_blocked(sub);
            let sub_style = if sub.completed_at.is_some() || sub_blocked {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(Color::White)
            };
            lines.push(Line::from(vec![
                Span::styled("    └─ ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("#{: <3} ", sub.task_number),
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC),
                ),
                status_glyph_span(sub, sub_blocked, None),
                Span::raw("  "),
                Span::styled(clean(&sub.title, false), sub_style),
            ]));
        }
    }

    lines
}

fn project_preview_lines(project_key: &str, store: &Store, width: u16) -> Vec<Line<'static>> {
    let Some(project) = store.get_project(project_key) else {
        return vec![];
    };
    let mut lines = vec![
        Line::from(vec![
            Span::raw("  "),
            Span::styled(
                "▌ ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                clean(&project.name, false),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::default(),
    ];

    if let Some(area_key) = &project.area_key
        && let Some(area) = store.get_area(area_key)
    {
        lines.push(detail_field_line(
            "Area",
            clean(&area.name, false),
            Style::default().fg(Color::Cyan),
        ));
    }

    let mut tasks: Vec<&Task> = store
        .get_tasks_for_project(project_key)
        .filter(|t| t.deleted_at.is_none() && t.completed_at.is_none())
        .collect();
    tasks.sort_by_key(|t| t.task_number);

    lines.push(detail_field_line(
        "Active",
        format!("{} tasks", tasks.len()),
        Style::default().fg(Color::White),
    ));

    if !tasks.is_empty() {
        lines.push(Line::default());
        lines.push(detail_section_header_line("Tasks", width));
        for task in tasks {
            let is_blocked = store.is_task_blocked(task);
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    format!("{:>3}  ", task.task_number),
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC),
                ),
                status_glyph_span(task, is_blocked, None),
                Span::raw("  "),
                Span::styled(
                    clean(&task.title, false),
                    if is_blocked {
                        Style::default().fg(Color::DarkGray)
                    } else {
                        Style::default().fg(Color::White)
                    },
                ),
            ]));
        }
    }

    lines
}

#[cfg(test)]
fn detail_text(task: &Task, store: &Store) -> String {
    let mut lines = vec![format!(
        "#{} {}",
        task.task_number,
        clean(&task.title, false)
    )];
    if let Some(notes) = &task.notes {
        lines.push(format!("\nNotes: {}", clean(notes, true)));
    }
    if let Some(project) = task
        .project_key
        .as_deref()
        .and_then(|key| store.get_project(key))
    {
        lines.push(format!("Project: {}", clean(&project.name, false)));
    }
    if let Some(area) = task.area_key.as_deref().and_then(|key| store.get_area(key)) {
        lines.push(format!("Area: {}", clean(&area.name, false)));
    }
    lines.push(format!("Schedule: {}", schedule(&task.when)));
    if let Some(date) = task.deadline {
        lines.push(format!("Deadline: {date}"));
    }
    if let Some(date) = task.defer_until {
        lines.push(format!("Deferred until: {date}"));
    }
    if !task.tags.is_empty() {
        lines.push(format!("Tags: {}", clean(&task.tags.join(", "), false)));
    }
    let blockers = store.get_blockers(task);
    if !blockers.is_empty() {
        lines.push(format!(
            "Blocked by: {}",
            blockers
                .iter()
                .map(|t| format!("#{} {}", t.task_number, clean(&t.title, false)))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if let Some(parent) = task
        .parent_task_key
        .as_deref()
        .and_then(|key| store.get_task(key))
    {
        lines.push(format!(
            "Parent: #{} {}",
            parent.task_number,
            clean(&parent.title, false)
        ));
    }
    let subtasks: Vec<_> = store
        .get_subtasks(&task.storage_key())
        .map(|t| format!("#{} {}", t.task_number, clean(&t.title, false)))
        .collect();
    if !subtasks.is_empty() {
        lines.push(format!("Subtasks: {}", subtasks.join(", ")));
    }
    if let Some(recurrence) = &task.recurrence {
        lines.push(format!("Repeats: {recurrence}"));
    }
    lines.join("\n")
}

fn title(app: &App) -> String {
    let context = match &app.view {
        View::Today => "Today".into(),
        View::Inbox => "Inbox".into(),
        View::Upcoming => "Upcoming".into(),
        View::Someday => "Someday".into(),
        View::Projects => "Projects".into(),
        View::Project(key) => app
            .store
            .get_project(key)
            .map(|p| clean(&p.name, false))
            .unwrap_or_default(),
        View::All => "All".into(),
    };
    if app.query.is_empty() {
        context
    } else {
        format!("{context} · search: {}", clean(&app.query, false))
    }
}

fn prompt_tail(text: &str, width: usize) -> String {
    let mut result = String::new();
    for c in text.chars().rev() {
        let mut candidate = String::new();
        candidate.push(c);
        candidate.push_str(&result);
        if UnicodeWidthStr::width(candidate.as_str()) > width {
            break;
        }
        result = candidate;
    }
    result
}

fn prompt_head(text: &str, width: usize) -> String {
    let mut result = String::new();
    for c in text.chars() {
        let mut candidate = result.clone();
        candidate.push(c);
        if UnicodeWidthStr::width(candidate.as_str()) > width {
            break;
        }
        result = candidate;
    }
    result
}

fn prompt_window(text: &str, cursor: usize, width: usize) -> (String, usize) {
    let mut clamped = cursor.min(text.len());
    while clamped > 0 && !text.is_char_boundary(clamped) {
        clamped -= 1;
    }
    let before = clean(&text[..clamped], false);
    let after = clean(&text[clamped..], false);
    let before_w = UnicodeWidthStr::width(before.as_str());
    let after_w = UnicodeWidthStr::width(after.as_str());
    if before_w + after_w <= width {
        return (format!("{before}{after}"), before_w);
    }
    let vis_before = prompt_tail(&before, width);
    let vis_before_w = UnicodeWidthStr::width(vis_before.as_str());
    let rem = width.saturating_sub(vis_before_w);
    let vis_after = prompt_head(&after, rem);
    (format!("{vis_before}{vis_after}"), vis_before_w)
}

fn render_header(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let tabs: &[(&str, &str, bool)] = &[
        ("1", "Today", matches!(app.view, View::Today)),
        ("2", "Inbox", matches!(app.view, View::Inbox)),
        ("3", "Upcoming", matches!(app.view, View::Upcoming)),
        ("4", "Someday", matches!(app.view, View::Someday)),
        (
            "5",
            "Projects",
            matches!(app.view, View::Projects | View::Project(_)),
        ),
        ("6", "All", matches!(app.view, View::All)),
    ];

    let mut bar_spans = vec![
        Span::styled(
            "  ▌ ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "tdo  ",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
    ];

    for (idx, (num, label, active)) in tabs.iter().enumerate() {
        if idx > 0 {
            bar_spans.push(Span::raw("  "));
        }
        if *active {
            bar_spans.push(Span::styled(
                format!(" {num}:{label} "),
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            bar_spans.push(Span::styled(*num, Style::default().fg(Color::Cyan)));
            bar_spans.push(Span::styled(
                format!(":{label}"),
                Style::default().fg(Color::DarkGray),
            ));
        }
    }

    let item_word = if app.view == View::Projects {
        if app.rows.len() == 1 {
            "project"
        } else {
            "projects"
        }
    } else if app.rows.len() == 1 {
        "task"
    } else {
        "tasks"
    };
    let count_badge = format!("({} {})", app.rows.len(), item_word);
    let view_title = clean(&title(app), false);

    let mut filter_badges = Vec::new();
    if app.show_completed && app.view != View::Projects {
        filter_badges.push("[+done]");
    }
    if app.ready_only && app.view != View::Projects {
        filter_badges.push("[ready]");
    }
    let filter_str = if filter_badges.is_empty() {
        String::new()
    } else {
        format!("{} ", filter_badges.join(" "))
    };

    let used = 4
        + UnicodeWidthStr::width(view_title.as_str())
        + 1
        + UnicodeWidthStr::width(count_badge.as_str())
        + 1
        + UnicodeWidthStr::width(filter_str.as_str())
        + 1;
    let dashes = usize::from(area.width).saturating_sub(used);

    let mut sub_spans = vec![
        Span::styled(
            "  ▌ ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{view_title} "),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{count_badge} "),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        ),
    ];
    if !filter_str.is_empty() {
        sub_spans.push(Span::styled(
            filter_str,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
    }
    sub_spans.push(Span::styled(
        "─".repeat(dashes),
        Style::default().fg(Color::DarkGray),
    ));
    let sub_line = Line::from(sub_spans);

    frame.render_widget(
        Paragraph::new(vec![Line::from(bar_spans), sub_line]),
        area,
    );
}

fn render_stream(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    if app.rows.is_empty() {
        frame.render_widget(
            Paragraph::new("  Nothing here. Press a to add to Inbox.")
                .style(Style::default().fg(Color::DarkGray))
                .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }

    let today = jiff::Zoned::now().date();
    let row_set: std::collections::HashSet<&str> = app.rows.iter().map(String::as_str).collect();
    let selected_idx = app.list.selected();
    let width = usize::from(area.width);

    let mut visual_lines: Vec<Line<'static>> = Vec::with_capacity(app.rows.len() + 8);
    let mut selected_visual_line = 0usize;
    let mut current_section: Option<String> = None;

    for (idx, key) in app.rows.iter().enumerate() {
        let is_selected = selected_idx == Some(idx);
        let section = if app.view == View::Projects {
            project_section(key, &app.store)
        } else {
            app.store
                .get_task(key)
                .and_then(|task| task_section(task, &app.view, today))
        };

        let mut header_start_line = None;
        if section.is_some() && section != current_section {
            if !visual_lines.is_empty() {
                visual_lines.push(Line::default());
            }
            header_start_line = Some(visual_lines.len());
            if let Some(ref sec_title) = section {
                visual_lines.push(section_header_line(sec_title));
            }
            current_section = section;
        }

        let item_line_index = visual_lines.len();
        if is_selected {
            // If this is the very first item under a section header, keep the section header visible
            selected_visual_line = header_start_line.unwrap_or(item_line_index);
        }

        if app.view == View::Projects {
            if let Some(line) = render_project_row(key, &app.store, width, is_selected) {
                visual_lines.push(line);
            }
        } else if let Some(task) = app.store.get_task(key) {
            let parent_in_view = task
                .parent_task_key
                .as_deref()
                .is_some_and(|pk| row_set.contains(pk));
            visual_lines.push(render_task_row(
                task,
                &app.store,
                width,
                is_selected,
                parent_in_view,
            ));
        }
    }

    let viewport_height = usize::from(area.height);
    if selected_visual_line < app.stream_scroll {
        app.stream_scroll = selected_visual_line;
    } else if selected_visual_line >= app.stream_scroll + viewport_height {
        app.stream_scroll = selected_visual_line.saturating_sub(viewport_height - 1);
    }
    let max_scroll = visual_lines.len().saturating_sub(viewport_height);
    app.stream_scroll = app.stream_scroll.min(max_scroll);

    frame.render_widget(
        Paragraph::new(visual_lines).scroll((app.stream_scroll as u16, 0)),
        area,
    );
}

fn render_inspector(frame: &mut Frame<'_>, app: &App, area: Rect, focused: bool) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let lines = if app.view == View::Projects {
        app.selected()
            .map(|key| project_preview_lines(key, &app.store, area.width))
            .unwrap_or_default()
    } else {
        app.task_key()
            .and_then(|key| app.store.get_task(&key))
            .map(|task| detail_lines(task, &app.store, area.width))
            .unwrap_or_default()
    };

    let scroll = if focused { app.detail_scroll } else { 0 };
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        area,
    );
}

fn render(frame: &mut Frame<'_>, app: &mut App) {
    let area = frame.area();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(area);

    render_header(frame, app, rows[0]);

    let body = rows[1];
    app.page_size = usize::from(body.height.saturating_sub(2).max(1));

    if body.width >= 88 && (app.show_inspector || app.detail) {
        let panels = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(58),
                Constraint::Length(1),
                Constraint::Percentage(42),
            ])
            .split(body);

        render_stream(frame, app, panels[0]);

        let sep_color = if app.detail {
            Color::Cyan
        } else {
            Color::DarkGray
        };
        let sep_lines: Vec<Line<'static>> = (0..panels[1].height)
            .map(|_| Line::from(Span::styled("│", Style::default().fg(sep_color))))
            .collect();
        frame.render_widget(Paragraph::new(sep_lines), panels[1]);

        render_inspector(frame, app, panels[2], app.detail);
    } else if app.detail {
        frame.render_widget(Clear, body);
        render_inspector(frame, app, body, true);
    } else {
        render_stream(frame, app, body);
    }

    let key_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(Color::DarkGray);

    let status_line = if app.status.is_empty() {
        Line::from("")
    } else if app.status_error {
        Line::from(vec![
            Span::styled(
                "  ✗ ",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::styled(clean(&app.status, false), Style::default().fg(Color::Red)),
        ])
    } else {
        Line::from(vec![
            Span::styled("  ✓ ", Style::default().fg(Color::Green)),
            Span::styled(clean(&app.status, false), Style::default().fg(Color::Green)),
        ])
    };

    let footer = if let Some(input) = &app.input {
        let label = match &input.kind {
            InputKind::Add => "Add to Inbox".to_string(),
            InputKind::AddSubtask { parent_number, .. } => format!("Subtask under #{parent_number}"),
            InputKind::AddProject => "New Project".to_string(),
            InputKind::Schedule(_) => "Schedule".to_string(),
            InputKind::Project(_) => "Project (empty clears)".to_string(),
            InputKind::Search => "Search".to_string(),
        };
        let prefix = "  ▌ ";
        let label_str = format!("{label}: ");
        let total_label_width =
            UnicodeWidthStr::width(prefix) + UnicodeWidthStr::width(label_str.as_str());
        let width = usize::from(rows[2].width).saturating_sub(total_label_width + 1);
        let (visible, cursor_offset) = prompt_window(&input.text, input.cursor, width);
        let cursor = total_label_width + cursor_offset;
        if rows[2].width > 0 && rows[2].height > 0 {
            frame.set_cursor_position((
                rows[2].x + (cursor.min(usize::from(rows[2].width - 1)) as u16),
                rows[2].y,
            ));
        }

        let hint_or_status = if !app.status.is_empty() {
            status_line
        } else {
            match &input.kind {
                InputKind::Project(_) => {
                    let needle = input.text.trim().to_lowercase();
                    let mut names: Vec<String> = app
                        .store
                        .projects
                        .values()
                        .filter(|p| {
                            p.deleted_at.is_none()
                                && p.completed_at.is_none()
                                && (needle.is_empty() || p.name.to_lowercase().contains(&needle))
                        })
                        .map(|p| clean(&p.name, false))
                        .collect();
                    names.sort();
                    if names.is_empty() {
                        Line::from(Span::styled("  (no matching projects)", desc_style))
                    } else {
                        Line::from(vec![
                            Span::styled("  Tab: ", key_style),
                            Span::styled(names.join(" · "), desc_style),
                        ])
                    }
                }
                InputKind::Schedule(_) => Line::from(vec![
                    Span::styled("  Tab: ", key_style),
                    Span::styled(
                        "today · tomorrow · someday · inbox · +3d · next mon · YYYY-MM-DD",
                        desc_style,
                    ),
                ]),
                _ => status_line,
            }
        };

        vec![
            Line::from(vec![
                Span::styled(prefix, key_style),
                Span::styled(label_str, key_style),
                Span::styled(
                    visible,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            hint_or_status,
            Line::from(vec![
                Span::raw("  "),
                Span::styled("Enter", key_style),
                Span::styled(" accept  ", desc_style),
                Span::styled("Tab", key_style),
                Span::styled(" complete  ", desc_style),
                Span::styled("Esc", key_style),
                Span::styled(" cancel  ", desc_style),
                Span::styled("Ctrl-C", key_style),
                Span::styled(" quit", desc_style),
            ]),
        ]
    } else {
        vec![
            Line::from(vec![
                Span::raw("  "),
                Span::styled("q/Ctrl-C", key_style),
                Span::styled(" quit  ", desc_style),
                Span::styled("a", key_style),
                Span::styled(" add Inbox  ", desc_style),
                Span::styled("A", key_style),
                Span::styled(" sub/proj  ", desc_style),
                Span::styled("x", key_style),
                Span::styled(" done  ", desc_style),
                Span::styled("t/S/i/s", key_style),
                Span::styled(" when  ", desc_style),
                Span::styled("p", key_style),
                Span::styled(" project", desc_style),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("e", key_style),
                Span::styled(" edit  ", desc_style),
                Span::styled("D", key_style),
                Span::styled(" del  ", desc_style),
                Span::styled("u", key_style),
                Span::styled(" undo  ", desc_style),
                Span::styled("c", key_style),
                Span::styled(" +done  ", desc_style),
                Span::styled("b", key_style),
                Span::styled(" ready  ", desc_style),
                Span::styled("d", key_style),
                Span::styled(" details  ", desc_style),
                Span::styled("/", key_style),
                Span::styled(" search  ", desc_style),
                Span::styled("?", key_style),
                Span::styled(" help", desc_style),
            ]),
            status_line,
        ]
    };
    frame.render_widget(Paragraph::new(footer).wrap(Wrap { trim: false }), rows[2]);

    if app.help {
        frame.render_widget(Clear, area);
        let help = Text::from(vec![
            Line::from(vec![
                Span::styled("  ▌ ", key_style),
                Span::styled(
                    "tdo keys",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::default(),
            Line::from(vec![
                Span::styled("  1", key_style),
                Span::styled(" Today · ", desc_style),
                Span::styled("2", key_style),
                Span::styled(" Inbox · ", desc_style),
                Span::styled("3", key_style),
                Span::styled(" Upcoming · ", desc_style),
                Span::styled("4", key_style),
                Span::styled(" Someday · ", desc_style),
                Span::styled("5", key_style),
                Span::styled(" Projects · ", desc_style),
                Span::styled("6", key_style),
                Span::styled(" All · ", desc_style),
                Span::styled("Tab/S-Tab", key_style),
                Span::styled(" cycle", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  j/k, ↑/↓, g/G, Home/End, Ctrl-d/u, PgUp/Dn, [/]", key_style),
                Span::styled(": navigate stream & sections", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  Enter", key_style),
                Span::styled(": open project/scroll details · ", desc_style),
                Span::styled("d", key_style),
                Span::styled(": toggle details · ", desc_style),
                Span::styled("Esc", key_style),
                Span::styled(": close/back/clear", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  a", key_style),
                Span::styled(": add Inbox · ", desc_style),
                Span::styled("A/n", key_style),
                Span::styled(": add subtask / new project · ", desc_style),
                Span::styled("x/Space", key_style),
                Span::styled(": complete or reopen", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  t", key_style),
                Span::styled(": Today · ", desc_style),
                Span::styled("S", key_style),
                Span::styled(": Someday · ", desc_style),
                Span::styled("i", key_style),
                Span::styled(": Inbox · ", desc_style),
                Span::styled("s", key_style),
                Span::styled(": schedule prompt · ", desc_style),
                Span::styled("p", key_style),
                Span::styled(": assign project", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  D/Del", key_style),
                Span::styled(": delete task · ", desc_style),
                Span::styled("u", key_style),
                Span::styled(": undo last complete/delete · ", desc_style),
                Span::styled("e", key_style),
                Span::styled(": external editor", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  c", key_style),
                Span::styled(": toggle completed · ", desc_style),
                Span::styled("b", key_style),
                Span::styled(": toggle ready-only · ", desc_style),
                Span::styled("/", key_style),
                Span::styled(": search · ", desc_style),
                Span::styled("r", key_style),
                Span::styled(": refresh", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  Prompt", key_style),
                Span::styled(
                    ": ←/→, Home/End, Ctrl-a/e/b/f/w/u, Tab completion · ",
                    desc_style,
                ),
                Span::styled("?/Esc", key_style),
                Span::styled(": close · ", desc_style),
                Span::styled("q/Ctrl-C", key_style),
                Span::styled(": quit", desc_style),
            ]),
        ]);
        frame.render_widget(
            Paragraph::new(help)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Cyan))
                        .title(" Help "),
                )
                .wrap(Wrap { trim: false }),
            area,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;
    use ratatui::{Terminal, backend::TestBackend};
    use saku_tdo::models::{
        project::Project,
        task::{Freq, Recurrence},
    };
    use tempfile::TempDir;

    fn fixture_task(suffix: &str, title: &str) -> Task {
        Task {
            storage_key_suffix: suffix.into(),
            title: title.into(),
            ..Task::default()
        }
    }

    fn recurrence(start: Date) -> Recurrence {
        Recurrence {
            freq: Freq::Daily,
            weekdays: vec![],
            monthly_anchor: None,
            until: None,
            dtstart: start,
        }
    }

    fn open_db() -> (TempDir, SqliteStorage) {
        let dir = tempfile::tempdir().unwrap();
        let db = SqliteStorage::open(dir.path().join("store.sqlite3")).unwrap();
        (dir, db)
    }

    fn key_for(db: &SqliteStorage, title: &str) -> String {
        db.load()
            .unwrap()
            .tasks
            .into_iter()
            .find_map(|(key, task)| (task.title == title).then_some(key))
            .unwrap()
    }

    fn metadata(dir: &TempDir) -> (i64, String) {
        rusqlite::Connection::open(dir.path().join("store.sqlite3"))
            .unwrap()
            .query_row(
                "SELECT generation, clock FROM store_metadata WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap()
    }

    #[test]
    fn view_filters_search_subtasks_and_selection_by_key() {
        let today = date(2026, 4, 15);
        let yesterday = date(2026, 4, 14);
        let tomorrow = date(2026, 4, 16);
        let mut store = Store::default();
        store.add_project(Project {
            name: "Orion".into(),
            ..Project::default()
        });
        let mut overdue = fixture_task("overdue", "Overdue");
        overdue.when = When::Scheduled { date: yesterday };
        store.add_task(overdue);
        let mut due = fixture_task("due", "Deadline");
        due.deadline = Some(today);
        store.add_task(due);
        let mut repeat = fixture_task("repeat", "Repeat");
        repeat.recurrence = Some(recurrence(today));
        store.add_task(repeat.clone());
        repeat.storage_key_suffix = "repeat-done".into();
        repeat.completed_occurrences.push(today);
        store.add_task(repeat);
        let mut child = fixture_task("child", "Child");
        child.parent_task_key = Some("task/overdue".into());
        child.depends_on.push("task/overdue".into());
        child.notes = Some("Needle in notes".into());
        child.project_key = Some("project/orion".into());
        store.add_task(child);
        let mut deferred = fixture_task("deferred", "Deferred");
        deferred.when = When::Scheduled { date: yesterday };
        deferred.defer_until = Some(tomorrow);
        deferred.project_key = Some("project/orion".into());
        store.add_task(deferred);
        let mut completed = fixture_task("completed", "Completed");
        completed.completed_at = Some(jiff::Timestamp::now());
        store.add_task(completed);
        let mut deleted = fixture_task("deleted", "Deleted");
        deleted.deleted_at = Some(jiff::Timestamp::now());
        store.add_task(deleted);

        let today_keys = task_keys(&store, &View::Today, "", today);
        for key in ["task/overdue", "task/due", "task/repeat"] {
            assert!(today_keys.contains(&key.to_string()), "{key}");
        }
        for key in [
            "task/repeat-done",
            "task/deferred",
            "task/completed",
            "task/deleted",
        ] {
            assert!(!today_keys.contains(&key.to_string()), "{key}");
        }
        assert!(task_keys(&store, &View::Inbox, "", today).contains(&"task/child".to_string()));
        assert!(task_keys(&store, &View::All, "", today).contains(&"task/deferred".to_string()));
        let project = View::Project("project/orion".into());
        assert!(task_keys(&store, &project, "", today).contains(&"task/deferred".to_string()));
        assert_eq!(
            task_keys(&store, &View::All, "NEEDLE", today),
            ["task/child"]
        );
        assert_eq!(task_keys(&store, &View::All, "orION", today).len(), 2);
        let details = detail_text(store.get_task("task/child").unwrap(), &store);
        assert!(details.contains("Parent: #1 Overdue"));
        assert!(details.contains("Blocked by: #1 Overdue"));

        let mut app = App::new(store);
        app.switch(View::All);
        let index = app.rows.iter().position(|key| key == "task/child").unwrap();
        app.list.select(Some(index));
        app.store.get_task_mut("task/child").unwrap().deadline = Some(date(2020, 1, 1));
        app.rebuild();
        assert_eq!(app.selected(), Some("task/child"));
        let index = app.list.selected().unwrap();
        app.store.tasks.remove("task/child");
        app.rebuild();
        assert_eq!(app.list.selected(), Some(index.min(app.rows.len() - 1)));
        app.store.tasks.clear();
        app.rebuild();
        assert_eq!(app.list.selected(), None);

        app.switch(View::Project("project/orion".into()));
        app.store
            .projects
            .get_mut("project/orion")
            .unwrap()
            .completed_at = Some(jiff::Timestamp::now());
        app.rebuild();
        assert_eq!(app.view, View::Projects);
        assert!(app.status.contains("no longer available"));
    }

    #[test]
    fn schedule_preserves_latest_values_and_complete_resolves_renumbered_key() {
        let (dir, mut first) = open_db();
        mutate(&mut first, Mutation::Add("selected".into())).unwrap();
        mutate(&mut first, Mutation::Add("other".into())).unwrap();
        let selected = key_for(&first, "selected");
        let other = key_for(&first, "other");
        assert!(matches!(
            first.load().unwrap().get_task(&selected).unwrap().when,
            When::Inbox
        ));
        assert_eq!(first.pending_changes().unwrap().len(), 2);
        let before_selection = first
            .load()
            .unwrap()
            .get_task(&selected)
            .unwrap()
            .task_number;
        let mut second = SqliteStorage::open(dir.path().join("store.sqlite3")).unwrap();
        second
            .transaction::<_, Box<dyn std::error::Error>>(|store, storage| {
                let other_number = store.get_task(&other).unwrap().task_number;
                let stamp = storage.next_modified_at()?;
                let task = store.get_task_mut(&selected).unwrap();
                task.notes = Some("new notes from another client".into());
                task.tags = vec!["fresh".into(), "tag".into()];
                task.task_number = other_number;
                task.modified_at = stamp;
                store.get_task_mut(&other).unwrap().task_number = before_selection;
                Ok(())
            })
            .unwrap();
        let current = first.load().unwrap().get_task(&selected).unwrap().clone();
        let target = date(2030, 5, 4);
        mutate(
            &mut first,
            Mutation::Schedule {
                key: selected.clone(),
                when: When::Scheduled { date: target },
            },
        )
        .unwrap();
        let after = first.load().unwrap().get_task(&selected).unwrap().clone();
        assert_eq!(
            after.notes.as_deref(),
            Some("new notes from another client")
        );
        assert_eq!(after.tags, ["fresh", "tag"]);
        assert_eq!(after.when, When::Scheduled { date: target });
        assert_ne!(after.modified_at, current.modified_at);
        let mut current_json = serde_json::to_value(current).unwrap();
        let mut after_json = serde_json::to_value(after).unwrap();
        for field in ["when", "modified_at"] {
            current_json.as_object_mut().unwrap().remove(field);
            after_json.as_object_mut().unwrap().remove(field);
        }
        assert_eq!(after_json, current_json);
        mutate(&mut first, Mutation::Complete(selected.clone())).unwrap();
        let store = first.load().unwrap();
        assert!(store.get_task(&selected).unwrap().completed_at.is_some());
        assert!(store.get_task(&other).unwrap().completed_at.is_none());
        assert_eq!(
            store.get_task(&other).unwrap().task_number,
            before_selection
        );
    }

    #[test]
    fn invalid_input_and_incomplete_children_cannot_change_storage() {
        let (dir, mut db) = open_db();
        mutate(&mut db, Mutation::Add("parent".into())).unwrap();
        let parent = key_for(&db, "parent");
        db.transaction::<_, Box<dyn std::error::Error>>(|store, storage| {
            let number = store.get_task(&parent).unwrap().task_number;
            add_task(
                store,
                storage,
                AddTaskParameters {
                    title: "child".into(),
                    notes: None,
                    when: When::Inbox,
                    deadline: None,
                    defer_until: None,
                    project: None,
                    area: None,
                    tags: vec![],
                    recurrence: None,
                    parent_task_number: Some(number),
                    estimate_minutes: None,
                },
            )?;
            Ok(())
        })
        .unwrap();
        let before: serde_json::Value = serde_json::from_str(&db.export_json().unwrap()).unwrap();
        let pending = db.pending_changes().unwrap();
        let meta = metadata(&dir);
        assert!(mutate(&mut db, Mutation::Add("  ".into())).is_err());
        assert!(parse_schedule("").is_err());
        assert!(parse_schedule("not a date").is_err());
        assert!(
            mutate(&mut db, Mutation::Complete(parent))
                .unwrap_err()
                .to_string()
                .contains("incomplete subtasks")
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&db.export_json().unwrap()).unwrap(),
            before
        );
        assert_eq!(db.pending_changes().unwrap(), pending);
        assert_eq!(metadata(&dir), meta);
    }

    #[test]
    fn recurring_completion_records_only_one_occurrence() {
        let (dir, mut db) = open_db();
        mutate(&mut db, Mutation::Add("recurring".into())).unwrap();
        let key = key_for(&db, "recurring");
        let today = jiff::Zoned::now().date();
        db.transaction::<_, Box<dyn std::error::Error>>(|store, storage| {
            let stamp = storage.next_modified_at()?;
            let task = store.get_task_mut(&key).unwrap();
            task.recurrence = Some(recurrence(today));
            task.modified_at = stamp;
            Ok(())
        })
        .unwrap();
        mutate(&mut db, Mutation::Complete(key.clone())).unwrap();
        let task = db.load().unwrap().get_task(&key).unwrap().clone();
        assert_eq!(task.completed_occurrences, [today]);
        assert!(task.completed_at.is_none());
        let before = metadata(&dir);
        assert!(mutate(&mut db, Mutation::Complete(key)).is_err());
        assert_eq!(metadata(&dir), before);
    }

    fn buffer_text(terminal: &Terminal<TestBackend>) -> String {
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[test]
    fn renders_tiny_narrow_wide_and_sanitizes_user_text() {
        let mut store = Store::default();
        let mut task = fixture_task("unicode", "長い 🐈 \u{1b}[31m title");
        task.notes = Some("first\nsecond \u{1b}]0;bad\u{7} 日本語".repeat(10));
        store.add_task(task);
        for (width, height) in [(1, 1), (40, 12), (120, 30)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            let mut app = App::new(store.clone());
            app.switch(View::All);
            terminal.draw(|frame| render(frame, &mut app)).unwrap();
            let text = buffer_text(&terminal);
            assert!(!text.contains('\u{1b}'));
            if width >= 40 {
                assert!(text.contains('長'));
                assert!(text.contains("q/Ctrl-C quit"));
                assert!(text.contains("a add Inbox"));
            }
            app.detail = true;
            terminal.draw(|frame| render(frame, &mut app)).unwrap();
            assert!(!buffer_text(&terminal).contains('\u{1b}'));
            app.help = true;
            terminal.draw(|frame| render(frame, &mut app)).unwrap();
            if width >= 40 {
                assert!(buffer_text(&terminal).contains("tdo keys"));
            }
            app.help = false;
            app.input = Some(Input::new(InputKind::Add, "日本語 🐈".repeat(10)));
            terminal.draw(|frame| render(frame, &mut app)).unwrap();
            if width >= 40 {
                assert!(buffer_text(&terminal).contains("Add to Inbox"));
            }
        }
        let mut empty = App::new(Store::default());
        empty.switch(View::Projects);
        let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
        terminal.draw(|frame| render(frame, &mut empty)).unwrap();
        assert!(buffer_text(&terminal).contains("Nothing here"));
    }

    #[test]
    fn classic_stream_groups_sections_subtasks_and_project_preview() {
        let today = jiff::Zoned::now().date();
        let yesterday = today.yesterday().unwrap();
        let tomorrow = today.tomorrow().unwrap();
        let mut store = Store::default();
        store.add_project(Project {
            name: "Apollo".into(),
            ..Project::default()
        });
        let mut parent = fixture_task("parent", "Parent today");
        parent.when = When::Scheduled { date: today };
        parent.project_key = Some("project/apollo".into());
        store.add_task(parent);

        let mut unrelated = fixture_task("unrelated", "Unrelated today");
        unrelated.when = When::Scheduled { date: today };
        store.add_task(unrelated);

        let mut child = fixture_task("child", "Subtask today");
        child.when = When::Scheduled { date: today };
        child.parent_task_key = Some("task/parent".into());
        store.add_task(child);

        let mut overdue = fixture_task("overdue", "Overdue item");
        overdue.when = When::Scheduled { date: yesterday };
        store.add_task(overdue);

        let mut upcoming = fixture_task("upcoming", "Future item");
        upcoming.when = When::Scheduled { date: tomorrow };
        store.add_task(upcoming);

        let mut someday = fixture_task("someday", "Someday item");
        someday.when = When::Someday;
        store.add_task(someday);

        let today_keys = task_keys(&store, &View::Today, "", today);
        assert_eq!(
            today_keys,
            [
                "task/overdue",
                "task/parent",
                "task/child",
                "task/unrelated"
            ]
        );
        assert_eq!(
            task_keys(&store, &View::Upcoming, "", today),
            ["task/upcoming"]
        );
        assert_eq!(
            task_keys(&store, &View::Someday, "", today),
            ["task/someday"]
        );

        let mut app = App::new(store);
        let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        let rendered = buffer_text(&terminal);
        assert!(rendered.contains("Overdue"));
        assert!(rendered.contains("└─"));
        assert!(rendered.contains("Apollo"));

        app.switch(View::Projects);
        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        let projects_rendered = buffer_text(&terminal);
        assert!(projects_rendered.contains("Apollo"));
        assert!(projects_rendered.contains("Parent today"));
        assert!(projects_rendered.contains('│'));

        app.show_inspector = false;
        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        let hidden_inspector = buffer_text(&terminal);
        assert!(hidden_inspector.contains("Apollo"));
        assert!(!hidden_inspector.contains("Parent today"));
        assert!(!hidden_inspector.contains('│'));
    }

    #[test]
    fn tier1_2_3_mutations_undo_filters_and_prompt_editing() {
        let (_dir, mut db) = open_db();
        let mut app = App::new(db.load().unwrap());

        // Tier 1: Add project, add task, assign project, add subtask
        app.apply_mutation(&mut db, Mutation::AddProject("Orion".into()))
            .unwrap();
        assert!(app.store.get_project("project/orion").is_some());

        app.apply_mutation(&mut db, Mutation::Add("Main task".into()))
            .unwrap();
        let main_key = key_for(&db, "Main task");

        app.apply_mutation(
            &mut db,
            Mutation::AssignProject {
                key: main_key.clone(),
                project: Some("orion".into()),
            },
        )
        .unwrap();
        assert_eq!(
            app.store.get_task(&main_key).unwrap().project_key.as_deref(),
            Some("project/orion")
        );

        app.apply_mutation(
            &mut db,
            Mutation::AddSubtask {
                parent_key: main_key.clone(),
                title: "Subtask one".into(),
            },
        )
        .unwrap();
        let sub_key = key_for(&db, "Subtask one");
        assert_eq!(
            app.store.get_task(&sub_key).unwrap().parent_task_key.as_deref(),
            Some(main_key.as_str())
        );

        // Complete subtask, then undo completion, then complete again and reopen via Complete
        app.apply_mutation(&mut db, Mutation::Complete(sub_key.clone()))
            .unwrap();
        assert!(app.store.get_task(&sub_key).unwrap().completed_at.is_some());
        let undo_action = app.last_undo.take().unwrap();
        app.apply_mutation(&mut db, Mutation::Undo(undo_action))
            .unwrap();
        assert!(app.store.get_task(&sub_key).unwrap().completed_at.is_none());

        app.apply_mutation(&mut db, Mutation::Complete(sub_key.clone()))
            .unwrap();
        assert!(app.store.get_task(&sub_key).unwrap().completed_at.is_some());
        app.apply_mutation(&mut db, Mutation::Complete(sub_key.clone()))
            .unwrap();
        assert!(app.store.get_task(&sub_key).unwrap().completed_at.is_none());

        // Delete subtask and undo deletion
        app.apply_mutation(&mut db, Mutation::Delete(sub_key.clone()))
            .unwrap();
        assert!(app.store.get_task(&sub_key).unwrap().deleted_at.is_some());
        let undo_del = app.last_undo.take().unwrap();
        app.apply_mutation(&mut db, Mutation::Undo(undo_del))
            .unwrap();
        assert!(app.store.get_task(&sub_key).unwrap().deleted_at.is_none());

        // Tier 2: View filters (show_completed & ready_only) and section jumping
        app.apply_mutation(&mut db, Mutation::Complete(sub_key.clone()))
            .unwrap();
        app.switch(View::Inbox);
        assert!(!app.rows.contains(&sub_key));
        app.show_completed = true;
        app.rebuild();
        assert!(app.rows.contains(&sub_key));

        // Tier 3: Cursor-aware Input & Tab completion
        let mut input = Input::new(InputKind::Add, "hello world".into());
        assert_eq!(input.cursor, 11);
        input.delete_word_back();
        assert_eq!(input.text, "hello ");
        input.insert_str("saku");
        assert_eq!(input.text, "hello saku");
        input.move_left();
        input.move_left();
        input.backspace();
        assert_eq!(input.text, "hello sku");
        input.move_home();
        assert_eq!(input.cursor, 0);
        input.move_right();
        input.clear_before_cursor();
        assert_eq!(input.text, "ello sku");

        let mut proj_input = Input::new(InputKind::Project(main_key), "ori".into());
        tab_complete_input(&mut proj_input, &app.store);
        assert_eq!(proj_input.text, "Orion");
    }
}

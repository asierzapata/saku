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
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};
use saku_storage::entity::Entity;
use saku_tdo::{
    date_parser::parse_natural_date,
    models::{
        store::Store,
        task::{Task, When, is_pending_on, order_tasks_with_store},
    },
    services::tasks::{
        AddTaskParameters, CompleteTaskParameters, EditTaskError, EditTaskParameters,
        MoveTaskParameters, add_task, complete_task, edit_task, move_task,
    },
    storage::{
        Storage,
        sqlite::{CommandMode, SqliteStorage},
    },
};
use unicode_width::UnicodeWidthStr;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Clone, Debug, PartialEq, Eq)]
enum View {
    Today,
    Inbox,
    Projects,
    Project(String),
    All,
}

enum InputKind {
    Add,
    Schedule(String),
    Search,
}

struct Input {
    kind: InputKind,
    text: String,
}

enum Mutation {
    Add(String),
    Complete(String),
    Schedule { key: String, when: When },
}

fn mutate(db: &mut SqliteStorage, mutation: Mutation) -> Result<String> {
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
            Ok(format!("Added #{} to Inbox", task.task_number))
        }
        Mutation::Complete(key) => {
            let current = available_task(store, &key)?;
            if current.recurrence.is_some()
                && current
                    .completed_occurrences
                    .contains(&jiff::Zoned::now().date())
            {
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
            Ok(format!("Completed #{number}"))
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
            Ok(format!("Scheduled #{}", current.task_number))
        }
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

fn task_keys(store: &Store, view: &View, query: &str, today: Date) -> Vec<String> {
    let query = query.to_lowercase();
    let tasks = store
        .get_active_tasks()
        .filter(|task| {
            if task.completed_at.is_some() {
                return false;
            }
            let visible = match view {
                View::Today => {
                    task.defer_until.is_none_or(|date| date <= today)
                        && if task.recurrence.is_some() {
                            is_pending_on(task, today)
                        } else {
                            matches!(task.when, When::Scheduled { date } if date <= today)
                                || task.deadline.is_some_and(|date| date <= today)
                        }
                }
                View::Inbox => {
                    matches!(task.when, When::Inbox)
                        && task.defer_until.is_none_or(|date| date <= today)
                }
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
                        .is_some_and(|project| project.name.to_lowercase().contains(&query)))
        })
        .collect();
    order_tasks_with_store(tasks, store)
        .into_iter()
        .map(Entity::storage_key)
        .collect()
}

struct App {
    store: Store,
    view: View,
    list: ListState,
    rows: Vec<String>,
    input: Option<Input>,
    query: String,
    status: String,
    help: bool,
    detail: bool,
    detail_scroll: u16,
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
            help: false,
            detail: false,
            detail_scroll: 0,
            page_size: 1,
        };
        app.rebuild();
        app
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
            self.status = "Project is no longer available".into();
            self.detail = false;
        }
        self.rows = if self.view == View::Projects {
            let mut projects: Vec<_> = self
                .store
                .get_active_projects()
                .filter(|project| project.completed_at.is_none())
                .collect();
            projects.sort_by(|a, b| {
                a.name
                    .to_lowercase()
                    .cmp(&b.name.to_lowercase())
                    .then_with(|| a.storage_key().cmp(&b.storage_key()))
            });
            projects.into_iter().map(Entity::storage_key).collect()
        } else {
            task_keys(
                &self.store,
                &self.view,
                &self.query,
                jiff::Zoned::now().date(),
            )
        };
        let index = previous
            .and_then(|key| self.rows.iter().position(|row| *row == key))
            .or_else(|| (!self.rows.is_empty()).then(|| previous_index.min(self.rows.len() - 1)));
        self.list.select(index);
        if self.rows.is_empty() {
            self.detail = false;
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
        self.rebuild();
    }

    fn select_delta(&mut self, delta: isize) {
        if self.rows.is_empty() {
            self.list.select(None);
            return;
        }
        let i = self.list.selected().unwrap_or(0);
        self.list.select(Some(
            i.saturating_add_signed(delta).min(self.rows.len() - 1),
        ));
    }

    fn task_key(&self) -> Option<String> {
        self.selected()
            .filter(|key| self.store.get_task(key).is_some())
            .map(str::to_owned)
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
                    input.text.extend(text.chars().filter_map(|c| {
                        if c.is_whitespace() {
                            Some(' ')
                        } else if c.is_control() {
                            None
                        } else {
                            Some(c)
                        }
                    }));
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
        match key.code {
            KeyCode::Esc => app.input = None,
            KeyCode::Backspace => {
                input.text.pop();
            }
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                    && !c.is_control() =>
            {
                input.text.push(c)
            }
            KeyCode::Enter if key.kind == KeyEventKind::Press => {
                let input = app.input.take().unwrap();
                let result = match &input.kind {
                    InputKind::Add => mutate(db, Mutation::Add(input.text.clone())),
                    InputKind::Schedule(key) => parse_schedule(&input.text).and_then(|when| {
                        mutate(
                            db,
                            Mutation::Schedule {
                                key: key.clone(),
                                when,
                            },
                        )
                    }),
                    InputKind::Search => {
                        app.view = View::All;
                        app.query = input.text.trim().to_string();
                        app.list.select(None);
                        app.rows.clear();
                        app.detail = false;
                        app.detail_scroll = 0;
                        Ok("Search applied".into())
                    }
                };
                match result {
                    Ok(status) => {
                        app.status = status;
                        app.reload(db)?;
                    }
                    Err(error) => {
                        app.status = error.to_string();
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
    match key.code {
        KeyCode::Char('q') => return Ok(true),
        KeyCode::Char('?') => app.help = true,
        KeyCode::Char('1') => app.switch(View::Today),
        KeyCode::Char('2') => app.switch(View::Inbox),
        KeyCode::Char('3') => app.switch(View::Projects),
        KeyCode::Char('4') => app.switch(View::All),
        KeyCode::Tab => app.switch(match app.view {
            View::Today => View::Inbox,
            View::Inbox => View::Projects,
            View::Projects | View::Project(_) => View::All,
            View::All => View::Today,
        }),
        KeyCode::Down | KeyCode::Char('j') => app.select_delta(1),
        KeyCode::Up | KeyCode::Char('k') => app.select_delta(-1),
        KeyCode::Home => {
            if !app.rows.is_empty() {
                app.list.select(Some(0));
            }
        }
        KeyCode::End => {
            if !app.rows.is_empty() {
                app.list.select(Some(app.rows.len() - 1));
            }
        }
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
            app.status = "Refreshed".into();
        }
        KeyCode::Char('a') if key.kind == KeyEventKind::Press => {
            app.input = Some(Input {
                kind: InputKind::Add,
                text: String::new(),
            });
        }
        KeyCode::Char('/') if key.kind == KeyEventKind::Press => {
            app.input = Some(Input {
                kind: InputKind::Search,
                text: app.query.clone(),
            });
        }
        KeyCode::Char('s') if key.kind == KeyEventKind::Press => {
            if let Some(key) = app.task_key() {
                app.input = Some(Input {
                    kind: InputKind::Schedule(key),
                    text: String::new(),
                });
            }
        }
        KeyCode::Char('x' | ' ') if key.kind == KeyEventKind::Press => {
            if let Some(key) = app.task_key() {
                app.status = match mutate(db, Mutation::Complete(key)) {
                    Ok(status) => status,
                    Err(error) => error.to_string(),
                };
                app.reload(db)?;
            }
        }
        KeyCode::Char('e') if key.kind == KeyEventKind::Press => {
            if let Some(key) = app.task_key() {
                guard.suspend()?;
                let result = edit_selected(db, &key);
                guard.resume(terminal)?;
                app.status = match result {
                    Ok(status) => status,
                    Err(error) => error.to_string(),
                };
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

fn task_line(task: &Task, store: &Store) -> Line<'static> {
    let title = format!("#{} {}", task.task_number, clean(&task.title, false));
    let mut text = String::new();
    if store.is_task_blocked(task) {
        text.push_str(" [blocked]");
    }
    if task.recurrence.is_some() {
        text.push_str(" [recurring]");
    }
    if let Some(parent) = task
        .parent_task_key
        .as_deref()
        .and_then(|key| store.get_task(key))
    {
        text.push_str(&format!(" (subtask of #{})", parent.task_number));
    }
    text.push_str(&format!(" · {}", schedule(&task.when)));
    if let Some(deadline) = task.deadline {
        text.push_str(&format!(" · due {deadline}"));
    }
    Line::from(vec![
        Span::raw(title),
        Span::styled(text, Style::default().fg(Color::DarkGray)),
    ])
}

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
    frame.render_widget(
        Paragraph::new(vec![
            Line::from("tdo  1:Today 2:Inbox 3:Projects 4:All"),
            Line::from(clean(&title(app), false)),
        ]),
        rows[0],
    );
    let body = rows[1];
    app.page_size = usize::from(body.height.saturating_sub(2).max(1));
    let panels = if body.width >= 90 {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(body)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(100)])
            .split(body)
    };
    let items: Vec<ListItem<'_>> = app
        .rows
        .iter()
        .filter_map(|key| {
            if app.view == View::Projects {
                app.store
                    .get_project(key)
                    .map(|project| ListItem::new(clean(&project.name, false)))
            } else {
                app.store
                    .get_task(key)
                    .map(|task| ListItem::new(task_line(task, &app.store)))
            }
        })
        .collect();
    let empty = items.is_empty();
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title(app)))
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("› ");
    frame.render_stateful_widget(list, panels[0], &mut app.list);
    if empty && panels[0].width > 2 && panels[0].height > 2 {
        let inner = Rect::new(
            panels[0].x + 1,
            panels[0].y + 1,
            panels[0].width - 2,
            panels[0].height - 2,
        );
        frame.render_widget(
            Paragraph::new("Nothing here. Press a to add to Inbox.")
                .style(Style::default().fg(Color::DarkGray))
                .wrap(Wrap { trim: false }),
            inner,
        );
    }
    if panels.len() > 1 {
        let text = app
            .task_key()
            .and_then(|key| app.store.get_task(&key))
            .map(|task| detail_text(task, &app.store))
            .unwrap_or_default();
        frame.render_widget(
            Paragraph::new(text)
                .block(Block::default().borders(Borders::ALL).title("Details"))
                .wrap(Wrap { trim: false }),
            panels[1],
        );
    }
    if app.detail {
        let text = app
            .task_key()
            .and_then(|key| app.store.get_task(&key))
            .map(|task| detail_text(task, &app.store))
            .unwrap_or_default();
        frame.render_widget(Clear, body);
        frame.render_widget(
            Paragraph::new(text)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Details · Enter/Esc close"),
                )
                .wrap(Wrap { trim: false })
                .scroll((app.detail_scroll, 0)),
            body,
        );
    }
    let footer = if let Some(input) = &app.input {
        let label = match input.kind {
            InputKind::Add => "Add to Inbox",
            InputKind::Schedule(_) => "Schedule",
            InputKind::Search => "Search",
        };
        let label = format!("{label}: ");
        let width =
            usize::from(rows[2].width).saturating_sub(UnicodeWidthStr::width(label.as_str()) + 1);
        let visible = prompt_tail(&clean(&input.text, false), width);
        let cursor =
            UnicodeWidthStr::width(label.as_str()) + UnicodeWidthStr::width(visible.as_str());
        if rows[2].width > 0 && rows[2].height > 0 {
            frame.set_cursor_position((
                rows[2].x + (cursor.min(usize::from(rows[2].width - 1)) as u16),
                rows[2].y,
            ));
        }
        vec![
            Line::from(format!("{label}{visible}")),
            Line::from(clean(&app.status, false)),
            Line::from("Enter accept  Esc cancel  Ctrl-C quit"),
        ]
    } else {
        vec![
            Line::from("q/Ctrl-C quit  a add Inbox  x done"),
            Line::from("s schedule  e edit  / search  ? help"),
            Line::from(clean(&app.status, false)).style(Style::default().fg(Color::DarkGray)),
        ]
    };
    frame.render_widget(Paragraph::new(footer).wrap(Wrap { trim: false }), rows[2]);
    if app.help {
        frame.render_widget(Clear, area);
        let help = Text::from(
            "tdo keys\n\n1 Today · 2 Inbox · 3 Projects · 4 All · Tab next\nj/k, arrows, Home/End, PgUp/PgDown: select or scroll\nEnter: open project/details · Esc: close/back/clear search\na: add to Inbox · x/Space: complete · s: schedule\ne: external editor · /: search · r: refresh\n? or Esc: close help · q or Ctrl-C: quit\n\nCapture always goes to Inbox; subtasks are selectable.",
        );
        frame.render_widget(
            Paragraph::new(help)
                .block(Block::default().borders(Borders::ALL).title("Help"))
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
            app.input = Some(Input {
                kind: InputKind::Add,
                text: "日本語 🐈".repeat(10),
            });
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
}

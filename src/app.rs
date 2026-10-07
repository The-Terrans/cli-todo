use crate::{
    db::{self, Filter, Project, Task},
    history::{Commit, History, SyncResult},
    Result,
};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use rusqlite::Connection;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

pub const FILTERS: [(&str, Filter); 3] = [
    ("All", Filter::All),
    ("Pending", Filter::Pending),
    ("Completed", Filter::Completed),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Add,
    Edit,
    ToggleCompletion,
    Delete,
    Move,
    ShowFilter(usize),
    OpenTask(i64),
    Quit,
}

const ACTIONS: [(&str, Action); 9] = [
    ("Add task", Action::Add),
    ("Edit selected task", Action::Edit),
    ("Complete / reopen selected task", Action::ToggleCompletion),
    ("Delete selected task", Action::Delete),
    ("Move selected task", Action::Move),
    ("Show all tasks", Action::ShowFilter(0)),
    ("Show pending tasks", Action::ShowFilter(1)),
    ("Show completed tasks", Action::ShowFilter(2)),
    ("Quit", Action::Quit),
];

pub enum Mode {
    Browse,
    Edit(Option<i64>, String),
    Delete(i64),
    Palette { query: String, selected: usize },
    ProjectEdit(Option<i64>, String),
    ProjectDelete(i64),
    CommitEdit(String),
    RemoteEdit(String),
    Move { task: i64, selected: usize },
}

pub struct App {
    db: Connection,
    palette_tasks: Vec<Task>,
    pub tasks: Vec<Task>,
    pub projects: Vec<Project>,
    pub selected_project: usize,
    pub current_project: Option<i64>,
    pub projects_focused: bool,
    pub commits_focused: bool,
    pub uncommitted_changes: bool,
    pub remote_url: String,
    sync_receiver: Option<Receiver<std::result::Result<SyncResult, String>>>,
    history: Option<History>,
    pub commits: Vec<Commit>,
    pub selected_commit: usize,
    pub commit_details: String,
    pub detail_scroll: u16,
    pub selected_task: usize,
    pub navigation_focused: bool,
    pub selected_filter: usize,
    pub mode: Mode,
    pub message: String,
}

impl App {
    pub fn new(db: Connection) -> Result<Self> {
        let mut app = Self {
            palette_tasks: vec![],
            tasks: vec![],
            projects: db::list_projects(&db)?,
            selected_project: 0,
            current_project: None,
            projects_focused: false,
            commits_focused: false,
            uncommitted_changes: false,
            remote_url: String::new(),
            sync_receiver: None,
            history: History::new(&db),
            commits: vec![],
            selected_commit: 0,
            commit_details: String::new(),
            detail_scroll: 0,
            selected_task: 0,
            navigation_focused: true,
            selected_filter: 0,
            mode: Mode::Browse,
            message: String::new(),
            db,
        };
        app.refresh_tasks()?;
        app.reload_commits();
        app.load_remote();
        app.update_commit_status();
        Ok(app)
    }

    fn refresh_tasks(&mut self) -> Result<()> {
        self.tasks = if self.projects_focused && self.current_project.is_none() {
            vec![]
        } else {
            db::list_tasks_in(
                &self.db,
                FILTERS[self.selected_filter].1,
                self.current_project,
            )?
        };
        self.selected_task = self.selected_task.min(self.tasks.len().saturating_sub(1));
        Ok(())
    }

    /// Returns true when the user requests quit.
    pub fn handle_key_event(&mut self, key: KeyEvent) -> Result<bool> {
        if key.kind != KeyEventKind::Press {
            return Ok(false);
        }
        let modifiers = key.modifiers.difference(KeyModifiers::SHIFT);
        if modifiers == KeyModifiers::CONTROL && matches!(key.code, KeyCode::Char('k' | 'K')) {
            self.toggle_palette()?;
            return Ok(false);
        }
        if !modifiers.is_empty() {
            return Ok(false);
        }
        self.handle_key(key.code)
    }

    fn handle_key(&mut self, key: KeyCode) -> Result<bool> {
        let changes = self.db.total_changes();
        let result = self.dispatch_key(key);
        if self.db.total_changes() != changes {
            self.update_commit_status();
        }
        result
    }

    fn dispatch_key(&mut self, key: KeyCode) -> Result<bool> {
        self.message.clear();
        if key == KeyCode::Char('q')
            && self.sync_receiver.is_some()
            && matches!(self.mode, Mode::Browse | Mode::Delete(_))
        {
            self.message = "Sync is in progress; wait before quitting".into();
            return Ok(false);
        }
        if key == KeyCode::Char('q') && matches!(self.mode, Mode::Browse | Mode::Delete(_)) {
            return self.execute_action(Action::Quit);
        }
        match self.mode {
            Mode::Browse => self.handle_browse_key(key)?,
            Mode::Edit(..) => self.handle_edit_key(key)?,
            Mode::Delete(id) => self.handle_delete_key(key, id)?,
            Mode::Palette { .. } => return self.handle_palette_key(key),
            Mode::ProjectEdit(..) => self.handle_project_edit_key(key)?,
            Mode::ProjectDelete(id) => self.handle_project_delete_key(key, id)?,
            Mode::CommitEdit(_) => self.handle_commit_edit_key(key)?,
            Mode::RemoteEdit(_) => self.handle_remote_edit_key(key),
            Mode::Move { .. } => self.handle_move_key(key)?,
        }
        Ok(false)
    }

    fn handle_browse_key(&mut self, key: KeyCode) -> Result<()> {
        match key {
            KeyCode::Char('0') => self.navigation_focused = false,
            KeyCode::Char('1') => self.focus_inbox()?,
            KeyCode::Left | KeyCode::Right if self.navigation_focused => self.cycle_section(key)?,
            KeyCode::Char('2') => self.focus_projects()?,
            KeyCode::Char('3') => self.focus_commits(),
            KeyCode::Esc => self.navigation_focused = true,
            _ if self.commits_focused => self.handle_commit_key(key),
            _ if self.navigation_focused && self.projects_focused => {
                self.handle_project_key(key)?
            }
            KeyCode::Char('a') => {
                self.execute_action(Action::Add)?;
            }
            _ if self.navigation_focused => self.handle_navigation_key(key)?,
            _ => self.handle_task_key(key)?,
        }
        Ok(())
    }

    fn focus_inbox(&mut self) -> Result<()> {
        self.navigation_focused = true;
        self.projects_focused = false;
        self.commits_focused = false;
        self.current_project = None;
        self.refresh_tasks()
    }

    fn focus_projects(&mut self) -> Result<()> {
        self.navigation_focused = true;
        self.projects_focused = true;
        self.commits_focused = false;
        self.preview_project()
    }

    fn cycle_section(&mut self, key: KeyCode) -> Result<()> {
        let section = if self.commits_focused {
            2
        } else if self.projects_focused {
            1
        } else {
            0
        };
        let next = (section + if key == KeyCode::Right { 1 } else { 2 }) % 3;
        match next {
            0 => self.focus_inbox()?,
            1 => self.focus_projects()?,
            _ => self.focus_commits(),
        }
        Ok(())
    }

    fn focus_commits(&mut self) {
        self.navigation_focused = true;
        self.projects_focused = false;
        self.commits_focused = true;
        self.reload_commits();
        self.update_commit_status();
    }

    fn update_commit_status(&mut self) -> bool {
        let result = match &self.history {
            Some(history) => history.has_changes(&self.db),
            None => Ok(true),
        };
        match result {
            Ok(changed) => {
                self.uncommitted_changes = changed;
                true
            }
            Err(error) => {
                self.uncommitted_changes = true;
                self.message = format!("Could not check todo changes: {error}");
                false
            }
        }
    }

    fn begin_commit(&mut self) {
        if self.sync_receiver.is_some() {
            self.message = "Wait for sync to finish before committing".into();
            return;
        }
        if !self.update_commit_status() {
            return;
        }
        if self.uncommitted_changes {
            self.mode = Mode::CommitEdit(String::new());
        } else {
            self.message = "No todo changes to commit".into();
        }
    }

    fn reload_commits(&mut self) {
        let result = self
            .history
            .as_ref()
            .ok_or("Todo history requires a file-backed database")
            .map_err(|error| -> Box<dyn std::error::Error> { error.into() })
            .and_then(History::list);
        match result {
            Ok(commits) => {
                self.commits = commits;
                self.selected_commit = self
                    .selected_commit
                    .min(self.commits.len().saturating_sub(1));
                self.preview_commit();
            }
            Err(error) => self.commit_details = format!("Could not load todo history: {error}"),
        }
    }

    fn preview_commit(&mut self) {
        self.detail_scroll = 0;
        self.commit_details = match (
            self.history.as_ref(),
            self.commits.get(self.selected_commit),
        ) {
            (Some(history), Some(commit)) => history
                .details(&commit.hash)
                .unwrap_or_else(|error| format!("Could not read commit: {error}")),
            _ => "No todo commits yet. Press c in [3] Commits to create a SQLite snapshot.".into(),
        };
    }

    fn handle_commit_key(&mut self, key: KeyCode) {
        if !self.navigation_focused {
            match key {
                KeyCode::Up => self.detail_scroll = self.detail_scroll.saturating_sub(1),
                KeyCode::Down => {
                    self.detail_scroll = (self.detail_scroll + 1).min(
                        self.commit_details
                            .lines()
                            .count()
                            .saturating_sub(1)
                            .min(u16::MAX as usize) as u16,
                    )
                }
                _ => {}
            }
            return;
        }
        match key {
            KeyCode::Char('c') => self.begin_commit(),
            KeyCode::Char('r') => self.begin_remote_edit(),
            KeyCode::Char('p') => self.start_sync(false),
            KeyCode::Char('P') => self.start_sync(true),
            KeyCode::Up => {
                self.selected_commit = self.selected_commit.saturating_sub(1);
                self.preview_commit();
            }
            KeyCode::Down => {
                self.selected_commit =
                    (self.selected_commit + 1).min(self.commits.len().saturating_sub(1));
                self.preview_commit();
            }
            KeyCode::Enter => self.navigation_focused = false,
            _ => {}
        }
    }

    fn load_remote(&mut self) {
        if let Some(history) = &self.history {
            match history.remote() {
                Ok(url) => self.remote_url = url,
                Err(error) => self.message = format!("Could not read origin: {error}"),
            }
        }
    }

    fn begin_remote_edit(&mut self) {
        if self.sync_receiver.is_some() {
            self.message = "Wait for sync to finish before changing origin".into();
            return;
        }
        self.load_remote();
        self.mode = Mode::RemoteEdit(self.remote_url.clone());
    }

    fn handle_remote_edit_key(&mut self, key: KeyCode) {
        let Mode::RemoteEdit(text) = &mut self.mode else {
            return;
        };
        match key {
            KeyCode::Esc => self.mode = Mode::Browse,
            KeyCode::Backspace => {
                text.pop();
            }
            KeyCode::Char(character) => text.push(character),
            KeyCode::Enter => self.save_remote(),
            _ => {}
        }
    }

    fn save_remote(&mut self) {
        let Mode::RemoteEdit(url) = &self.mode else {
            return;
        };
        let result = match &self.history {
            Some(history) => history.set_remote(url),
            None => Err("Todo history requires a file-backed database".into()),
        };
        match result {
            Ok(()) => {
                self.mode = Mode::Browse;
                self.load_remote();
                self.message = if self.remote_url.is_empty() {
                    "Origin removed"
                } else {
                    "Origin configured"
                }
                .into();
            }
            Err(error) => self.message = error.to_string(),
        }
    }

    fn start_sync(&mut self, pull: bool) {
        if self.sync_receiver.is_some() {
            self.message = "Sync is already in progress".into();
            return;
        }
        if pull {
            if !self.update_commit_status() {
                return;
            }
            if self.uncommitted_changes {
                self.message = "Commit your todo changes before pulling".into();
                return;
            }
        }
        let Some(history) = self.history.clone() else {
            self.message = "Todo history requires a file-backed database".into();
            return;
        };
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let result = if pull {
                history.fetch_pull()
            } else {
                history.push()
            };
            let _ = sender.send(result.map_err(|error| error.to_string()));
        });
        self.sync_receiver = Some(receiver);
        self.message = if pull {
            "Pulling todo checkpoints…"
        } else {
            "Pushing todo checkpoints…"
        }
        .into();
    }

    pub fn poll_sync(&mut self) {
        let Some(receiver) = &self.sync_receiver else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("Sync worker stopped unexpectedly".into()),
        };
        self.sync_receiver = None;
        let result = result
            .map_err(|error| -> Box<dyn std::error::Error> { error.into() })
            .and_then(|result| self.finish_sync(result));
        self.message = match result {
            Ok(message) => message,
            Err(error) => {
                self.reload_commits();
                self.update_commit_status();
                format!("Sync failed: {error}")
            }
        };
    }

    fn finish_sync(&mut self, result: SyncResult) -> Result<String> {
        let message = match result {
            SyncResult::Message(message) => message,
            SyncResult::Pull(plan) => {
                if !matches!(self.mode, Mode::Browse) {
                    return Err(
                        "Finish or cancel the open dialog before applying a pull; retry afterward"
                            .into(),
                    );
                }
                let history = self.history.as_ref().ok_or("Todo history unavailable")?;
                let message = history.apply_pull(&mut self.db, plan)?;
                self.projects = db::list_projects(&self.db)?;
                if !self
                    .projects
                    .iter()
                    .any(|project| Some(project.id) == self.current_project)
                {
                    self.current_project = None;
                }
                self.refresh_projects()?;
                message
            }
        };
        self.reload_commits();
        self.load_remote();
        self.update_commit_status();
        Ok(message)
    }

    fn handle_commit_edit_key(&mut self, key: KeyCode) -> Result<()> {
        let Mode::CommitEdit(text) = &mut self.mode else {
            return Ok(());
        };
        match key {
            KeyCode::Esc => self.mode = Mode::Browse,
            KeyCode::Backspace => {
                text.pop();
            }
            KeyCode::Char(character) => text.push(character),
            KeyCode::Enter => self.save_commit(),
            _ => {}
        }
        Ok(())
    }

    fn save_commit(&mut self) {
        let Mode::CommitEdit(message) = &self.mode else {
            return;
        };
        let result = match &self.history {
            Some(history) => history.commit(&self.db, message),
            None => Err("Todo history requires a file-backed database".into()),
        };
        match result {
            Ok(message) => {
                self.mode = Mode::Browse;
                self.selected_commit = 0;
                self.reload_commits();
                if self.update_commit_status() {
                    self.message = message;
                }
            }
            Err(error) => self.message = error.to_string(),
        }
    }

    fn preview_project(&mut self) -> Result<()> {
        self.current_project = self
            .projects
            .get(self.selected_project)
            .map(|project| project.id);
        self.select_filter(0)
    }

    fn handle_navigation_key(&mut self, key: KeyCode) -> Result<()> {
        let filter = match key {
            KeyCode::Down => (self.selected_filter + 1) % FILTERS.len(),
            KeyCode::Up => (self.selected_filter + FILTERS.len() - 1) % FILTERS.len(),
            KeyCode::Enter => {
                self.navigation_focused = false;
                return Ok(());
            }
            _ => return Ok(()),
        };
        self.current_project = None;
        self.select_filter(filter)
    }

    fn select_filter(&mut self, filter: usize) -> Result<()> {
        self.selected_filter = filter;
        self.selected_task = 0;
        self.refresh_tasks()
    }

    fn handle_task_key(&mut self, key: KeyCode) -> Result<()> {
        let action = match key {
            KeyCode::Down => {
                self.selected_task =
                    (self.selected_task + 1).min(self.tasks.len().saturating_sub(1));
                return Ok(());
            }
            KeyCode::Up => {
                self.selected_task = self.selected_task.saturating_sub(1);
                return Ok(());
            }
            KeyCode::Enter | KeyCode::Char('e') => Action::Edit,
            KeyCode::Char('d') => Action::Delete,
            KeyCode::Char('m') => Action::Move,
            KeyCode::Char(' ') => Action::ToggleCompletion,
            _ => return Ok(()),
        };
        self.execute_action(action)?;
        Ok(())
    }

    fn execute_action(&mut self, action: Action) -> Result<bool> {
        match action {
            Action::Add => {
                if self.commits_focused {
                    self.focus_inbox()?;
                }
                self.mode = Mode::Edit(None, String::new());
            }
            Action::Edit => {
                if let Some(task) = self.tasks.get(self.selected_task) {
                    self.mode = Mode::Edit(Some(task.id), task.title.clone());
                }
            }
            Action::Delete => {
                if let Some(task) = self.tasks.get(self.selected_task) {
                    self.mode = Mode::Delete(task.id);
                }
            }
            Action::ToggleCompletion => {
                if let Some(task) = self.tasks.get(self.selected_task) {
                    db::toggle_task(&self.db, task.id)?;
                    self.refresh_tasks()?;
                }
            }
            Action::Move => {
                if let Some(task) = self.tasks.get(self.selected_task) {
                    self.mode = Mode::Move {
                        task: task.id,
                        selected: 0,
                    };
                }
            }
            Action::ShowFilter(filter) => {
                if self.commits_focused {
                    self.focus_inbox()?;
                }
                self.select_filter(filter)?;
                self.navigation_focused = false;
            }
            Action::OpenTask(id) => {
                self.commits_focused = false;
                if let Some(task) = self.palette_tasks.iter().find(|task| task.id == id) {
                    self.current_project = task.project_id;
                    self.projects_focused = task.project_id.is_some();
                    if let Some(index) = self
                        .projects
                        .iter()
                        .position(|project| Some(project.id) == task.project_id)
                    {
                        self.selected_project = index;
                    }
                }
                self.select_filter(0)?;
                if let Some(index) = self.tasks.iter().position(|task| task.id == id) {
                    self.selected_task = index;
                }
                self.navigation_focused = false;
            }
            Action::Quit => return Ok(true),
        }
        Ok(false)
    }

    fn toggle_palette(&mut self) -> Result<()> {
        match self.mode {
            Mode::Browse => {
                self.palette_tasks = db::list_tasks(&self.db, Filter::All)?;
                self.mode = Mode::Palette {
                    query: String::new(),
                    selected: 0,
                }
            }
            Mode::Palette { .. } => self.mode = Mode::Browse,
            _ => {}
        }
        Ok(())
    }

    pub fn palette_actions(&self) -> Vec<(&'static str, Action)> {
        let Mode::Palette { query, .. } = &self.mode else {
            return vec![];
        };
        let query = query.trim().to_lowercase();
        let has_task = !self.commits_focused && self.tasks.get(self.selected_task).is_some();
        ACTIONS
            .iter()
            .copied()
            .filter(|(label, action)| {
                (has_task || matches!(action, Action::Add | Action::ShowFilter(_) | Action::Quit))
                    && label.to_lowercase().contains(&query)
            })
            .collect()
    }

    pub fn palette_results(&self) -> Vec<(String, Action)> {
        let Mode::Palette { query, .. } = &self.mode else {
            return vec![];
        };
        let query = query.trim().to_lowercase();
        let mut results: Vec<_> = self
            .palette_actions()
            .into_iter()
            .map(|(label, action)| (format!("Action: {label}"), action))
            .collect();
        results.extend(
            self.palette_tasks
                .iter()
                .filter(|task| task.title.to_lowercase().contains(&query))
                .map(|task| {
                    (
                        format!(
                            "Task: [{}] {}",
                            if task.done { "x" } else { " " },
                            task.title
                        ),
                        Action::OpenTask(task.id),
                    )
                }),
        );
        results
    }

    fn handle_palette_key(&mut self, key: KeyCode) -> Result<bool> {
        let actions = self.palette_results();
        let Mode::Palette { query, selected } = &mut self.mode else {
            return Ok(false);
        };
        match key {
            KeyCode::Esc => self.mode = Mode::Browse,
            KeyCode::Char(character) => {
                query.push(character);
                *selected = 0;
            }
            KeyCode::Backspace => {
                query.pop();
                *selected = 0;
            }
            KeyCode::Up => *selected = selected.saturating_sub(1),
            KeyCode::Down => *selected = (*selected + 1).min(actions.len().saturating_sub(1)),
            KeyCode::Enter => {
                if let Some((_, action)) = actions.get(*selected) {
                    self.mode = Mode::Browse;
                    return self.execute_action(*action);
                }
            }
            _ => {}
        }
        Ok(false)
    }

    fn handle_edit_key(&mut self, key: KeyCode) -> Result<()> {
        let Mode::Edit(_, text) = &mut self.mode else {
            return Ok(());
        };
        match key {
            KeyCode::Esc => self.mode = Mode::Browse,
            KeyCode::Backspace => {
                text.pop();
            }
            KeyCode::Char(character) => text.push(character),
            KeyCode::Enter => self.save_edit()?,
            _ => {}
        }
        Ok(())
    }

    fn save_edit(&mut self) -> Result<()> {
        let Mode::Edit(id, text) = &self.mode else {
            return Ok(());
        };
        if text.trim().is_empty() {
            self.message = "Title cannot be empty".into();
            return Ok(());
        }
        if id.is_some() {
            db::save_task(&self.db, *id, text)?;
        } else {
            db::create_task(&self.db, text, self.current_project)?;
        }
        self.mode = Mode::Browse;
        self.refresh_tasks()
    }

    fn refresh_projects(&mut self) -> Result<()> {
        self.projects = db::list_projects(&self.db)?;
        self.selected_project = self
            .selected_project
            .min(self.projects.len().saturating_sub(1));
        if self.projects_focused {
            self.preview_project()
        } else {
            self.refresh_tasks()
        }
    }

    fn handle_project_key(&mut self, key: KeyCode) -> Result<()> {
        match key {
            KeyCode::Down => {
                self.selected_project =
                    (self.selected_project + 1).min(self.projects.len().saturating_sub(1));
                self.preview_project()?;
            }
            KeyCode::Up => {
                self.selected_project = self.selected_project.saturating_sub(1);
                self.preview_project()?;
            }
            KeyCode::Char('a') => self.mode = Mode::ProjectEdit(None, String::new()),
            KeyCode::Char('e') => {
                if let Some(project) = self.projects.get(self.selected_project) {
                    self.mode = Mode::ProjectEdit(Some(project.id), project.name.clone());
                }
            }
            KeyCode::Char('d') => {
                if let Some(project) = self.projects.get(self.selected_project) {
                    self.mode = Mode::ProjectDelete(project.id);
                }
            }
            KeyCode::Enter => {
                if let Some(project) = self.projects.get(self.selected_project) {
                    self.current_project = Some(project.id);
                    self.select_filter(0)?;
                    self.navigation_focused = false;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_project_edit_key(&mut self, key: KeyCode) -> Result<()> {
        let Mode::ProjectEdit(_, text) = &mut self.mode else {
            return Ok(());
        };
        match key {
            KeyCode::Esc => self.mode = Mode::Browse,
            KeyCode::Backspace => {
                text.pop();
            }
            KeyCode::Char(character) => text.push(character),
            KeyCode::Enter => self.save_project_edit()?,
            _ => {}
        }
        Ok(())
    }

    fn save_project_edit(&mut self) -> Result<()> {
        let Mode::ProjectEdit(id, name) = &self.mode else {
            return Ok(());
        };
        if let Err(error) = db::save_project(&self.db, *id, name) {
            self.message = error.to_string();
            return Ok(());
        }
        self.mode = Mode::Browse;
        self.refresh_projects()
    }

    fn handle_project_delete_key(&mut self, key: KeyCode, id: i64) -> Result<()> {
        match key {
            KeyCode::Enter | KeyCode::Char('y') => {
                db::delete_project(&self.db, id)?;
                if self.current_project == Some(id) {
                    self.current_project = None;
                }
                self.mode = Mode::Browse;
                self.refresh_projects()?;
            }
            KeyCode::Esc | KeyCode::Char('n') => self.mode = Mode::Browse,
            _ => {}
        }
        Ok(())
    }

    fn handle_move_key(&mut self, key: KeyCode) -> Result<()> {
        let Mode::Move { task, selected } = &mut self.mode else {
            return Ok(());
        };
        match key {
            KeyCode::Esc => self.mode = Mode::Browse,
            KeyCode::Up => *selected = selected.saturating_sub(1),
            KeyCode::Down => *selected = (*selected + 1).min(self.projects.len()),
            KeyCode::Enter => {
                let project = selected
                    .checked_sub(1)
                    .and_then(|index| self.projects.get(index))
                    .map(|p| p.id);
                db::move_task(&self.db, *task, project)?;
                self.mode = Mode::Browse;
                self.refresh_tasks()?;
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_delete_key(&mut self, key: KeyCode, id: i64) -> Result<()> {
        match key {
            KeyCode::Char('y') | KeyCode::Enter => {
                db::delete_task(&self.db, id)?;
                self.mode = Mode::Browse;
                self.refresh_tasks()?;
            }
            KeyCode::Esc | KeyCode::Char('n') => self.mode = Mode::Browse,
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn app() -> Result<App> {
        App::new(db::open(Path::new(":memory:"))?)
    }

    fn search_actions(app: &mut App, query: &str) -> Result<()> {
        app.handle_key_event(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL))?;
        assert!(matches!(app.mode, Mode::Palette { .. }));
        for character in query.chars() {
            assert!(
                !app.handle_key_event(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE))?
            );
        }
        Ok(())
    }

    fn wait_for_sync(app: &mut App) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while app.sync_receiver.is_some() {
            assert!(std::time::Instant::now() < deadline, "sync timed out");
            app.poll_sync();
            thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    #[test]
    fn remote_controls_sync_in_background_and_block_unsafe_pulls() -> Result<()> {
        use std::{env, fs, process::Command, time::SystemTime};
        let stamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_nanos();
        let root = env::temp_dir().join(format!("cli-todo-app-sync-{stamp}"));
        fs::create_dir_all(&root)?;
        let remote = root.join("remote.git");
        assert!(Command::new("git")
            .args(["init", "--bare", "--quiet", "--initial-branch=main"])
            .arg(&remote)
            .output()?
            .status
            .success());
        let publisher_root = root.join("publisher");
        fs::create_dir_all(&publisher_root)?;
        let publisher_db = db::open(&publisher_root.join("tasks.sqlite3"))?;
        let publisher = History::new(&publisher_db).unwrap();
        publisher.set_remote(remote.to_str().unwrap())?;
        for args in [
            ["config", "user.name", "Todo Test"],
            ["config", "user.email", "todo@example.test"],
            ["config", "commit.gpgsign", "false"],
        ] {
            assert!(Command::new("git")
                .arg("-C")
                .arg(&publisher.directory)
                .args(args)
                .output()?
                .status
                .success());
        }
        db::save_task(&publisher_db, None, "remote task")?;
        publisher.commit(&publisher_db, "first checkpoint")?;
        publisher.push()?;
        let consumer = root.join("consumer");
        fs::create_dir_all(&consumer)?;
        let mut app = App::new(db::open(&consumer.join("tasks.sqlite3"))?)?;
        app.handle_key(KeyCode::Char('3'))?;
        app.handle_key(KeyCode::Char('r'))?;
        for character in remote.to_str().unwrap().chars() {
            app.handle_key(KeyCode::Char(character))?;
        }
        app.handle_key(KeyCode::Enter)?;
        assert_eq!(app.remote_url, remote.to_str().unwrap());
        app.handle_key(KeyCode::Char('P'))?;
        assert!(app.sync_receiver.is_some());
        app.handle_key(KeyCode::Char('r'))?;
        assert!(matches!(app.mode, Mode::Browse));
        assert!(!app.handle_key(KeyCode::Char('q'))?);
        app.handle_key(KeyCode::Char('1'))?;
        wait_for_sync(&mut app);
        assert_eq!(app.tasks[0].title, "remote task", "{}", app.message);
        assert!(!app.uncommitted_changes);
        assert!(app.message.contains("pulled and applied"));
        app.handle_key(KeyCode::Char('3'))?;
        app.handle_key(KeyCode::Char('p'))?;
        wait_for_sync(&mut app);
        assert_eq!(app.message, "Todo checkpoints pushed");
        app.handle_key(KeyCode::Char('1'))?;
        app.handle_key(KeyCode::Char('a'))?;
        app.handle_key(KeyCode::Char('x'))?;
        app.handle_key(KeyCode::Enter)?;
        app.handle_key(KeyCode::Char('3'))?;
        app.handle_key(KeyCode::Char('P'))?;
        assert!(app.sync_receiver.is_none());
        assert!(app.message.contains("Commit your todo changes"));
        db::delete_task(&app.db, app.db.last_insert_rowid())?;
        app.update_commit_status();
        db::save_task(&publisher_db, None, "new remote task")?;
        publisher.commit(&publisher_db, "second checkpoint")?;
        publisher.push()?;
        app.handle_key(KeyCode::Char('P'))?;
        app.handle_key(KeyCode::Char('1'))?;
        app.handle_key(KeyCode::Char('a'))?;
        app.handle_key(KeyCode::Char('y'))?;
        app.handle_key(KeyCode::Enter)?;
        wait_for_sync(&mut app);
        assert!(
            app.message.contains("changed during pull"),
            "{}",
            app.message
        );
        assert_eq!(db::list_tasks(&app.db, Filter::All)?.len(), 2);
        assert_eq!(app.commits.len(), 1);
        app.handle_key(KeyCode::Char('3'))?;
        app.handle_key(KeyCode::Char('r'))?;
        app.handle_key(KeyCode::Esc)?;
        assert!(!app.remote_url.is_empty());
        app.mode = Mode::RemoteEdit(String::new());
        app.handle_key(KeyCode::Enter)?;
        assert!(app.remote_url.is_empty());
        app.handle_key(KeyCode::Char('p'))?;
        wait_for_sync(&mut app);
        assert!(app.message.contains("Configure origin"));
        drop(app);
        drop(publisher_db);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn commit_marker_and_no_changes_notification_follow_database_changes() -> Result<()> {
        use std::{env, fs, process::Command, time::SystemTime};
        let stamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_nanos();
        let root = env::temp_dir().join(format!("cli-todo-status-{stamp}"));
        fs::create_dir_all(&root)?;
        let path = root.join("tasks.sqlite3");
        let mut app = App::new(db::open(&path)?)?;
        assert!(!app.uncommitted_changes);
        app.handle_key(KeyCode::Char('3'))?;
        app.handle_key(KeyCode::Char('c'))?;
        assert!(matches!(app.mode, Mode::Browse));
        assert_eq!(app.message, "No todo changes to commit");
        app.handle_key(KeyCode::Char('1'))?;
        app.handle_key(KeyCode::Char('a'))?;
        app.handle_key(KeyCode::Char('x'))?;
        app.handle_key(KeyCode::Enter)?;
        assert!(app.uncommitted_changes);
        let history = root.join("history");
        fs::create_dir_all(&history)?;
        for args in [
            vec!["init", "--quiet"],
            vec!["config", "user.name", "Todo Test"],
            vec!["config", "user.email", "todo@example.test"],
            vec!["config", "commit.gpgsign", "false"],
        ] {
            assert!(Command::new("git")
                .arg("-C")
                .arg(&history)
                .args(args)
                .output()?
                .status
                .success());
        }
        app.handle_key(KeyCode::Char('3'))?;
        app.handle_key(KeyCode::Char('c'))?;
        app.handle_key(KeyCode::Char('m'))?;
        app.handle_key(KeyCode::Enter)?;
        assert!(matches!(app.mode, Mode::Browse), "{}", app.message);
        assert!(!app.uncommitted_changes);
        app.handle_key(KeyCode::Char('c'))?;
        assert_eq!(app.message, "No todo changes to commit");
        assert_eq!(app.commits.len(), 1);
        drop(app);
        let mut app = App::new(db::open(&path)?)?;
        assert!(!app.uncommitted_changes);
        assert_eq!(app.commits.len(), 1);
        assert!(!app.commits_focused);
        app.handle_key(KeyCode::Enter)?;
        app.handle_key(KeyCode::Char(' '))?;
        assert!(app.uncommitted_changes);
        drop(app);
        let app = App::new(db::open(&path)?)?;
        assert!(app.uncommitted_changes);
        drop(app);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn commit_dialog_preserves_text_and_handles_errors_without_exiting() -> Result<()> {
        let mut app = app()?;
        app.handle_key(KeyCode::Char('3'))?;
        assert!(app.commits_focused);
        app.handle_key(KeyCode::Char('c'))?;
        assert!(!app.handle_key(KeyCode::Char('q'))?);
        app.handle_key(KeyCode::Enter)?;
        assert!(matches!(&app.mode, Mode::CommitEdit(text) if text == "q"));
        assert!(app.message.contains("file-backed"));
        app.handle_key(KeyCode::Esc)?;
        assert!(matches!(app.mode, Mode::Browse));
        app.handle_key(KeyCode::Enter)?;
        assert!(!app.navigation_focused);
        app.handle_key(KeyCode::Esc)?;
        assert!(app.navigation_focused && app.commits_focused);
        Ok(())
    }

    #[test]
    fn left_cursor_previews_each_project_without_enter() -> Result<()> {
        let mut app = app()?;
        db::save_task(&app.db, None, "inbox")?;
        db::save_project(&app.db, None, "First")?;
        let first = app.db.last_insert_rowid();
        db::create_task(&app.db, "first task", Some(first))?;
        db::save_project(&app.db, None, "Second")?;
        let second = app.db.last_insert_rowid();
        db::create_task(&app.db, "second task", Some(second))?;
        app.refresh_projects()?;
        app.handle_key(KeyCode::Char('2'))?;
        assert!(app.navigation_focused);
        assert_eq!(app.tasks[0].title, "first task");
        app.handle_key(KeyCode::Down)?;
        assert!(app.navigation_focused);
        assert_eq!(app.tasks[0].title, "second task");
        app.handle_key(KeyCode::Up)?;
        assert_eq!(app.tasks[0].title, "first task");
        app.handle_key(KeyCode::Enter)?;
        app.handle_key(KeyCode::Esc)?;
        assert_eq!(app.tasks[0].title, "first task");
        app.handle_key(KeyCode::Left)?;
        assert_eq!(app.tasks[0].title, "inbox");
        Ok(())
    }

    #[test]
    fn projects_create_move_search_rename_and_confirm_deletion() -> Result<()> {
        let mut app = app()?;
        db::save_task(&app.db, None, "keep unassigned")?;
        app.handle_key(KeyCode::Char('2'))?;
        app.handle_key(KeyCode::Enter)?;
        assert!(app.navigation_focused);
        app.handle_key(KeyCode::Char('a'))?;
        app.handle_key(KeyCode::Enter)?;
        assert!(!app.message.is_empty());
        app.handle_key(KeyCode::Char('W'))?;
        app.handle_key(KeyCode::Enter)?;
        let project = app.projects[0].id;
        app.handle_key(KeyCode::Enter)?;
        assert_eq!(app.current_project, Some(project));
        app.handle_key(KeyCode::Char('a'))?;
        app.handle_key(KeyCode::Char('x'))?;
        app.handle_key(KeyCode::Enter)?;
        let task = app.tasks[0].id;
        assert_eq!(app.tasks[0].project_id, Some(project));
        app.handle_key(KeyCode::Char('m'))?;
        app.handle_key(KeyCode::Esc)?;
        assert_eq!(app.tasks.len(), 1);
        app.handle_key(KeyCode::Char('m'))?;
        app.handle_key(KeyCode::Enter)?;
        assert!(app.tasks.is_empty());
        app.handle_key(KeyCode::Char('1'))?;
        search_actions(&mut app, "x")?;
        app.handle_key(KeyCode::Enter)?;
        assert_eq!(app.tasks[app.selected_task].id, task);
        app.handle_key(KeyCode::Char('m'))?;
        app.handle_key(KeyCode::Down)?;
        app.handle_key(KeyCode::Enter)?;
        search_actions(&mut app, "x")?;
        app.handle_key(KeyCode::Enter)?;
        assert_eq!(app.current_project, Some(project));
        app.handle_key(KeyCode::Esc)?;
        assert!(app.projects_focused);
        app.handle_key(KeyCode::Char('e'))?;
        app.handle_key(KeyCode::Char('!'))?;
        app.handle_key(KeyCode::Enter)?;
        assert_eq!(app.projects[0].name, "W!");
        app.handle_key(KeyCode::Char('d'))?;
        app.handle_key(KeyCode::Esc)?;
        assert_eq!(app.projects.len(), 1);
        app.handle_key(KeyCode::Char('d'))?;
        app.handle_key(KeyCode::Char('y'))?;
        assert!(app.projects.is_empty());
        assert_eq!(app.current_project, None);
        assert!(app.tasks.is_empty());
        app.handle_key(KeyCode::Char('1'))?;
        assert_eq!(app.tasks[0].title, "keep unassigned");
        Ok(())
    }

    #[test]
    fn enter_and_escape_switch_panels_while_tab_does_nothing() -> Result<()> {
        let mut app = app()?;
        for key in [KeyCode::Esc, KeyCode::Tab, KeyCode::BackTab] {
            app.handle_key(key)?;
            assert!(app.navigation_focused);
        }
        app.handle_key(KeyCode::Enter)?;
        assert!(!app.navigation_focused);
        for key in [KeyCode::Tab, KeyCode::BackTab] {
            app.handle_key(key)?;
            assert!(!app.navigation_focused);
        }
        app.handle_key(KeyCode::Esc)?;
        assert!(app.navigation_focused);
        Ok(())
    }

    #[test]
    fn number_keys_focus_sections_but_remain_text_in_dialogs() -> Result<()> {
        let mut app = app()?;
        app.handle_key(KeyCode::Char('0'))?;
        assert!(!app.navigation_focused);
        app.handle_key(KeyCode::Char('1'))?;
        assert!(app.navigation_focused);
        app.handle_key(KeyCode::Char('a'))?;
        app.handle_key(KeyCode::Char('0'))?;
        app.handle_key(KeyCode::Char('1'))?;
        assert!(matches!(&app.mode, Mode::Edit(None, text) if text == "01"));
        assert!(app.navigation_focused);
        app.handle_key(KeyCode::Esc)?;
        search_actions(&mut app, "01")?;
        assert!(matches!(&app.mode, Mode::Palette { query, .. } if query == "01"));
        assert!(app.navigation_focused);
        Ok(())
    }

    #[test]
    fn palette_finds_tasks_outside_current_filter() -> Result<()> {
        let mut app = app()?;
        db::save_task(&app.db, None, "Buy MILK")?;
        let id = app.db.last_insert_rowid();
        db::toggle_task(&app.db, id)?;
        app.select_filter(1)?;
        assert!(app.tasks.is_empty());
        search_actions(&mut app, "milk")?;
        let results = app.palette_results();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, Action::OpenTask(id));
        app.handle_key(KeyCode::Enter)?;
        assert_eq!(app.selected_filter, 0);
        assert_eq!(app.tasks[app.selected_task].id, id);
        assert!(!app.navigation_focused);
        app.handle_key(KeyCode::Char('e'))?;
        assert!(matches!(app.mode, Mode::Edit(Some(task_id), _) if task_id == id));
        Ok(())
    }

    #[test]
    fn palette_search_and_arrows_select_a_filter() -> Result<()> {
        let mut app = app()?;
        search_actions(&mut app, " SHOW ")?;
        assert_eq!(app.palette_actions().len(), 3);
        for _ in 0..10 {
            app.handle_key(KeyCode::Down)?;
        }
        assert!(matches!(app.mode, Mode::Palette { selected: 2, .. }));
        app.handle_key(KeyCode::Backspace)?;
        assert!(matches!(app.mode, Mode::Palette { selected: 0, .. }));
        app.handle_key(KeyCode::Down)?;
        app.handle_key(KeyCode::Down)?;
        app.handle_key(KeyCode::Up)?;
        app.handle_key(KeyCode::Enter)?;
        assert_eq!(app.selected_filter, 1);
        assert!(!app.navigation_focused);
        search_actions(&mut app, "")?;
        app.handle_key(KeyCode::Esc)?;
        assert!(matches!(app.mode, Mode::Browse));
        assert!(!app.navigation_focused);
        Ok(())
    }

    #[test]
    fn palette_handles_no_matches_and_requires_enter_to_quit() -> Result<()> {
        let mut app = app()?;
        search_actions(&mut app, "")?;
        assert_eq!(
            app.palette_actions()
                .iter()
                .map(|(_, action)| *action)
                .collect::<Vec<_>>(),
            [
                Action::Add,
                Action::ShowFilter(0),
                Action::ShowFilter(1),
                Action::ShowFilter(2),
                Action::Quit
            ]
        );
        app.handle_key(KeyCode::Char('z'))?;
        assert!(app.palette_actions().is_empty());
        app.handle_key(KeyCode::Down)?;
        app.handle_key(KeyCode::Up)?;
        assert!(!app.handle_key(KeyCode::Enter)?);
        assert!(matches!(app.mode, Mode::Palette { selected: 0, .. }));
        app.handle_key(KeyCode::Esc)?;
        search_actions(&mut app, "q")?;
        assert_eq!(app.palette_actions(), [("Quit", Action::Quit)]);
        assert!(app.handle_key(KeyCode::Enter)?);
        Ok(())
    }

    #[test]
    fn palette_reuses_task_actions_and_delete_confirmation() -> Result<()> {
        let mut app = app()?;
        search_actions(&mut app, "add")?;
        app.handle_key(KeyCode::Enter)?;
        app.handle_key(KeyCode::Char('x'))?;
        app.handle_key(KeyCode::Enter)?;
        assert_eq!(app.tasks[0].title, "x");
        assert!(app.navigation_focused);
        search_actions(&mut app, "edit")?;
        app.handle_key(KeyCode::Enter)?;
        app.handle_key(KeyCode::Char('!'))?;
        app.handle_key(KeyCode::Enter)?;
        assert_eq!(app.tasks[0].title, "x!");
        search_actions(&mut app, "complete")?;
        app.handle_key(KeyCode::Enter)?;
        assert!(app.tasks[0].done);
        search_actions(&mut app, "delete")?;
        app.handle_key(KeyCode::Enter)?;
        assert!(matches!(app.mode, Mode::Delete(_)));
        app.handle_key_event(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL))?;
        assert!(matches!(app.mode, Mode::Delete(_)));
        assert_eq!(db::list_tasks(&app.db, Filter::All)?.len(), 1);
        app.handle_key(KeyCode::Char('n'))?;
        search_actions(&mut app, "delete")?;
        app.handle_key(KeyCode::Enter)?;
        app.handle_key(KeyCode::Enter)?;
        assert!(db::list_tasks(&app.db, Filter::All)?.is_empty());
        Ok(())
    }

    #[test]
    fn control_keys_preserve_drafts_and_do_not_run_plain_shortcuts() -> Result<()> {
        let mut app = app()?;
        let ctrl_k = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL);
        app.handle_key_event(KeyEvent::new_with_kind(
            KeyCode::Char('k'),
            KeyModifiers::CONTROL,
            KeyEventKind::Release,
        ))?;
        assert!(matches!(app.mode, Mode::Browse));
        app.handle_key_event(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE))?;
        assert!(matches!(app.mode, Mode::Browse));
        app.handle_key_event(ctrl_k)?;
        app.handle_key_event(ctrl_k)?;
        assert!(matches!(app.mode, Mode::Browse));
        for character in ['a', 'q', 'd'] {
            assert!(!app.handle_key_event(KeyEvent::new(
                KeyCode::Char(character),
                KeyModifiers::CONTROL
            ))?);
        }
        assert!(matches!(app.mode, Mode::Browse));
        app.handle_key(KeyCode::Char('a'))?;
        app.handle_key(KeyCode::Char('x'))?;
        app.handle_key_event(ctrl_k)?;
        assert!(matches!(&app.mode, Mode::Edit(None, text) if text == "x"));
        app.handle_key(KeyCode::Esc)?;
        assert!(app.tasks.is_empty());
        Ok(())
    }

    #[test]
    fn editing_validates_titles_and_keeps_shortcuts_as_text() -> Result<()> {
        let mut app = app()?;
        app.handle_key(KeyCode::Char('a'))?;
        app.handle_key(KeyCode::Enter)?;
        assert_eq!(app.message, "Title cannot be empty");
        assert!(matches!(app.mode, Mode::Edit(..)));
        assert!(!app.handle_key(KeyCode::Char('q'))?);
        app.handle_key(KeyCode::Char('a'))?;
        app.handle_key(KeyCode::Backspace)?;
        app.handle_key(KeyCode::Enter)?;
        assert_eq!(app.tasks[0].title, "q");

        app.handle_key(KeyCode::Enter)?;
        app.handle_key(KeyCode::Enter)?;
        app.handle_key(KeyCode::Char('!'))?;
        app.handle_key(KeyCode::Enter)?;
        assert_eq!(app.tasks[0].title, "q!");
        assert!(app.handle_key(KeyCode::Char('q'))?);
        Ok(())
    }

    #[test]
    fn escape_discards_new_and_edited_titles() -> Result<()> {
        let mut app = app()?;
        db::save_task(&app.db, None, "original")?;
        app.refresh_tasks()?;
        app.handle_key(KeyCode::Char('a'))?;
        app.handle_key(KeyCode::Char('x'))?;
        app.handle_key(KeyCode::Esc)?;
        assert_eq!(app.tasks.len(), 1);
        app.handle_key(KeyCode::Enter)?;
        app.handle_key(KeyCode::Char('e'))?;
        app.handle_key(KeyCode::Backspace)?;
        app.handle_key(KeyCode::Esc)?;
        assert!(matches!(app.mode, Mode::Browse));
        assert_eq!(db::list_tasks(&app.db, Filter::All)?[0].title, "original");
        Ok(())
    }

    #[test]
    fn horizontal_arrows_only_switch_left_sections() -> Result<()> {
        let mut app = app()?;
        let filter = app.selected_filter;
        for key in [KeyCode::Right, KeyCode::Left] {
            for _ in 0..2 {
                app.handle_key(key)?;
                assert!(app.navigation_focused);
                assert!(app.projects_focused || app.commits_focused);
            }
            app.handle_key(key)?;
            assert!(app.navigation_focused && !app.projects_focused && !app.commits_focused);
            assert_eq!(app.selected_filter, filter);
        }
        app.handle_key(KeyCode::Enter)?;
        for key in [KeyCode::Left, KeyCode::Right] {
            app.handle_key(key)?;
            assert!(!app.navigation_focused);
            assert_eq!(app.selected_filter, filter);
        }
        Ok(())
    }

    #[test]
    fn navigation_wraps_filters_and_clamps_task_selection() -> Result<()> {
        let mut app = app()?;
        db::save_task(&app.db, None, "first")?;
        db::save_task(&app.db, None, "second")?;
        app.refresh_tasks()?;
        app.handle_key(KeyCode::Up)?;
        assert_eq!(app.selected_filter, 2);
        assert!(app.tasks.is_empty());
        app.handle_key(KeyCode::Down)?;
        assert_eq!(app.selected_filter, 0);
        app.handle_key(KeyCode::Enter)?;
        assert!(!app.navigation_focused);
        app.handle_key(KeyCode::Up)?;
        assert_eq!(app.selected_task, 0);
        app.handle_key(KeyCode::Down)?;
        app.handle_key(KeyCode::Down)?;
        assert_eq!(app.selected_task, 1);
        app.handle_key(KeyCode::Esc)?;
        assert!(app.navigation_focused);
        app.handle_key(KeyCode::Down)?;
        assert_eq!(app.selected_filter, 1);
        assert_eq!(app.selected_task, 0);
        app.handle_key(KeyCode::Up)?;
        assert_eq!(app.selected_filter, 0);
        Ok(())
    }

    #[test]
    fn completion_refreshes_filtered_tasks() -> Result<()> {
        let mut app = app()?;
        db::save_task(&app.db, None, "task")?;
        app.handle_key(KeyCode::Down)?;
        app.handle_key(KeyCode::Enter)?;
        app.handle_key(KeyCode::Char(' '))?;
        assert!(app.tasks.is_empty());
        assert_eq!(app.selected_task, 0);
        app.handle_key(KeyCode::Esc)?;
        app.handle_key(KeyCode::Down)?;
        assert!(app.tasks[0].done);
        app.handle_key(KeyCode::Enter)?;
        app.handle_key(KeyCode::Char(' '))?;
        assert!(app.tasks.is_empty());
        Ok(())
    }

    #[test]
    fn deletion_requires_confirmation() -> Result<()> {
        let mut app = app()?;
        db::save_task(&app.db, None, "task")?;
        app.refresh_tasks()?;
        app.handle_key(KeyCode::Char('d'))?;
        assert!(matches!(app.mode, Mode::Browse));
        app.handle_key(KeyCode::Enter)?;
        for cancel in [KeyCode::Esc, KeyCode::Char('n')] {
            app.handle_key(KeyCode::Char('d'))?;
            app.handle_key(cancel)?;
            assert!(matches!(app.mode, Mode::Browse));
            assert_eq!(db::list_tasks(&app.db, Filter::All)?.len(), 1);
        }
        app.handle_key(KeyCode::Char('d'))?;
        assert!(app.handle_key(KeyCode::Char('q'))?);
        app.handle_key(KeyCode::Enter)?;
        assert!(app.tasks.is_empty());
        assert_eq!(app.selected_task, 0);
        for key in [
            KeyCode::Down,
            KeyCode::Enter,
            KeyCode::Char('e'),
            KeyCode::Char('d'),
            KeyCode::Char(' '),
        ] {
            app.handle_key(key)?;
        }
        assert!(matches!(app.mode, Mode::Browse));
        assert_eq!(app.selected_task, 0);
        Ok(())
    }
}

use crate::{
    constants::{ACTIONS, FILTERS, KEYBINDINGS, MAX_SUBTASK_DEPTH},
    dates::{date_phrases, parse_due_date, remove_date_phrase},
    db,
    types::{
        Action, App, AutoDate, Filter, History, Mode, SyncResult, Task, TaskDraft, TaskField,
        TaskPane, TaskView,
    },
    Result,
};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use rusqlite::Connection;
use std::sync::mpsc::{self, TryRecvError};
use std::thread;

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
            help_page_size: 20,
            selected_task: 0,
            task_view: None,
            task_path: vec![],
            new_task_parent: None,
            viewport: ratatui::layout::Rect::new(0, 0, 80, 24),
            input_cursor: crate::editor::TextCursor::default(),
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

    pub fn resize(&mut self, height: u16) {
        self.viewport.height = height;
        if let Some(view) = &mut self.task_view {
            view.scroll = view
                .scroll
                .min(crate::ui::task_detail_scroll_bounds(view, self.viewport).0);
        }
        self.help_page_size = height.saturating_sub(4).max(1);
        let maximum = self.max_help_scroll();
        if let Mode::Help { scroll } = &mut self.mode {
            *scroll = (*scroll).min(maximum);
        }
    }

    fn max_help_scroll(&self) -> u16 {
        KEYBINDINGS
            .len()
            .saturating_sub(self.help_page_size as usize) as u16
    }

    fn handle_help_key(&mut self, key: KeyCode) {
        let maximum = self.max_help_scroll();
        let page = self.help_page_size;
        let Mode::Help { scroll } = &mut self.mode else {
            return;
        };
        match key {
            KeyCode::Esc | KeyCode::Char('?') => self.mode = Mode::Browse,
            KeyCode::Up => *scroll = scroll.saturating_sub(1),
            KeyCode::Down => *scroll = scroll.saturating_add(1).min(maximum),
            KeyCode::PageUp => *scroll = scroll.saturating_sub(page),
            KeyCode::PageDown => *scroll = scroll.saturating_add(page).min(maximum),
            KeyCode::Home => *scroll = 0,
            KeyCode::End => *scroll = maximum,
            _ => {}
        }
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
        self.refresh_task_view()
    }

    fn clear_task_view(&mut self) {
        self.task_view = None;
        self.task_path.clear();
    }

    fn refresh_task_view(&mut self) -> Result<()> {
        let Some(view) = &self.task_view else {
            return Ok(());
        };
        let (mut id, old_id, selected, scroll, focus) = (
            view.task.id,
            view.task.id,
            view.selected_child,
            view.scroll,
            view.focus,
        );
        let tasks = db::list_tasks(&self.db, Filter::All)?;
        loop {
            if let Some(task) = tasks
                .iter()
                .find(|task| task.id == id && task.project_id == self.current_project)
            {
                let children: Vec<_> = tasks
                    .iter()
                    .filter(|child| child.parent_id == Some(id))
                    .cloned()
                    .collect();
                let selected_child = if id == old_id {
                    selected.min(children.len().saturating_sub(1))
                } else {
                    0
                };
                let mut view = TaskView {
                    focus: if id == old_id && !children.is_empty() {
                        focus
                    } else {
                        TaskPane::Details
                    },
                    task: task.clone(),
                    parent_title: parent_title(&tasks, task),
                    children,
                    selected_child,
                    scroll: if id == old_id { scroll } else { 0 },
                };
                view.scroll = view
                    .scroll
                    .min(crate::ui::task_detail_scroll_bounds(&view, self.viewport).0);
                self.task_view = Some(view);
                return Ok(());
            }
            if let Some(parent) = self.task_path.pop() {
                id = parent;
            } else {
                self.clear_task_view();
                return Ok(());
            }
        }
    }

    fn open_task(&mut self, id: i64, child: bool) -> Result<()> {
        let tasks = db::list_tasks(&self.db, Filter::All)?;
        let task = tasks
            .iter()
            .find(|task| task.id == id)
            .ok_or("Task no longer exists")?
            .clone();
        if child {
            if let Some(view) = &self.task_view {
                self.task_path.push(view.task.id);
            }
        } else {
            self.task_path.clear();
            let mut parent = task.parent_id;
            while let Some(id) = parent {
                if self.task_path.len() > MAX_SUBTASK_DEPTH {
                    return Err("Invalid task hierarchy".into());
                }
                self.task_path.push(id);
                parent = tasks
                    .iter()
                    .find(|task| task.id == id)
                    .ok_or("Subtask parent is missing")?
                    .parent_id;
            }
            self.task_path.reverse();
        }
        let parent_title = parent_title(&tasks, &task);
        let children = tasks
            .into_iter()
            .filter(|child| child.parent_id == Some(id))
            .collect();
        self.task_view = Some(TaskView {
            focus: TaskPane::Details,
            task,
            parent_title,
            children,
            selected_child: 0,
            scroll: 0,
        });
        Ok(())
    }

    fn back_task(&mut self) -> Result<()> {
        let child = self.task_view.as_ref().map(|view| view.task.id);
        if let Some(parent) = self.task_path.pop() {
            self.open_task(parent, false)?;
            if let Some(view) = &mut self.task_view {
                view.focus = if view.children.is_empty() {
                    TaskPane::Details
                } else {
                    TaskPane::Subtasks
                };
                view.selected_child = view
                    .children
                    .iter()
                    .position(|task| Some(task.id) == child)
                    .unwrap_or(0);
            }
        } else {
            self.clear_task_view();
        }
        Ok(())
    }

    fn current_task_id(&self) -> Option<i64> {
        self.task_view
            .as_ref()
            .filter(|_| !self.navigation_focused)
            .map(|view| view.task.id)
            .or_else(|| self.tasks.get(self.selected_task).map(|task| task.id))
    }

    fn handle_opened_task_key(&mut self, key: KeyCode) -> Result<()> {
        let Some(view) = &mut self.task_view else {
            return Ok(());
        };
        let (maximum, page) = crate::ui::task_detail_scroll_bounds(view, self.viewport);
        match key {
            KeyCode::Left | KeyCode::Right if !view.children.is_empty() => {
                view.focus = match view.focus {
                    TaskPane::Details => TaskPane::Subtasks,
                    TaskPane::Subtasks => TaskPane::Details,
                };
            }
            KeyCode::Up if view.focus == TaskPane::Subtasks => {
                view.selected_child = view.selected_child.saturating_sub(1)
            }
            KeyCode::Down if view.focus == TaskPane::Subtasks => {
                view.selected_child =
                    (view.selected_child + 1).min(view.children.len().saturating_sub(1))
            }
            KeyCode::Up => view.scroll = view.scroll.saturating_sub(1),
            KeyCode::Down => view.scroll = view.scroll.saturating_add(1).min(maximum),
            KeyCode::PageUp => view.scroll = view.scroll.saturating_sub(page),
            KeyCode::PageDown => view.scroll = view.scroll.saturating_add(page).min(maximum),
            KeyCode::Home => view.scroll = 0,
            KeyCode::End => view.scroll = maximum,
            KeyCode::Enter if view.focus == TaskPane::Subtasks => {
                if let Some(child) = view.children.get(view.selected_child) {
                    let id = child.id;
                    self.open_task(id, true)?;
                }
            }
            KeyCode::Char('x') => {
                let id = view.task.id;
                db::toggle_task(&self.db, id)?;
                self.refresh_tasks()?;
            }
            KeyCode::Char(' ') => {
                let id = if view.focus == TaskPane::Subtasks {
                    view.children
                        .get(view.selected_child)
                        .map(|child| child.id)
                        .unwrap_or(view.task.id)
                } else {
                    view.task.id
                };
                db::toggle_task(&self.db, id)?;
                self.refresh_tasks()?;
            }
            KeyCode::Enter | KeyCode::Char('e') => {
                self.execute_action(Action::Edit)?;
            }
            KeyCode::Char('d') => {
                self.execute_action(Action::Delete)?;
            }
            KeyCode::Char('m') => {
                self.execute_action(Action::Move)?;
            }
            _ => {}
        }
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
        if modifiers == KeyModifiers::CONTROL && matches!(key.code, KeyCode::Char('s' | 'S')) {
            if matches!(&self.mode, Mode::Edit(_, draft) if draft.focus != TaskField::Title) {
                let changes = self.db.total_changes();
                self.message.clear();
                let result = self.save_edit();
                if self.db.total_changes() != changes {
                    self.update_commit_status();
                }
                result?;
            }
            return Ok(false);
        }
        if modifiers == KeyModifiers::ALT && matches!(key.code, KeyCode::Char('j' | 'k')) {
            let changes = self.db.total_changes();
            self.message.clear();
            let result = self.reorder_selected(key.code == KeyCode::Char('j'));
            if self.db.total_changes() != changes {
                self.update_commit_status();
            }
            return result.map(|_| false);
        }
        if !modifiers.is_empty() {
            return Ok(false);
        }
        self.handle_key(key.code)
    }

    /// Swaps the selected task or focused subtask with its visible neighbour.
    fn reorder_selected(&mut self, down: bool) -> Result<()> {
        if !matches!(self.mode, Mode::Browse) || self.navigation_focused || self.commits_focused {
            return Ok(());
        }
        let (tasks, selected) = match &self.task_view {
            Some(view) if view.focus == TaskPane::Subtasks => (&view.children, view.selected_child),
            Some(_) => return Ok(()),
            None => (&self.tasks, self.selected_task),
        };
        let Some(target) = (if down {
            selected.checked_add(1)
        } else {
            selected.checked_sub(1)
        }) else {
            return Ok(());
        };
        let (Some(task), Some(neighbour)) = (tasks.get(selected), tasks.get(target)) else {
            return Ok(());
        };
        db::swap_tasks(&self.db, task.id, neighbour.id)?;
        match &mut self.task_view {
            Some(view) => view.selected_child = target,
            None => self.selected_task = target,
        }
        self.refresh_tasks()
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
            && matches!(self.mode, Mode::Browse)
        {
            self.message = "Sync is in progress; wait before quitting".into();
            return Ok(false);
        }
        if key == KeyCode::Char('q') && matches!(self.mode, Mode::Browse) {
            return self.execute_action(Action::Quit);
        }
        if let Mode::ProjectEdit(_, text) | Mode::CommitEdit(text) | Mode::RemoteEdit(text) =
            &mut self.mode
        {
            if self.input_cursor.handle(text, key, false) {
                return Ok(false);
            }
        }
        match self.mode {
            Mode::Browse => self.handle_browse_key(key)?,
            Mode::Help { .. } => self.handle_help_key(key),
            Mode::Edit(..) => self.handle_edit_key(key)?,
            Mode::Delete(id) => self.handle_delete_key(key, id)?,
            Mode::Nuke { .. } => self.handle_nuke_key(key)?,
            Mode::ResetHistory { .. } => self.handle_reset_history_key(key),
            Mode::Palette { .. } => return self.handle_palette_key(key),
            Mode::ProjectEdit(..) => self.handle_project_edit_key(key)?,
            Mode::ProjectDelete(id) => self.handle_project_delete_key(key, id)?,
            Mode::CommitEdit(_) => self.handle_commit_edit_key(key)?,
            Mode::NoChanges => {
                if matches!(key, KeyCode::Enter | KeyCode::Esc) {
                    self.mode = Mode::Browse;
                }
            }
            Mode::RemoteEdit(_) => self.handle_remote_edit_key(key),
            Mode::Move { .. } => self.handle_move_key(key)?,
        }
        Ok(false)
    }

    fn handle_browse_key(&mut self, key: KeyCode) -> Result<()> {
        match key {
            KeyCode::Char('?') => self.mode = Mode::Help { scroll: 0 },
            KeyCode::Char('c') => self.begin_commit(),
            KeyCode::Char('D') => self.begin_nuke()?,
            KeyCode::Tab | KeyCode::BackTab => {
                self.navigation_focused = !self.navigation_focused;
            }
            KeyCode::Char('0') => self.navigation_focused = false,
            KeyCode::Char('1') => self.focus_inbox()?,
            KeyCode::Left | KeyCode::Right if self.navigation_focused => self.cycle_section(key)?,
            KeyCode::Char('2') => self.focus_projects()?,
            KeyCode::Char('3') => self.focus_commits(),
            KeyCode::Esc if self.task_view.is_some() && !self.navigation_focused => {
                self.back_task()?
            }
            KeyCode::Esc => self.navigation_focused = true,
            _ if self.commits_focused => self.handle_commit_key(key),
            _ if self.navigation_focused && self.projects_focused => {
                self.handle_project_key(key)?
            }
            KeyCode::Char('a') => {
                self.execute_action(Action::Add)?;
            }
            _ if self.navigation_focused => self.handle_navigation_key(key)?,
            _ if self.task_view.is_some() => self.handle_opened_task_key(key)?,
            _ => self.handle_task_key(key)?,
        }
        Ok(())
    }

    fn begin_nuke(&mut self) -> Result<()> {
        if self.sync_receiver.is_some() {
            self.message = "Wait for sync to finish before nuking todo data".into();
            return Ok(());
        }
        self.mode = Mode::Nuke {
            tasks: db::list_tasks(&self.db, Filter::All)?.len(),
            projects: db::list_projects(&self.db)?.len(),
            commits: self.commits.len(),
        };
        Ok(())
    }

    fn handle_nuke_key(&mut self, key: KeyCode) -> Result<()> {
        match key {
            KeyCode::Esc => self.mode = Mode::Browse,
            KeyCode::Enter => {
                let result = match &self.history {
                    Some(history) => history.nuke(&self.db),
                    None => db::nuke(&self.db),
                };
                if let Err(error) = result {
                    self.refresh_projects()?;
                    self.reload_commits();
                    self.load_remote();
                    self.update_commit_status();
                    self.message = format!("Could not nuke todo data: {error}");
                    return Ok(());
                }
                self.projects.clear();
                self.palette_tasks.clear();
                self.selected_project = 0;
                self.selected_filter = 0;
                self.mode = Mode::Browse;
                self.focus_inbox()?;
                self.selected_commit = 0;
                self.reload_commits();
                self.load_remote();
                self.update_commit_status();
                self.message =
                    "Tasks, projects, local commits and remote settings deleted. Backups kept."
                        .into();
            }
            _ => {}
        }
        Ok(())
    }

    fn begin_reset_history(&mut self) {
        if self.sync_receiver.is_some() {
            self.message = "Wait for sync to finish before resetting todo history".into();
            return;
        }
        self.mode = Mode::ResetHistory {
            commits: self.commits.len(),
        };
    }

    fn handle_reset_history_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Esc => self.mode = Mode::Browse,
            KeyCode::Enter => {
                if self.sync_receiver.is_some() {
                    self.message = "Wait for sync to finish before resetting todo history".into();
                    return;
                }
                let result = self.history.as_ref().map_or(Ok(()), History::reset);
                self.reload_commits();
                self.load_remote();
                self.update_commit_status();
                match result {
                    Ok(()) => {
                        self.mode = Mode::Browse;
                        self.message = "Local history and remote settings deleted. Todos kept. No backup created.".into();
                    }
                    Err(error) => {
                        self.message = format!("Could not reset todo history: {error}");
                    }
                }
            }
            _ => {}
        }
    }

    fn focus_inbox(&mut self) -> Result<()> {
        self.clear_task_view();
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
        self.clear_task_view();
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
            self.input_cursor = crate::editor::TextCursor::default();
            self.mode = Mode::CommitEdit(String::new());
        } else {
            self.mode = Mode::NoChanges;
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
            _ => "No todo commits yet. Press c to create a SQLite snapshot.".into(),
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
        self.input_cursor = crate::editor::TextCursor::default();
        self.mode = Mode::RemoteEdit(self.remote_url.clone());
    }

    fn handle_remote_edit_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Esc => self.mode = Mode::Browse,
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
                if !matches!(self.mode, Mode::Browse | Mode::Help { .. }) {
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
        match key {
            KeyCode::Esc => self.mode = Mode::Browse,
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
        self.clear_task_view();
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
            KeyCode::Enter => {
                if let Some(id) = self.current_task_id() {
                    self.open_task(id, false)?;
                }
                return Ok(());
            }
            KeyCode::Char('x') => {
                if let Some(id) = self.current_task_id() {
                    db::toggle_task(&self.db, id)?;
                    self.refresh_tasks()?;
                }
                return Ok(());
            }
            KeyCode::Char('e') => Action::Edit,
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
                self.new_task_parent = self
                    .task_view
                    .as_ref()
                    .filter(|_| !self.navigation_focused)
                    .map(|view| view.task.id);
                let depth = if let Some(parent) = self.new_task_parent {
                    db::task_draft(&self.db, parent)?.depth + 1
                } else {
                    0
                };
                if depth > MAX_SUBTASK_DEPTH {
                    self.new_task_parent = None;
                    self.message = "Subtask nesting limit reached".into();
                    return Ok(false);
                }
                self.mode = Mode::Edit(
                    None,
                    TaskDraft {
                        depth,
                        ..TaskDraft::default()
                    },
                );
            }
            Action::Edit => {
                if let Some(id) = self.current_task_id() {
                    self.new_task_parent = None;
                    self.mode = Mode::Edit(Some(id), db::task_draft(&self.db, id)?);
                }
            }
            Action::Delete => {
                let id = match self.task_view.as_ref().filter(|_| !self.navigation_focused) {
                    Some(view) if view.focus == TaskPane::Subtasks => {
                        view.children.get(view.selected_child).map(|child| child.id)
                    }
                    _ => self.current_task_id(),
                };
                if let Some(id) = id {
                    self.mode = Mode::Delete(id);
                }
            }
            Action::ToggleCompletion => {
                if let Some(id) = self.current_task_id() {
                    db::toggle_task(&self.db, id)?;
                    self.refresh_tasks()?;
                }
            }
            Action::Move => {
                if let Some(id) = self.current_task_id() {
                    self.mode = Mode::Move {
                        task: id,
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
                self.open_task(id, false)?;
                self.navigation_focused = false;
            }
            Action::ResetHistory => self.begin_reset_history(),
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
        let has_task = !self.commits_focused && self.current_task_id().is_some();
        ACTIONS
            .iter()
            .copied()
            .filter(|(label, action)| {
                (has_task
                    || matches!(
                        action,
                        Action::Add | Action::ShowFilter(_) | Action::ResetHistory | Action::Quit
                    ))
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
        let Mode::Edit(_, draft) = &mut self.mode else {
            return Ok(());
        };
        match key {
            KeyCode::Esc if draft.auto_date.is_some() => Self::dismiss_auto_date(draft, true),
            KeyCode::Esc => self.cancel_edit(),
            KeyCode::Tab => {
                draft.focus = match draft.focus {
                    TaskField::Title => TaskField::Description,
                    TaskField::Description => TaskField::DueDate,
                    TaskField::DueDate => TaskField::Title,
                }
            }
            KeyCode::BackTab => {
                draft.focus = match draft.focus {
                    TaskField::Title => TaskField::DueDate,
                    TaskField::Description => TaskField::Title,
                    TaskField::DueDate => TaskField::Description,
                }
            }
            KeyCode::Enter if draft.focus != TaskField::Description => self.save_edit()?,
            _ => {
                let key = if key == KeyCode::Enter {
                    KeyCode::Char('\n')
                } else {
                    key
                };
                let deleting = matches!(key, KeyCode::Backspace | KeyCode::Delete);
                let changing = deleting || matches!(key, KeyCode::Char(_));
                if deleting {
                    let text = draft.field_text(draft.focus);
                    let range = draft.cursors[draft.focus as usize].deletion_range(text, key);
                    if draft.auto_date.as_ref().is_some_and(|auto| {
                        auto.field == draft.focus
                            && range.start < auto.phrase.range.end
                            && range.end > auto.phrase.range.start
                    }) {
                        Self::dismiss_auto_date(draft, true);
                    }
                }
                if changing && draft.focus == TaskField::DueDate {
                    Self::dismiss_auto_date(draft, false);
                }
                draft.edit_focused(key);
                if changing && draft.focus != TaskField::DueDate {
                    if deleting {
                        Self::refresh_auto_date(draft, false);
                    } else {
                        Self::update_auto_date(draft);
                    }
                }
            }
        }
        Ok(())
    }

    fn dismiss_auto_date(draft: &mut TaskDraft, clear: bool) {
        if let Some(auto) = draft.auto_date.take() {
            let text = if auto.field == TaskField::Title {
                &draft.title
            } else {
                &draft.description
            };
            draft
                .ignored_dates
                .push((auto.field, text[auto.phrase.range].to_ascii_lowercase()));
            if clear {
                draft.due_date.clear();
                draft.cursors[TaskField::DueDate as usize] = Default::default();
            }
        }
    }

    fn update_auto_date(draft: &mut TaskDraft) {
        Self::refresh_auto_date(draft, true);
    }

    fn refresh_auto_date(draft: &mut TaskDraft, allow_new: bool) {
        let today = chrono::Local::now().date_naive();
        let candidates: Vec<_> = [
            (TaskField::Title, &draft.title),
            (TaskField::Description, &draft.description),
        ]
        .into_iter()
        .flat_map(|(field, text)| {
            date_phrases(text, today).into_iter().map(move |phrase| {
                let keyword = text[phrase.range.clone()].to_ascii_lowercase();
                (
                    AutoDate {
                        field,
                        phrase,
                        previous_due_date: String::new(),
                    },
                    keyword,
                )
            })
        })
        .collect();
        draft.ignored_dates.retain(|ignored| {
            candidates
                .iter()
                .any(|(auto, keyword)| ignored == &(auto.field, keyword.clone()))
        });
        if !allow_new && draft.auto_date.is_none() {
            for (auto, keyword) in &candidates {
                let ignored = (auto.field, keyword.clone());
                if !draft.ignored_dates.contains(&ignored) {
                    draft.ignored_dates.push(ignored);
                }
            }
        }
        let previous = draft
            .auto_date
            .as_ref()
            .map(|auto| auto.previous_due_date.clone())
            .unwrap_or_else(|| draft.due_date.clone());
        let mut next = candidates
            .into_iter()
            .find(|(auto, keyword)| !draft.ignored_dates.contains(&(auto.field, keyword.clone())))
            .map(|(auto, _)| auto);
        if let Some(auto) = &mut next {
            auto.previous_due_date = previous;
            let due_date = auto.phrase.date.to_string();
            if draft.due_date != due_date {
                draft.cursors[TaskField::DueDate as usize] = Default::default();
                draft.due_date = due_date;
            }
        } else if let Some(auto) = &draft.auto_date {
            draft.due_date = auto.previous_due_date.clone();
            draft.cursors[TaskField::DueDate as usize] = Default::default();
        }
        draft.auto_date = next;
    }

    fn cancel_edit(&mut self) {
        self.new_task_parent = None;
        self.mode = Mode::Browse;
    }

    fn save_edit(&mut self) -> Result<()> {
        let Mode::Edit(id, mut draft) = std::mem::replace(&mut self.mode, Mode::Browse) else {
            return Ok(());
        };
        let mut saved = draft.clone();
        if let Some(auto) = &draft.auto_date {
            let text = if auto.field == TaskField::Title {
                &mut saved.title
            } else {
                &mut saved.description
            };
            *text = remove_date_phrase(text, auto.phrase.range.clone());
            saved.due_date = auto.phrase.date.to_string();
        }
        if saved.title.trim().is_empty() {
            draft.focus = TaskField::Title;
            self.message = if draft.title.trim().is_empty() {
                "Title cannot be empty"
            } else {
                "Title cannot be empty after removing the date phrase"
            }
            .into();
            self.mode = Mode::Edit(id, draft);
            return Ok(());
        }
        if let Err(error) = parse_due_date(&saved.due_date) {
            draft.focus = TaskField::DueDate;
            self.message = error.to_string();
            self.mode = Mode::Edit(id, draft);
            return Ok(());
        }
        let result = if let Some(parent) = self.new_task_parent {
            db::save_subtask_tree(&self.db, parent, &saved)
        } else if let Some(id) = id {
            db::reword_task(&self.db, id, &saved)
        } else {
            db::save_task_tree(&self.db, None, &saved, self.current_project)
        };
        if let Err(error) = result {
            self.mode = Mode::Edit(id, draft);
            return Err(error);
        }
        let added_child = self.new_task_parent.take().is_some();
        self.refresh_tasks()?;
        if added_child {
            if let Some(view) = &mut self.task_view {
                view.selected_child = view.children.len().saturating_sub(1);
            }
        }
        Ok(())
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
            KeyCode::Char('a') => {
                self.input_cursor = crate::editor::TextCursor::default();
                self.mode = Mode::ProjectEdit(None, String::new());
            }
            KeyCode::Char('e') => {
                if let Some(project) = self.projects.get(self.selected_project) {
                    self.input_cursor = crate::editor::TextCursor::default();
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
        match key {
            KeyCode::Esc => self.mode = Mode::Browse,
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
            KeyCode::Enter => {
                db::delete_project(&self.db, id)?;
                if self.current_project == Some(id) {
                    self.current_project = None;
                }
                self.mode = Mode::Browse;
                self.refresh_projects()?;
            }
            KeyCode::Esc => self.mode = Mode::Browse,
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
            KeyCode::Enter => {
                db::delete_task(&self.db, id)?;
                self.mode = Mode::Browse;
                self.refresh_tasks()?;
            }
            KeyCode::Esc => self.mode = Mode::Browse,
            _ => {}
        }
        Ok(())
    }
}

fn parent_title(tasks: &[Task], task: &Task) -> Option<String> {
    let parent = task.parent_id?;
    tasks
        .iter()
        .find(|candidate| candidate.id == parent)
        .map(|parent| parent.title.clone())
}

#[cfg(test)]
#[path = "tests/app.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/reset_history.rs"]
mod reset_history_tests;

#[cfg(test)]
#[path = "tests/editor.rs"]
mod editor_tests;

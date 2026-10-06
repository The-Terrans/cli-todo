use crate::{
    db::{self, Filter, Task},
    Result,
};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use rusqlite::Connection;

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
    ShowFilter(usize),
    Quit,
}

const ACTIONS: [(&str, Action); 8] = [
    ("Add task", Action::Add),
    ("Edit selected task", Action::Edit),
    ("Complete / reopen selected task", Action::ToggleCompletion),
    ("Delete selected task", Action::Delete),
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
}

pub struct App {
    db: Connection,
    pub tasks: Vec<Task>,
    pub selected_task: usize,
    pub navigation_focused: bool,
    pub selected_filter: usize,
    pub mode: Mode,
    pub message: String,
}

impl App {
    pub fn new(db: Connection) -> Result<Self> {
        let mut app = Self {
            db,
            tasks: vec![],
            selected_task: 0,
            navigation_focused: true,
            selected_filter: 0,
            mode: Mode::Browse,
            message: String::new(),
        };
        app.refresh_tasks()?;
        Ok(app)
    }

    fn refresh_tasks(&mut self) -> Result<()> {
        self.tasks = db::list_tasks(&self.db, FILTERS[self.selected_filter].1)?;
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
            self.toggle_palette();
            return Ok(false);
        }
        if !modifiers.is_empty() {
            return Ok(false);
        }
        self.handle_key(key.code)
    }

    fn handle_key(&mut self, key: KeyCode) -> Result<bool> {
        self.message.clear();
        if key == KeyCode::Char('q') && matches!(self.mode, Mode::Browse | Mode::Delete(_)) {
            return self.execute_action(Action::Quit);
        }
        match self.mode {
            Mode::Browse => self.handle_browse_key(key)?,
            Mode::Edit(..) => self.handle_edit_key(key)?,
            Mode::Delete(id) => self.handle_delete_key(key, id)?,
            Mode::Palette { .. } => return self.handle_palette_key(key),
        }
        Ok(false)
    }

    fn handle_browse_key(&mut self, key: KeyCode) -> Result<()> {
        match key {
            KeyCode::Tab | KeyCode::BackTab => self.navigation_focused = !self.navigation_focused,
            KeyCode::Char('a') => {
                self.execute_action(Action::Add)?;
            }
            _ if self.navigation_focused => self.handle_navigation_key(key)?,
            _ => self.handle_task_key(key)?,
        }
        Ok(())
    }

    fn handle_navigation_key(&mut self, key: KeyCode) -> Result<()> {
        let filter = match key {
            KeyCode::Down | KeyCode::Right => (self.selected_filter + 1) % FILTERS.len(),
            KeyCode::Up | KeyCode::Left => {
                (self.selected_filter + FILTERS.len() - 1) % FILTERS.len()
            }
            KeyCode::Enter => {
                self.navigation_focused = false;
                return Ok(());
            }
            _ => return Ok(()),
        };
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
            KeyCode::Char(' ') => Action::ToggleCompletion,
            _ => return Ok(()),
        };
        self.execute_action(action)?;
        Ok(())
    }

    fn execute_action(&mut self, action: Action) -> Result<bool> {
        match action {
            Action::Add => self.mode = Mode::Edit(None, String::new()),
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
            Action::ShowFilter(filter) => {
                self.select_filter(filter)?;
                self.navigation_focused = false;
            }
            Action::Quit => return Ok(true),
        }
        Ok(false)
    }

    fn toggle_palette(&mut self) {
        match self.mode {
            Mode::Browse => {
                self.mode = Mode::Palette {
                    query: String::new(),
                    selected: 0,
                }
            }
            Mode::Palette { .. } => self.mode = Mode::Browse,
            Mode::Edit(..) | Mode::Delete(_) => {}
        }
    }

    pub fn palette_actions(&self) -> Vec<(&'static str, Action)> {
        let Mode::Palette { query, .. } = &self.mode else {
            return vec![];
        };
        let query = query.trim().to_lowercase();
        let has_task = self.tasks.get(self.selected_task).is_some();
        ACTIONS
            .iter()
            .copied()
            .filter(|(label, action)| {
                (has_task || matches!(action, Action::Add | Action::ShowFilter(_) | Action::Quit))
                    && label.to_lowercase().contains(&query)
            })
            .collect()
    }

    fn handle_palette_key(&mut self, key: KeyCode) -> Result<bool> {
        let actions = self.palette_actions();
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
        db::save_task(&self.db, *id, text)?;
        self.mode = Mode::Browse;
        self.refresh_tasks()
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
        app.handle_key(KeyCode::Tab)?;
        app.handle_key(KeyCode::Char('e'))?;
        app.handle_key(KeyCode::Backspace)?;
        app.handle_key(KeyCode::Esc)?;
        assert!(matches!(app.mode, Mode::Browse));
        assert_eq!(db::list_tasks(&app.db, Filter::All)?[0].title, "original");
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
        app.handle_key(KeyCode::BackTab)?;
        assert!(app.navigation_focused);
        app.handle_key(KeyCode::Right)?;
        assert_eq!(app.selected_filter, 1);
        assert_eq!(app.selected_task, 0);
        app.handle_key(KeyCode::Left)?;
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
        app.handle_key(KeyCode::Tab)?;
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
        app.handle_key(KeyCode::Tab)?;
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

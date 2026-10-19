mod app;
mod constants;
mod dates;
mod db;
mod editor;
mod editor_command;
mod external_editor;
mod history;
mod types;
mod ui;

use crossterm::{
    cursor::SetCursorStyle,
    event::{self, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::DefaultTerminal;
use std::{env, fs, path::PathBuf, time::Duration};
use types::{App, Result};

fn main() -> Result<()> {
    let directory = data_directory()?;
    fs::create_dir_all(&directory)?;
    let db = db::open(&directory.join("tasks.sqlite3"))?;
    let mut app = App::new(db)?;
    run(&mut app)
}

fn data_directory() -> Result<PathBuf> {
    let root = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .ok_or("Set HOME or an absolute XDG_DATA_HOME for task storage")?;
    Ok(root.join("cli-todo"))
}

fn run(app: &mut App) -> Result<()> {
    let mut terminal = ratatui::try_init()?;
    let cursor_style = CursorStyle;
    let result = (|| {
        execute!(std::io::stdout(), SetCursorStyle::SteadyBlock)?;
        event_loop(&mut terminal, app)
    })();
    drop(cursor_style);
    ratatui::restore();
    result
}

// Also reset on unwinding; Ratatui's panic hook restores the remaining terminal state.
struct CursorStyle;

impl Drop for CursorStyle {
    fn drop(&mut self) {
        let _ = execute!(std::io::stdout(), SetCursorStyle::DefaultUserShape);
    }
}

fn edit_task_externally(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    let types::Mode::Edit(_, draft) = &app.mode else {
        return Ok(());
    };
    let session = match external_editor::ExternalEdit::prepare(draft) {
        Ok(session) => session,
        Err(error) => {
            app.message = error.to_string();
            return Ok(());
        }
    };
    let result = (|| -> Result<()> {
        terminal.show_cursor()?;
        execute!(
            std::io::stdout(),
            SetCursorStyle::DefaultUserShape,
            LeaveAlternateScreen
        )?;
        disable_raw_mode()?;
        session.launch()
    })();
    // Always restore the TUI, including after spawn failures and nonzero exits.
    if let Err(error) = resume_terminal(terminal, app) {
        return Err(session.recovery_message(error).into());
    }
    match result {
        Ok(()) => session.finish(app),
        Err(error) => app.message = session.recovery_message(error),
    }
    Ok(())
}

fn resume_terminal(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    enable_raw_mode()?;
    execute!(
        std::io::stdout(),
        EnterAlternateScreen,
        SetCursorStyle::SteadyBlock
    )?;
    terminal.hide_cursor()?;
    terminal.autoresize()?;
    terminal.clear()?;
    let size = terminal.size()?;
    app.viewport.width = size.width;
    app.resize(size.height);
    Ok(())
}

fn event_loop(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    let size = terminal.size()?;
    app.viewport.width = size.width;
    app.resize(size.height);
    loop {
        app.poll_sync();
        terminal.draw(|frame| ui::draw(frame, app))?;
        if !event::poll(Duration::from_millis(100))? {
            continue;
        }
        match event::read()? {
            Event::Key(key) => {
                if external_editor::requested(key, &app.mode) {
                    edit_task_externally(terminal, app)?;
                } else if app.handle_key_event(key)? {
                    return Ok(());
                }
            }
            Event::Resize(width, height) => {
                app.viewport.width = width;
                app.resize(height);
            }
            _ => {}
        }
    }
}

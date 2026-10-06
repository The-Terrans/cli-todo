mod app;
mod db;
mod ui;

use app::App;
use crossterm::event::{self, Event};
use ratatui::DefaultTerminal;
use std::{env, error::Error, fs, path::PathBuf};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

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
    let result = event_loop(&mut terminal, app);
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|frame| ui::draw(frame, app))?;
        if let Event::Key(key) = event::read()? {
            if app.handle_key_event(key)? {
                return Ok(());
            }
        }
    }
}

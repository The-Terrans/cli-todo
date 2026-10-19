use super::*;
use crate::{types::SubtaskDraft, ui};
use ratatui::{backend::TestBackend, Terminal};
use std::{fs, time::SystemTime};

fn open_reset(app: &mut App) -> Result<()> {
    app.handle_key_event(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL))?;
    for character in "Reset todo history".chars() {
        app.handle_key(KeyCode::Char(character))?;
    }
    assert_eq!(
        app.palette_actions(),
        [("Reset todo history", Action::ResetHistory)]
    );
    app.handle_key(KeyCode::Enter)?;
    Ok(())
}

#[test]
fn reset_history_preserves_all_todo_data_and_can_connect_a_fresh_repo() -> Result<()> {
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_nanos();
    let root = std::env::temp_dir().join(format!("cli-todo-reset-{stamp}"));
    fs::create_dir_all(&root)?;
    let path = root.join("tasks.sqlite3");
    let database = db::open(&path)?;
    db::save_project(&database, None, "Work")?;
    let project = database.last_insert_rowid();
    db::save_task_tree(
        &database,
        None,
        &TaskDraft {
            title: "parent".into(),
            description: "parent details".into(),
            due_date: "2028-02-29".into(),
            subtasks: vec![SubtaskDraft {
                id: None,
                done: true,
                draft: TaskDraft {
                    title: "child".into(),
                    description: "child details".into(),
                    due_date: "2028-03-01".into(),
                    ..TaskDraft::default()
                },
            }],
            ..TaskDraft::default()
        },
        Some(project),
    )?;
    let mut app = App::new(database)?;
    let history = app.history.as_ref().unwrap().clone();
    let old_remote = root.join("old-remote.git");
    assert!(std::process::Command::new("git")
        .args(["init", "--bare", "--quiet"])
        .arg(&old_remote)
        .status()?
        .success());
    history.set_remote(old_remote.to_str().unwrap())?;
    configure_author(&history)?;
    history.commit(&app.db, "old checkpoint")?;
    history.push()?;
    let old_head = fs::read(old_remote.join("refs/heads/main"))?;
    let backup = root.join("backups/keep.sqlite3");
    fs::create_dir_all(backup.parent().unwrap())?;
    fs::write(&backup, "existing backup")?;
    let tasks = db::list_tasks(&app.db, Filter::All)?;
    let projects = db::list_projects(&app.db)?;
    app.reload_commits();
    app.load_remote();
    app.focus_projects()?;
    app.navigation_focused = false;
    let id = app.tasks[0].id;
    app.open_task(id, false)?;
    app.task_view.as_mut().unwrap().focus = TaskPane::Subtasks;
    let context = (
        app.current_project,
        app.navigation_focused,
        app.projects_focused,
    );
    open_reset(&mut app)?;
    assert!(matches!(app.mode, Mode::ResetHistory { commits: 1 }));
    let mut terminal = Terminal::new(TestBackend::new(100, 25))?;
    terminal.draw(|frame| ui::draw(frame, &app))?;
    let text: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    for expected in [
        "Permanently delete 1 local commits",
        "All todos, subtasks, projects",
        "No new backup",
        "Enter: confirm",
        "Esc: cancel",
    ] {
        assert!(text.contains(expected), "missing {expected}");
    }
    for key in [
        KeyCode::Char('y'),
        KeyCode::Char('n'),
        KeyCode::Char('q'),
        KeyCode::Tab,
    ] {
        assert!(!app.handle_key(key)?);
        assert!(matches!(app.mode, Mode::ResetHistory { .. }));
        assert_eq!(history.list()?.len(), 1);
    }
    app.handle_key(KeyCode::Esc)?;
    assert_eq!(history.list()?.len(), 1);
    assert!(!history.remote()?.is_empty());
    open_reset(&mut app)?;
    app.handle_key(KeyCode::Enter)?;
    assert!(matches!(app.mode, Mode::Browse));
    assert_eq!(db::list_tasks(&app.db, Filter::All)?, tasks);
    assert_eq!(db::list_projects(&app.db)?, projects);
    assert_eq!(
        (
            app.current_project,
            app.navigation_focused,
            app.projects_focused
        ),
        context
    );
    assert_eq!(app.task_view.as_ref().unwrap().task.id, id);
    assert_eq!(app.task_view.as_ref().unwrap().focus, TaskPane::Subtasks);
    assert!(app.commits.is_empty() && app.remote_url.is_empty());
    assert!(app.uncommitted_changes);
    assert!(!history.directory.exists());
    assert_eq!(fs::read_to_string(&backup)?, "existing backup");
    assert_eq!(fs::read(old_remote.join("refs/heads/main"))?, old_head);
    drop(app);
    let mut app = App::new(db::open(&path)?)?;
    assert!(app.commits.is_empty() && app.remote_url.is_empty());
    assert_eq!(db::list_tasks(&app.db, Filter::All)?, tasks);
    let fresh_remote = root.join("fresh-remote.git");
    assert!(std::process::Command::new("git")
        .args(["init", "--bare", "--quiet"])
        .arg(&fresh_remote)
        .status()?
        .success());
    let history = app.history.as_ref().unwrap();
    history.set_remote(fresh_remote.to_str().unwrap())?;
    configure_author(history)?;
    history.commit(&app.db, "fresh checkpoint")?;
    history.push()?;
    assert_eq!(history.list()?.len(), 1);
    assert_eq!(history.list()?[0].subject, "fresh checkpoint");
    app.reload_commits();
    app.load_remote();
    open_reset(&mut app)?;
    app.handle_key(KeyCode::Enter)?;
    // Resetting an already empty history is also safe.
    open_reset(&mut app)?;
    app.handle_key(KeyCode::Enter)?;
    assert!(matches!(app.mode, Mode::Browse));
    assert_eq!(db::list_tasks(&app.db, Filter::All)?, tasks);
    drop(app);
    fs::remove_dir_all(root)?;
    Ok(())
}

fn configure_author(history: &History) -> Result<()> {
    for (key, value) in [
        ("user.name", "Todo Test"),
        ("user.email", "todo@example.test"),
        ("commit.gpgsign", "false"),
    ] {
        assert!(std::process::Command::new("git")
            .arg("-C")
            .arg(&history.directory)
            .args(["config", key, value])
            .status()?
            .success());
    }
    Ok(())
}

#[test]
fn reset_history_blocks_sync_and_reports_cleanup_failure_without_touching_todos() -> Result<()> {
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_nanos();
    let root = std::env::temp_dir().join(format!("cli-todo-reset-error-{stamp}"));
    fs::create_dir_all(&root)?;
    let mut app = App::new(db::open(&root.join("tasks.sqlite3"))?)?;
    db::save_task(&app.db, None, "keep")?;
    let (_sender, receiver) = mpsc::channel();
    app.sync_receiver = Some(receiver);
    open_reset(&mut app)?;
    assert!(matches!(app.mode, Mode::Browse));
    assert!(app.message.contains("Wait for sync"));
    app.sync_receiver = None;
    open_reset(&mut app)?;
    let (_sender, receiver) = mpsc::channel();
    app.sync_receiver = Some(receiver);
    app.handle_key(KeyCode::Enter)?;
    assert!(matches!(app.mode, Mode::ResetHistory { .. }));
    assert!(app.message.contains("Wait for sync"));
    app.sync_receiver = None;
    let directory = app.history.as_ref().unwrap().directory.clone();
    fs::write(&directory, "not a directory")?;
    app.handle_key(KeyCode::Enter)?;
    assert!(matches!(app.mode, Mode::ResetHistory { .. }));
    assert!(app.message.contains("Could not reset todo history"));
    assert_eq!(db::list_tasks(&app.db, Filter::All)?[0].title, "keep");
    assert_eq!(fs::read_to_string(&directory)?, "not a directory");
    fs::remove_file(&directory)?;
    app.handle_key(KeyCode::Enter)?;
    assert!(matches!(app.mode, Mode::Browse));
    drop(app);
    fs::remove_dir_all(root)?;
    Ok(())
}

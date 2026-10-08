
use super::*;
use std::path::Path;

fn app() -> Result<App> {
    App::new(db::open(Path::new(":memory:"))?)
}

fn search_actions(app: &mut App, query: &str) -> Result<()> {
    app.handle_key_event(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL))?;
    assert!(matches!(app.mode, Mode::Palette { .. }));
    for character in query.chars() {
        assert!(!app.handle_key_event(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE))?);
    }
    Ok(())
}

#[test]
fn keybindings_help_scrolls_closes_and_preserves_focus_and_text_entry() -> Result<()> {
    let mut app = app()?;
    db::save_project(&app.db, None, "Work")?;
    db::create_task(&app.db, "project task", Some(app.db.last_insert_rowid()))?;
    app.refresh_projects()?;
    app.resize(10);
    for section in ['1', '2', '0', '3'] {
        app.handle_key(KeyCode::Char(section))?;
        let focus = (
            app.navigation_focused,
            app.projects_focused,
            app.commits_focused,
            app.current_project,
            app.selected_task,
            app.selected_project,
            app.selected_filter,
        );
        app.handle_key_event(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT))?;
        assert!(matches!(app.mode, Mode::Help { scroll: 0 }));
        app.handle_key(KeyCode::Down)?;
        app.handle_key(KeyCode::PageDown)?;
        assert!(matches!(app.mode, Mode::Help { scroll: 7 }));
        app.handle_key(KeyCode::PageUp)?;
        app.handle_key(KeyCode::Up)?;
        app.handle_key(KeyCode::Up)?;
        assert!(matches!(app.mode, Mode::Help { scroll: 0 }));
        app.handle_key(KeyCode::End)?;
        app.handle_key(KeyCode::Down)?;
        assert!(
            matches!(app.mode, Mode::Help { scroll } if scroll == (KEYBINDINGS.len() - 6) as u16)
        );
        app.handle_key(KeyCode::Home)?;
        for ignored in ['q', 'D', 'r', 'p', '1'] {
            assert!(!app.handle_key(KeyCode::Char(ignored))?);
            assert!(matches!(app.mode, Mode::Help { .. }));
        }
        app.handle_key(KeyCode::Char('?'))?;
        assert!(matches!(app.mode, Mode::Browse));
        assert_eq!(
            focus,
            (
                app.navigation_focused,
                app.projects_focused,
                app.commits_focused,
                app.current_project,
                app.selected_task,
                app.selected_project,
                app.selected_filter
            )
        );
    }
    app.handle_key(KeyCode::Char('?'))?;
    app.handle_key(KeyCode::End)?;
    app.resize(100);
    assert!(matches!(app.mode, Mode::Help { scroll: 0 }));
    app.handle_key(KeyCode::Esc)?;
    app.handle_key(KeyCode::Char('2'))?;
    app.handle_key(KeyCode::Char('a'))?;
    app.handle_key(KeyCode::Char('?'))?;
    assert!(matches!(&app.mode, Mode::ProjectEdit(None, text) if text == "?"));
    app.handle_key(KeyCode::Esc)?;
    app.handle_key_event(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL))?;
    app.handle_key(KeyCode::Char('?'))?;
    assert!(matches!(&app.mode, Mode::Palette { query, .. } if query == "?"));
    app.handle_key(KeyCode::Esc)?;
    assert_eq!(db::list_tasks(&app.db, Filter::All)?.len(), 1);
    assert_eq!(db::list_projects(&app.db)?.len(), 1);
    Ok(())
}

#[test]
fn nuke_requires_confirmation_and_keeps_failed_deletions_atomic() -> Result<()> {
    let mut app = app()?;
    db::save_project(&app.db, None, "Work")?;
    let work = app.db.last_insert_rowid();
    db::create_task(&app.db, "completed project task", Some(work))?;
    db::toggle_task(&app.db, app.db.last_insert_rowid())?;
    db::save_project(&app.db, None, "Home")?;
    db::create_task(
        &app.db,
        "other project task",
        Some(app.db.last_insert_rowid()),
    )?;
    db::create_task(&app.db, "inbox task", None)?;
    app.refresh_projects()?;
    for section in ['1', '2', '3', '0'] {
        app.handle_key(KeyCode::Char(section))?;
        app.handle_key_event(KeyEvent::new(KeyCode::Char('D'), KeyModifiers::SHIFT))?;
        assert!(matches!(
            app.mode,
            Mode::Nuke {
                tasks: 3,
                projects: 2,
                ..
            }
        ));
        for ignored in ['q', 'y', 'n'] {
            assert!(!app.handle_key(KeyCode::Char(ignored))?);
            assert!(matches!(app.mode, Mode::Nuke { .. }));
            assert_eq!(db::list_tasks(&app.db, Filter::All)?.len(), 3);
        }
        app.handle_key(KeyCode::Esc)?;
        assert!(matches!(app.mode, Mode::Browse));
        assert_eq!(db::list_tasks(&app.db, Filter::All)?.len(), 3);
        assert_eq!(db::list_projects(&app.db)?.len(), 2);
    }
    app.handle_key(KeyCode::Char('1'))?;
    app.handle_key(KeyCode::Char('a'))?;
    app.handle_key_event(KeyEvent::new(KeyCode::Char('D'), KeyModifiers::SHIFT))?;
    assert!(matches!(&app.mode, Mode::Edit(None, text) if text.title == "D"));
    app.handle_key(KeyCode::Esc)?;
    app.handle_key(KeyCode::Char('2'))?;
    app.handle_key(KeyCode::Enter)?;
    app.handle_key(KeyCode::Char('D'))?;
    app.db.execute_batch("CREATE TRIGGER prevent_nuke BEFORE DELETE ON projects BEGIN SELECT RAISE(ABORT, 'blocked'); END;")?;
    app.handle_key(KeyCode::Enter)?;
    assert!(matches!(app.mode, Mode::Nuke { .. }));
    assert!(app.message.contains("Could not nuke"));
    assert_eq!(db::list_tasks(&app.db, Filter::All)?.len(), 3);
    assert_eq!(db::list_projects(&app.db)?.len(), 2);
    app.db.execute_batch("DROP TRIGGER prevent_nuke;")?;
    app.handle_key(KeyCode::Enter)?;
    assert!(matches!(app.mode, Mode::Browse));
    assert!(app.tasks.is_empty() && app.projects.is_empty());
    assert!(db::list_tasks(&app.db, Filter::All)?.is_empty());
    assert!(db::list_projects(&app.db)?.is_empty());
    assert!(app.navigation_focused && !app.projects_focused && !app.commits_focused);
    assert_eq!(app.current_project, None);
    assert_eq!(app.selected_task, 0);
    assert_eq!(app.selected_project, 0);
    assert_eq!(app.selected_filter, 0);
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
    app.handle_key(KeyCode::Char('D'))?;
    assert!(matches!(app.mode, Mode::Browse));
    assert!(app.message.contains("Wait for sync"));
    assert!(!app.handle_key(KeyCode::Char('q'))?);
    app.handle_key(KeyCode::Char('1'))?;
    app.handle_key(KeyCode::Char('?'))?;
    wait_for_sync(&mut app);
    assert_eq!(app.tasks[0].title, "remote task", "{}", app.message);
    assert!(!app.uncommitted_changes);
    assert!(app.message.contains("pulled and applied"));
    assert!(matches!(app.mode, Mode::Help { .. }));
    app.handle_key(KeyCode::Esc)?;
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
    assert!(matches!(app.mode, Mode::NoChanges));
    assert!(app.message.is_empty());
    assert!(!root.join("history").exists());
    for ignored in ['q', 'c', 'y', 'n', '1'] {
        assert!(!app.handle_key(KeyCode::Char(ignored))?);
        assert!(matches!(app.mode, Mode::NoChanges));
    }
    app.handle_key(KeyCode::Esc)?;
    assert!(matches!(app.mode, Mode::Browse));
    assert!(app.navigation_focused && app.commits_focused);
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
    assert!(matches!(app.mode, Mode::NoChanges));
    assert_eq!(app.commits.len(), 1);
    app.handle_key(KeyCode::Enter)?;
    assert!(matches!(app.mode, Mode::Browse));
    assert!(app.navigation_focused && app.commits_focused);
    drop(app);
    let mut app = App::new(db::open(&path)?)?;
    assert!(!app.uncommitted_changes);
    assert_eq!(app.commits.len(), 1);
    assert!(!app.commits_focused);
    app.handle_key(KeyCode::Enter)?;
    app.handle_key(KeyCode::Char(' '))?;
    assert!(app.uncommitted_changes);
    drop(app);
    let mut app = App::new(db::open(&path)?)?;
    assert!(app.uncommitted_changes);
    app.history
        .as_ref()
        .unwrap()
        .set_remote("/not-contacted/remote.git")?;
    app.load_remote();
    let backup = root.join("backups/keep.sqlite3");
    fs::create_dir_all(backup.parent().unwrap())?;
    fs::write(&backup, "existing backup")?;
    app.handle_key(KeyCode::Char('D'))?;
    assert!(matches!(app.mode, Mode::Nuke { commits: 1, .. }));
    app.handle_key(KeyCode::Esc)?;
    assert_eq!(app.history.as_ref().unwrap().list()?.len(), 1);
    assert_eq!(
        app.history.as_ref().unwrap().remote()?,
        "/not-contacted/remote.git"
    );
    app.db.execute_batch("CREATE TRIGGER prevent_nuke BEFORE DELETE ON tasks BEGIN SELECT RAISE(ABORT, 'blocked'); END;")?;
    app.handle_key(KeyCode::Char('D'))?;
    app.handle_key(KeyCode::Enter)?;
    assert!(matches!(app.mode, Mode::Nuke { .. }));
    assert!(app.message.contains("Could not nuke"));
    assert_eq!(db::list_tasks(&app.db, Filter::All)?.len(), 1);
    assert_eq!(app.history.as_ref().unwrap().list()?.len(), 1);
    assert_eq!(
        app.history.as_ref().unwrap().remote()?,
        "/not-contacted/remote.git"
    );
    assert!(!fs::read_dir(&root)?.any(|entry| entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".nuke-history-")));
    app.db.execute_batch("DROP TRIGGER prevent_nuke;")?;
    app.handle_key(KeyCode::Enter)?;
    assert!(app.tasks.is_empty() && app.projects.is_empty());
    assert!(app.commits.is_empty());
    assert!(app.history.as_ref().unwrap().list()?.is_empty());
    assert!(app.history.as_ref().unwrap().remote()?.is_empty());
    assert!(app.remote_url.is_empty());
    assert!(!history.exists());
    assert_eq!(fs::read_to_string(&backup)?, "existing backup");
    assert!(!app.uncommitted_changes);
    drop(app);
    let app = App::new(db::open(&path)?)?;
    assert!(app.tasks.is_empty() && app.projects.is_empty());
    assert!(app.commits.is_empty());
    assert!(app.remote_url.is_empty());
    assert!(!app.uncommitted_changes);
    drop(app);
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn task_editor_toggles_fields_and_saves_multiline_descriptions() -> Result<()> {
    let mut app = app()?;
    app.handle_key(KeyCode::Char('a'))?;
    app.handle_key(KeyCode::Char('t'))?;
    app.handle_key_event(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL))?;
    assert!(matches!(app.mode, Mode::Edit(..)));
    assert!(db::list_tasks(&app.db, Filter::All)?.is_empty());
    app.handle_key(KeyCode::Tab)?;
    app.handle_key(KeyCode::Char('界'))?;
    app.handle_key(KeyCode::Enter)?;
    app.handle_key(KeyCode::Char('?'))?;
    app.handle_key(KeyCode::Backspace)?;
    app.handle_key(KeyCode::Char('c'))?;
    assert!(
        matches!(&app.mode, Mode::Edit(_, draft) if draft.title == "t" && draft.description == "界\nc" && draft.description_focused)
    );
    app.handle_key_event(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL))?;
    assert!(matches!(app.mode, Mode::Edit(..)));
    app.handle_key_event(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL))?;
    assert!(matches!(app.mode, Mode::Browse));
    assert_eq!(app.tasks[0].description, "界\nc");
    app.handle_key(KeyCode::Enter)?;
    app.handle_key(KeyCode::Char('e'))?;
    assert!(
        matches!(&app.mode, Mode::Edit(_, draft) if draft.title == "t" && draft.description == "界\nc" && !draft.description_focused)
    );
    app.handle_key(KeyCode::Tab)?;
    app.handle_key(KeyCode::Char('!'))?;
    app.handle_key(KeyCode::Esc)?;
    assert_eq!(app.tasks[0].description, "界\nc");
    app.handle_key(KeyCode::Char('e'))?;
    app.handle_key(KeyCode::Tab)?;
    app.handle_key(KeyCode::Char('!'))?;
    app.handle_key(KeyCode::Tab)?;
    app.handle_key(KeyCode::Enter)?;
    assert_eq!(app.tasks[0].description, "界\nc!");
    app.handle_key(KeyCode::Char('a'))?;
    app.handle_key(KeyCode::Tab)?;
    app.handle_key(KeyCode::Char('x'))?;
    app.handle_key_event(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL))?;
    assert_eq!(app.message, "Title cannot be empty");
    assert!(
        matches!(&app.mode, Mode::Edit(_, draft) if !draft.description_focused && draft.description == "x")
    );
    app.handle_key(KeyCode::Char('n'))?;
    app.handle_key(KeyCode::Enter)?;
    assert_eq!(db::list_tasks(&app.db, Filter::All)?[1].description, "x");
    for description in [false, true] {
        app.handle_key(KeyCode::Char('a'))?;
        if description {
            app.handle_key(KeyCode::Tab)?;
        }
        app.handle_key(KeyCode::Char('z'))?;
        app.handle_key(KeyCode::Esc)?;
        assert_eq!(db::list_tasks(&app.db, Filter::All)?.len(), 2);
    }
    Ok(())
}

#[test]
fn commit_shortcut_works_in_every_panel_and_preserves_focus() -> Result<()> {
    let mut app = app()?;
    db::save_project(&app.db, None, "Work")?;
    db::create_task(&app.db, "project task", Some(app.db.last_insert_rowid()))?;
    db::save_task(&app.db, None, "inbox task")?;
    app.refresh_projects()?;
    for section in ['1', '2', '3'] {
        for preview in [false, true] {
            app.handle_key(KeyCode::Char(section))?;
            if preview {
                app.handle_key(KeyCode::Char('0'))?;
            }
            let focus = (
                app.navigation_focused,
                app.projects_focused,
                app.commits_focused,
                app.current_project,
                app.selected_task,
                app.selected_project,
                app.selected_filter,
            );
            app.handle_key(KeyCode::Char('c'))?;
            assert!(matches!(&app.mode, Mode::CommitEdit(text) if text.is_empty()));
            app.handle_key(KeyCode::Char('c'))?;
            assert!(matches!(&app.mode, Mode::CommitEdit(text) if text == "c"));
            app.handle_key(KeyCode::Esc)?;
            assert!(matches!(app.mode, Mode::Browse));
            assert_eq!(
                focus,
                (
                    app.navigation_focused,
                    app.projects_focused,
                    app.commits_focused,
                    app.current_project,
                    app.selected_task,
                    app.selected_project,
                    app.selected_filter
                )
            );
        }
    }
    app.handle_key(KeyCode::Char('1'))?;
    app.handle_key(KeyCode::Char('a'))?;
    app.handle_key(KeyCode::Char('c'))?;
    assert!(matches!(&app.mode, Mode::Edit(None, text) if text.title == "c"));
    app.handle_key(KeyCode::Esc)?;
    assert_eq!(db::list_tasks(&app.db, Filter::All)?.len(), 2);
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
    for ignored in ['y', 'n', 'q'] {
        assert!(!app.handle_key(KeyCode::Char(ignored))?);
        assert!(matches!(app.mode, Mode::ProjectDelete(_)));
        assert_eq!(app.projects.len(), 1);
    }
    app.handle_key(KeyCode::Enter)?;
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
    assert!(matches!(&app.mode, Mode::Edit(None, text) if text.title == "01"));
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
    app.handle_key(KeyCode::Esc)?;
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
    assert!(matches!(&app.mode, Mode::Edit(None, text) if text.title == "x"));
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
    app.handle_key(KeyCode::Char('d'))?;
    for ignored in ['y', 'n', 'q'] {
        assert!(!app.handle_key(KeyCode::Char(ignored))?);
        assert!(matches!(app.mode, Mode::Delete(_)));
        assert_eq!(db::list_tasks(&app.db, Filter::All)?.len(), 1);
    }
    app.handle_key(KeyCode::Esc)?;
    assert!(matches!(app.mode, Mode::Browse));
    assert_eq!(db::list_tasks(&app.db, Filter::All)?.len(), 1);
    app.handle_key(KeyCode::Char('d'))?;
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

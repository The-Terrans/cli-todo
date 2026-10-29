use super::*;
use crate::{db, types::SubtaskDraft};
use std::sync::atomic::{AtomicU64, Ordering};

struct TempDirectory(PathBuf);
impl TempDirectory {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "cli-todo-external-test-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
}
impl Drop for TempDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn command() -> EditorCommand {
    EditorCommand::parse("unused-editor").unwrap()
}

#[test]
fn shortcut_only_opens_from_task_popups_on_control_g_press() {
    let mode = Mode::Edit(None, TaskDraft::default());
    assert!(requested(
        KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL),
        &mode
    ));
    assert!(requested(
        KeyEvent::new(
            KeyCode::Char('G'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT
        ),
        &mode
    ));
    for key in [
        KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Char('g'), KeyModifiers::ALT),
        KeyEvent::new_with_kind(
            KeyCode::Char('g'),
            KeyModifiers::CONTROL,
            KeyEventKind::Release,
        ),
    ] {
        assert!(!requested(key, &mode));
    }
    for mode in [
        Mode::Browse,
        Mode::Help { scroll: 0 },
        Mode::ProjectEdit(None, String::new()),
        Mode::CommitEdit(String::new()),
        Mode::RemoteEdit(String::new()),
        Mode::Palette {
            query: String::new(),
            selected: 0,
        },
    ] {
        assert!(!requested(
            KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL),
            &mode
        ));
    }
}

#[test]
fn file_format_preserves_description_whitespace_and_task_structure() -> Result<()> {
    let previous = TaskDraft {
        title: "before".into(),
        due_date: "2027-01-01".into(),
        focus: TaskField::Description,
        depth: 3,
        subtasks: vec![SubtaskDraft {
            id: Some(42),
            done: true,
            draft: TaskDraft {
                title: "child".into(),
                ..TaskDraft::default()
            },
        }],
        ..TaskDraft::default()
    };
    let imported = parse_task(
        "Title: after tomorrow\nDue date: 2028-02-29\n\n  next week\n\ntrailing spaces  \n",
        &previous,
    )?;
    assert_eq!(imported.title, "after tomorrow");
    assert_eq!(imported.due_date, "2028-02-29");
    assert_eq!(imported.description, "  next week\n\ntrailing spaces  \n");
    assert_eq!(imported.focus, TaskField::Description);
    assert_eq!(imported.depth, 3);
    assert_eq!(imported.subtasks[0].id, Some(42));
    assert!(imported.subtasks[0].done);
    assert_eq!(imported.subtasks[0].draft.title, "child");
    assert!(imported.auto_date.is_none());
    assert!(imported
        .ignored_dates
        .iter()
        .any(|(_, phrase)| phrase == "tomorrow"));
    assert!(imported
        .ignored_dates
        .iter()
        .any(|(_, phrase)| phrase == "next week"));
    let imported = parse_task("Title: CRLF\r\nDue date: \r\n\r\nbody\r\n", &previous)?;
    assert_eq!(imported.title, "CRLF");
    assert!(imported.due_date.is_empty());
    assert_eq!(imported.description, "body\r\n");
    let placeholder = parse_task(
        "Title: no deadline\nDue date:  YYYY-MM-DD \n\nbody",
        &previous,
    )?;
    assert!(
        placeholder.due_date.is_empty(),
        "unchanged placeholder must clear any previous date"
    );
    assert!(parse_task("Title: blank body\nDue date:\n\n", &previous)?
        .description
        .is_empty());
    for invalid in [
        "",
        "Title: task",
        "Title: \nDue date:\n\n",
        "Wrong: task\nDue date:\n\n",
        "Title: task\nWrong:\n\n",
        "Title: task\nDue date: 2026-02-30\n\n",
        "Title: task\nDue date: YYYY-01-DD\n\n",
        "Title: task\nDue date:\nnot blank\nbody",
        "Title: task\nDue date:\n",
    ] {
        assert!(
            parse_task(invalid, &previous).is_err(),
            "accepted {invalid:?}"
        );
    }
    Ok(())
}

#[test]
fn private_file_import_is_draft_only_and_success_removes_temporary_data() -> Result<()> {
    let root = TempDirectory::new();
    let mut app = App::new_showing_all(db::open(Path::new(":memory:"))?)?;
    app.mode = Mode::Edit(
        None,
        TaskDraft {
            title: "before".into(),
            ..TaskDraft::default()
        },
    );
    let Mode::Edit(_, draft) = &app.mode else {
        unreachable!()
    };
    let session = ExternalEdit::create(draft, command(), &root.0)?;
    let path = session.path.clone();
    assert_eq!(
        fs::read_to_string(&path)?,
        "Title: before\nDue date: YYYY-MM-DD\n\n"
    );
    let dated = TaskDraft {
        title: "dated".into(),
        due_date: "2028-02-29".into(),
        ..TaskDraft::default()
    };
    let existing = ExternalEdit::create(&dated, command(), &root.0)?;
    assert!(fs::read_to_string(&existing.path)?.contains("Due date: 2028-02-29\n"));
    fs::remove_dir_all(existing.path.parent().unwrap())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(fs::metadata(&path)?.permissions().mode() & 0o777, 0o600);
        assert_eq!(
            fs::metadata(path.parent().unwrap())?.permissions().mode() & 0o777,
            0o700
        );
    }
    fs::write(
        &path,
        "Title: task tomorrow\nDue date: 2028-02-29\n\nnext week literally\n",
    )?;
    session.finish(&mut app);
    assert!(!path.parent().unwrap().exists());
    assert!(db::list_tasks(&app.db, crate::types::Filter::All)?.is_empty());
    assert!(matches!(&app.mode, Mode::Edit(_, draft) if draft.title == "task tomorrow"));
    // Even typing after import cannot silently reinterpret imported date phrases.
    app.handle_key_event(KeyEvent::new(KeyCode::Char('!'), KeyModifiers::NONE))?;
    assert!(
        matches!(&app.mode, Mode::Edit(_, draft) if draft.auto_date.is_none() && draft.due_date == "2028-02-29")
    );
    app.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))?;
    let task = db::list_tasks(&app.db, crate::types::Filter::All)?.remove(0);
    assert_eq!(task.title, "task tomorrow!");
    assert_eq!(task.description, "next week literally\n");
    assert_eq!(task.due_date.unwrap().to_string(), "2028-02-29");
    Ok(())
}

#[test]
fn invalid_import_keeps_original_draft_and_recovery_file() -> Result<()> {
    let root = TempDirectory::new();
    for bytes in [
        b"bad header".as_slice(),
        b"Title: task\nDue date: impossible\n\n",
        b"\xff",
    ] {
        let mut app = App::new_showing_all(db::open(Path::new(":memory:"))?)?;
        app.mode = Mode::Edit(
            None,
            TaskDraft {
                title: "keep".into(),
                description: "original".into(),
                ..TaskDraft::default()
            },
        );
        let Mode::Edit(_, draft) = &app.mode else {
            unreachable!()
        };
        let session = ExternalEdit::create(draft, command(), &root.0)?;
        let path = session.path.clone();
        fs::write(&path, bytes)?;
        session.finish(&mut app);
        assert!(
            matches!(&app.mode, Mode::Edit(_, draft) if draft.title == "keep" && draft.description == "original")
        );
        assert!(app.message.contains(path.to_str().unwrap()));
        assert_eq!(fs::read(&path)?, bytes);
    }
    let draft = TaskDraft {
        title: "multiline\ntitle".into(),
        ..TaskDraft::default()
    };
    assert!(ExternalEdit::create(&draft, command(), &root.0).is_err());
    Ok(())
}

#[test]
fn editor_spawn_and_nonzero_exit_failures_retain_files() -> Result<()> {
    let root = TempDirectory::new();
    let draft = TaskDraft {
        title: "keep".into(),
        ..TaskDraft::default()
    };
    let session = ExternalEdit::create(
        &draft,
        EditorCommand::parse("/nonexistent/cli-todo-editor")?,
        &root.0,
    )?;
    assert!(session.launch().is_err());
    assert!(session.path.exists());
    #[cfg(unix)]
    {
        let session = ExternalEdit::create(
            &draft,
            EditorCommand::parse("/bin/sh -c 'exit 7'")?,
            &root.0,
        )?;
        assert!(session.launch().is_err());
        assert!(session.path.exists());
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn cleanup_failure_after_import_names_the_directory_without_implying_a_draft() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let root = TempDirectory::new();
    let mut app = App::new_showing_all(db::open(Path::new(":memory:"))?)?;
    app.mode = Mode::Edit(None, TaskDraft::default());
    let Mode::Edit(_, draft) = &app.mode else {
        unreachable!()
    };
    let session = ExternalEdit::create(draft, command(), &root.0)?;
    let directory = session.path.parent().unwrap().to_path_buf();
    fs::write(&session.path, "Title: imported\nDue date: YYYY-MM-DD\n\n")?;
    // A read-only parent blocks removing the session directory.
    fs::set_permissions(&root.0, fs::Permissions::from_mode(0o500))?;
    assert!(session.finish(&mut app));
    fs::set_permissions(&root.0, fs::Permissions::from_mode(0o700))?;
    if !directory.exists() {
        return Ok(()); // Running as root ignores the read-only parent.
    }
    assert!(matches!(&app.mode, Mode::Edit(_, draft) if draft.title == "imported"));
    assert!(app.message.starts_with("Temporary directory not removed: "));
    assert!(app.message.contains(directory.to_str().unwrap()));
    assert!(!app.message.contains("Recovery") && !app.message.contains("Draft"));
    Ok(())
}

#[test]
fn recovery_paths_wrap_in_task_popup_errors() -> Result<()> {
    use ratatui::{backend::TestBackend, Terminal};
    let mut app = App::new_showing_all(db::open(Path::new(":memory:"))?)?;
    for focus in [TaskField::Title, TaskField::Description, TaskField::DueDate] {
        app.mode = Mode::Edit(
            None,
            TaskDraft {
                title: "keep".into(),
                focus,
                ..TaskDraft::default()
            },
        );
        app.message = format!(
            "Recovery: /tmp/{}/task.txt — invalid external format",
            "private-directory/".repeat(7)
        );
        let mut terminal = Terminal::new(TestBackend::new(80, 25))?;
        terminal.draw(|frame| crate::ui::draw(frame, &app))?;
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(
            text.contains("task.txt"),
            "recovery path tail must be visible"
        );
        assert!(text.contains("invalid external format"));
    }
    Ok(())
}

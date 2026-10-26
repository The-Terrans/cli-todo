use super::tests::keys;
use super::*;
use crate::editor::TextCursor;
use std::path::Path;

#[test]
fn e_edits_tasks_and_subtasks_while_remote_r_and_project_e_stay_unchanged() -> Result<()> {
    let mut app = App::new_showing_all(db::open(Path::new(":memory:"))?)?;
    db::save_task_tree(
        &app.db,
        None,
        &TaskDraft {
            title: "parent".into(),
            subtasks: vec![crate::types::SubtaskDraft {
                id: None,
                done: false,
                draft: TaskDraft {
                    title: "child".into(),
                    ..TaskDraft::default()
                },
            }],
            ..TaskDraft::default()
        },
        None,
    )?;
    app.refresh_tasks()?;
    app.navigation_focused = false;
    // Main list, parent Details, parent Subtasks, and child Details.
    for step in 0..4 {
        app.handle_key(KeyCode::Char('r'))?;
        assert!(
            matches!(app.mode, Mode::Sort { .. }),
            "r sorts tasks instead of editing them"
        );
        app.handle_key(KeyCode::Esc)?;
        let title = if step == 3 { "child" } else { "parent" };
        app.handle_key(KeyCode::Char('e'))?;
        assert!(matches!(&app.mode, Mode::Edit(_, draft) if draft.title == title));
        keys(&mut app, &[KeyCode::Char('e'), KeyCode::Char('r')])?;
        assert!(matches!(&app.mode, Mode::Edit(_, draft) if draft.title == format!("{title}er")));
        app.handle_key(KeyCode::Esc)?;
        match step {
            0 | 2 => {
                app.handle_key(KeyCode::Enter)?;
            }
            1 => {
                app.handle_key(KeyCode::Right)?;
            }
            _ => {}
        }
    }
    app.handle_key(KeyCode::Char('3'))?;
    app.handle_key(KeyCode::Char('e'))?;
    assert!(matches!(app.mode, Mode::Browse));
    app.handle_key(KeyCode::Char('r'))?;
    assert!(matches!(app.mode, Mode::RemoteEdit(_)));
    app.handle_key(KeyCode::Esc)?;
    db::save_project(&app.db, None, "Work")?;
    app.refresh_projects()?;
    app.handle_key(KeyCode::Char('2'))?;
    app.handle_key(KeyCode::Char('e'))?;
    assert!(matches!(&app.mode, Mode::ProjectEdit(Some(_), name) if name == "Work"));
    Ok(())
}

#[test]
fn cursor_inserts_and_deletes_unicode_without_crossing_utf8_boundaries() {
    let mut cursor = TextCursor::default();
    let mut text = "a界é".to_string();
    for key in [
        KeyCode::Left,
        KeyCode::Char('🙂'),
        KeyCode::Left,
        KeyCode::Delete,
    ] {
        assert!(cursor.handle(&mut text, key, false));
    }
    assert_eq!(text, "a界é");
    assert_eq!(cursor.position(&text), "a界".len());
    cursor.handle(&mut text, KeyCode::Backspace, false);
    assert_eq!(text, "aé");
    cursor.handle(&mut text, KeyCode::Home, false);
    cursor.handle(&mut text, KeyCode::Backspace, false);
    assert_eq!(cursor.marked(&text), "▏aé");
    cursor.handle(&mut text, KeyCode::Char('界'), false);
    assert_eq!(text, "界aé");
    cursor.handle(&mut text, KeyCode::End, false);
    cursor.handle(&mut text, KeyCode::Delete, false);
    assert_eq!(cursor.marked(&text), "界aé▏");
    assert!(!cursor.handle(&mut text, KeyCode::Up, false));
    assert!(!cursor.handle(&mut text, KeyCode::Enter, false));
}

#[test]
fn vertical_movement_uses_explicit_lines_and_preserves_preferred_display_column() {
    let mut cursor = TextCursor::default();
    let mut text = "abcd\nx\n界abcd\n".to_string();
    cursor.handle(&mut text, KeyCode::Up, true);
    assert_eq!(cursor.marked(&text), "abcd\nx\n▏界abcd\n");
    cursor.handle(&mut text, KeyCode::End, true);
    cursor.handle(&mut text, KeyCode::Left, true);
    cursor.handle(&mut text, KeyCode::Left, true);
    assert_eq!(cursor.marked(&text), "abcd\nx\n界ab▏cd\n");
    cursor.handle(&mut text, KeyCode::Up, true);
    assert_eq!(cursor.marked(&text), "abcd\nx▏\n界abcd\n");
    cursor.handle(&mut text, KeyCode::Up, true);
    assert_eq!(cursor.marked(&text), "abcd▏\nx\n界abcd\n");
    cursor.handle(&mut text, KeyCode::Up, true);
    cursor.handle(&mut text, KeyCode::Down, true);
    cursor.handle(&mut text, KeyCode::Down, true);
    assert_eq!(cursor.marked(&text), "abcd\nx\n界ab▏cd\n");
    cursor.handle(&mut text, KeyCode::Home, true);
    cursor.handle(&mut text, KeyCode::Backspace, true);
    assert_eq!(text, "abcd\nx界abcd\n");
    cursor.handle(&mut text, KeyCode::Char('\n'), true);
    assert_eq!(text, "abcd\nx\n界abcd\n");
    cursor.handle(&mut text, KeyCode::Home, true);
    cursor.handle(&mut text, KeyCode::Up, true);
    cursor.handle(&mut text, KeyCode::Down, true);
    assert_eq!(cursor.marked(&text), "abcd\nx\n▏界abcd\n");
}

#[test]
fn task_editor_remembers_each_cursor_and_saves_in_place_changes() -> Result<()> {
    let mut app = App::new_showing_all(db::open(Path::new(":memory:"))?)?;
    app.mode = Mode::Edit(
        None,
        TaskDraft {
            title: "abcd".into(),
            description: "first\nsecond\nlast".into(),
            due_date: "2028-02-29".into(),
            ..TaskDraft::default()
        },
    );
    keys(
        &mut app,
        &[
            KeyCode::Home,
            KeyCode::Right,
            KeyCode::Char('!'),
            KeyCode::Tab,
            KeyCode::Home,
            KeyCode::Up,
            KeyCode::Char('X'),
            KeyCode::Enter,
            KeyCode::Tab,
            KeyCode::Left,
            KeyCode::Backspace,
            KeyCode::Char('2'),
            KeyCode::BackTab,
            KeyCode::Char('Y'),
            KeyCode::BackTab,
            KeyCode::Delete,
        ],
    )?;
    let Mode::Edit(_, draft) = &app.mode else {
        panic!("editor closed");
    };
    assert_eq!(draft.title, "a!cd");
    assert_eq!(draft.description, "first\nX\nYsecond\nlast");
    assert_eq!(draft.due_date, "2028-02-29");
    assert_eq!(draft.cursors[0].position(&draft.title), 2);
    app.handle_key(KeyCode::Enter)?;
    let task = db::list_tasks(&app.db, Filter::All)?.remove(0);
    assert_eq!(task.title, "a!cd");
    assert_eq!(task.description, "first\nX\nYsecond\nlast");
    assert_eq!(task.due_date.unwrap().to_string(), "2028-02-29");
    // Rewording begins at each field's end, not the previous editor's cursor.
    app.navigation_focused = false;
    app.execute_action(Action::Edit)?;
    app.handle_key(KeyCode::Char('!'))?;
    assert!(matches!(&app.mode, Mode::Edit(_, draft) if draft.title == "a!cd!"));
    app.handle_key(KeyCode::Esc)?;
    assert_eq!(db::list_tasks(&app.db, Filter::All)?[0].title, "a!cd");
    Ok(())
}

#[test]
fn automatic_dates_follow_middle_edits_and_delete_dismisses_only_the_touched_phrase() -> Result<()>
{
    let mut app = App::new_showing_all(db::open(Path::new(":memory:"))?)?;
    app.handle_key(KeyCode::Char('a'))?;
    for character in "work tomorrow tail".chars() {
        app.handle_key(KeyCode::Char(character))?;
    }
    keys(&mut app, &[KeyCode::Home, KeyCode::Char('!')])?;
    let Mode::Edit(_, draft) = &app.mode else {
        panic!("editor closed");
    };
    assert_eq!(
        &draft.title[draft.auto_date.as_ref().unwrap().phrase.range.clone()],
        "tomorrow"
    );
    keys(&mut app, &[KeyCode::Delete])?;
    assert!(matches!(&app.mode, Mode::Edit(_, draft) if draft.auto_date.is_some()));
    keys(
        &mut app,
        &[
            KeyCode::Right,
            KeyCode::Right,
            KeyCode::Right,
            KeyCode::Right,
            KeyCode::Delete,
        ],
    )?;
    assert!(
        matches!(&app.mode, Mode::Edit(_, draft) if draft.auto_date.is_none() && draft.due_date.is_empty())
    );
    app.handle_key(KeyCode::Esc)?;
    app.handle_key(KeyCode::Char('a'))?;
    for character in "task tomorrow".chars() {
        app.handle_key(KeyCode::Char(character))?;
    }
    // Moving the cursor must not dismiss the automatic date.
    keys(
        &mut app,
        &[
            KeyCode::Home,
            KeyCode::Right,
            KeyCode::End,
            KeyCode::Tab,
            KeyCode::Tab,
            KeyCode::Home,
        ],
    )?;
    assert!(matches!(&app.mode, Mode::Edit(_, draft) if draft.auto_date.is_some()));
    app.handle_key(KeyCode::Delete)?;
    assert!(
        matches!(&app.mode, Mode::Edit(_, draft) if draft.auto_date.is_none() && draft.due_date.len() == 9)
    );
    Ok(())
}

#[test]
fn compact_editors_share_cursor_keys_and_reset_cursor_on_open() -> Result<()> {
    let mut app = App::new_showing_all(db::open(Path::new(":memory:"))?)?;
    for mode in [
        Mode::ProjectEdit(None, "abc".into()),
        Mode::CommitEdit("abc".into()),
        Mode::RemoteEdit("abc".into()),
    ] {
        app.mode = mode;
        app.input_cursor = TextCursor::default();
        keys(
            &mut app,
            &[
                KeyCode::Home,
                KeyCode::Right,
                KeyCode::Char('界'),
                KeyCode::Delete,
                KeyCode::Backspace,
            ],
        )?;
        let (Mode::ProjectEdit(_, text) | Mode::CommitEdit(text) | Mode::RemoteEdit(text)) =
            &app.mode
        else {
            panic!("editor closed");
        };
        assert_eq!(text, "ac");
        app.handle_key(KeyCode::Esc)?;
        assert!(matches!(app.mode, Mode::Browse));
    }
    app.remote_url = "repo".into();
    app.begin_remote_edit();
    app.handle_key(KeyCode::Char('!'))?;
    assert!(matches!(&app.mode, Mode::RemoteEdit(text) if text == "repo!"));
    app.handle_key(KeyCode::Esc)?;
    app.handle_key(KeyCode::Char('2'))?;
    app.handle_key(KeyCode::Char('a'))?;
    for character in "Work".chars() {
        app.handle_key(KeyCode::Char(character))?;
    }
    app.handle_key(KeyCode::Enter)?;
    app.handle_key(KeyCode::Char('e'))?;
    keys(
        &mut app,
        &[KeyCode::Home, KeyCode::Char('!'), KeyCode::Enter],
    )?;
    assert_eq!(app.projects[0].name, "!Work");
    // Search still uses arrows to select results, not to move its text cursor.
    app.handle_key_event(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL))?;
    keys(
        &mut app,
        &[KeyCode::Char('a'), KeyCode::Left, KeyCode::Char('b')],
    )?;
    assert!(matches!(&app.mode, Mode::Palette { query, .. } if query == "ab"));
    Ok(())
}

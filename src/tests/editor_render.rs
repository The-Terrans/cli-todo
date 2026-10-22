use super::*;
use crate::{db, editor::TextCursor};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    backend::{Backend, TestBackend},
    buffer::Buffer,
    layout::Position,
    widgets::Widget,
    Terminal,
};
use std::path::Path;

fn screen(app: &App, width: u16, height: u16) -> (Buffer, Position) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| draw(frame, app)).unwrap();
    let position = terminal.backend_mut().get_cursor_position().unwrap();
    (terminal.backend().buffer().clone(), position)
}

fn key(app: &mut App, key: KeyCode) {
    app.handle_key_event(KeyEvent::new(key, KeyModifiers::NONE))
        .unwrap();
}

fn assert_no_marker(buffer: &Buffer) {
    assert!(
        buffer.content.iter().all(|cell| cell.symbol() != "▏"),
        "native cursor must not occupy a text cell"
    );
}

#[test]
fn native_cursor_moves_over_letters_without_moving_editor_text() -> crate::types::Result<()> {
    let mut app = App::new_showing_all(db::open(Path::new(":memory:"))?)?;
    app.mode = Mode::Edit(
        None,
        TaskDraft {
            title: "abc".into(),
            description: "first\nsecond\nlast".into(),
            due_date: "2028-02-29".into(),
            ..TaskDraft::default()
        },
    );
    for field in 0..3 {
        key(&mut app, KeyCode::Home);
        let (before, start) = screen(&app, 80, 25);
        assert_no_marker(&before);
        assert_eq!(
            before[start].symbol(),
            if field == 0 {
                "a"
            } else if field == 1 {
                "l"
            } else {
                "2"
            }
        );
        key(&mut app, KeyCode::Right);
        let (after, next) = screen(&app, 80, 25);
        assert_eq!(
            before, after,
            "moving cursor must not alter any rendered text cell"
        );
        assert_eq!(next, Position::new(start.x + 1, start.y));
        key(&mut app, KeyCode::End);
        let (after, end) = screen(&app, 80, 25);
        assert_eq!(before, after);
        assert_eq!(after[end].symbol(), " ");
        key(&mut app, KeyCode::Tab);
    }
    // Up/Down also move only the native cursor when the viewport need not scroll.
    key(&mut app, KeyCode::Tab);
    key(&mut app, KeyCode::Home);
    let (before, start) = screen(&app, 80, 25);
    key(&mut app, KeyCode::Up);
    let (after, up) = screen(&app, 80, 25);
    assert_eq!(before, after);
    assert_eq!(up, Position::new(start.x, start.y - 1));
    assert_eq!(after[up].symbol(), "s");
    for size in [(1, 1), (5, 3), (20, 5)] {
        screen(&app, size.0, size.1);
    }
    Ok(())
}

#[test]
fn wrapping_does_not_change_when_the_cursor_moves() {
    for input in [
        "one two three four five",
        "a界b cdefghij\nlast line",
        "before ▏ after\nnext",
        "\n\n",
    ] {
        for width in [1, 3, 8, 20] {
            let mut draft = TaskDraft {
                description: input.into(),
                focus: TaskField::Description,
                ..TaskDraft::default()
            };
            let mut baseline = None;
            for position in input.char_indices().map(|(i, _)| i).chain([input.len()]) {
                draft.cursors[1] = TextCursor::default();
                for _ in 0..input.len() {
                    draft.edit_focused(KeyCode::Left);
                }
                while draft.cursors[1].position(input) < position {
                    draft.edit_focused(KeyCode::Right);
                }
                let text = task_field_text(&draft, TaskField::Description);
                assert_eq!(
                    text.lines
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join("\n"),
                    input
                );
                let paragraph = Paragraph::new(text).wrap(Wrap { trim: false });
                let height = paragraph.line_count(width).max(1) as u16;
                let area = Rect::new(0, 0, width, height);
                let mut buffer = Buffer::empty(area);
                paragraph.render(area, &mut buffer);
                if let Some(before) = &baseline {
                    assert_eq!(before, &buffer);
                } else {
                    baseline = Some(buffer);
                }
                let (column, row) = wrapped_cursor(&draft, width);
                assert!(column < width && row < height as usize);
            }
        }
    }
}

#[test]
fn wrapped_native_cursor_matches_words_newlines_and_wide_characters() {
    for (input, width, positions) in [
        (
            "one two three",
            8,
            vec![
                (0, (0, 0)),
                (4, (4, 0)),
                (7, (0, 1)),
                (8, (0, 1)),
                (13, (5, 1)),
            ],
        ),
        (
            "a界b",
            8,
            vec![(0, (0, 0)), (1, (1, 0)), (4, (3, 0)), (5, (4, 0))],
        ),
        ("\n\n", 8, vec![(0, (0, 0)), (1, (0, 1)), (2, (0, 2))]),
        ("abcdefgh", 8, vec![(0, (0, 0)), (8, (7, 0))]),
    ] {
        for (position, expected) in positions {
            let mut draft = TaskDraft {
                description: input.into(),
                focus: TaskField::Description,
                ..TaskDraft::default()
            };
            for _ in 0..input.len() {
                draft.edit_focused(KeyCode::Left);
            }
            while draft.cursors[1].position(input) < position {
                draft.edit_focused(KeyCode::Right);
            }
            assert_eq!(
                wrapped_cursor(&draft, width),
                expected,
                "{input:?}, offset {position}"
            );
        }
    }
}

#[test]
fn compact_dialogs_keep_text_fixed_and_scroll_to_native_cursor() -> crate::types::Result<()> {
    let mut app = App::new_showing_all(db::open(Path::new(":memory:"))?)?;
    for mode in [
        Mode::RemoteEdit("url".into()),
        Mode::CommitEdit("msg".into()),
        Mode::ProjectEdit(None, "name".into()),
    ] {
        app.mode = mode;
        app.input_cursor = TextCursor::default();
        key(&mut app, KeyCode::Home);
        let (before, start) = screen(&app, 80, 25);
        assert_no_marker(&before);
        key(&mut app, KeyCode::Right);
        let (after, next) = screen(&app, 80, 25);
        assert_eq!(before, after);
        assert_eq!(next, Position::new(start.x + 1, start.y));
    }
    app.mode = Mode::RemoteEdit("url".repeat(80));
    app.input_cursor = TextCursor::default();
    let (buffer, cursor) = screen(&app, 80, 25);
    assert_no_marker(&buffer);
    assert_eq!(buffer[cursor].symbol(), " ");
    assert_eq!(buffer[(cursor.x - 1, cursor.y)].symbol(), "l");
    key(&mut app, KeyCode::Left);
    let (after, left) = screen(&app, 80, 25);
    assert_eq!(
        buffer, after,
        "long inputs must not scroll while the cursor remains visible"
    );
    assert_eq!(left, Position::new(cursor.x - 1, cursor.y));
    assert_eq!(after[left].symbol(), "l");
    app.mode = Mode::Palette {
        query: "abc".into(),
        selected: 0,
    };
    let (before, cursor) = screen(&app, 80, 25);
    assert_no_marker(&before);
    assert_eq!(before[cursor].symbol(), " ");
    assert_eq!(before[(cursor.x - 1, cursor.y)].symbol(), "c");
    app.mode = Mode::Palette {
        query: "search".repeat(80),
        selected: 0,
    };
    let (buffer, cursor) = screen(&app, 80, 25);
    assert_no_marker(&buffer);
    assert_eq!(buffer[(cursor.x - 1, cursor.y)].symbol(), "h");
    Ok(())
}

#[test]
fn scrolling_keeps_description_cursor_visible() -> crate::types::Result<()> {
    let mut app = App::new_showing_all(db::open(Path::new(":memory:"))?)?;
    app.mode = Mode::Edit(
        None,
        TaskDraft {
            title: "abc".repeat(80),
            description: format!("{}last", "line\n".repeat(40)),
            focus: TaskField::Description,
            ..TaskDraft::default()
        },
    );
    let (buffer, cursor) = screen(&app, 80, 15);
    assert_eq!(buffer[(cursor.x - 1, cursor.y)].symbol(), "t");
    key(&mut app, KeyCode::Up);
    let (after, up) = screen(&app, 80, 15);
    assert_eq!(
        buffer, after,
        "description must not scroll while the cursor remains visible"
    );
    assert_eq!(up, Position::new(cursor.x, cursor.y - 1));
    for _ in 0..39 {
        key(&mut app, KeyCode::Up);
    }
    key(&mut app, KeyCode::Home);
    let (buffer, cursor) = screen(&app, 80, 15);
    assert_eq!(buffer[cursor].symbol(), "l");
    assert_no_marker(&buffer);
    Ok(())
}

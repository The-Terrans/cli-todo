use super::*;
use crate::{dates::parse_due_date, db, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{backend::TestBackend, Terminal};
use std::path::Path;

/// Counts yellow boxes drawn around a selected task inside a panel.
fn selection_boxes(buffer: &ratatui::buffer::Buffer) -> usize {
    buffer
        .content
        .chunks(buffer.area.width.into())
        .flat_map(|row| row.windows(2))
        .filter(|pair| {
            pair[0].symbol() == "│" && pair[1].symbol() == "╭" && pair[1].fg == Color::LightYellow
        })
        .count()
}

#[test]
fn live_date_keywords_are_highlighted_and_escape_removes_the_highlight() -> Result<()> {
    let mut app = App::new_showing_all(db::open(Path::new(":memory:"))?)?;
    app.handle_key_event(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE))?;
    for c in "chore today".chars() {
        app.handle_key_event(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))?;
    }
    let mut terminal = Terminal::new(TestBackend::new(100, 25))?;
    terminal.draw(|frame| draw(frame, &app))?;
    let highlighted: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .filter(|cell| {
            cell.fg == Color::LightYellow && cell.modifier.contains(Modifier::UNDERLINED)
        })
        .map(|cell| cell.symbol())
        .collect();
    assert_eq!(highlighted, "today");
    app.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))?;
    terminal.draw(|frame| draw(frame, &app))?;
    assert!(!terminal
        .backend()
        .buffer()
        .content
        .iter()
        .any(|cell| cell.modifier.contains(Modifier::UNDERLINED)));
    assert!(
        matches!(&app.mode, Mode::Edit(_, draft) if draft.title == "chore today" && draft.due_date.is_empty())
    );
    app.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))?;
    for c in "メモ\nnext\nweek".chars() {
        let key = if c == '\n' {
            KeyCode::Enter
        } else {
            KeyCode::Char(c)
        };
        app.handle_key_event(KeyEvent::new(key, KeyModifiers::NONE))?;
    }
    terminal.draw(|frame| draw(frame, &app))?;
    let highlighted: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .filter(|cell| {
            cell.fg == Color::LightYellow && cell.modifier.contains(Modifier::UNDERLINED)
        })
        .map(|cell| cell.symbol())
        .collect();
    assert_eq!(highlighted, "nextweek");
    assert!(
        matches!(&app.mode, Mode::Edit(_, draft) if draft.auto_date.as_ref().unwrap().field == TaskField::Description)
    );
    Ok(())
}

#[test]
fn task_details_use_one_third_with_a_heading_based_minimum_and_fit_small_terminals() {
    for (height, heading_lines) in (0..=90).flat_map(|height| [(height, 2), (height, 3)]) {
        let area = Rect::new(2, 3, 80, height);
        let (details, children) = task_view_areas(area, true, heading_lines);
        let children = children.unwrap();
        let expected = (height / 3)
            .max(heading_lines + 4)
            .min(height.saturating_sub(3));
        assert_eq!(details.height, expected);
        assert_eq!(children.height, height - expected);
        assert_eq!(details.x, area.x);
        assert_eq!(details.y, area.y);
        assert_eq!(details.width, area.width);
        assert_eq!(children.y, details.bottom());
        assert_eq!(children.bottom(), area.bottom());
        assert_eq!(task_view_areas(area, false, heading_lines), (area, None));
        let inner = panel("", false).inner(details);
        let (heading, divider, description) = task_detail_sections(inner, heading_lines);
        assert_eq!(heading.height, inner.height.min(heading_lines));
        assert_eq!(divider.height, u16::from(inner.height > heading_lines));
        assert_eq!(divider.y, heading.bottom());
        assert_eq!(description.y, divider.bottom());
        assert_eq!(description.bottom(), inner.bottom());
        if details.height >= heading_lines + 4 {
            assert!(description.height >= 1);
        }
    }
}

#[test]
fn task_detail_heading_stays_visible_while_description_scrolls() -> Result<()> {
    let db = db::open(Path::new(":memory:"))?;
    db::save_task_tree(
        &db,
        None,
        &TaskDraft {
            title: "pinned title".into(),
            due_date: "2000-01-01".into(),
            description: "first line\n".repeat(30) + "last description line",
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
    let mut app = App::new_showing_all(db)?;
    app.viewport.width = 100;
    app.resize(25);
    app.handle_key_event(KeyEvent::new(KeyCode::Char('0'), KeyModifiers::NONE))?;
    app.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))?;
    app.handle_key_event(KeyEvent::new(KeyCode::End, KeyModifiers::NONE))?;
    assert!(app.task_view.as_ref().unwrap().scroll > 0);
    let mut terminal = Terminal::new(TestBackend::new(100, 25))?;
    terminal.draw(|frame| draw(frame, &app))?;
    let text: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(text.contains("[ ] pinned title"));
    assert!(text.contains("Due: 2000-01-01"));
    assert!(text.contains("─ Description"));
    assert!(text.contains("last description line"));
    assert!(!text.contains("Status:"));
    Ok(())
}

#[test]
fn opened_tasks_render_inline_details_and_optional_subtasks() -> Result<()> {
    let db = db::open(Path::new(":memory:"))?;
    db::save_task_tree(
        &db,
        None,
        &TaskDraft {
            title: "parent".into(),
            description: "inline description".into(),
            due_date: "2000-01-01".into(),
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
    let mut app = App::new_showing_all(db)?;
    app.handle_key_event(KeyEvent::new(KeyCode::Char('0'), KeyModifiers::NONE))?;
    app.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))?;
    let mut terminal = Terminal::new(TestBackend::new(100, 25))?;
    terminal.draw(|frame| draw(frame, &app))?;
    let cells = &terminal.backend().buffer().content;
    let text: String = cells.iter().map(|cell| cell.symbol()).collect();
    assert!(text.contains("Task details"));
    assert!(text.contains("inline description"));
    assert!(text.contains("Due: 2000-01-01"));
    assert!(text.contains("Subtasks"));
    assert!(text.contains("[ ] child"));
    assert!(!text.contains("Edit task: Title"));
    assert_eq!(
        cells.iter().filter(|cell| cell.symbol() == "┌").count(),
        0,
        "no editor popup"
    );
    assert_eq!(selection_boxes(terminal.backend().buffer()), 0);
    let corners: Vec<_> = cells
        .iter()
        .enumerate()
        .filter(|(index, cell)| index % 100 >= 25 && cell.symbol() == "╭")
        .map(|(_, cell)| cell.fg)
        .collect();
    assert_eq!(corners, vec![Color::LightYellow, Color::DarkGray]);
    app.handle_key_event(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE))?;
    terminal.draw(|frame| draw(frame, &app))?;
    let cells = &terminal.backend().buffer().content;
    assert_eq!(selection_boxes(terminal.backend().buffer()), 1);
    let corners: Vec<_> = cells
        .iter()
        .enumerate()
        .filter(|(index, cell)| index % 100 >= 25 && cell.symbol() == "╭")
        .map(|(_, cell)| cell.fg)
        .collect();
    assert_eq!(
        corners,
        vec![Color::DarkGray, Color::LightYellow, Color::LightYellow]
    );
    app.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))?;
    terminal.draw(|frame| draw(frame, &app))?;
    let text: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(text.contains("Task details") && text.contains("child"));
    assert!(!text.contains("Subtasks"), "hide empty subtask section");
    assert!(matches!(app.mode, Mode::Browse));
    Ok(())
}

#[test]
fn date_badges_highlight_only_overdue_unfinished_tasks() -> Result<()> {
    let today = parse_due_date("2026-08-15")?.unwrap();
    let due = parse_due_date("2026-08-14")?;
    let overdue = task_title_line("dated", false, 2, due, Style::default(), today);
    assert_eq!(overdue.spans[0].content, "[ ] dated (2 subtasks)");
    assert_eq!(overdue.spans[1].content, " [due: 2026-08-14]");
    assert_eq!(overdue.spans[1].style.fg, Some(Color::LightRed));
    let selected = task_title_line("dated", false, 0, due, task_selection_style(), today);
    assert_eq!(selected.spans[1].style.fg, Some(Color::LightRed));
    let completed = task_title_line("dated", true, 0, due, Style::default(), today);
    assert_eq!(completed.spans[1].style.fg, Some(Color::DarkGray));
    let due_today = task_title_line("dated", false, 0, Some(today), Style::default(), today);
    assert_eq!(due_today.spans[1].style.fg, Some(Color::Gray));
    assert_eq!(
        task_title_line("undated", false, 0, None, Style::default(), today)
            .spans
            .len(),
        1
    );
    Ok(())
}

#[test]
fn due_date_field_has_its_own_focus_cursor_and_validation_row() -> Result<()> {
    let mut app = App::new_showing_all(db::open(Path::new(":memory:"))?)?;
    app.mode = Mode::Edit(
        None,
        TaskDraft {
            title: "dated".into(),
            due_date: "2026-02-30".into(),
            focus: TaskField::DueDate,
            ..TaskDraft::default()
        },
    );
    app.message = "Due date must be a valid YYYY-MM-DD date".into();
    let mut terminal = Terminal::new(TestBackend::new(80, 25))?;
    terminal.draw(|frame| draw(frame, &app))?;
    let cells = &terminal.backend().buffer().content;
    let text: String = cells.iter().map(|cell| cell.symbol()).collect();
    assert!(text.contains("2026-02-30"));
    assert!(text.contains("YYYY-MM-DD; blank clears"));
    assert!(text.contains(&app.message));
    assert_eq!(
        cells
            .iter()
            .filter(|cell| cell.symbol().contains('▏'))
            .count(),
        0,
        "native cursor must not insert a glyph"
    );
    let corners: Vec<_> = cells.iter().filter(|cell| cell.symbol() == "┌").collect();
    assert_eq!(corners.len(), 3);
    assert_eq!(corners[2].fg, Color::LightYellow);
    assert!(corners
        .iter()
        .enumerate()
        .all(|(i, cell)| i == 2 || cell.fg == Color::DarkGray));
    Ok(())
}

#[test]
fn titles_show_direct_subtask_counts_in_lists_and_opened_views() -> Result<()> {
    use crate::types::SubtaskDraft;
    let db = db::open(Path::new(":memory:"))?;
    let leaf = SubtaskDraft {
        id: None,
        done: true,
        draft: TaskDraft {
            title: "grandchild".into(),
            ..TaskDraft::default()
        },
    };
    let child = SubtaskDraft {
        id: None,
        done: true,
        draft: TaskDraft {
            title: "child".into(),
            subtasks: vec![leaf],
            ..TaskDraft::default()
        },
    };
    let sibling = SubtaskDraft {
        id: None,
        done: false,
        draft: TaskDraft {
            title: "sibling".into(),
            ..TaskDraft::default()
        },
    };
    db::save_task_tree(
        &db,
        None,
        &TaskDraft {
            title: "parent".into(),
            subtasks: vec![child, sibling],
            ..TaskDraft::default()
        },
        None,
    )?;
    db::save_task_details(&db, None, "solo", "", None)?;
    let mut app = App::new_showing_all(db)?;
    assert_eq!(app.tasks[0].subtask_count, 2);
    assert_eq!(app.tasks[1].subtask_count, 0);
    let mut terminal = Terminal::new(TestBackend::new(100, 25))?;
    terminal.draw(|frame| draw(frame, &app))?;
    let text: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(text.contains("parent (2 subtasks)"));
    assert!(!text.contains("solo ("));
    app.handle_key_event(KeyEvent::new(KeyCode::Char('0'), KeyModifiers::NONE))?;
    app.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))?;
    terminal.draw(|frame| draw(frame, &app))?;
    let text: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(text.contains("child (1 subtask)"));
    assert!(!text.contains("sibling ("));
    assert_eq!(subtask_count_label(0), "");
    assert_eq!(subtask_count_label(1), " (1 subtask)");
    Ok(())
}

#[test]
fn creation_and_reword_popups_have_only_three_fields_and_one_cursor() -> Result<()> {
    let mut app = App::new_showing_all(db::open(Path::new(":memory:"))?)?;
    let mut terminal = Terminal::new(TestBackend::new(80, 25))?;
    for id in [None, Some(1)] {
        for (index, focus) in [TaskField::Title, TaskField::Description, TaskField::DueDate]
            .into_iter()
            .enumerate()
        {
            app.mode = Mode::Edit(
                id,
                TaskDraft {
                    title: "parent".into(),
                    focus,
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
            );
            terminal.draw(|frame| draw(frame, &app))?;
            let cells = &terminal.backend().buffer().content;
            let text: String = cells.iter().map(|cell| cell.symbol()).collect();
            assert!(text.contains(if id.is_some() {
                "Edit task: Title"
            } else {
                "Add task: Title"
            }));
            assert!(text.contains("Description"));
            assert!(text.contains("Due date"));
            assert!(!text.contains("Subtasks"));
            assert!(!text.contains("[ ] child"));
            assert_eq!(
                cells
                    .iter()
                    .filter(|cell| cell.symbol().contains('▏'))
                    .count(),
                0,
                "native cursor must not insert a glyph"
            );
            let corners: Vec<_> = cells.iter().filter(|cell| cell.symbol() == "┌").collect();
            assert_eq!(corners.len(), 3);
            for (field, corner) in corners.iter().enumerate() {
                assert_eq!(
                    corner.fg,
                    if field == index {
                        Color::LightYellow
                    } else {
                        Color::DarkGray
                    }
                );
            }
        }
    }
    Ok(())
}

#[test]
fn task_descriptions_use_a_dim_preview_without_extra_empty_rows() -> Result<()> {
    let db = db::open(Path::new(":memory:"))?;
    db::save_task_details(&db, None, "with details", "first line\nsecond line", None)?;
    db::save_task_details(&db, None, "without details", "", None)?;
    db::save_task_details(&db, None, "blank details", " \n ", None)?;
    let mut app = App::new_showing_all(db)?;
    let mut terminal = Terminal::new(TestBackend::new(100, 25))?;
    for focused in [false, true] {
        if focused {
            app.handle_key_event(KeyEvent::new(KeyCode::Char('0'), KeyModifiers::NONE))?;
        }
        terminal.draw(|frame| draw(frame, &app))?;
        let buffer = terminal.backend().buffer();
        // The selection box adds a row above and below the selected task.
        let pad = u16::from(focused);
        let preview = &buffer[(32, 2 + pad)];
        assert_eq!(preview.symbol(), "f");
        assert_eq!(preview.fg, Color::DarkGray);
        assert_eq!(preview.bg, Color::Reset);
        assert!(preview.modifier.contains(Modifier::DIM));
        assert!(!preview.modifier.contains(Modifier::BOLD));
        assert_eq!(buffer[(32, 1 + pad)].symbol(), "w");
        assert_eq!(buffer[(32, 3 + 2 * pad)].symbol(), "w");
        assert_eq!(buffer[(32, 4 + 2 * pad)].symbol(), "b");
        assert_eq!(buffer[(32, 5 + 2 * pad)].symbol(), " ");
        let start = usize::from(2 + pad) * 100;
        let row: String = buffer.content[start..start + 100]
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(row.contains("first line …"));
        let text: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert!(!text.contains("second line"));
        if focused {
            assert_eq!(buffer[(26, 1)].symbol(), "╭");
            assert_eq!(buffer[(26, 1)].fg, Color::LightYellow);
            assert_eq!(buffer[(26, 4)].symbol(), "╰");
            assert_eq!(buffer[(32, 2)].bg, Color::Reset);
            assert!(buffer[(32, 2)].modifier.contains(Modifier::BOLD));
        }
    }
    app.handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))?;
    terminal.draw(|frame| draw(frame, &app))?;
    assert_eq!(terminal.backend().buffer()[(26, 3)].symbol(), "╭");
    assert_eq!(app.selected_task, 1);
    Ok(())
}

#[test]
fn input_width_has_a_minimum_but_never_overflows_small_terminals() {
    for width in [1, 30, 59, 60, 61, 62, 80, 100, 160] {
        let area = Rect::new(7, 3, width, 25);
        let dialog = input_dialog_area(area, 3);
        assert_eq!(
            dialog.width,
            (width * 70 / 100).max(60).min(width.saturating_sub(2))
        );
        assert_eq!(dialog.x, area.x + (width - dialog.width) / 2);
        assert!(dialog.right() <= area.right());
        if width >= 62 {
            assert!(dialog.width >= 60);
        }
    }
}

#[test]
fn task_editor_height_scales_and_stays_centered_within_the_terminal() {
    for height in [1, 8, 14, 20, 30, 50] {
        let terminal = Rect::new(7, 3, 100, height);
        let dialog = task_editor_area(terminal);
        let expected = (height * 70 / 100).max(15).min(height.saturating_sub(2));
        assert_eq!(dialog.height, expected);
        assert_eq!(dialog.y, terminal.y + (height - expected) / 2);
        assert!(dialog.bottom() <= terminal.bottom());
        if height >= 17 {
            assert!(dialog.height >= 15);
        }
    }
    let mut app = App::new_showing_all(db::open(Path::new(":memory:")).unwrap()).unwrap();
    app.mode = Mode::Edit(None, TaskDraft::default());
    for height in [20, 30, 50] {
        let mut terminal = Terminal::new(TestBackend::new(100, height)).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let cells = &terminal.backend().buffer().content;
        let corners: Vec<_> = cells
            .iter()
            .enumerate()
            .filter(|(_, cell)| cell.symbol() == "┌")
            .map(|(index, _)| index / 100)
            .collect();
        let bottom = cells.iter().position(|cell| cell.symbol() == "└").unwrap() / 100;
        assert_eq!(bottom - corners[0] + 1, 3, "title must remain compact");
        assert_eq!(corners[1] - corners[0], 3);
    }
}

#[test]
fn task_editor_shows_only_the_focused_fields_footer_and_cursor() -> Result<()> {
    let mut app = App::new_showing_all(db::open(Path::new(":memory:"))?)?;
    app.mode = Mode::Edit(
        None,
        TaskDraft {
            title: "task".into(),
            description: format!("{}\nLAST", "界 words 😀 ".repeat(100)),
            ..TaskDraft::default()
        },
    );
    for focused in [false, true] {
        let Mode::Edit(_, draft) = &mut app.mode else {
            unreachable!();
        };
        draft.focus = if focused {
            TaskField::Description
        } else {
            TaskField::Title
        };
        let mut terminal = Terminal::new(TestBackend::new(80, 25))?;
        terminal.draw(|frame| draw(frame, &app))?;
        let cells = &terminal.backend().buffer().content;
        let text: String = cells.iter().map(|cell| cell.symbol()).collect();
        assert!(text.contains("Description"));
        assert!(text.contains("Press <tab> to toggle focus"));
        assert_eq!(text.contains("Enter: save"), !focused);
        assert_eq!(text.contains("<c-s>: save"), focused);
        assert_eq!(text.matches("Esc: cancel").count(), 1);
        assert_eq!(
            cells
                .iter()
                .filter(|cell| cell.symbol().contains('▏'))
                .count(),
            0,
            "native cursor must not insert a glyph"
        );
        assert!(text.contains(if focused { "LAST" } else { "task" }));
        let corners: Vec<_> = cells.iter().filter(|cell| cell.symbol() == "┌").collect();
        assert_eq!(corners.len(), 3);
        assert_eq!(corners[2].fg, Color::DarkGray);
        assert_eq!(
            corners[0].fg,
            if focused {
                Color::DarkGray
            } else {
                Color::LightYellow
            }
        );
        assert_eq!(
            corners[1].fg,
            if focused {
                Color::LightYellow
            } else {
                Color::DarkGray
            }
        );
    }
    Ok(())
}

#[test]
fn input_dialogs_use_relative_width_and_scroll_long_titles() -> Result<()> {
    for width in [80, 160] {
        for kind in ["task", "project", "commit", "remote"] {
            let mut app = App::new_showing_all(db::open(Path::new(":memory:"))?)?;
            let input = format!("{}end", "界".repeat(80));
            app.mode = match kind {
                "project" => Mode::ProjectEdit(None, input),
                "commit" => Mode::CommitEdit(input),
                "remote" => Mode::RemoteEdit(input),
                _ => Mode::Edit(
                    None,
                    TaskDraft {
                        title: input,
                        ..TaskDraft::default()
                    },
                ),
            };
            let mut terminal = Terminal::new(TestBackend::new(width, 25))?;
            for error in ["", "Title cannot be empty"] {
                app.message = error.into();
                terminal.draw(|frame| draw(frame, &app))?;
                let cells = &terminal.backend().buffer().content;
                let top_left = cells.iter().position(|cell| cell.symbol() == "┌").unwrap();
                let top_right = cells.iter().position(|cell| cell.symbol() == "┐").unwrap();
                let bottom_left = cells.iter().position(|cell| cell.symbol() == "└").unwrap();
                let relative_width = (width as usize * 70 / 100).max(60);
                assert_eq!(top_right - top_left + 1, relative_width);
                assert_eq!(
                    top_left % width as usize,
                    (width as usize - relative_width) / 2
                );
                assert_eq!(
                    (bottom_left - top_left) / width as usize + 1,
                    if error.is_empty() { 3 } else { 4 }
                );
                let text: String = cells.iter().map(|cell| cell.symbol()).collect();
                assert!(
                    text.contains("end"),
                    "input tail must remain visible beside the native cursor"
                );
                let bottom = bottom_left / width as usize;
                let footer: String = cells[bottom * width as usize..(bottom + 1) * width as usize]
                    .iter()
                    .map(|cell| cell.symbol())
                    .collect();
                let submit = if kind == "commit" { "commit" } else { "save" };
                assert!(footer.contains(&format!("Enter: {submit} ── Esc: cancel")));
                let red_rows = cells
                    .chunks(width as usize)
                    .filter(|row| row.iter().any(|cell| cell.fg == Color::LightRed))
                    .count();
                assert_eq!(red_rows, usize::from(!error.is_empty()));
                if !error.is_empty() {
                    assert!(text.contains(error));
                }
            }
        }
    }
    Ok(())
}

#[test]
fn no_changes_notification_is_a_popup_not_a_footer_message() -> Result<()> {
    let mut app = App::new_showing_all(db::open(Path::new(":memory:"))?)?;
    app.mode = Mode::NoChanges;
    let mut terminal = Terminal::new(TestBackend::new(80, 25))?;
    terminal.draw(|frame| draw(frame, &app))?;
    let text: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(text.contains("No todo changes to commit."));
    assert!(text.contains("Enter/Esc: close"));
    let footer: String = terminal.backend().buffer().content[1920..2000]
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert_eq!(footer.trim(), "?: keybindings");
    app.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))?;
    terminal.draw(|frame| draw(frame, &app))?;
    let text: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(!text.contains("No todo changes to commit."));
    Ok(())
}

#[test]
fn footer_is_compact_and_help_is_readable_in_a_short_terminal() -> Result<()> {
    let mut app = App::new_showing_all(db::open(Path::new(":memory:"))?)?;
    let mut terminal = Terminal::new(TestBackend::new(100, 25))?;
    for section in ['1', '2', '3', '0'] {
        app.handle_key_event(KeyEvent::new(KeyCode::Char(section), KeyModifiers::NONE))?;
        terminal.draw(|frame| draw(frame, &app))?;
        let footer: String = terminal.backend().buffer().content[2400..2500]
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert_eq!(footer.trim(), "?: keybindings");
    }
    app.message = "Sync failed: example".into();
    terminal.draw(|frame| draw(frame, &app))?;
    let footer: String = terminal.backend().buffer().content[2400..2500]
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(footer.contains("?: keybindings | Sync failed: example"));
    app.resize(12);
    app.handle_key_event(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT))?;
    let mut terminal = Terminal::new(TestBackend::new(80, 12))?;
    terminal.draw(|frame| draw(frame, &app))?;
    let text: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(text.contains("Keybindings") && text.contains("BROWSING"));
    assert!(text.contains("?/Esc: close"));
    app.handle_key_event(KeyEvent::new(KeyCode::End, KeyModifiers::NONE))?;
    terminal.draw(|frame| draw(frame, &app))?;
    let text: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(text.contains("Tab") && text.contains("Cycles"));
    app.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))?;
    terminal.draw(|frame| draw(frame, &app))?;
    let text: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(!text.contains("Keybindings"));
    Ok(())
}

#[test]
fn nuke_warning_is_one_red_line_with_confirmation_in_bottom_border() -> Result<()> {
    let db = db::open(Path::new(":memory:"))?;
    db::save_task(&db, None, "task")?;
    db::save_project(&db, None, "Work")?;
    let mut app = App::new_showing_all(db)?;
    app.handle_key_event(KeyEvent::new(KeyCode::Char('D'), KeyModifiers::SHIFT))?;
    for width in [80, 100] {
        let mut terminal = Terminal::new(TestBackend::new(width, 25))?;
        terminal.draw(|frame| draw(frame, &app))?;
        let rows: Vec<_> = terminal
            .backend()
            .buffer()
            .content
            .chunks(width as usize)
            .collect();
        let warnings: Vec<_> = rows
            .iter()
            .filter(|row| row.iter().any(|cell| cell.fg == Color::LightRed))
            .collect();
        assert_eq!(warnings.len(), 1);
        let warning: String = warnings[0].iter().map(|cell| cell.symbol()).collect();
        assert!(warning.contains("Delete 1 tasks, 1 projects, 0 commits + remote settings?"));
        assert!(warnings[0]
            .iter()
            .filter(|cell| cell.fg == Color::LightRed)
            .all(|cell| cell.modifier.contains(Modifier::BOLD)));
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        let hint_row = rows
            .iter()
            .find(|row| {
                let text: String = row.iter().map(|cell| cell.symbol()).collect();
                text.contains("──Enter: confirm ── Esc: cancel ──")
            })
            .expect("confirmation hints missing");
        assert!(hint_row.iter().any(|cell| cell.symbol() == "└"));
        assert!(hint_row.iter().any(|cell| cell.symbol() == "┘"));
        for cell in hint_row
            .iter()
            .filter(|cell| cell.symbol().chars().any(char::is_alphabetic))
        {
            assert_eq!(cell.fg, Color::LightYellow);
            assert_eq!(cell.bg, Color::Reset);
            assert!(!cell.modifier.contains(Modifier::BOLD));
        }
        let footer = "──Enter: confirm ── Esc: cancel ──";
        let footer_width = footer.chars().count();
        let footer_start = hint_row
            .windows(footer_width)
            .position(|cells| cells.iter().map(|cell| cell.symbol()).collect::<String>() == footer)
            .expect("footer missing");
        let right_corner = hint_row
            .iter()
            .position(|cell| cell.symbol() == "┘")
            .unwrap();
        assert_eq!(
            footer_start + footer_width,
            right_corner,
            "footer must align right"
        );
        assert!(!text.contains("y/Enter") && !text.contains("n/Esc"));
        assert!(text.contains("Backups and remote repository are kept."));
    }
    Ok(())
}

#[test]
fn section_title_separators_match_the_border_in_every_focus_state() -> Result<()> {
    let db = db::open(Path::new(":memory:"))?;
    db::save_project(&db, None, "Work─Home")?;
    let mut app = App::new_showing_all(db)?;
    let mut terminal = Terminal::new(TestBackend::new(100, 25))?;
    for key in ['1', '0', '2', '0', '3', '0'] {
        app.handle_key_event(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE))?;
        terminal.draw(|frame| draw(frame, &app))?;
        let buffer = terminal.backend().buffer();
        for (x, y) in [(0, 0), (0, 8), (0, 16), (25, 0)] {
            let border = &buffer[(x, y)];
            for offset in [1, 5] {
                let separator = &buffer[(x + offset, y)];
                assert_eq!(separator.symbol(), "─");
                assert_eq!(separator.fg, border.fg, "section {key} at ({x}, {y})");
                assert_eq!(separator.bg, border.bg);
                assert_eq!(separator.modifier, border.modifier);
            }
            let label = &buffer[(x + 6, y)];
            assert_eq!(
                label.bg,
                if border.fg == Color::LightYellow {
                    Color::LightYellow
                } else {
                    Color::Reset
                }
            );
        }
    }
    Ok(())
}

#[test]
fn empty_focused_panels_have_high_contrast_titles_and_borders() -> Result<()> {
    let mut app = App::new_showing_all(db::open(Path::new(":memory:"))?)?;
    let mut terminal = Terminal::new(TestBackend::new(100, 25))?;
    for (key, focused, inactive) in [
        ('2', (0, 8), (0, 0)),
        ('3', (0, 16), (0, 8)),
        ('0', (25, 0), (0, 16)),
    ] {
        app.handle_key_event(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE))?;
        terminal.draw(|frame| draw(frame, &app))?;
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[focused].fg, Color::LightYellow, "section {key}");
        assert!(buffer[focused].modifier.contains(Modifier::BOLD));
        let title = &buffer[(focused.0 + 2, focused.1)];
        assert_eq!(title.fg, Color::Black);
        assert_eq!(title.bg, Color::LightYellow);
        assert_eq!(buffer[inactive].fg, Color::DarkGray);
        assert_ne!(buffer[(inactive.0 + 2, inactive.1)].bg, Color::LightYellow);
    }
    Ok(())
}

#[test]
fn only_focused_section_has_a_highlight() -> Result<()> {
    let db = db::open(Path::new(":memory:"))?;
    db::save_project(&db, None, "Work")?;
    let project = db.last_insert_rowid();
    db::create_task(&db, "project task", Some(project))?;
    db::save_task(&db, None, "inbox task")?;
    let mut app = App::new_showing_all(db)?;
    let mut terminal = Terminal::new(TestBackend::new(100, 25))?;
    for key in ['1', '2', '0'] {
        app.handle_key_event(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE))?;
        terminal.draw(|frame| draw(frame, &app))?;
        let highlighted_rows = terminal
            .backend()
            .buffer()
            .content
            .chunks(100)
            .filter(|row| {
                row.iter()
                    .any(|cell| cell.symbol() == ">" && cell.bg == Color::LightYellow)
            })
            .count();
        let boxes = selection_boxes(terminal.backend().buffer());
        assert_eq!(highlighted_rows + boxes, 1, "section {key}");
        assert_eq!(boxes, usize::from(key == '0'), "section {key}");
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(3, 1)].symbol(), "T", "Inbox indent changed");
        assert_eq!(buffer[(3, 9)].symbol(), "W", "Projects indent changed");
        for bottom in [7, 15, 23] {
            assert_eq!(buffer[(0, bottom)].symbol(), "╰", "Uneven section heights");
        }
        let first_task = 1 + u16::from(key == '0');
        assert_eq!(
            buffer[(28, first_task)].symbol(),
            "[",
            "Tasks indent changed"
        );
    }
    Ok(())
}

#[test]
fn task_details_show_project_header_and_subtask_parent() -> Result<()> {
    let db = db::open(Path::new(":memory:"))?;
    db::save_project(&db, None, "Work")?;
    let project = db.last_insert_rowid();
    db::save_task_tree(
        &db,
        None,
        &TaskDraft {
            title: "parent task".into(),
            subtasks: vec![crate::types::SubtaskDraft {
                id: None,
                done: false,
                draft: TaskDraft {
                    title: "child task".into(),
                    ..TaskDraft::default()
                },
            }],
            ..TaskDraft::default()
        },
        Some(project),
    )?;
    let mut app = App::new_showing_all(db)?;
    let mut terminal = Terminal::new(TestBackend::new(100, 25))?;
    let mut screen = |app: &App| -> Result<String> {
        terminal.draw(|frame| draw(frame, app))?;
        Ok(terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect())
    };
    for key in [KeyCode::Char('2'), KeyCode::Enter, KeyCode::Enter] {
        app.handle_key_event(KeyEvent::new(key, KeyModifiers::NONE))?;
    }
    let text = screen(&app)?;
    assert!(text.contains("Task details: Work"));
    assert!(
        !text.contains("Parent:"),
        "top-level tasks have no parent line"
    );
    for key in [KeyCode::Right, KeyCode::Enter] {
        app.handle_key_event(KeyEvent::new(key, KeyModifiers::NONE))?;
    }
    let text = screen(&app)?;
    assert!(text.contains("Task details: Work"));
    assert!(text.contains("[ ] child task"));
    assert!(text.contains("Parent: parent task"));
    assert!(text.contains("Due: none"));
    Ok(())
}

#[test]
fn inbox_lists_every_project_with_tags_and_filters() -> Result<()> {
    let db = db::open(Path::new(":memory:"))?;
    db::save_project(&db, None, "Work")?;
    let work = db.last_insert_rowid();
    db::save_task_details(&db, None, "write report", "", Some(work))?;
    let report = db.last_insert_rowid();
    db::save_task_details(&db, None, "buy milk", "", None)?;
    db::save_task_details(&db, None, "done at work", "", Some(work))?;
    db::toggle_task(&db, db.last_insert_rowid())?;
    let mut app = App::new_showing_all(db)?;
    let mut terminal = Terminal::new(TestBackend::new(100, 25))?;
    let mut screen = |app: &App| -> Result<String> {
        terminal.draw(|frame| draw(frame, app))?;
        Ok(terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect())
    };
    let press = |app: &mut App, key| app.handle_key_event(KeyEvent::new(key, KeyModifiers::NONE));
    let titles =
        |app: &App| -> Vec<String> { app.tasks.iter().map(|task| task.title.clone()).collect() };
    assert_eq!(titles(&app), ["write report", "buy milk", "done at work"]);
    let text = screen(&app)?;
    assert!(text.contains("[ ] write report [Work]"));
    assert!(
        text.contains("[ ] buy milk "),
        "unassigned tasks have no tag"
    );
    assert!(text.contains("Tasks · All"), "title names the filter");
    for key in [KeyCode::Char('r'), KeyCode::Down, KeyCode::Enter] {
        press(&mut app, key)?;
    }
    assert!(
        screen(&app)?.contains("Tasks · All · Created date ↑ (oldest first)"),
        "title names a non-manual sort"
    );
    for key in [
        KeyCode::Char('r'),
        KeyCode::Up,
        KeyCode::Enter,
        KeyCode::Char('1'),
    ] {
        press(&mut app, key)?;
    }
    press(&mut app, KeyCode::Down)?;
    assert_eq!(titles(&app), ["write report", "buy milk"]);
    press(&mut app, KeyCode::Down)?;
    assert_eq!(titles(&app), ["done at work"]);
    press(&mut app, KeyCode::Char('2'))?;
    assert!(
        !screen(&app)?.contains("[Work]"),
        "project lists omit the tag"
    );
    press(&mut app, KeyCode::Char('1'))?;
    assert_eq!(app.selected_filter, 3, "Inbox restores Completed");
    press(&mut app, KeyCode::Up)?;
    press(&mut app, KeyCode::Up)?;
    assert_eq!(app.selected_filter, crate::constants::ALL_FILTER);
    for key in [KeyCode::Enter, KeyCode::Enter, KeyCode::Char('x')] {
        press(&mut app, key)?;
    }
    let view = app
        .task_view
        .as_ref()
        .expect("project task stays open in Inbox");
    assert_eq!(view.task.id, report);
    assert!(view.task.done);
    assert!(screen(&app)?.contains("Task details: Work"));
    press(&mut app, KeyCode::Char('m'))?;
    assert!(screen(&app)?.contains("No project"));
    press(&mut app, KeyCode::Esc)?;
    press(&mut app, KeyCode::Esc)?;
    press(&mut app, KeyCode::Char('a'))?;
    for character in "new inbox task".chars() {
        press(&mut app, KeyCode::Char(character))?;
    }
    press(&mut app, KeyCode::Enter)?;
    let added = app
        .tasks
        .iter()
        .find(|task| task.title == "new inbox task")
        .unwrap();
    assert_eq!(added.project_id, None);
    Ok(())
}

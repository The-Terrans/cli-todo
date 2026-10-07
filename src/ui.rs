use crate::app::{App, Mode, FILTERS};
use ratatui::{
    layout::{Constraint, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    widgets::{
        Block, BorderType, Borders, Clear, HighlightSpacing, List, ListItem, ListState, Paragraph,
    },
    Frame,
};

pub fn draw(frame: &mut Frame, app: &App) {
    let areas = Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).split(frame.area());
    let panels = Layout::horizontal([
        Constraint::Percentage(25),
        Constraint::Length(0), // Gap between panels.
        Constraint::Min(0),
    ])
    .split(areas[0]);
    let sections = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Fill(1),
        Constraint::Fill(1),
    ])
    .split(panels[0]);
    draw_navigation(frame, sections[0], app);
    draw_projects(frame, sections[1], app);
    draw_commits(frame, sections[2], app);
    if app.commits_focused {
        draw_commit_details(frame, panels[2], app);
    } else {
        draw_tasks(frame, panels[2], app);
    }
    draw_help(frame, areas[1], app);
    draw_dialog(frame, app);
}

fn panel(title: &str, focused: bool) -> Block<'_> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if focused {
            Color::Cyan
        } else {
            Color::DarkGray
        }))
}

fn selection_style() -> Style {
    Style::default()
        .fg(Color::Black)
        .bg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}

fn draw_navigation(frame: &mut Frame, area: Rect, app: &App) {
    let active = matches!(app.mode, Mode::Browse)
        && app.navigation_focused
        && !app.projects_focused
        && !app.commits_focused;
    let mut state = ListState::default().with_selected(active.then_some(app.selected_filter));
    let filters = FILTERS.iter().map(|(label, _)| *label);
    frame.render_stateful_widget(
        List::new(filters)
            .highlight_spacing(HighlightSpacing::Always)
            .block(
                panel(
                    "─[1]─Inbox",
                    app.navigation_focused && !app.projects_focused && !app.commits_focused,
                )
                .border_type(BorderType::Rounded),
            )
            .highlight_style(selection_style())
            .highlight_symbol("> "),
        area,
        &mut state,
    );
}

fn draw_projects(frame: &mut Frame, area: Rect, app: &App) {
    let active = matches!(app.mode, Mode::Browse) && app.navigation_focused && app.projects_focused;
    let mut state = ListState::default()
        .with_selected((active && !app.projects.is_empty()).then_some(app.selected_project));
    frame.render_stateful_widget(
        List::new(app.projects.iter().map(|project| project.name.as_str()))
            .highlight_spacing(HighlightSpacing::Always)
            .block(
                panel(
                    "─[2]─Projects",
                    app.navigation_focused && app.projects_focused,
                )
                .border_type(BorderType::Rounded),
            )
            .highlight_style(selection_style())
            .highlight_symbol("> "),
        area,
        &mut state,
    );
    if app.projects.is_empty() {
        frame.render_widget(
            Paragraph::new("2 → a: add project"),
            area.inner(Margin::new(1, 1)),
        );
    }
}

fn draw_commits(frame: &mut Frame, area: Rect, app: &App) {
    let active = matches!(app.mode, Mode::Browse) && app.navigation_focused && app.commits_focused;
    let mut state = ListState::default()
        .with_selected((active && !app.commits.is_empty()).then_some(app.selected_commit));
    let title = if app.uncommitted_changes {
        "─[3]─Commits* "
    } else {
        "─[3]─Commits "
    };
    let items = app.commits.iter().map(|commit| {
        format!(
            "{} {}",
            &commit.hash[..7.min(commit.hash.len())],
            commit.subject
        )
    });
    frame.render_stateful_widget(
        List::new(items)
            .highlight_spacing(HighlightSpacing::Always)
            .block(
                panel(title, app.navigation_focused && app.commits_focused)
                    .border_type(BorderType::Rounded),
            )
            .highlight_style(selection_style())
            .highlight_symbol("> "),
        area,
        &mut state,
    );
    if app.commits.is_empty() {
        frame.render_widget(
            Paragraph::new("3 → c: checkpoint"),
            area.inner(Margin::new(1, 1)),
        );
    }
}

fn draw_commit_details(frame: &mut Frame, area: Rect, app: &App) {
    let text = if app.message.is_empty() {
        app.commit_details.clone()
    } else {
        format!("{}\n\n{}", app.message, app.commit_details)
    };
    let remote = if app.remote_url.is_empty() {
        "not configured (r to set)"
    } else {
        &app.remote_url
    };
    let text = format!("Origin: {remote}\n\n{text}");
    frame.render_widget(
        Paragraph::new(text).scroll((app.detail_scroll, 0)).block(
            panel("─[0]─Commit details", !app.navigation_focused).border_type(BorderType::Rounded),
        ),
        area,
    );
}

fn draw_tasks(frame: &mut Frame, area: Rect, app: &App) {
    let items = app.tasks.iter().map(|task| {
        ListItem::new(format!(
            "[{}] {}",
            if task.done { "x" } else { " " },
            task.title
        ))
    });
    let active = matches!(app.mode, Mode::Browse) && !app.navigation_focused;
    let mut state = ListState::default()
        .with_selected((active && !app.tasks.is_empty()).then_some(app.selected_task));
    let title = match app
        .current_project
        .and_then(|id| app.projects.iter().find(|project| project.id == id))
    {
        Some(project) => format!("─[0]─Tasks: {}", project.name),
        None => "─[0]─Tasks".into(),
    };
    frame.render_stateful_widget(
        List::new(items)
            .highlight_spacing(HighlightSpacing::Always)
            .block(panel(&title, !app.navigation_focused).border_type(BorderType::Rounded))
            .highlight_style(selection_style())
            .highlight_symbol("> "),
        area,
        &mut state,
    );
    if app.tasks.is_empty() {
        frame.render_widget(
            Paragraph::new("No tasks here. Press a to add."),
            area.inner(Margin::new(2, 1)),
        );
    }
}

fn draw_help(frame: &mut Frame, area: Rect, app: &App) {
    if !app.message.is_empty() {
        frame.render_widget(
            Paragraph::new(app.message.lines().next().unwrap_or(""))
                .style(Style::default().fg(Color::Cyan)),
            area,
        );
        return;
    }
    if app.commits_focused {
        frame.render_widget(Paragraph::new("r: remote | p: push | P: pull | c: checkpoint | Arrows: move | Enter: details | Esc: back | q: quit"), area);
        return;
    }
    frame.render_widget(
        Paragraph::new(concat!(
            "0/1/2/3: section | Arrows: move | Enter: open | a: add | e: edit | ",
            "Space: complete | m: move | c: checkpoint | d: delete | Esc: back/cancel | q: quit | ",
            "Ctrl+K: search tasks/actions",
        )),
        area,
    );
}

fn draw_dialog(frame: &mut Frame, app: &App) {
    let (title, text) = match &app.mode {
        Mode::Browse => return,
        Mode::Palette { .. } => return draw_palette(frame, app),
        Mode::Move { .. } => return draw_move_dialog(frame, app),
        Mode::RemoteEdit(text) => (
            " Todo remote (origin) ",
            format!("{text}▏\nEnter: save · blank removes origin · Esc: cancel\n{}", app.message),
        ),
        Mode::CommitEdit(text) => (
            " Commit todo snapshot ",
            format!("{text}▏\nEnter: commit SQLite snapshot · Esc: cancel\n{}", app.message),
        ),
        Mode::ProjectEdit(id, text) => (
            if id.is_some() { " Rename project " } else { " Add project " },
            format!("{text}▏\nEnter: save · Esc: cancel\n{}", app.message),
        ),
        Mode::ProjectDelete(_) => (
            " Delete project AND its tasks? ",
            "All tasks in this project will be permanently deleted.\ny/Enter: confirm · n/Esc: cancel".into(),
        ),
        Mode::Edit(id, text) => (
            if id.is_some() {
                " Edit task "
            } else {
                " Add task "
            },
            format!("{text}▏\nEnter: save · Esc: cancel\n{}", app.message),
        ),
        Mode::Delete(_) => (
            " Delete task? ",
            "Permanently delete selected task?\ny/Enter: confirm · n/Esc: cancel".into(),
        ),
    };
    let area = dialog_area(frame.area(), 6);
    frame.render_widget(Clear, area);
    frame.render_widget(Paragraph::new(text).block(panel(title, true)), area);
}

fn draw_move_dialog(frame: &mut Frame, app: &App) {
    let Mode::Move { selected, .. } = &app.mode else {
        return;
    };
    let area = dialog_area(frame.area(), 10);
    let block = panel(" Move task: Enter selects, Esc cancels ", true);
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    let destinations =
        std::iter::once("Inbox").chain(app.projects.iter().map(|project| project.name.as_str()));
    let mut state = ListState::default().with_selected(Some(*selected));
    frame.render_stateful_widget(
        List::new(destinations)
            .highlight_style(selection_style())
            .highlight_symbol("> "),
        inner,
        &mut state,
    );
}

fn draw_palette(frame: &mut Frame, app: &App) {
    let Mode::Palette { query, selected } = &app.mode else {
        return;
    };
    let area = dialog_area(frame.area(), 13);
    let block = panel(" Commands ", true);
    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(block.inner(area));
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(format!("Search tasks/actions: {query}▏")),
        rows[0],
    );

    let actions = app.palette_results();
    if actions.is_empty() {
        frame.render_widget(Paragraph::new("No matching tasks or actions"), rows[1]);
    } else {
        let mut state = ListState::default().with_selected(Some(*selected));
        frame.render_stateful_widget(
            List::new(actions.iter().map(|(label, _)| label.as_str()))
                .highlight_style(selection_style())
                .highlight_symbol("> "),
            rows[1],
            &mut state,
        );
    }
    frame.render_widget(
        Paragraph::new("↑/↓: choose  Enter: run  Esc/Ctrl+K: close"),
        rows[2],
    );
}

fn dialog_area(area: Rect, height: u16) -> Rect {
    let rows = Layout::vertical([
        Constraint::Percentage(30),
        Constraint::Length(height),
        Constraint::Min(0),
    ])
    .split(area);
    Layout::horizontal([
        Constraint::Percentage(10),
        Constraint::Percentage(80),
        Constraint::Percentage(10),
    ])
    .split(rows[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, Result};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::{backend::TestBackend, Terminal};
    use std::path::Path;

    #[test]
    fn only_focused_section_has_a_highlight() -> Result<()> {
        let db = db::open(Path::new(":memory:"))?;
        db::save_project(&db, None, "Work")?;
        let project = db.last_insert_rowid();
        db::create_task(&db, "project task", Some(project))?;
        db::save_task(&db, None, "inbox task")?;
        let mut app = App::new(db)?;
        let mut terminal = Terminal::new(TestBackend::new(100, 25))?;
        for key in ['1', '2', '0'] {
            app.handle_key_event(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE))?;
            terminal.draw(|frame| draw(frame, &app))?;
            let highlighted_rows = terminal
                .backend()
                .buffer()
                .content
                .chunks(100)
                .filter(|row| row.iter().any(|cell| cell.bg == Color::Cyan))
                .count();
            assert_eq!(highlighted_rows, 1, "section {key}");
            let buffer = terminal.backend().buffer();
            assert_eq!(buffer[(3, 1)].symbol(), "A", "Inbox indent changed");
            assert_eq!(buffer[(3, 9)].symbol(), "W", "Projects indent changed");
            for bottom in [7, 15, 23] {
                assert_eq!(buffer[(0, bottom)].symbol(), "╰", "Uneven section heights");
            }
            assert_eq!(buffer[(28, 1)].symbol(), "[", "Tasks indent changed");
        }
        Ok(())
    }
}

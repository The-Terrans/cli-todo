use crate::{
    constants::{FILTERS, KEYBINDINGS},
    types::{App, Mode, TaskDraft},
};
use ratatui::{
    layout::{Alignment, Constraint, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, HighlightSpacing, List, ListItem, ListState, Paragraph,
        Wrap,
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
    draw_footer(frame, areas[1], app);
    draw_dialog(frame, app);
}

fn panel(title: &str, focused: bool) -> Block<'_> {
    let border = if focused {
        Style::default()
            .fg(Color::LightYellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let title = match title
        .strip_prefix('─')
        .and_then(|rest| rest.split_once('─'))
    {
        Some((shortcut, label)) => Line::from(vec![
            Span::styled("─", border.bg(Color::Reset)),
            Span::raw(shortcut),
            Span::styled("─", border.bg(Color::Reset)),
            Span::raw(label),
        ]),
        None => Line::raw(title),
    };
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border)
        .title_style(if focused {
            selection_style()
        } else {
            Style::default().fg(Color::Gray)
        })
}

fn selection_style() -> Style {
    Style::default()
        .fg(Color::Black)
        .bg(Color::LightYellow)
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
            Paragraph::new("c: checkpoint"),
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
    let active = matches!(app.mode, Mode::Browse) && !app.navigation_focused;
    let items = app.tasks.iter().enumerate().map(|(index, task)| {
        let title_style = if active && index == app.selected_task {
            selection_style()
        } else {
            Style::default().fg(Color::Reset)
        };
        let mut lines = vec![Line::styled(
            format!("[{}] {}", if task.done { "x" } else { " " }, task.title,),
            title_style,
        )];
        let mut description = task
            .description
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty());
        if let Some(preview) = description.next() {
            let suffix = if description.next().is_some() {
                " …"
            } else {
                ""
            };
            lines.push(Line::styled(
                format!("    {preview}{suffix}"),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::DIM),
            ));
        }
        ListItem::new(lines)
    });
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
            .style(Style::default().fg(Color::Black))
            .highlight_style(Style::default().bg(Color::LightYellow))
            .highlight_symbol("> "),
        area,
        &mut state,
    );
    if app.tasks.is_empty() {
        frame.render_widget(
            Paragraph::new("No tasks here. Press a to add.")
                .style(Style::default().fg(Color::Reset)),
            area.inner(Margin::new(2, 1)),
        );
    }
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App) {
    let text = if app.message.is_empty() {
        "?: keybindings".into()
    } else {
        format!(
            "?: keybindings | {}",
            app.message.lines().next().unwrap_or("")
        )
    };
    frame.render_widget(
        Paragraph::new(text).style(Style::default().fg(if app.message.is_empty() {
            Color::Gray
        } else {
            Color::Cyan
        })),
        area,
    );
}

fn draw_dialog(frame: &mut Frame, app: &App) {
    let (title, text): (&str, String) = match &app.mode {
        Mode::Browse => return,
        Mode::Help { .. } => return draw_keybindings(frame, app),
        Mode::Palette { .. } => return draw_palette(frame, app),
        Mode::Move { .. } => return draw_move_dialog(frame, app),
        Mode::RemoteEdit(text) => {
            return draw_input_dialog(
                frame,
                " Todo remote (blank removes origin) ",
                text,
                &app.message,
                "save",
            );
        }
        Mode::CommitEdit(text) => {
            return draw_input_dialog(
                frame,
                " Commit todo snapshot ",
                text,
                &app.message,
                "commit",
            );
        }
        Mode::ProjectEdit(id, text) => {
            return draw_input_dialog(
                frame,
                if id.is_some() {
                    " Rename project "
                } else {
                    " Add project "
                },
                text,
                &app.message,
                "save",
            );
        }
        Mode::NoChanges => return draw_no_changes_dialog(frame),
        Mode::Nuke { .. } => return draw_nuke_dialog(frame, app),
        Mode::ProjectDelete(_) => (
            " Delete project AND its tasks? ",
            "All tasks in this project will be permanently deleted.\nEnter: confirm · Esc: cancel"
                .into(),
        ),
        Mode::Edit(id, draft) => return draw_task_editor(frame, id.is_some(), draft, &app.message),
        Mode::Delete(_) => (
            " Delete task? ",
            "Permanently delete selected task?\nEnter: confirm · Esc: cancel".into(),
        ),
    };
    let area = dialog_area(frame.area(), 6);
    frame.render_widget(Clear, area);
    frame.render_widget(Paragraph::new(text).block(panel(title, true)), area);
}

fn draw_task_editor(frame: &mut Frame, editing: bool, draft: &TaskDraft, message: &str) {
    let area = task_editor_area(frame.area());
    let rows = Layout::vertical([
        Constraint::Length(if message.is_empty() { 3 } else { 4 }),
        Constraint::Min(3),
    ])
    .split(area);
    let hint = Style::default()
        .fg(Color::LightYellow)
        .bg(Color::Reset)
        .remove_modifier(Modifier::BOLD);
    let mut title = panel(
        if editing {
            " Edit task: Title "
        } else {
            " Add task: Title "
        },
        !draft.description_focused,
    );
    if !draft.description_focused {
        title = title.title_bottom(
            Line::styled(" Enter: save ── Esc: cancel ", hint).alignment(Alignment::Right),
        );
    }
    let title_inner = title.inner(rows[0]);
    let title_text = Line::raw(if draft.description_focused {
        draft.title.clone()
    } else {
        format!("{}▏", draft.title)
    });
    let horizontal = title_text
        .width()
        .saturating_sub(title_inner.width as usize)
        .min(u16::MAX as usize) as u16;
    let mut description = panel(" Description ", draft.description_focused).title(
        Line::styled(
            " Press <tab> to toggle focus ",
            hint.fg(if draft.description_focused {
                Color::LightYellow
            } else {
                Color::Gray
            }),
        )
        .alignment(Alignment::Right),
    );
    if draft.description_focused {
        description = description.title_bottom(
            Line::styled(" <c-s>: save ── Esc: cancel ", hint).alignment(Alignment::Right),
        );
    }
    let description_inner = description.inner(rows[1]);
    let text = if draft.description_focused {
        format!("{}▏", draft.description)
    } else {
        draft.description.clone()
    };
    let paragraph = Paragraph::new(text).wrap(Wrap { trim: false });
    let vertical = if draft.description_focused {
        paragraph
            .line_count(description_inner.width)
            .saturating_sub(description_inner.height as usize)
            .min(u16::MAX as usize) as u16
    } else {
        0
    };
    frame.render_widget(Clear, area);
    frame.render_widget(title, rows[0]);
    frame.render_widget(
        Paragraph::new(title_text).scroll((0, horizontal)),
        Rect {
            height: title_inner.height.min(1),
            ..title_inner
        },
    );
    if !message.is_empty() && title_inner.height > 1 {
        frame.render_widget(
            Paragraph::new(message.lines().next().unwrap_or(""))
                .style(Style::default().fg(Color::LightRed)),
            Rect {
                y: title_inner.y + 1,
                height: 1,
                ..title_inner
            },
        );
    }
    frame.render_widget(description, rows[1]);
    frame.render_widget(paragraph.scroll((vertical, 0)), description_inner);
}

fn draw_input_dialog(frame: &mut Frame, title: &str, text: &str, message: &str, submit: &str) {
    let area = input_dialog_area(frame.area(), if message.is_empty() { 3 } else { 4 });
    let block = panel(title, true).title_bottom(
        Line::styled(
            format!(" Enter: {submit} ── Esc: cancel "),
            Style::default()
                .fg(Color::LightYellow)
                .bg(Color::Reset)
                .remove_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Right),
    );
    let inner = block.inner(area);
    let input = Line::raw(format!("{text}▏"));
    let scroll = input
        .width()
        .saturating_sub(inner.width as usize)
        .min(u16::MAX as usize) as u16;
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(input).scroll((0, scroll)),
        Rect {
            height: inner.height.min(1),
            ..inner
        },
    );
    if !message.is_empty() && inner.height > 1 {
        frame.render_widget(
            Paragraph::new(message.lines().next().unwrap_or(""))
                .style(Style::default().fg(Color::LightRed)),
            Rect {
                y: inner.y + 1,
                height: 1,
                ..inner
            },
        );
    }
}

fn draw_no_changes_dialog(frame: &mut Frame) {
    let area = dialog_area(frame.area(), 3);
    let block = panel(" Commit ", true).title_bottom(
        Line::styled(
            " Enter/Esc: close ",
            Style::default()
                .fg(Color::LightYellow)
                .bg(Color::Reset)
                .remove_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Right),
    );
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new("No todo changes to commit.").block(block),
        area,
    );
}

fn draw_keybindings(frame: &mut Frame, app: &App) {
    let Mode::Help { scroll } = app.mode else {
        return;
    };
    let area = frame.area().inner(Margin::new(2, 1));
    let lines: Vec<_> = KEYBINDINGS
        .iter()
        .map(|text| {
            if !text.is_empty() && !text.starts_with(' ') {
                Line::styled(
                    *text,
                    Style::default()
                        .fg(Color::LightYellow)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Line::raw(*text)
            }
        })
        .collect();
    let block = panel(" Keybindings ", true)
        .border_type(BorderType::Rounded)
        .title_bottom(
            Line::styled(
                " Up/Down: scroll ── ?/Esc: close ",
                Style::default()
                    .fg(Color::LightYellow)
                    .bg(Color::Reset)
                    .remove_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Right),
        );
    frame.render_widget(Clear, area);
    frame.render_widget(Paragraph::new(lines).scroll((scroll, 0)).block(block), area);
}

fn draw_nuke_dialog(frame: &mut Frame, app: &App) {
    let Mode::Nuke {
        tasks,
        projects,
        commits,
    } = app.mode
    else {
        return;
    };
    let mut lines = vec![
        Line::styled(
            format!(
                "Delete {tasks} tasks, {projects} projects, {commits} commits + remote settings?"
            ),
            Style::default()
                .fg(Color::LightRed)
                .add_modifier(Modifier::BOLD),
        ),
        Line::raw("Backups and remote repository are kept."),
    ];
    if !app.message.is_empty() {
        lines.push(Line::styled(
            app.message.lines().next().unwrap_or(""),
            Style::default().fg(Color::LightRed),
        ));
    }
    let area = dialog_area(frame.area(), lines.len() as u16 + 2);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).block(
            panel(" Nuke ", true).title_bottom(
                Line::styled(
                    "──Enter: confirm ── Esc: cancel ──",
                    Style::default()
                        .fg(Color::LightYellow)
                        .bg(Color::Reset)
                        .remove_modifier(Modifier::BOLD),
                )
                .alignment(Alignment::Right),
            ),
        ),
        area,
    );
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

fn task_editor_area(area: Rect) -> Rect {
    let height = ((u32::from(area.height) * 70 / 100) as u16)
        .max(12)
        .min(area.height.saturating_sub(2));
    let mut dialog = input_dialog_area(area, height);
    dialog.height = height;
    dialog.y = area.y + (area.height - height) / 2;
    dialog
}

fn input_dialog_area(area: Rect, height: u16) -> Rect {
    let mut dialog = dialog_area(area, height);
    dialog.width = ((u32::from(area.width) * 70 / 100) as u16)
        .max(60)
        .min(area.width.saturating_sub(2));
    dialog.x = area.x + (area.width - dialog.width) / 2;
    dialog
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
#[path = "tests/ui.rs"]
mod tests;

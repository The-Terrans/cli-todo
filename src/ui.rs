#[path = "editor_render.rs"]
mod editor_render;
use editor_render::{set_cursor, set_input_cursor, task_field_text, wrapped_cursor};

#[cfg(test)]
#[path = "tests/editor_render.rs"]
mod editor_render_tests;

use crate::{
    constants::{FILTERS, KEYBINDINGS, SORTS},
    dates::is_overdue,
    types::{App, Mode, Sort, Task, TaskDraft, TaskField, TaskPane, TaskView},
};
use chrono::{Local, NaiveDate};
use ratatui::{
    layout::{Alignment, Constraint, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
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
    } else if let Some(view) = &app.task_view {
        draw_task_view(frame, panels[2], app, view);
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

/// Selected tasks are boxed by `draw_task_list`, so they get no fill.
fn task_selection_style() -> Style {
    Style::default()
        .fg(Color::Reset)
        .add_modifier(Modifier::BOLD)
}

/// Renders tasks inside `block` and draws side borders around the selected one.
fn draw_task_list(
    frame: &mut Frame,
    area: Rect,
    block: Block,
    items: Vec<ListItem<'static>>,
    selected: Option<usize>,
) {
    let heights: Vec<usize> = items.iter().map(ListItem::height).collect();
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let mut state = ListState::default().with_selected(selected);
    // The blank gutter leaves room for the left border and its padding.
    frame.render_stateful_widget(
        List::new(items)
            .highlight_spacing(HighlightSpacing::Always)
            .highlight_symbol("  "),
        inner,
        &mut state,
    );
    let Some(selected) = selected else {
        return;
    };
    let above: usize = heights[state.offset().min(selected)..selected].iter().sum();
    let y = inner.y.saturating_add(above.min(u16::MAX as usize) as u16);
    let height =
        (heights[selected].min(u16::MAX as usize) as u16).min(inner.bottom().saturating_sub(y));
    let area = Rect::new(inner.x, y, inner.width, height);
    frame.render_widget(
        Block::new()
            .borders(Borders::LEFT | Borders::RIGHT)
            .border_style(Style::default().fg(Color::LightYellow)),
        area,
    );
    // Keep one blank column before the right border, even for long titles.
    if area.width > 2 {
        let buffer = frame.buffer_mut();
        for row in area.rows() {
            buffer[(area.right() - 2, row.y)].reset();
        }
    }
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

/// Details keeps room for its borders, heading, divider and one description row.
fn task_view_areas(area: Rect, children: bool, heading_lines: u16) -> (Rect, Option<Rect>) {
    if !children {
        return (area, None);
    }
    let height = (area.height / 3)
        .max(heading_lines + 4)
        .min(area.height.saturating_sub(3));
    let rows = Layout::vertical([Constraint::Length(height), Constraint::Min(3)]).split(area);
    (rows[0], Some(rows[1]))
}

fn task_detail_sections(inner: Rect, heading_lines: u16) -> (Rect, Rect, Rect) {
    let rows = Layout::vertical([
        Constraint::Length(inner.height.min(heading_lines)),
        Constraint::Length(u16::from(inner.height > heading_lines)),
        Constraint::Min(0),
    ])
    .split(inner);
    (rows[0], rows[1], rows[2])
}

fn task_heading_text(view: &TaskView) -> Text<'static> {
    let task = &view.task;
    let mut lines = vec![task_title_line(
        &task.title,
        task.done,
        task.subtask_count,
        None,
        Style::default()
            .fg(Color::Reset)
            .add_modifier(Modifier::BOLD),
        Local::now().date_naive(),
    )];
    if let Some(parent) = &view.parent_title {
        lines.push(Line::styled(
            format!("Parent: {parent}"),
            Style::default().fg(Color::Gray),
        ));
    }
    let overdue = task
        .due_date
        .is_some_and(|date| is_overdue(date, task.done, Local::now().date_naive()));
    lines.push(Line::styled(
        format!(
            "Due: {}",
            task.due_date
                .map(|date| date.to_string())
                .unwrap_or_else(|| "none".into())
        ),
        Style::default().fg(if overdue {
            Color::LightRed
        } else {
            Color::Gray
        }),
    ));
    Text::from(lines)
}

fn task_description_text(task: &Task) -> Text<'static> {
    if task.description.is_empty() {
        Text::from(Line::styled(
            "No description",
            Style::default().fg(Color::DarkGray),
        ))
    } else {
        Text::from(
            task.description
                .lines()
                .map(|line| Line::styled(line.to_owned(), Style::default().fg(Color::Gray)))
                .collect::<Vec<_>>(),
        )
    }
}

pub fn task_detail_scroll_bounds(view: &TaskView, viewport: Rect) -> (u16, u16) {
    let body = Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).split(viewport)[0];
    let area = Layout::horizontal([Constraint::Percentage(25), Constraint::Min(0)]).split(body)[1];
    let heading_lines = task_heading_text(view).height() as u16;
    let (details, _) = task_view_areas(area, !view.children.is_empty(), heading_lines);
    let (_, _, description) = task_detail_sections(panel("", false).inner(details), heading_lines);
    if description.height == 0 {
        return (0, 1);
    }
    let lines = Paragraph::new(task_description_text(&view.task))
        .wrap(Wrap { trim: false })
        .line_count(description.width);
    (
        lines
            .saturating_sub(description.height as usize)
            .min(u16::MAX as usize) as u16,
        description.height.max(1),
    )
}

fn draw_task_view(frame: &mut Frame, area: Rect, app: &App, view: &TaskView) {
    let focused = matches!(app.mode, Mode::Browse) && !app.navigation_focused;
    let heading_text = task_heading_text(view);
    let heading_lines = heading_text.height() as u16;
    let (details, children) = task_view_areas(area, !view.children.is_empty(), heading_lines);
    let details_focused = focused && view.focus == TaskPane::Details;
    let title = project_title(app, "─[0]─Task details", view.task.project_id);
    let block = panel(&title, details_focused).border_type(BorderType::Rounded);
    let (heading, divider, description) = task_detail_sections(block.inner(details), heading_lines);
    frame.render_widget(block, details);
    frame.render_widget(Paragraph::new(heading_text), heading);
    frame.render_widget(
        Paragraph::new(format!(
            "─ Description {}",
            "─".repeat(divider.width.saturating_sub(14) as usize)
        ))
        .style(Style::default().fg(if details_focused {
            Color::LightYellow
        } else {
            Color::DarkGray
        })),
        divider,
    );
    frame.render_widget(
        Paragraph::new(task_description_text(&view.task))
            .wrap(Wrap { trim: false })
            .scroll((view.scroll, 0)),
        description,
    );
    if let Some(area) = children {
        let focused = focused && view.focus == TaskPane::Subtasks;
        let today = Local::now().date_naive();
        let items = view
            .children
            .iter()
            .enumerate()
            .map(|(index, child)| {
                let selected = focused && index == view.selected_child;
                let style = if selected {
                    task_selection_style()
                } else {
                    Style::default().fg(Color::Reset)
                };
                let title = task_title_line(
                    &child.title,
                    child.done,
                    child.subtask_count,
                    child.due_date,
                    style,
                    today,
                );
                ListItem::new(title)
            })
            .collect();
        draw_task_list(
            frame,
            area,
            panel(" Subtasks ", focused).border_type(BorderType::Rounded),
            items,
            focused.then_some(view.selected_child),
        );
    }
}

fn draw_tasks(frame: &mut Frame, area: Rect, app: &App) {
    let active = matches!(app.mode, Mode::Browse) && !app.navigation_focused;
    let today = Local::now().date_naive();
    let items = app.tasks.iter().enumerate().map(|(index, task)| {
        let selected = active && index == app.selected_task;
        let title_style = if selected {
            task_selection_style()
        } else {
            Style::default().fg(Color::Reset)
        };
        let mut title = task_title_line(
            &task.title,
            task.done,
            task.subtask_count,
            task.due_date,
            title_style,
            today,
        );
        // Inbox mixes every project, so label each task with its own.
        if let Some(project) = task
            .project_id
            .filter(|_| app.current_project.is_none())
            .and_then(|id| app.projects.iter().find(|project| project.id == id))
        {
            title.spans.insert(
                1,
                Span::styled(
                    format!(" [{}]", project.name),
                    Style::default()
                        .fg(Color::Gray)
                        .remove_modifier(Modifier::BOLD),
                ),
            );
        }
        let mut lines = vec![title];
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
    let mut title = project_title(app, "─[0]─Tasks", app.current_project);
    title.push_str(&format!(" · {}", FILTERS[app.selected_filter].0));
    if app.sort != Sort::Manual {
        if let Some((label, _)) = SORTS.iter().find(|(_, sort)| *sort == app.sort) {
            title.push_str(&format!(" · {label}"));
        }
    }
    draw_task_list(
        frame,
        area,
        panel(&title, !app.navigation_focused).border_type(BorderType::Rounded),
        items.collect(),
        (active && !app.tasks.is_empty()).then_some(app.selected_task),
    );
    if app.tasks.is_empty() {
        frame.render_widget(
            Paragraph::new("No tasks here. Press a to add.")
                .style(Style::default().fg(Color::Reset)),
            area.inner(Margin::new(2, 1)),
        );
    }
}

fn project_title(app: &App, label: &str, project: Option<i64>) -> String {
    match project.and_then(|id| app.projects.iter().find(|project| project.id == id)) {
        Some(project) => format!("{label}: {}", project.name),
        None => label.into(),
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
        Mode::Filter { selected } => {
            return draw_picker(
                frame,
                " Filter tasks: Enter selects, Esc cancels ",
                FILTERS.iter().map(|(label, _)| *label),
                *selected,
                FILTERS.len() as u16 + 2,
            )
        }
        Mode::Sort { selected } => {
            return draw_picker(
                frame,
                " Sort pending tasks: Enter selects, Esc cancels ",
                SORTS.iter().map(|(label, _)| *label),
                *selected,
                SORTS.len() as u16 + 2,
            )
        }
        Mode::RemoteEdit(text) => {
            return draw_input_dialog(
                frame,
                " Todo remote (blank removes origin) ",
                text,
                &app.input_cursor,
                &app.message,
                "save",
            );
        }
        Mode::CommitEdit(text) => {
            return draw_input_dialog(
                frame,
                " Commit todo snapshot ",
                text,
                &app.input_cursor,
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
                &app.input_cursor,
                &app.message,
                "save",
            );
        }
        Mode::NoChanges => return draw_no_changes_dialog(frame),
        Mode::Nuke { .. } => return draw_nuke_dialog(frame, app),
        Mode::ResetHistory { .. } => return draw_reset_history_dialog(frame, app),
        Mode::ProjectDelete(_) => (
            " Delete project AND its tasks? ",
            "All tasks in this project will be permanently deleted.\nEnter: confirm · Esc: cancel"
                .into(),
        ),
        Mode::Edit(id, draft) => {
            return draw_task_editor(
                frame,
                id.is_some(),
                draft,
                &app.message,
                app.new_task_parent.is_some()
                    || app
                        .task_view
                        .as_ref()
                        .is_some_and(|view| view.task.parent_id.is_some()),
            )
        }
        Mode::Delete(_) => (
            " Delete task AND its subtasks? ",
            "Permanently delete this task and its descendants?\nEnter: confirm · Esc: cancel"
                .into(),
        ),
    };
    let area = dialog_area(frame.area(), 6);
    frame.render_widget(Clear, area);
    frame.render_widget(Paragraph::new(text).block(panel(title, true)), area);
}

fn draw_task_editor(
    frame: &mut Frame,
    editing: bool,
    draft: &TaskDraft,
    message: &str,
    subtask: bool,
) {
    let title_focused = draft.focus == TaskField::Title;
    let description_focused = draft.focus == TaskField::Description;
    let due_focused = draft.focus == TaskField::DueDate;
    let area = task_editor_area(frame.area());
    let error_height = if message.is_empty() {
        0
    } else {
        Paragraph::new(message)
            .wrap(Wrap { trim: false })
            .line_count(area.width.saturating_sub(2))
            .max(1)
            .min(area.height.saturating_sub(9).max(1) as usize) as u16
    };
    let rows = Layout::vertical([
        Constraint::Length(3 + if due_focused { 0 } else { error_height }),
        Constraint::Fill(1),
        Constraint::Length(3 + if due_focused { error_height } else { 0 }),
    ])
    .split(area);
    let hint = Style::default()
        .fg(Color::LightYellow)
        .bg(Color::Reset)
        .remove_modifier(Modifier::BOLD);
    let mut title = panel(
        match (editing, subtask) {
            (true, true) => " Edit subtask: Title ",
            (false, true) => " Add subtask: Title ",
            (true, false) => " Edit task: Title ",
            (false, false) => " Add task: Title ",
        },
        title_focused,
    );
    let escape = if draft.auto_date.is_some() {
        "Esc: clear date"
    } else {
        "Esc: cancel"
    };
    if title_focused {
        title = title.title_bottom(
            Line::styled(format!(" Enter: save ── {escape} "), hint).alignment(Alignment::Right),
        );
    }
    let title_inner = title.inner(rows[0]);
    let title_text = task_field_text(draft, TaskField::Title);
    let horizontal =
        draft.cursors[TaskField::Title as usize].horizontal_scroll(&draft.title, title_inner.width);
    let mut description = panel(" Description ", description_focused).title(
        Line::styled(
            " Press <tab> to toggle focus ",
            hint.fg(if description_focused {
                Color::LightYellow
            } else {
                Color::Gray
            }),
        )
        .alignment(Alignment::Right),
    );
    if description_focused {
        description = description.title_bottom(
            Line::styled(format!(" <c-s>: save ── {escape} "), hint).alignment(Alignment::Right),
        );
    }
    let description_inner = description.inner(rows[1]);
    let text = task_field_text(draft, TaskField::Description);
    let (cursor_column, cursor_row) = if description_focused {
        wrapped_cursor(draft, description_inner.width)
    } else {
        (0, 0)
    };
    let paragraph = Paragraph::new(text).wrap(Wrap { trim: false });
    let vertical = if description_focused {
        draft.cursors[TaskField::Description as usize].vertical_scroll(
            cursor_row,
            description_inner.height,
            paragraph.line_count(description_inner.width).max(1),
        )
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
    if !message.is_empty() && !due_focused && title_inner.height > 1 {
        frame.render_widget(
            Paragraph::new(message)
                .wrap(Wrap { trim: false })
                .style(Style::default().fg(Color::LightRed)),
            Rect {
                y: title_inner.y + 1,
                height: title_inner.height.saturating_sub(1),
                ..title_inner
            },
        );
    }
    frame.render_widget(description, rows[1]);
    frame.render_widget(paragraph.scroll((vertical, 0)), description_inner);
    if title_focused {
        set_input_cursor(
            frame,
            title_inner,
            &draft.cursors[TaskField::Title as usize],
            &draft.title,
            horizontal,
        );
    } else if description_focused {
        set_cursor(
            frame,
            description_inner,
            cursor_column,
            cursor_row.saturating_sub(vertical as usize),
        );
    }
    draw_due_date(
        frame,
        rows[2],
        draft,
        if due_focused { message } else { "" },
    );
}

fn draw_due_date(frame: &mut Frame, area: Rect, draft: &TaskDraft, message: &str) {
    let focused = draft.focus == TaskField::DueDate;
    let mut block = panel(" Due date (YYYY-MM-DD; blank clears) ", focused);
    if focused {
        block = block.title_bottom(
            Line::styled(
                if draft.auto_date.is_some() {
                    " Enter: save ── Esc: clear date "
                } else {
                    " Enter: save ── Esc: cancel "
                },
                Style::default()
                    .fg(Color::LightYellow)
                    .bg(Color::Reset)
                    .remove_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Right),
        );
    }
    let inner = block.inner(area);
    let text = Line::raw(if !focused && draft.due_date.is_empty() {
        "No due date".into()
    } else {
        draft.due_date.clone()
    });
    let horizontal =
        draft.cursors[TaskField::DueDate as usize].horizontal_scroll(&draft.due_date, inner.width);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(text).scroll((0, horizontal)),
        Rect {
            height: inner.height.min(1),
            ..inner
        },
    );
    if focused {
        set_input_cursor(
            frame,
            inner,
            &draft.cursors[TaskField::DueDate as usize],
            &draft.due_date,
            horizontal,
        );
    }
    if !message.is_empty() && inner.height > 1 {
        frame.render_widget(
            Paragraph::new(message)
                .wrap(Wrap { trim: false })
                .style(Style::default().fg(Color::LightRed)),
            Rect {
                y: inner.y + 1,
                height: inner.height.saturating_sub(1),
                ..inner
            },
        );
    }
}

fn task_title_line(
    title: &str,
    done: bool,
    count: usize,
    due: Option<NaiveDate>,
    style: Style,
    today: NaiveDate,
) -> Line<'static> {
    let mut spans = vec![Span::raw(format!(
        "[{}] {title}{}",
        if done { "x" } else { " " },
        subtask_count_label(count)
    ))];
    if let Some(date) = due {
        let overdue = is_overdue(date, done, today);
        let color = if overdue {
            Color::LightRed
        } else if done {
            Color::DarkGray
        } else {
            Color::Gray
        };
        let mut badge = Style::default().fg(color).remove_modifier(Modifier::BOLD);
        if overdue {
            badge = badge.add_modifier(Modifier::BOLD);
        }
        if done {
            badge = badge.add_modifier(Modifier::DIM);
        }
        spans.push(Span::styled(format!(" [due: {date}]"), badge));
    }
    Line::from(spans).style(style)
}

fn subtask_count_label(count: usize) -> String {
    match count {
        0 => String::new(),
        1 => " (1 subtask)".into(),
        _ => format!(" ({count} subtasks)"),
    }
}

fn draw_input_dialog(
    frame: &mut Frame,
    title: &str,
    text: &str,
    cursor: &crate::editor::TextCursor,
    message: &str,
    submit: &str,
) {
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
    let input = Line::raw(text);
    let scroll = cursor.horizontal_scroll(text, inner.width);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(input).scroll((0, scroll)),
        Rect {
            height: inner.height.min(1),
            ..inner
        },
    );
    set_input_cursor(frame, inner, cursor, text, scroll);
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

fn draw_reset_history_dialog(frame: &mut Frame, app: &App) {
    let Mode::ResetHistory { commits } = app.mode else {
        return;
    };
    let mut lines = vec![
        Line::styled(
            format!("Permanently delete {commits} local commits + remote settings?"),
            Style::default()
                .fg(Color::LightRed)
                .add_modifier(Modifier::BOLD),
        ),
        Line::raw("All todos, subtasks, projects and existing backups are kept."),
        Line::raw("Remote repository is untouched. No new backup is created."),
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
            panel(" Reset todo history ", true).title_bottom(
                Line::styled(
                    "──Enter: confirm ── Esc: cancel ──",
                    Style::default().fg(Color::LightYellow).bg(Color::Reset),
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
    let destinations = std::iter::once("No project")
        .chain(app.projects.iter().map(|project| project.name.as_str()));
    draw_picker(
        frame,
        " Move task: Enter selects, Esc cancels ",
        destinations,
        *selected,
        10,
    );
}

/// Draws a popup list of choices with `selected` highlighted.
fn draw_picker<'a>(
    frame: &mut Frame,
    title: &str,
    items: impl IntoIterator<Item = &'a str>,
    selected: usize,
    height: u16,
) {
    let area = dialog_area(frame.area(), height);
    let block = panel(title, true);
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    let mut state = ListState::default().with_selected(Some(selected));
    frame.render_stateful_widget(
        List::new(items)
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
    let search = format!("Search tasks/projects/actions: {query}");
    let cursor = crate::editor::TextCursor::default();
    let scroll = cursor.horizontal_scroll(&search, rows[0].width);
    frame.render_widget(Paragraph::new(search.as_str()).scroll((0, scroll)), rows[0]);
    set_input_cursor(frame, rows[0], &cursor, &search, scroll);

    let actions = app.palette_results();
    if actions.is_empty() {
        frame.render_widget(
            Paragraph::new("No matching tasks, projects or actions"),
            rows[1],
        );
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
        .max(15)
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

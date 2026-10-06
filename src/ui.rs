use crate::app::{App, Mode, FILTERS};
use ratatui::{
    layout::{Constraint, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};

pub fn draw(frame: &mut Frame, app: &App) {
    let areas = Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).split(frame.area());
    let panels = Layout::horizontal([Constraint::Percentage(25), Constraint::Percentage(75)])
        .split(areas[0]);
    draw_navigation(frame, panels[0], app);
    draw_tasks(frame, panels[1], app);
    draw_help(frame, areas[1]);
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
    let mut state = ListState::default().with_selected(Some(app.selected_filter));
    let filters = FILTERS.iter().map(|(label, _)| *label);
    frame.render_stateful_widget(
        List::new(filters)
            .block(panel(" Inbox ", app.navigation_focused))
            .highlight_style(selection_style())
            .highlight_symbol("> "),
        area,
        &mut state,
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
    let mut state =
        ListState::default().with_selected((!app.tasks.is_empty()).then_some(app.selected_task));
    frame.render_stateful_widget(
        List::new(items)
            .block(panel(" Tasks ", !app.navigation_focused))
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

fn draw_help(frame: &mut Frame, area: Rect) {
    frame.render_widget(
        Paragraph::new(concat!(
            "Tab: panel | Arrows: move | Enter: open | a: add | e: edit | ",
            "Space: complete | d: delete | Esc: cancel | q: quit | ",
            "Ctrl+K: search tasks/actions",
        )),
        area,
    );
}

fn draw_dialog(frame: &mut Frame, app: &App) {
    let (title, text) = match &app.mode {
        Mode::Browse => return,
        Mode::Palette { .. } => return draw_palette(frame, app),
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

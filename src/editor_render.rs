use crate::{
    editor::TextCursor,
    types::{TaskDraft, TaskField},
};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Paragraph, Widget, Wrap},
    Frame,
};

pub(super) fn task_field_text(draft: &TaskDraft, field: TaskField) -> Text<'static> {
    let text = draft.field_text(field);
    let range = draft
        .auto_date
        .as_ref()
        .filter(|auto| auto.field == field)
        .map(|auto| &auto.phrase.range);
    let mut lines = Vec::new();
    let mut spans = Vec::new();
    for (position, character) in text.char_indices() {
        if character == '\n' {
            lines.push(Line::from(std::mem::take(&mut spans)));
        } else {
            let style = if range.is_some_and(|range| range.contains(&position)) {
                Style::default()
                    .fg(Color::LightYellow)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                Style::default()
            };
            spans.push(Span::styled(character.to_string(), style));
        }
    }
    lines.push(Line::from(spans));
    Text::from(lines)
}

/// Measure the unmodified text with the same word-wrapper used for display.
/// Offscreen styles identify source characters without inserting a cursor glyph.
pub(super) fn wrapped_cursor(draft: &TaskDraft, width: u16) -> (u16, usize) {
    if width == 0 {
        return (0, 0);
    }
    let text = &draft.description;
    let cursor = draft.cursors[TaskField::Description as usize].position(text);
    let mut offset = 0;
    let mut row = 0;
    for line in text.split('\n') {
        let paragraph = Paragraph::new(line).wrap(Wrap { trim: false });
        let height = paragraph.line_count(width).max(1);
        if cursor <= offset + line.len() {
            let spans: Vec<_> = line
                .char_indices()
                .map(|(position, character)| {
                    Span::styled(
                        character.to_string(),
                        Style::default().add_modifier(if offset + position >= cursor {
                            Modifier::REVERSED
                        } else {
                            Modifier::DIM
                        }),
                    )
                })
                .collect();
            let area = Rect::new(0, 0, width, height.min(u16::MAX as usize) as u16);
            let mut buffer = Buffer::empty(area);
            Paragraph::new(Line::from(spans))
                .wrap(Wrap { trim: false })
                .render(area, &mut buffer);
            if let Some(index) = buffer
                .content
                .iter()
                .position(|cell| cell.modifier.contains(Modifier::REVERSED))
            {
                return (
                    (index % width as usize) as u16,
                    row + index / width as usize,
                );
            }
            // End of line, or whitespace omitted by word wrapping: use the end
            // of the last rendered character, clamped to the last visible cell.
            if let Some(index) = buffer
                .content
                .iter()
                .rposition(|cell| cell.modifier.contains(Modifier::DIM))
            {
                let column =
                    index % width as usize + Span::raw(buffer.content[index].symbol()).width();
                return (
                    column.min(width as usize - 1) as u16,
                    row + index / width as usize,
                );
            }
            return (0, row);
        }
        row += height;
        offset += line.len() + 1;
    }
    (0, row)
}

pub(super) fn set_cursor(frame: &mut Frame, area: Rect, column: u16, row: usize) {
    if area.width > 0 && area.height > 0 {
        let x = area.x + column.min(area.width - 1);
        let y = area.y + row.min(area.height as usize - 1) as u16;
        if frame.area().contains((x, y).into()) {
            frame.set_cursor_position((x, y));
        }
    }
}

pub(super) fn set_input_cursor(
    frame: &mut Frame,
    area: Rect,
    cursor: &TextCursor,
    text: &str,
    scroll: u16,
) {
    let column = cursor
        .column(text)
        .saturating_sub(scroll as usize)
        .min(u16::MAX as usize) as u16;
    set_cursor(frame, area, column, 0);
}

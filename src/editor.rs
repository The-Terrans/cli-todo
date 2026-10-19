use crate::types::{TaskDraft, TaskField};
use crossterm::event::KeyCode;
use ratatui::text::Span;
use std::{cell::Cell, ops::Range};

impl TaskDraft {
    pub fn field_text(&self, field: TaskField) -> &str {
        match field {
            TaskField::Title => &self.title,
            TaskField::Description => &self.description,
            TaskField::DueDate => &self.due_date,
        }
    }

    pub fn edit_focused(&mut self, key: KeyCode) {
        let text = match self.focus {
            TaskField::Title => &mut self.title,
            TaskField::Description => &mut self.description,
            TaskField::DueDate => &mut self.due_date,
        };
        self.cursors[self.focus as usize].handle(text, key, self.focus == TaskField::Description);
    }
}

/// Byte offsets stay on UTF-8 boundaries. An unset cursor starts at the text end.
#[derive(Clone, Default)]
pub struct TextCursor {
    position: Option<usize>,
    preferred_column: Option<usize>,
    horizontal_offset: Cell<usize>,
    vertical_offset: Cell<usize>,
}

impl TextCursor {
    pub fn position(&self, text: &str) -> usize {
        let mut position = self.position.unwrap_or(text.len()).min(text.len());
        while !text.is_char_boundary(position) {
            position -= 1;
        }
        position
    }

    pub fn deletion_range(&self, text: &str, key: KeyCode) -> Range<usize> {
        let position = self.position(text);
        match key {
            KeyCode::Backspace => {
                text[..position]
                    .char_indices()
                    .next_back()
                    .map_or(position, |(i, _)| i)..position
            }
            KeyCode::Delete => {
                position..position + text[position..].chars().next().map_or(0, char::len_utf8)
            }
            _ => position..position,
        }
    }

    pub fn handle(&mut self, text: &mut String, key: KeyCode, multiline: bool) -> bool {
        let position = self.position(text);
        let next = match key {
            KeyCode::Left => text[..position]
                .char_indices()
                .next_back()
                .map_or(0, |(i, _)| i),
            KeyCode::Right => position + text[position..].chars().next().map_or(0, char::len_utf8),
            KeyCode::Home => text[..position].rfind('\n').map_or(0, |i| i + 1),
            KeyCode::End => position + text[position..].find('\n').unwrap_or(text.len() - position),
            KeyCode::Up | KeyCode::Down if multiline => {
                let start = text[..position].rfind('\n').map_or(0, |i| i + 1);
                let column = *self
                    .preferred_column
                    .get_or_insert_with(|| Span::raw(&text[start..position]).width());
                let target = if key == KeyCode::Up {
                    if start == 0 {
                        return true;
                    }
                    let end = start - 1;
                    text[..end].rfind('\n').map_or(0, |i| i + 1)..end
                } else {
                    let Some(end) = text[position..].find('\n').map(|i| position + i) else {
                        return true;
                    };
                    let next = end + 1;
                    next..next + text[next..].find('\n').unwrap_or(text.len() - next)
                };
                let mut width = 0;
                let mut destination = target.start;
                for character in text[target.clone()].chars() {
                    let next_width = width + Span::raw(character.to_string()).width();
                    if next_width > column {
                        break;
                    }
                    width = next_width;
                    destination += character.len_utf8();
                }
                self.position = Some(destination);
                return true;
            }
            KeyCode::Char(character) => {
                text.insert(position, character);
                position + character.len_utf8()
            }
            KeyCode::Backspace | KeyCode::Delete => {
                let range = self.deletion_range(text, key);
                let next = range.start;
                text.replace_range(range, "");
                next
            }
            _ => return false,
        };
        self.position = Some(next);
        self.preferred_column = None;
        true
    }

    #[cfg(test)]
    pub fn marked(&self, text: &str) -> String {
        let position = self.position(text);
        format!("{}▏{}", &text[..position], &text[position..])
    }

    pub fn column(&self, text: &str) -> usize {
        Span::raw(&text[..self.position(text)]).width()
    }

    pub fn horizontal_scroll(&self, text: &str, width: u16) -> u16 {
        Self::reveal(
            &self.horizontal_offset,
            self.column(text),
            width,
            Span::raw(text).width().saturating_add(1),
        )
    }

    pub fn vertical_scroll(&self, row: usize, height: u16, lines: usize) -> u16 {
        Self::reveal(&self.vertical_offset, row, height, lines)
    }

    fn reveal(saved: &Cell<usize>, position: usize, size: u16, total: usize) -> u16 {
        if size == 0 {
            return 0;
        }
        let maximum = total.saturating_sub(size as usize);
        let mut offset = saved.get().min(maximum).min(position);
        if position >= offset.saturating_add(size as usize) {
            offset = position.saturating_add(1).saturating_sub(size as usize);
        }
        let offset = offset.min(u16::MAX as usize);
        saved.set(offset);
        offset as u16
    }
}

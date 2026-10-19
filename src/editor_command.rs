use crate::types::Result;
use std::{env, ffi::OsString, process::Command};

pub(crate) struct EditorCommand {
    executable: String,
    arguments: Vec<String>,
}

impl EditorCommand {
    pub fn from_env() -> Result<Self> {
        Self::from_values(env::var_os("VISUAL"), env::var_os("EDITOR"))
    }

    fn from_values(visual: Option<OsString>, editor: Option<OsString>) -> Result<Self> {
        for value in [visual, editor].into_iter().flatten() {
            let value = value
                .into_string()
                .map_err(|_| "VISUAL/EDITOR must contain valid UTF-8")?;
            if !value.trim().is_empty() {
                return Self::parse(&value);
            }
        }
        Err("Set VISUAL or EDITOR to an editor that waits until editing finishes".into())
    }

    /// Split quoted arguments without invoking a shell or expanding variables.
    pub(crate) fn parse(value: &str) -> Result<Self> {
        let mut words = Vec::new();
        let mut word = String::new();
        let mut quote = None;
        let mut started = false;
        let mut characters = value.chars().peekable();
        while let Some(character) = characters.next() {
            match (quote, character) {
                (Some('\''), '\'') | (Some('"'), '"') => quote = None,
                (Some('\''), _) => word.push(character),
                (_, '\\') => {
                    let escaped = characters
                        .next()
                        .ok_or("Trailing backslash in VISUAL/EDITOR")?;
                    if quote == Some('"') && !matches!(escaped, '$' | '`' | '"' | '\\' | '\n') {
                        word.push('\\');
                    }
                    if escaped != '\n' {
                        word.push(escaped);
                    }
                    started = true;
                }
                (None, '\'' | '"') => {
                    quote = Some(character);
                    started = true;
                }
                (None, _) if character.is_whitespace() => {
                    if started {
                        words.push(std::mem::take(&mut word));
                        started = false;
                    }
                }
                _ => {
                    word.push(character);
                    started = true;
                }
            }
        }
        if quote.is_some() {
            return Err("Unclosed quote in VISUAL/EDITOR".into());
        }
        if started {
            words.push(word);
        }
        if words.first().is_none_or(String::is_empty) {
            return Err("VISUAL/EDITOR needs an executable name".into());
        }
        let executable = words.remove(0);
        Ok(Self {
            executable,
            arguments: words,
        })
    }

    pub fn command(&self) -> Command {
        let mut command = Command::new(&self.executable);
        command.args(&self.arguments);
        command
    }
}

#[cfg(test)]
#[path = "tests/editor_command.rs"]
mod tests;

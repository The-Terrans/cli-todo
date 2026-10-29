use crate::{
    dates::{date_phrases, parse_due_date},
    editor_command::EditorCommand,
    types::{App, Mode, Result, TaskDraft, TaskField},
};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::SystemTime,
};

const DUE_DATE_PLACEHOLDER: &str = "YYYY-MM-DD";

pub(crate) fn requested(key: KeyEvent, mode: &Mode) -> bool {
    key.kind == KeyEventKind::Press
        && key.modifiers.difference(KeyModifiers::SHIFT) == KeyModifiers::CONTROL
        && matches!(key.code, KeyCode::Char('g' | 'G'))
        && matches!(mode, Mode::Edit(..))
}

pub(crate) struct ExternalEdit {
    pub path: PathBuf,
    command: EditorCommand,
}

impl ExternalEdit {
    pub fn prepare(draft: &TaskDraft) -> Result<Self> {
        Self::create(draft, EditorCommand::from_env()?, &std::env::temp_dir())
    }

    fn create(draft: &TaskDraft, command: EditorCommand, root: &Path) -> Result<Self> {
        if draft.title.contains(['\n', '\r']) || draft.due_date.contains(['\n', '\r']) {
            return Err(
                "Title and Due date must each fit on one line before opening an external editor"
                    .into(),
            );
        }
        let stamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_nanos() as u64;
        for attempt in 0..100 {
            let directory = root.join(format!(
                "cli-todo-edit-{}-{stamp:x}-{attempt}",
                std::process::id()
            ));
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            builder.mode(0o700);
            match builder.create(&directory) {
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
                Ok(()) => {}
            }
            let path = directory.join("task.md");
            let result = (|| -> Result<()> {
                let mut options = OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                options.mode(0o600);
                let mut file = options.open(&path)?;
                write!(
                    file,
                    "Title: {}\nDue date: {}\n\n{}",
                    draft.title,
                    if draft.due_date.trim().is_empty() {
                        DUE_DATE_PLACEHOLDER
                    } else {
                        &draft.due_date
                    },
                    draft.description
                )?;
                file.flush()?;
                Ok(())
            })();
            if let Err(error) = result {
                let _ = fs::remove_dir_all(directory);
                return Err(error);
            }
            return Ok(Self { path, command });
        }
        Err("Could not allocate a private external-editor directory".into())
    }

    pub fn launch(&self) -> Result<()> {
        let status = self.command.command().arg(&self.path).status()?;
        if !status.success() {
            return Err(format!("External editor exited with {status}").into());
        }
        Ok(())
    }

    pub fn recovery_message(&self, error: impl std::fmt::Display) -> String {
        format!("Recovery: {} — {error}", self.path.display())
    }

    /// Returns true when the edited file was imported into the popup draft.
    pub fn finish(self, app: &mut App) -> bool {
        let result = (|| -> Result<TaskDraft> {
            let Mode::Edit(_, previous) = &app.mode else {
                return Err("Task popup is no longer open".into());
            };
            parse_task(&fs::read_to_string(&self.path)?, previous)
        })();
        match result {
            Ok(imported) => {
                if let Mode::Edit(_, draft) = &mut app.mode {
                    *draft = imported;
                }
                // Shift+E saves right after this, so the message must not imply an unsaved draft.
                let directory = self.path.parent().unwrap();
                app.message = match fs::remove_dir_all(directory) {
                    Ok(()) => String::new(),
                    Err(error) => format!(
                        "Temporary directory not removed: {} — {error}",
                        directory.display()
                    ),
                };
                true
            }
            Err(error) => {
                app.message = self.recovery_message(error);
                false
            }
        }
    }
}

fn parse_task(text: &str, previous: &TaskDraft) -> Result<TaskDraft> {
    let (title, rest) = text
        .split_once('\n')
        .ok_or("Expected Title and Due date headers followed by a blank line")?;
    let (due_date, rest) = rest
        .split_once('\n')
        .ok_or("Expected a Due date header followed by a blank line")?;
    let (separator, description) = rest
        .split_once('\n')
        .ok_or("Expected a blank line before Description")?;
    if !separator.trim_end_matches('\r').is_empty() {
        return Err("Expected a blank line before Description".into());
    }
    let title = title
        .trim_end_matches('\r')
        .strip_prefix("Title:")
        .ok_or("First line must start with Title:")?;
    let title = title.strip_prefix(' ').unwrap_or(title);
    if title.trim().is_empty() {
        return Err("Title cannot be empty".into());
    }
    let due_date = due_date
        .trim_end_matches('\r')
        .strip_prefix("Due date:")
        .ok_or("Second line must start with Due date:")?
        .trim();
    let due_date = if due_date == DUE_DATE_PLACEHOLDER {
        ""
    } else {
        due_date
    };
    parse_due_date(due_date)?;
    let mut draft = previous.clone();
    draft.title = title.into();
    draft.description = description.into();
    draft.due_date = due_date.into();
    draft.auto_date = None;
    draft.cursors = Default::default();
    // Imported date phrases are literal; later typing still detects new phrases.
    let today = chrono::Local::now().date_naive();
    draft.ignored_dates = [
        (TaskField::Title, &draft.title),
        (TaskField::Description, &draft.description),
    ]
    .into_iter()
    .flat_map(|(field, text)| {
        date_phrases(text, today)
            .into_iter()
            .map(move |phrase| (field, text[phrase.range].to_ascii_lowercase()))
    })
    .collect();
    Ok(draft)
}

#[cfg(test)]
#[path = "tests/external_editor.rs"]
mod tests;

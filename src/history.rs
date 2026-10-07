use crate::Result;
use rusqlite::Connection;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

pub struct Commit {
    pub hash: String,
    pub subject: String,
}

pub struct History {
    pub directory: PathBuf,
}

impl History {
    pub fn new(database: &Connection) -> Option<Self> {
        let path = database.path()?;
        if path.is_empty() || path == ":memory:" {
            return None;
        }
        Some(Self {
            directory: Path::new(path).parent()?.join("history"),
        })
    }

    fn git(&self, args: &[&str]) -> Result<Output> {
        Ok(Command::new("git")
            .arg("-C")
            .arg(&self.directory)
            .args(args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_COMMON_DIR")
            .output()?)
    }

    fn git_text(&self, args: &[&str]) -> Result<String> {
        let output = self.git(args)?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr)
                .trim()
                .to_string()
                .into());
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    pub fn list(&self) -> Result<Vec<Commit>> {
        if !self.directory.join(".git").exists() {
            return Ok(vec![]);
        }
        if !self
            .git(&["rev-parse", "--verify", "--quiet", "HEAD"])?
            .status
            .success()
        {
            return Ok(vec![]);
        }
        let log = self.git_text(&["log", "--format=%H%x09%s"])?;
        Ok(log
            .lines()
            .filter_map(|line| line.split_once('\t'))
            .map(|(hash, subject)| Commit {
                hash: hash.into(),
                subject: subject.into(),
            })
            .collect())
    }

    pub fn details(&self, hash: &str) -> Result<String> {
        self.git_text(&[
            "--no-pager",
            "show",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
            "--stat",
            "--format=fuller",
            hash,
            "--",
        ])
    }

    pub fn has_changes(&self, database: &Connection) -> Result<bool> {
        if self.list()?.is_empty() {
            return Ok(database.query_row(
                "SELECT EXISTS(SELECT 1 FROM tasks) OR EXISTS(SELECT 1 FROM projects)",
                [],
                |row| row.get(0),
            )?);
        }
        let filename = format!(".status-{}.sqlite3", std::process::id());
        let path = self.directory.join(&filename);
        let result = (|| -> Result<bool> {
            self.snapshot_to(database, &path)?;
            let current = self.git_text(&["hash-object", "--no-filters", "--", &filename])?;
            let committed = self.git_text(&["rev-parse", "HEAD:tasks.sqlite3"])?;
            Ok(current.trim() != committed.trim())
        })();
        if path.exists() {
            let _ = fs::remove_file(path);
        }
        result
    }

    pub fn commit(&self, database: &Connection, message: &str) -> Result<String> {
        let message = message.trim();
        if message.is_empty() {
            return Err("Commit message cannot be empty".into());
        }
        if !self.has_changes(database)? {
            return Ok("No todo changes to commit".into());
        }
        fs::create_dir_all(&self.directory)?;
        if !self.directory.join(".git").exists() {
            self.git_text(&["init", "--quiet"])?;
        }
        self.snapshot_to(database, &self.directory.join("tasks.sqlite3"))?;
        self.git_text(&["add", "--", "tasks.sqlite3"])?;
        let diff = self.git(&["diff", "--cached", "--quiet", "--", "tasks.sqlite3"])?;
        match diff.status.code() {
            Some(0) => return Ok("No todo changes to commit".into()),
            Some(1) => {}
            _ => return Err("Could not compare todo snapshot".into()),
        }
        self.git_text(&["commit", "--only", "-m", message, "--", "tasks.sqlite3"])?;
        Ok("Todo snapshot committed".into())
    }

    fn snapshot_to(&self, database: &Connection, destination: &Path) -> Result<()> {
        let temporary = self
            .directory
            .join(format!(".snapshot-{}.sqlite3", std::process::id()));
        // VACUUM INTO makes a consistent backup without copying the live database.
        if temporary.exists() {
            fs::remove_file(&temporary)?;
        }
        let result = (|| -> Result<()> {
            database.execute(
                "VACUUM INTO ?1",
                [temporary.to_str().ok_or("Invalid snapshot path")?],
            )?;
            fs::rename(&temporary, destination)?;
            Ok(())
        })();
        if temporary.exists() {
            let _ = fs::remove_file(temporary);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use std::{env, time::SystemTime};

    #[test]
    fn commits_snapshot_live_database_and_preserve_history() -> Result<()> {
        let stamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_nanos();
        let root = env::temp_dir().join(format!("cli-todo-history-{stamp}"));
        fs::create_dir_all(&root)?;
        let database = db::open(&root.join("tasks.sqlite3"))?;
        let history = History::new(&database).unwrap();
        assert!(history.list()?.is_empty());
        assert!(!history.has_changes(&database)?);
        assert_eq!(
            history.commit(&database, "empty")?,
            "No todo changes to commit"
        );
        assert!(!history.directory.exists());
        assert!(history.commit(&database, " ").is_err());
        fs::create_dir_all(&history.directory)?;
        history.git_text(&["init", "--quiet"])?;
        history.git_text(&["config", "user.name", "Todo Test"])?;
        history.git_text(&["config", "user.email", "todo@example.test"])?;
        history.git_text(&["config", "commit.gpgsign", "false"])?;
        db::save_project(&database, None, "Work")?;
        db::create_task(&database, "first", Some(database.last_insert_rowid()))?;
        assert!(history.has_changes(&database)?);
        assert_eq!(
            history.commit(&database, "first checkpoint")?,
            "Todo snapshot committed"
        );
        assert!(!history.has_changes(&database)?);
        let first = history.list()?.remove(0);
        assert!(history.details(&first.hash)?.contains("first checkpoint"));
        assert_eq!(
            history.commit(&database, "unchanged")?,
            "No todo changes to commit"
        );
        db::save_task(&database, None, "second")?;
        assert!(history.has_changes(&database)?);
        let lock = history.directory.join(".git/index.lock");
        fs::write(&lock, "locked")?;
        assert!(history.commit(&database, "second checkpoint").is_err());
        assert_eq!(db::list_tasks(&database, db::Filter::All)?.len(), 2);
        fs::remove_file(lock)?;
        history.commit(&database, "second checkpoint")?;
        assert_eq!(history.list()?.len(), 2);
        assert!(!history.has_changes(&database)?);
        let output = history.git(&["show", &format!("{}:tasks.sqlite3", first.hash)])?;
        assert!(output.status.success());
        let old = root.join("old.sqlite3");
        fs::write(&old, output.stdout)?;
        let snapshot = db::open(&old)?;
        assert_eq!(db::list_tasks(&snapshot, db::Filter::All)?.len(), 1);
        assert_eq!(db::list_projects(&snapshot)?[0].name, "Work");
        assert_eq!(db::list_tasks(&database, db::Filter::All)?.len(), 2);
        drop(snapshot);
        drop(database);
        fs::remove_dir_all(root)?;
        Ok(())
    }
}

use crate::{
    types::{Commit, Filter, History, PullPlan, SyncResult},
    Result,
};
use rusqlite::{backup::Backup, Connection, OpenFlags};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
    time::{Duration, SystemTime},
};

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
            .env(
                "GIT_SSH_COMMAND",
                format!(
                    "{} -o BatchMode=yes -o ConnectTimeout=15",
                    std::env::var("GIT_SSH_COMMAND").unwrap_or_else(|_| "ssh".into())
                ),
            )
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
            let snapshot = self.git(&["show", "HEAD:tasks.sqlite3"])?;
            if !snapshot.status.success() {
                return Err("Committed SQLite snapshot is missing".into());
            }
            fs::write(&path, snapshot.stdout)?;
            let committed = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
            // Compare todo contents, not SQLite headers changed by backup/restore.
            Ok(crate::db::list_tasks(database, Filter::All)?
                != crate::db::list_tasks(&committed, Filter::All)?
                || crate::db::list_projects(database)? != crate::db::list_projects(&committed)?)
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
            self.git_text(&["init", "--quiet", "--initial-branch=main"])?;
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

    pub fn nuke(&self, database: &Connection) -> Result<()> {
        if !self.directory.try_exists()? {
            return crate::db::nuke(database);
        }
        let stamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_nanos();
        let staged = self
            .directory
            .with_file_name(format!(".nuke-history-{stamp}"));
        // Keep history recoverable until the database transaction succeeds.
        fs::rename(&self.directory, &staged)?;
        if let Err(error) = crate::db::nuke(database) {
            if let Err(restore) = fs::rename(&staged, &self.directory) {
                return Err(format!("Database deletion failed: {error}; history restore failed: {restore}. History remains at {}", staged.display()).into());
            }
            return Err(error);
        }
        if let Err(error) = fs::remove_dir_all(&staged) {
            let remaining = match fs::rename(&staged, &self.directory) {
                Ok(()) => self.directory.clone(),
                Err(_) => staged,
            };
            return Err(format!("Tasks and projects deleted, but history cleanup failed: {error}. Remaining files: {}", remaining.display()).into());
        }
        Ok(())
    }

    pub fn remote(&self) -> Result<String> {
        if !self.directory.join(".git").exists() {
            return Ok(String::new());
        }
        if !self
            .git_text(&["remote"])?
            .lines()
            .any(|remote| remote == "origin")
        {
            return Ok(String::new());
        }
        Ok(self
            .git_text(&["remote", "get-url", "origin"])?
            .trim()
            .into())
    }

    pub fn set_remote(&self, url: &str) -> Result<()> {
        let url = url.trim();
        if url.starts_with('-') || url.contains('\n') || url.contains('\r') {
            return Err("Enter a valid remote URL or absolute repository path".into());
        }
        fs::create_dir_all(&self.directory)?;
        if !self.directory.join(".git").exists() {
            self.git_text(&["init", "--quiet", "--initial-branch=main"])?;
        }
        let existing = self.remote()?;
        if url.is_empty() {
            if !existing.is_empty() {
                self.git_text(&["remote", "remove", "origin"])?;
            }
        } else if existing.is_empty() {
            self.git_text(&["remote", "add", "origin", url])?;
        } else {
            self.git_text(&["remote", "set-url", "origin", url])?;
        }
        Ok(())
    }

    fn require_remote(&self) -> Result<()> {
        if self.remote()?.is_empty() {
            return Err("Configure origin first with r in Commits".into());
        }
        Ok(())
    }

    fn head(&self) -> Result<Option<String>> {
        let output = self.git(&["rev-parse", "--verify", "--quiet", "HEAD"])?;
        if output.status.success() {
            return Ok(Some(String::from_utf8_lossy(&output.stdout).trim().into()));
        }
        if output.status.code() == Some(1) {
            return Ok(None);
        }
        Err("Could not read todo repository HEAD".into())
    }

    fn tracking_branch(&self) -> Result<Option<String>> {
        let branch = self.git_text(&["symbolic-ref", "--short", "HEAD"])?;
        let output = self.git(&[
            "config",
            "--get",
            &format!("branch.{}.merge", branch.trim()),
        ])?;
        if output.status.success() {
            return Ok(Some(String::from_utf8_lossy(&output.stdout).trim().into()));
        }
        if output.status.code() == Some(1) {
            return Ok(None);
        }
        Err("Could not read remote branch configuration".into())
    }

    fn require_clean_repository(&self) -> Result<()> {
        if !self
            .git_text(&["status", "--porcelain", "--untracked-files=all"])?
            .trim()
            .is_empty()
        {
            return Err(
                "Todo history has uncommitted Git files; resolve them before syncing".into(),
            );
        }
        Ok(())
    }

    pub fn push(&self) -> Result<SyncResult> {
        self.require_remote()?;
        if self.head()?.is_none() {
            return Err("Create a todo checkpoint before pushing".into());
        }
        let branch = self.tracking_branch()?.unwrap_or(format!(
            "refs/heads/{}",
            self.git_text(&["symbolic-ref", "--short", "HEAD"])?.trim()
        ));
        self.git_text(&[
            "push",
            "--set-upstream",
            "origin",
            &format!("HEAD:{branch}"),
        ])?;
        Ok(SyncResult::Message("Todo checkpoints pushed".into()))
    }

    pub fn fetch_pull(&self) -> Result<SyncResult> {
        self.require_remote()?;
        self.require_clean_repository()?;
        let previous_head = self.head()?;
        let branch = match self.tracking_branch()? {
            Some(branch) => branch,
            None => {
                let refs =
                    self.git_text(&["ls-remote", "--symref", "origin", "HEAD", "refs/heads/*"])?;
                let default = refs.lines().find_map(|line| {
                    line.strip_prefix("ref: ")
                        .and_then(|line| line.strip_suffix("\tHEAD"))
                });
                let branches: Vec<_> = refs
                    .lines()
                    .filter_map(|line| line.split_once('\t'))
                    .map(|(_, name)| name)
                    .filter(|name| name.starts_with("refs/heads/"))
                    .collect();
                default
                    .or_else(|| {
                        if branches.len() == 1 {
                            Some(branches[0])
                        } else {
                            None
                        }
                    })
                    .ok_or("Remote has no default branch or checkpoints yet")?
                    .to_string()
            }
        };
        let name = branch
            .strip_prefix("refs/heads/")
            .ok_or("Unsupported remote branch")?;
        let tracking = format!("refs/remotes/origin/{name}");
        self.git_text(&[
            "fetch",
            "--no-tags",
            "origin",
            &format!("+{branch}:{tracking}"),
        ])?;
        let hash = self.git_text(&["rev-parse", &tracking])?.trim().to_string();
        if let Some(local) = &previous_head {
            if local == &hash {
                return Ok(SyncResult::Message(
                    "Todo history is already up to date".into(),
                ));
            }
            if !self.is_ancestor(local, &hash)? {
                if self.is_ancestor(&hash, local)? {
                    return Ok(SyncResult::Message(
                        "Local history is ahead; push your checkpoints".into(),
                    ));
                }
                return Err(
                    "Todo histories have diverged; pull refused without changing local data".into(),
                );
            }
        }
        let snapshot = self.git(&["show", &format!("{hash}:tasks.sqlite3")])?;
        if !snapshot.status.success() {
            return Err("Remote commit has no SQLite snapshot".into());
        }
        Ok(SyncResult::Pull(PullPlan {
            previous_head,
            hash,
            branch,
            snapshot: snapshot.stdout,
        }))
    }

    fn is_ancestor(&self, ancestor: &str, descendant: &str) -> Result<bool> {
        match self
            .git(&["merge-base", "--is-ancestor", ancestor, descendant])?
            .status
            .code()
        {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err("Could not compare todo histories".into()),
        }
    }

    pub fn apply_pull(&self, database: &mut Connection, plan: PullPlan) -> Result<String> {
        if self.has_changes(database)? {
            return Err("Todo data changed during pull; commit it first".into());
        }
        self.require_clean_repository()?;
        if self.head()? != plan.previous_head {
            return Err("Local history changed during pull; retry".into());
        }
        let root = self.directory.parent().ok_or("Invalid history directory")?;
        let incoming = root.join(format!(".incoming-{}.sqlite3", std::process::id()));
        fs::write(&incoming, &plan.snapshot)?;
        let result = self.restore_pull(database, &plan, &incoming);
        let _ = fs::remove_file(incoming);
        result
    }

    fn restore_pull(
        &self,
        database: &mut Connection,
        plan: &PullPlan,
        incoming: &Path,
    ) -> Result<String> {
        let source = Connection::open_with_flags(incoming, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let integrity: String = source.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
        if integrity != "ok" {
            return Err("Remote SQLite snapshot failed integrity checks".into());
        }
        crate::db::list_tasks(&source, Filter::All)?;
        crate::db::list_projects(&source)?;
        let valid_schema: bool = source.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_foreign_key_list('tasks') WHERE \"table\"='projects' AND \"from\"='project_id' AND on_delete='CASCADE')",
            [], |row| row.get(0),
        )?;
        if !valid_schema {
            return Err("Remote snapshot is not a compatible cli-todo database".into());
        }
        if source
            .prepare("PRAGMA foreign_key_check")?
            .query([])?
            .next()?
            .is_some()
        {
            return Err("Remote snapshot contains invalid project references".into());
        }
        let stamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_nanos();
        let backups = self
            .directory
            .parent()
            .ok_or("Invalid history directory")?
            .join("backups");
        fs::create_dir_all(&backups)?;
        let backup = backups.join(format!("before-pull-{stamp}.sqlite3"));
        self.snapshot_to(database, &backup)?;
        let local = self.git_text(&["symbolic-ref", "--short", "HEAD"])?;
        self.git_text(&[
            "config",
            &format!("branch.{}.remote", local.trim()),
            "origin",
        ])?;
        self.git_text(&[
            "config",
            &format!("branch.{}.merge", local.trim()),
            &plan.branch,
        ])?;
        let apply = copy_database(&source, database)
            .and_then(|_| crate::db::migrate_description(database))
            .and_then(|_| {
                self.git_text(&["merge", "--ff-only", "--no-edit", &plan.hash])
                    .map(|_| ())
            });
        if let Err(error) = apply {
            let original = Connection::open_with_flags(&backup, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
            if let Err(rollback) = copy_database(&original, database) {
                return Err(format!(
                    "Pull failed: {error}; database rollback failed: {rollback}. Backup: {}",
                    backup.display()
                )
                .into());
            }
            return Err(format!(
                "Pull failed; original database restored: {error}. Backup: {}",
                backup.display()
            )
            .into());
        }
        Ok(format!(
            "Todo snapshot pulled and applied. Backup: {}",
            backup.display()
        ))
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

fn copy_database(source: &Connection, destination: &mut Connection) -> Result<()> {
    let backup = Backup::new(source, destination)?;
    backup.run_to_completion(128, Duration::from_millis(10), None)?;
    Ok(())
}

#[cfg(test)]
#[path = "tests/history.rs"]
mod tests;

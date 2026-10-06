use crate::Result;
use rusqlite::{params, Connection};
use std::path::Path;

#[derive(Debug)]
pub struct Task {
    pub id: i64,
    pub title: String,
    pub done: bool,
}

#[derive(Clone, Copy)]
pub enum Filter {
    All,
    Pending,
    Completed,
}

pub fn open(path: &Path) -> Result<Connection> {
    let db = Connection::open(path)?;
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS tasks (
            id INTEGER PRIMARY KEY,
            title TEXT NOT NULL CHECK(length(trim(title)) > 0),
            done INTEGER NOT NULL DEFAULT 0 CHECK(done IN (0,1))
        );",
    )?;
    Ok(db)
}

pub fn save_task(db: &Connection, id: Option<i64>, title: &str) -> Result<()> {
    let title = title.trim();
    if title.is_empty() {
        return Err("Title cannot be empty".into());
    }
    if let Some(id) = id {
        db.execute("UPDATE tasks SET title=?1 WHERE id=?2", params![title, id])?;
    } else {
        db.execute("INSERT INTO tasks(title) VALUES (?1)", [title])?;
    }
    Ok(())
}

pub fn list_tasks(db: &Connection, filter: Filter) -> Result<Vec<Task>> {
    let done = match filter {
        Filter::All => None,
        Filter::Pending => Some(false),
        Filter::Completed => Some(true),
    };
    let mut statement =
        db.prepare("SELECT id,title,done FROM tasks WHERE ?1 IS NULL OR done=?1 ORDER BY id")?;
    let rows = statement.query_map([done], |row| {
        Ok(Task {
            id: row.get(0)?,
            title: row.get(1)?,
            done: row.get(2)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn delete_task(db: &Connection, id: i64) -> Result<()> {
    db.execute("DELETE FROM tasks WHERE id=?1", [id])?;
    Ok(())
}

pub fn toggle_task(db: &Connection, id: i64) -> Result<()> {
    db.execute("UPDATE tasks SET done=1-done WHERE id=?1", [id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, fs, time::SystemTime};

    #[test]
    fn task_changes_persist_across_restarts() -> Result<()> {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_nanos();
        let path = env::temp_dir().join(format!(
            "cli-todo-{}-{timestamp}.sqlite3",
            std::process::id()
        ));
        let db = open(&path)?;
        save_task(&db, None, "  first  ")?;
        let task = list_tasks(&db, Filter::All)?.remove(0);
        assert_eq!(task.title, "first");
        assert!(!task.done);
        save_task(&db, Some(task.id), "edited")?;
        toggle_task(&db, task.id)?;
        drop(db);

        let db = open(&path)?;
        let tasks = list_tasks(&db, Filter::All)?;
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "edited");
        assert!(tasks[0].done);
        delete_task(&db, task.id)?;
        drop(db);

        let db = open(&path)?;
        assert!(list_tasks(&db, Filter::All)?.is_empty());
        drop(db);
        fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn filters_match_task_completion() -> Result<()> {
        let db = open(Path::new(":memory:"))?;
        save_task(&db, None, "pending")?;
        save_task(&db, None, "completed")?;
        let completed_id = db.last_insert_rowid();
        toggle_task(&db, completed_id)?;
        assert_eq!(list_tasks(&db, Filter::All)?.len(), 2);
        assert_eq!(list_tasks(&db, Filter::Pending)?[0].title, "pending");
        let completed = list_tasks(&db, Filter::Completed)?;
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].id, completed_id);
        toggle_task(&db, completed_id)?;
        assert!(list_tasks(&db, Filter::Completed)?.is_empty());
        assert_eq!(list_tasks(&db, Filter::Pending)?.len(), 2);
        Ok(())
    }

    #[test]
    fn blank_titles_cannot_be_saved() -> Result<()> {
        let db = open(Path::new(":memory:"))?;
        assert!(save_task(&db, None, " \n\t ").is_err());
        save_task(&db, None, "keep this")?;
        assert!(save_task(&db, Some(db.last_insert_rowid()), "").is_err());
        assert_eq!(list_tasks(&db, Filter::All)?[0].title, "keep this");
        Ok(())
    }
}

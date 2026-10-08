use crate::Result;
use rusqlite::{params, Connection};
use std::path::Path;

#[derive(Debug, PartialEq, Eq)]
pub struct Task {
    pub id: i64,
    pub title: String,
    pub description: String,
    pub done: bool,
    pub project_id: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Project {
    pub id: i64,
    pub name: String,
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
        "PRAGMA foreign_keys = ON;
        CREATE TABLE IF NOT EXISTS projects (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE CHECK(length(trim(name)) > 0)
        );
        CREATE TABLE IF NOT EXISTS tasks (
            id INTEGER PRIMARY KEY,
            title TEXT NOT NULL CHECK(length(trim(title)) > 0),
            done INTEGER NOT NULL DEFAULT 0 CHECK(done IN (0,1))
        );",
    )?;
    let mut columns = db.prepare("PRAGMA table_info(tasks)")?;
    let names = columns
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if !names.iter().any(|name| name == "project_id") {
        db.execute_batch("ALTER TABLE tasks ADD COLUMN project_id INTEGER REFERENCES projects(id) ON DELETE CASCADE;")?;
    }
    drop(columns);
    migrate_description(&db)?;
    Ok(db)
}

fn has_description(db: &Connection) -> Result<bool> {
    Ok(db.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('tasks') WHERE name='description')",
        [],
        |row| row.get(0),
    )?)
}

pub fn migrate_description(db: &Connection) -> Result<()> {
    if !has_description(db)? {
        db.execute_batch("ALTER TABLE tasks ADD COLUMN description TEXT NOT NULL DEFAULT '';")?;
    }
    Ok(())
}

pub fn save_task_details(
    db: &Connection,
    id: Option<i64>,
    title: &str,
    description: &str,
    project: Option<i64>,
) -> Result<()> {
    let title = title.trim();
    if title.is_empty() {
        return Err("Title cannot be empty".into());
    }
    if let Some(id) = id {
        db.execute(
            "UPDATE tasks SET title=?1,description=?2 WHERE id=?3",
            params![title, description, id],
        )?;
    } else {
        db.execute(
            "INSERT INTO tasks(title,description,project_id) VALUES (?1,?2,?3)",
            params![title, description, project],
        )?;
    }
    Ok(())
}

#[cfg(test)]
pub fn save_task(db: &Connection, id: Option<i64>, title: &str) -> Result<()> {
    let title = title.trim();
    if title.is_empty() {
        return Err("Title cannot be empty".into());
    }
    if let Some(id) = id {
        db.execute("UPDATE tasks SET title=?1 WHERE id=?2", params![title, id])?;
    } else {
        create_task(db, title, None)?;
    }
    Ok(())
}

#[cfg(test)]
pub fn create_task(db: &Connection, title: &str, project: Option<i64>) -> Result<()> {
    let title = title.trim();
    if title.is_empty() {
        return Err("Title cannot be empty".into());
    }
    db.execute(
        "INSERT INTO tasks(title,project_id) VALUES (?1,?2)",
        params![title, project],
    )?;
    Ok(())
}

pub fn list_tasks(db: &Connection, filter: Filter) -> Result<Vec<Task>> {
    let done = match filter {
        Filter::All => None,
        Filter::Pending => Some(false),
        Filter::Completed => Some(true),
    };
    // Old Git snapshots predate descriptions and remain readable without migration.
    let description = if has_description(db)? {
        "description"
    } else {
        "''"
    };
    let mut statement = db.prepare(&format!(
        "SELECT id,title,done,project_id,{description} FROM tasks WHERE ?1 IS NULL OR done=?1 ORDER BY id"
    ))?;
    let rows = statement.query_map([done], |row| {
        Ok(Task {
            id: row.get(0)?,
            title: row.get(1)?,
            description: row.get(4)?,
            done: row.get(2)?,
            project_id: row.get(3)?,
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

pub fn list_tasks_in(db: &Connection, filter: Filter, project: Option<i64>) -> Result<Vec<Task>> {
    Ok(list_tasks(db, filter)?
        .into_iter()
        .filter(|task| task.project_id == project)
        .collect())
}

pub fn move_task(db: &Connection, id: i64, project: Option<i64>) -> Result<()> {
    db.execute(
        "UPDATE tasks SET project_id=?1 WHERE id=?2",
        params![project, id],
    )?;
    Ok(())
}

pub fn list_projects(db: &Connection) -> Result<Vec<Project>> {
    let mut statement = db.prepare("SELECT id,name FROM projects ORDER BY id")?;
    let rows = statement.query_map([], |row| {
        Ok(Project {
            id: row.get(0)?,
            name: row.get(1)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn save_project(db: &Connection, id: Option<i64>, name: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Project name cannot be empty".into());
    }
    if let Some(id) = id {
        db.execute("UPDATE projects SET name=?1 WHERE id=?2", params![name, id])?;
    } else {
        db.execute("INSERT INTO projects(name) VALUES (?1)", [name])?;
    }
    Ok(())
}

pub fn nuke(db: &Connection) -> Result<()> {
    let transaction = db.unchecked_transaction()?;
    transaction.execute("DELETE FROM tasks", [])?;
    transaction.execute("DELETE FROM projects", [])?;
    transaction.commit()?;
    Ok(())
}

pub fn delete_project(db: &Connection, id: i64) -> Result<()> {
    db.execute("DELETE FROM projects WHERE id=?1", [id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, fs, time::SystemTime};

    #[test]
    fn descriptions_persist_and_legacy_tasks_get_empty_descriptions() -> Result<()> {
        let stamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_nanos();
        let path = env::temp_dir().join(format!("cli-todo-description-{stamp}.sqlite3"));
        let legacy = Connection::open(&path)?;
        legacy.execute_batch("CREATE TABLE tasks(id INTEGER PRIMARY KEY,title TEXT NOT NULL,done INTEGER NOT NULL DEFAULT 0); INSERT INTO tasks(title) VALUES ('legacy');")?;
        drop(legacy);
        let db = open(&path)?;
        assert_eq!(list_tasks(&db, Filter::All)?[0].description, "");
        save_project(&db, None, "Work")?;
        let project = db.last_insert_rowid();
        save_task_details(
            &db,
            None,
            "  titled  ",
            "first line\n界 emoji 😀\n",
            Some(project),
        )?;
        let id = db.last_insert_rowid();
        toggle_task(&db, id)?;
        drop(db);
        let db = open(&path)?;
        let tasks = list_tasks(&db, Filter::All)?;
        assert_eq!(tasks[1].title, "titled");
        assert_eq!(tasks[1].description, "first line\n界 emoji 😀\n");
        assert_eq!(tasks[1].project_id, Some(project));
        assert!(tasks[1].done);
        assert!(save_task_details(&db, Some(id), " ", "discarded", None).is_err());
        save_task_details(&db, Some(id), "renamed", "updated", None)?;
        let task = list_tasks(&db, Filter::All)?.remove(1);
        assert_eq!(task.description, "updated");
        assert_eq!(task.project_id, Some(project));
        assert!(task.done);
        drop(db);
        fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn nuke_clears_all_data_and_rolls_back_on_failure() -> Result<()> {
        let db = open(Path::new(":memory:"))?;
        save_project(&db, None, "Work")?;
        let project = db.last_insert_rowid();
        create_task(&db, "project task", Some(project))?;
        let task = db.last_insert_rowid();
        toggle_task(&db, task)?;
        create_task(&db, "inbox task", None)?;
        db.execute_batch("CREATE TRIGGER prevent_nuke BEFORE DELETE ON projects BEGIN SELECT RAISE(ABORT, 'blocked'); END;")?;
        assert!(nuke(&db).is_err());
        assert_eq!(list_tasks(&db, Filter::All)?.len(), 2);
        assert_eq!(list_projects(&db)?.len(), 1);
        db.execute_batch("DROP TRIGGER prevent_nuke;")?;
        nuke(&db)?;
        assert!(list_tasks(&db, Filter::All)?.is_empty());
        assert!(list_projects(&db)?.is_empty());
        nuke(&db)?;
        save_project(&db, None, "New project")?;
        create_task(&db, "new task", Some(db.last_insert_rowid()))?;
        assert_eq!(list_tasks(&db, Filter::All)?.len(), 1);
        Ok(())
    }

    #[test]
    fn old_databases_migrate_without_losing_tasks() -> Result<()> {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_nanos();
        let path = env::temp_dir().join(format!("cli-todo-migration-{timestamp}.sqlite3"));
        let db = Connection::open(&path)?;
        db.execute_batch("CREATE TABLE tasks (id INTEGER PRIMARY KEY, title TEXT NOT NULL, done INTEGER NOT NULL DEFAULT 0);
            INSERT INTO tasks(title,done) VALUES ('existing',1);")?;
        drop(db);
        let db = open(&path)?;
        let tasks = list_tasks_in(&db, Filter::All, None)?;
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "existing");
        assert!(tasks[0].done);
        save_project(&db, None, "Work")?;
        let project = db.last_insert_rowid();
        create_task(&db, "project task", Some(project))?;
        drop(db);
        let db = open(&path)?;
        assert_eq!(list_projects(&db)?[0].name, "Work");
        assert_eq!(
            list_tasks_in(&db, Filter::All, Some(project))?[0].title,
            "project task"
        );
        delete_project(&db, project)?;
        assert_eq!(list_tasks(&db, Filter::All)?.len(), 1);
        drop(db);
        fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn projects_scope_move_and_cascade_tasks() -> Result<()> {
        let db = open(Path::new(":memory:"))?;
        save_project(&db, None, "Work")?;
        let project = db.last_insert_rowid();
        save_task(&db, None, "Inbox task")?;
        let inbox_task = db.last_insert_rowid();
        save_task(&db, None, "Project task")?;
        let task = db.last_insert_rowid();
        move_task(&db, task, Some(project))?;
        assert_eq!(list_tasks_in(&db, Filter::All, None)?.len(), 1);
        assert_eq!(list_tasks_in(&db, Filter::All, Some(project))?[0].id, task);
        move_task(&db, task, None)?;
        assert_eq!(list_tasks_in(&db, Filter::All, None)?.len(), 2);
        move_task(&db, task, Some(project))?;
        save_project(&db, Some(project), "Renamed")?;
        assert_eq!(list_projects(&db)?[0].name, "Renamed");
        assert!(save_project(&db, None, "Renamed").is_err());
        assert!(save_project(&db, None, " ").is_err());
        delete_project(&db, project)?;
        assert!(list_projects(&db)?.is_empty());
        assert_eq!(list_tasks(&db, Filter::All)?.len(), 1);
        assert_eq!(list_tasks(&db, Filter::All)?[0].id, inbox_task);
        Ok(())
    }

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

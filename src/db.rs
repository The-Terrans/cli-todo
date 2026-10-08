use crate::{
    types::{Filter, Project, Task},
    Result,
};
use rusqlite::{params, Connection};
use std::path::Path;

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
#[path = "tests/db.rs"]
mod tests;

use crate::{
    constants::MAX_SUBTASK_DEPTH,
    dates::parse_due_date,
    types::{Filter, Project, Sort, SubtaskDraft, Task, TaskDraft},
    Result,
};
use chrono::{Local, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension};
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    path::Path,
};

/// Sets `done` to parameter ?2, stamping newly completed tasks and clearing reopened ones.
const SET_DONE: &str = "done=?2,completed_at=CASE WHEN NOT ?2 THEN NULL WHEN done THEN completed_at ELSE strftime('%Y-%m-%d %H:%M:%f','now') END";

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
    migrate_subtasks(&db)?;
    migrate_due_dates(&db)?;
    migrate_positions(&db)?;
    migrate_completed_at(&db)?;
    migrate_sorts(&db)?;
    validate_hierarchy(&db)?;
    Ok(db)
}

fn has_column(db: &Connection, column: &str) -> Result<bool> {
    Ok(db.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('tasks') WHERE name=?1)",
        [column],
        |row| row.get(0),
    )?)
}

pub fn migrate_description(db: &Connection) -> Result<()> {
    if !has_column(db, "description")? {
        db.execute_batch("ALTER TABLE tasks ADD COLUMN description TEXT NOT NULL DEFAULT '';")?;
    }
    Ok(())
}

pub fn migrate_subtasks(db: &Connection) -> Result<()> {
    if !has_column(db, "parent_id")? {
        db.execute_batch("ALTER TABLE tasks ADD COLUMN parent_id INTEGER REFERENCES tasks(id) ON DELETE CASCADE;")?;
    }
    db.execute_batch("CREATE INDEX IF NOT EXISTS tasks_parent_id ON tasks(parent_id);")?;
    Ok(())
}

pub fn migrate_due_dates(db: &Connection) -> Result<()> {
    if !has_column(db, "due_date")? {
        db.execute_batch("ALTER TABLE tasks ADD COLUMN due_date TEXT;")?;
    }
    Ok(())
}

pub fn migrate_positions(db: &Connection) -> Result<()> {
    if !has_column(db, "position")? {
        db.execute_batch("ALTER TABLE tasks ADD COLUMN position INTEGER;")?;
    }
    // Existing tasks keep their creation order.
    db.execute_batch("UPDATE tasks SET position=id WHERE position IS NULL;")?;
    Ok(())
}

/// Tasks completed before this column existed keep no completion time.
pub fn migrate_completed_at(db: &Connection) -> Result<()> {
    if !has_column(db, "completed_at")? {
        db.execute_batch("ALTER TABLE tasks ADD COLUMN completed_at TEXT;")?;
    }
    Ok(())
}

/// Saved list sorts: one row per project (NULL filter) and one per Inbox filter (NULL
/// project). Manual order has no row.
pub fn migrate_sorts(db: &Connection) -> Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS list_sorts (
            project_id INTEGER REFERENCES projects(id) ON DELETE CASCADE,
            filter TEXT,
            sort TEXT NOT NULL,
            UNIQUE(project_id, filter)
        );",
    )?;
    Ok(())
}

/// A project shares one sort across its filters; each Inbox filter has its own.
fn sort_list(project: Option<i64>, filter: Filter) -> (Option<i64>, Option<&'static str>) {
    if project.is_some() {
        return (project, None);
    }
    let name = match filter {
        Filter::Today => "today",
        Filter::All => "all",
        Filter::Pending => "pending",
        Filter::Completed => "completed",
    };
    (None, Some(name))
}

const SORT_NAMES: [(Sort, &str); 2] = [
    (Sort::CreatedAsc, "created_asc"),
    (Sort::CreatedDesc, "created_desc"),
];

/// Returns the saved sort for `project`, or Inbox's `filter` when `None`; unknown names fall
/// back to Manual.
pub fn load_sort(db: &Connection, project: Option<i64>, filter: Filter) -> Result<Sort> {
    let (project, filter) = sort_list(project, filter);
    let name: Option<String> = db
        .query_row(
            "SELECT sort FROM list_sorts WHERE project_id IS ?1 AND filter IS ?2",
            params![project, filter],
            |row| row.get(0),
        )
        .optional()?;
    Ok(SORT_NAMES
        .iter()
        .find(|(_, saved)| Some(*saved) == name.as_deref())
        .map_or(Sort::Manual, |(sort, _)| *sort))
}

pub fn save_sort(db: &Connection, project: Option<i64>, filter: Filter, sort: Sort) -> Result<()> {
    let (project, filter) = sort_list(project, filter);
    let transaction = db.unchecked_transaction()?;
    transaction.execute(
        "DELETE FROM list_sorts WHERE project_id IS ?1 AND filter IS ?2",
        params![project, filter],
    )?;
    if let Some((_, name)) = SORT_NAMES.iter().find(|(saved, _)| *saved == sort) {
        transaction.execute(
            "INSERT INTO list_sorts(project_id,filter,sort) VALUES (?1,?2,?3)",
            params![project, filter, name],
        )?;
    }
    transaction.commit()?;
    Ok(())
}

pub fn swap_tasks(db: &Connection, first: i64, second: i64) -> Result<()> {
    let transaction = db.unchecked_transaction()?;
    let position = |id: i64| -> Result<i64> {
        Ok(
            transaction.query_row("SELECT position FROM tasks WHERE id=?1", [id], |row| {
                row.get(0)
            })?,
        )
    };
    let (first_position, second_position) = (position(first)?, position(second)?);
    for (id, position) in [(first, second_position), (second, first_position)] {
        transaction.execute(
            "UPDATE tasks SET position=?1 WHERE id=?2",
            params![position, id],
        )?;
    }
    transaction.commit()?;
    Ok(())
}

fn save_due_date(db: &Connection, id: i64, input: &str) -> Result<()> {
    let date = parse_due_date(input)?.map(|date| date.to_string());
    db.execute(
        "UPDATE tasks SET due_date=?1 WHERE id=?2",
        params![date, id],
    )?;
    Ok(())
}

pub fn validate_hierarchy(db: &Connection) -> Result<()> {
    if has_column(db, "parent_id")? {
        let valid: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_foreign_key_list('tasks') WHERE \"table\"='tasks' AND \"from\"='parent_id' AND on_delete='CASCADE')", [], |row| row.get(0),
        )?;
        if !valid {
            return Err("Subtask schema is missing cascading parent references".into());
        }
    }
    let tasks = list_tasks(db, Filter::All)?;
    let tasks: HashMap<_, _> = tasks.iter().map(|task| (task.id, task)).collect();
    for task in tasks.values() {
        let mut visited = HashSet::from([task.id]);
        let mut parent = task.parent_id;
        while let Some(id) = parent {
            if !visited.insert(id) {
                return Err("Subtask hierarchy contains a cycle".into());
            }
            if visited.len() > MAX_SUBTASK_DEPTH + 1 {
                return Err("Subtask nesting limit exceeded".into());
            }
            let ancestor = tasks.get(&id).ok_or("Subtask parent is missing")?;
            if ancestor.project_id != task.project_id {
                return Err("Subtasks must share their parent's project".into());
            }
            parent = ancestor.parent_id;
        }
    }
    Ok(())
}

pub fn task_draft(db: &Connection, id: i64) -> Result<TaskDraft> {
    validate_hierarchy(db)?;
    let mut tasks = list_tasks(db, Filter::All)?;
    let index = tasks
        .iter()
        .position(|task| task.id == id)
        .ok_or("Task no longer exists")?;
    let root = tasks.remove(index);
    let mut depth = 0;
    let mut ancestor = root.parent_id;
    while let Some(id) = ancestor {
        depth += 1;
        ancestor = tasks
            .iter()
            .find(|task| task.id == id)
            .ok_or("Subtask parent is missing")?
            .parent_id;
    }
    let mut children: HashMap<i64, Vec<Task>> = HashMap::new();
    for task in tasks {
        if let Some(parent) = task.parent_id {
            children.entry(parent).or_default().push(task);
        }
    }
    Ok(draft_from_task(root, &mut children, depth))
}

fn draft_from_task(task: Task, children: &mut HashMap<i64, Vec<Task>>, depth: usize) -> TaskDraft {
    let subtasks = children
        .remove(&task.id)
        .unwrap_or_default()
        .into_iter()
        .map(|child| SubtaskDraft {
            id: Some(child.id),
            done: child.done,
            draft: draft_from_task(child, children, depth + 1),
        })
        .collect();
    TaskDraft {
        title: task.title,
        description: task.description,
        due_date: task
            .due_date
            .map(|date| date.to_string())
            .unwrap_or_default(),
        subtasks,
        depth,
        ..TaskDraft::default()
    }
}

pub fn reword_task(db: &Connection, id: i64, draft: &TaskDraft) -> Result<()> {
    let transaction = db.unchecked_transaction()?;
    save_task_details(
        &transaction,
        Some(id),
        &draft.title,
        &draft.description,
        None,
    )?;
    save_due_date(&transaction, id, &draft.due_date)?;
    transaction.commit()?;
    Ok(())
}

pub fn save_task_tree(
    db: &Connection,
    id: Option<i64>,
    draft: &TaskDraft,
    project: Option<i64>,
) -> Result<()> {
    save_tree(db, id, draft, project, None)
}

pub fn save_subtask_tree(db: &Connection, parent: i64, draft: &TaskDraft) -> Result<()> {
    let project = db.query_row(
        "SELECT project_id FROM tasks WHERE id=?1",
        [parent],
        |row| row.get(0),
    )?;
    save_tree(db, None, draft, project, Some(parent))
}

fn save_tree(
    db: &Connection,
    id: Option<i64>,
    draft: &TaskDraft,
    project: Option<i64>,
    parent: Option<i64>,
) -> Result<()> {
    let transaction = db.unchecked_transaction()?;
    save_task_details(&transaction, id, &draft.title, &draft.description, project)?;
    let id = id.unwrap_or_else(|| transaction.last_insert_rowid());
    if let Some(parent) = parent {
        transaction.execute(
            "UPDATE tasks SET parent_id=?1 WHERE id=?2",
            params![parent, id],
        )?;
    }
    save_due_date(&transaction, id, &draft.due_date)?;
    let project =
        transaction.query_row("SELECT project_id FROM tasks WHERE id=?1", [id], |row| {
            row.get(0)
        })?;
    save_children(&transaction, id, &draft.subtasks, project)?;
    validate_hierarchy(&transaction)?;
    transaction.commit()?;
    Ok(())
}

fn save_children(
    db: &Connection,
    parent: i64,
    children: &[SubtaskDraft],
    project: Option<i64>,
) -> Result<()> {
    let existing = db
        .prepare("SELECT id FROM tasks WHERE parent_id=?1")?
        .query_map([parent], |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for id in existing {
        if !children.iter().any(|child| child.id == Some(id)) {
            delete_task(db, id)?;
        }
    }
    let mut seen = HashSet::new();
    for child in children {
        if let Some(id) = child.id {
            let owner: Option<i64> =
                db.query_row("SELECT parent_id FROM tasks WHERE id=?1", [id], |row| {
                    row.get(0)
                })?;
            if owner != Some(parent) || !seen.insert(id) {
                return Err("Invalid subtask ownership".into());
            }
        }
        save_task_details(
            db,
            child.id,
            &child.draft.title,
            &child.draft.description,
            project,
        )?;
        let id = child.id.unwrap_or_else(|| db.last_insert_rowid());
        save_due_date(db, id, &child.draft.due_date)?;
        db.execute(
            &format!("UPDATE tasks SET parent_id=?1,{SET_DONE} WHERE id=?3"),
            params![parent, child.done, id],
        )?;
        save_children(db, id, &child.draft.subtasks, project)?;
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
            "INSERT INTO tasks(title,description,project_id,position)
             VALUES (?1,?2,?3,(SELECT COALESCE(MAX(position),0)+1 FROM tasks))",
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
        "INSERT INTO tasks(title,project_id,position)
         VALUES (?1,?2,(SELECT COALESCE(MAX(position),0)+1 FROM tasks))",
        params![title, project],
    )?;
    Ok(())
}

pub fn list_tasks(db: &Connection, filter: Filter) -> Result<Vec<Task>> {
    let done = match filter {
        Filter::All => None,
        Filter::Today | Filter::Pending => Some(false),
        Filter::Completed => Some(true),
    };
    // Old Git snapshots predate descriptions and remain readable without migration.
    let description = if has_column(db, "description")? {
        "description"
    } else {
        "''"
    };
    let (parent, subtask_count) = if has_column(db, "parent_id")? {
        (
            "parent_id",
            "(SELECT COUNT(*) FROM tasks AS child WHERE child.parent_id=tasks.id)",
        )
    } else {
        ("NULL", "0")
    };
    let due_date = if has_column(db, "due_date")? {
        "due_date"
    } else {
        "NULL"
    };
    let position = if has_column(db, "position")? {
        "position"
    } else {
        "id"
    };
    let completed_at = if has_column(db, "completed_at")? {
        "completed_at"
    } else {
        "NULL"
    };
    let mut statement = db.prepare(&format!(
        "SELECT id,title,done,project_id,{description},{parent},{subtask_count},{due_date},{completed_at} FROM tasks WHERE ?1 IS NULL OR done=?1 ORDER BY {position},id"
    ))?;
    let rows = statement.query_map([done], |row| {
        let date: Option<String> = row.get(7)?;
        let due_date = parse_due_date(date.as_deref().unwrap_or("")).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                7,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    error.to_string(),
                )),
            )
        })?;
        Ok(Task {
            id: row.get(0)?,
            title: row.get(1)?,
            description: row.get(4)?,
            done: row.get(2)?,
            project_id: row.get(3)?,
            parent_id: row.get(5)?,
            subtask_count: row.get(6)?,
            due_date,
            completed_at: row.get(8)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn delete_task(db: &Connection, id: i64) -> Result<()> {
    db.execute("DELETE FROM tasks WHERE id=?1", [id])?;
    Ok(())
}

pub fn toggle_task(db: &Connection, id: i64) -> Result<()> {
    let transaction = db.unchecked_transaction()?;
    let done: bool =
        transaction.query_row("SELECT done FROM tasks WHERE id=?1", [id], |row| row.get(0))?;
    set_task_done(&transaction, id, !done)?;
    transaction.commit()?;
    Ok(())
}

fn set_task_done(db: &Connection, id: i64, done: bool) -> Result<()> {
    db.execute(
        &format!(
            "WITH RECURSIVE descendants(id) AS (
            SELECT id FROM tasks WHERE id=?1
            UNION SELECT tasks.id FROM tasks JOIN descendants ON tasks.parent_id=descendants.id
        ) UPDATE tasks SET {SET_DONE} WHERE id=?1 OR (?2 AND id IN descendants)"
        ),
        params![id, done],
    )?;
    Ok(())
}

/// Lists top-level tasks in `project`, or all top-level tasks (any project or none) when `None`.
/// Pending tasks come first in `sort` order, then completed tasks, most recently completed first.
pub fn list_tasks_in(
    db: &Connection,
    filter: Filter,
    project: Option<i64>,
    sort: Sort,
) -> Result<Vec<Task>> {
    let due = if filter == Filter::Today {
        Some(roots_due_by(
            &list_tasks(db, Filter::All)?,
            Local::now().date_naive(),
        ))
    } else {
        None
    };
    let mut tasks: Vec<_> = list_tasks(db, filter)?
        .into_iter()
        .filter(|task| {
            task.parent_id.is_none()
                && project.is_none_or(|id| task.project_id == Some(id))
                && due.as_ref().is_none_or(|due| due.contains(&task.id))
        })
        .collect();
    tasks.sort_by(|a, b| compare_tasks(a, b, sort));
    Ok(tasks)
}

/// Children of `parent` in manual order, completed ones last, most recently completed first.
pub fn subtasks_of(tasks: &[Task], parent: i64) -> Vec<Task> {
    let mut children: Vec<_> = tasks
        .iter()
        .filter(|task| task.parent_id == Some(parent))
        .cloned()
        .collect();
    children.sort_by(|a, b| compare_tasks(a, b, Sort::Manual));
    children
}

/// Pending tasks in `sort` order, then completed tasks, most recently completed first.
/// Meant for stable sorts: ties keep the manual order. Ids grow with creation, so they order by creation.
fn compare_tasks(a: &Task, b: &Task, sort: Sort) -> Ordering {
    a.done.cmp(&b.done).then_with(|| match (a.done, sort) {
        // `None` sorts before `Some`, so reversing puts unrecorded completions last.
        (true, _) => b.completed_at.cmp(&a.completed_at),
        (false, Sort::Manual) => Ordering::Equal,
        (false, Sort::CreatedAsc) => a.id.cmp(&b.id),
        (false, Sort::CreatedDesc) => b.id.cmp(&a.id),
    })
}

/// Top-level ids whose task or any descendant is unfinished and due on or before `today`.
pub fn roots_due_by(tasks: &[Task], today: NaiveDate) -> HashSet<i64> {
    let parents: HashMap<_, _> = tasks.iter().map(|task| (task.id, task.parent_id)).collect();
    tasks
        .iter()
        .filter(|task| !task.done && task.due_date.is_some_and(|date| date <= today))
        .map(|task| {
            let mut id = task.id;
            for _ in 0..=MAX_SUBTASK_DEPTH {
                match parents.get(&id) {
                    Some(Some(parent)) => id = *parent,
                    _ => break,
                }
            }
            id
        })
        .collect()
}

pub fn move_task(db: &Connection, id: i64, project: Option<i64>) -> Result<()> {
    db.execute(
        "WITH RECURSIVE descendants(id) AS (
            SELECT id FROM tasks WHERE id=?2
            UNION SELECT tasks.id FROM tasks JOIN descendants ON tasks.parent_id=descendants.id
        ) UPDATE tasks SET project_id=?1,parent_id=CASE WHEN id=?2 THEN NULL ELSE parent_id END
          WHERE id IN descendants",
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
    transaction.execute("DELETE FROM list_sorts", [])?;
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


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

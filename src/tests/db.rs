use super::*;
use std::{env, fs, time::SystemTime};

#[test]
fn reword_updates_only_task_fields_and_rolls_back_invalid_dates() -> Result<()> {
    let db = open(Path::new(":memory:"))?;
    save_project(&db, None, "Work")?;
    let project = db.last_insert_rowid();
    save_task_tree(&db, None, &nested_draft(), Some(project))?;
    let before = list_tasks(&db, Filter::All)?;
    let id = before[0].id;
    let mut draft = task_draft(&db, id)?;
    draft.title = "reworded".into();
    draft.description = "new notes".into();
    draft.due_date = "2028-02-29".into();
    draft.subtasks.clear();
    reword_task(&db, id, &draft)?;
    let after = list_tasks(&db, Filter::All)?;
    assert_eq!(after[0].title, "reworded");
    assert_eq!(after[0].description, "new notes");
    assert_eq!(after[0].due_date, parse_due_date("2028-02-29")?);
    assert_eq!(after[0].project_id, Some(project));
    assert_eq!(after[0].done, before[0].done);
    assert_eq!(&after[1..], &before[1..]);
    draft.title = "must roll back".into();
    draft.due_date = "2026-02-30".into();
    assert!(reword_task(&db, id, &draft).is_err());
    assert_eq!(list_tasks(&db, Filter::All)?, after);
    Ok(())
}

#[test]
fn adding_children_inherits_project_and_completion_can_toggle() -> Result<()> {
    let db = open(Path::new(":memory:"))?;
    save_project(&db, None, "Work")?;
    let project = db.last_insert_rowid();
    save_task_tree(&db, None, &nested_draft(), Some(project))?;
    let parent = list_tasks(&db, Filter::All)?[0].id;
    toggle_task(&db, parent)?;
    save_subtask_tree(
        &db,
        parent,
        &TaskDraft {
            title: "new child".into(),
            ..TaskDraft::default()
        },
    )?;
    let tasks = list_tasks(&db, Filter::All)?;
    assert_eq!(tasks.last().unwrap().parent_id, Some(parent));
    assert_eq!(tasks.last().unwrap().project_id, Some(project));
    assert!(!tasks.last().unwrap().done);
    toggle_task(&db, parent)?;
    assert!(!list_tasks(&db, Filter::All)?[0].done);
    toggle_task(&db, parent)?;
    assert!(list_tasks(&db, Filter::All)?.iter().all(|task| task.done));
    toggle_task(&db, parent)?;
    let tasks = list_tasks(&db, Filter::All)?;
    assert!(!tasks[0].done);
    assert!(tasks[1..].iter().all(|task| task.done));
    assert_eq!(list_tasks_in(&db, Filter::All, Some(project))?.len(), 1);
    Ok(())
}

#[test]
fn due_dates_persist_for_tasks_and_subtasks_without_reordering() -> Result<()> {
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_nanos();
    let path = env::temp_dir().join(format!("cli-todo-dates-{stamp}.sqlite3"));
    let db = open(&path)?;
    let mut draft = nested_draft();
    draft.due_date = "2030-12-31".into();
    draft.subtasks[0].draft.due_date = "2028-02-29".into();
    save_task_tree(&db, None, &draft, None)?;
    drop(db);
    let db = open(&path)?;
    let tasks = list_tasks(&db, Filter::All)?;
    assert_eq!(tasks[0].title, "parent");
    assert_eq!(tasks[0].due_date, parse_due_date("2030-12-31")?);
    assert_eq!(tasks[1].due_date, parse_due_date("2028-02-29")?);
    let mut loaded = task_draft(&db, tasks[0].id)?;
    assert_eq!(loaded.subtasks[0].draft.due_date, "2028-02-29");
    loaded.due_date.clear();
    save_task_tree(&db, Some(tasks[0].id), &loaded, None)?;
    assert_eq!(list_tasks(&db, Filter::All)?[0].due_date, None);
    assert_eq!(
        list_tasks(&db, Filter::All)?[1].due_date,
        parse_due_date("2028-02-29")?
    );
    let before = list_tasks(&db, Filter::All)?;
    loaded.title = "changed".into();
    loaded.subtasks[0].draft.due_date = "2026-02-30".into();
    assert!(save_task_tree(&db, Some(tasks[0].id), &loaded, None).is_err());
    assert_eq!(list_tasks(&db, Filter::All)?, before);
    drop(db);
    fs::remove_file(path)?;
    Ok(())
}

fn nested_draft() -> TaskDraft {
    let leaf = SubtaskDraft {
        id: None,
        done: false,
        draft: TaskDraft {
            title: "grandchild".into(),
            description: "nested details".into(),
            ..TaskDraft::default()
        },
    };
    let child = SubtaskDraft {
        id: None,
        done: false,
        draft: TaskDraft {
            title: "child".into(),
            subtasks: vec![leaf],
            ..TaskDraft::default()
        },
    };
    let sibling = SubtaskDraft {
        id: None,
        done: false,
        draft: TaskDraft {
            title: "sibling".into(),
            ..TaskDraft::default()
        },
    };
    TaskDraft {
        title: "parent".into(),
        subtasks: vec![child, sibling],
        ..TaskDraft::default()
    }
}

#[test]
fn nested_subtasks_persist_move_and_delete_as_a_tree() -> Result<()> {
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_nanos();
    let path = env::temp_dir().join(format!("cli-todo-subtasks-{stamp}.sqlite3"));
    let db = open(&path)?;
    save_task_tree(&db, None, &nested_draft(), None)?;
    drop(db);
    let db = open(&path)?;
    let tasks = list_tasks(&db, Filter::All)?;
    assert_eq!(tasks.len(), 4);
    assert_eq!(tasks[1].parent_id, Some(tasks[0].id));
    assert_eq!(tasks[2].parent_id, Some(tasks[1].id));
    assert_eq!(list_tasks_in(&db, Filter::All, None)?.len(), 1);
    let loaded = task_draft(&db, tasks[0].id)?;
    assert_eq!(
        loaded.subtasks[0].draft.subtasks[0].draft.description,
        "nested details"
    );
    assert_eq!(task_draft(&db, tasks[2].id)?.depth, 2);
    save_project(&db, None, "Work")?;
    let project = db.last_insert_rowid();
    move_task(&db, tasks[0].id, Some(project))?;
    assert!(list_tasks(&db, Filter::All)?
        .iter()
        .all(|task| task.project_id == Some(project)));
    move_task(&db, tasks[1].id, None)?;
    validate_hierarchy(&db)?;
    assert_eq!(list_tasks_in(&db, Filter::All, None)?.len(), 1);
    assert_eq!(list_tasks_in(&db, Filter::All, None)?[0].parent_id, None);
    delete_project(&db, project)?;
    assert_eq!(list_tasks(&db, Filter::All)?.len(), 2);
    delete_task(&db, tasks[1].id)?;
    assert!(list_tasks(&db, Filter::All)?.is_empty());
    drop(db);
    fs::remove_file(path)?;
    Ok(())
}

#[test]
fn completing_a_parent_completes_descendants_but_reopening_does_not() -> Result<()> {
    let db = open(Path::new(":memory:"))?;
    save_task_tree(&db, None, &nested_draft(), None)?;
    let tasks = list_tasks(&db, Filter::All)?;
    toggle_task(&db, tasks[0].id)?;
    assert!(list_tasks(&db, Filter::All)?.iter().all(|task| task.done));
    toggle_task(&db, tasks[0].id)?;
    let reopened = list_tasks(&db, Filter::All)?;
    assert!(!reopened[0].done);
    assert!(reopened[1..].iter().all(|task| task.done));
    toggle_task(&db, tasks[1].id)?;
    assert!(!list_tasks(&db, Filter::All)?[1].done);
    assert!(list_tasks(&db, Filter::All)?[2].done);
    toggle_task(&db, tasks[1].id)?;
    assert!(list_tasks(&db, Filter::All)?[1..]
        .iter()
        .all(|task| task.done));
    Ok(())
}

#[test]
fn saving_subtask_trees_rolls_back_invalid_edits_and_rejects_bad_graphs() -> Result<()> {
    let db = open(Path::new(":memory:"))?;
    save_task_tree(&db, None, &nested_draft(), None)?;
    let before = list_tasks(&db, Filter::All)?;
    let mut draft = task_draft(&db, before[0].id)?;
    draft.title = "changed".into();
    draft.subtasks[0].draft.subtasks[0].draft.title.clear();
    assert!(save_task_tree(&db, Some(before[0].id), &draft, None).is_err());
    assert_eq!(list_tasks(&db, Filter::All)?, before);
    draft = task_draft(&db, before[0].id)?;
    draft.subtasks[0].id = Some(before[0].id);
    assert!(save_task_tree(&db, Some(before[0].id), &draft, None).is_err());
    assert_eq!(list_tasks(&db, Filter::All)?, before);
    db.execute(
        "UPDATE tasks SET parent_id=?1 WHERE id=?2",
        params![before[2].id, before[0].id],
    )?;
    assert!(validate_hierarchy(&db)
        .unwrap_err()
        .to_string()
        .contains("cycle"));
    db.execute(
        "UPDATE tasks SET parent_id=NULL WHERE id=?1",
        [before[0].id],
    )?;
    save_project(&db, None, "Other")?;
    db.execute(
        "UPDATE tasks SET project_id=?1 WHERE id=?2",
        params![db.last_insert_rowid(), before[1].id],
    )?;
    assert!(validate_hierarchy(&db)
        .unwrap_err()
        .to_string()
        .contains("project"));
    Ok(())
}

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

#[test]
fn positions_migrate_in_creation_order_swap_and_append_new_tasks() -> Result<()> {
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_nanos();
    let path = env::temp_dir().join(format!("cli-todo-position-{stamp}.sqlite3"));
    let legacy = Connection::open(&path)?;
    legacy.execute_batch("CREATE TABLE tasks(id INTEGER PRIMARY KEY,title TEXT NOT NULL,done INTEGER NOT NULL DEFAULT 0); INSERT INTO tasks(title) VALUES ('first'),('second');")?;
    drop(legacy);
    let titles = |db: &Connection| -> Result<Vec<String>> {
        Ok(list_tasks(db, Filter::All)?
            .into_iter()
            .map(|task| task.title)
            .collect())
    };
    let db = open(&path)?;
    assert_eq!(titles(&db)?, ["first", "second"]);
    swap_tasks(&db, 1, 2)?;
    create_task(&db, "third", None)?;
    assert_eq!(titles(&db)?, ["second", "first", "third"]);
    drop(db);
    let db = open(&path)?;
    assert_eq!(titles(&db)?, ["second", "first", "third"]);
    drop(db);
    fs::remove_file(path)?;
    Ok(())
}

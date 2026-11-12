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
    // Subtasks keep their fields; only the parent title they carry follows the rename.
    let children: Vec<_> = before[1..]
        .iter()
        .cloned()
        .map(|mut task| {
            if task.parent_id == Some(id) {
                task.parent_title = Some("reworded".into());
            }
            task
        })
        .collect();
    assert_eq!(after[1..], children);
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
    assert_eq!(
        list_tasks_in(&db, Filter::All, Some(project), Sort::Manual)?.len(),
        1
    );
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

#[test]
fn duplicate_copies_the_subtree_in_place_and_starts_pending() -> Result<()> {
    let db = open(Path::new(":memory:"))?;
    save_project(&db, None, "Work")?;
    let project = db.last_insert_rowid();
    let mut draft = nested_draft();
    draft.due_date = "2028-02-29".into();
    draft.subtasks[1].done = true;
    save_task_tree(&db, None, &draft, Some(project))?;
    let root = list_tasks(&db, Filter::All)?[0].id;
    toggle_task(&db, root)?;
    let before = list_tasks(&db, Filter::All)?;
    duplicate_task(&db, root)?;
    let after = list_tasks(&db, Filter::All)?;
    assert_eq!(&after[..before.len()], &before[..]);
    let copy = &after[before.len()];
    assert_eq!((copy.title.as_str(), copy.done), ("parent", false));
    assert_eq!((copy.project_id, copy.parent_id), (Some(project), None));
    assert_eq!(copy.due_date, parse_due_date("2028-02-29")?);
    let copied = task_draft(&db, copy.id)?;
    let original = task_draft(&db, root)?;
    assert_eq!(
        format!(
            "{:?}",
            copied
                .subtasks
                .iter()
                .map(|s| (&s.draft.title, s.done))
                .collect::<Vec<_>>()
        ),
        format!(
            "{:?}",
            original
                .subtasks
                .iter()
                .map(|s| (&s.draft.title, s.done))
                .collect::<Vec<_>>()
        )
    );
    assert_eq!(
        copied.subtasks[0].draft.subtasks[0].draft.description,
        "nested details"
    );
    let child = original.subtasks[0].id.unwrap();
    duplicate_task(&db, child)?;
    let children = task_draft(&db, root)?.subtasks;
    assert_eq!(children.len(), 3);
    assert_eq!(children[2].draft.title, "child");
    assert_eq!(children[2].draft.subtasks[0].draft.title, "grandchild");
    validate_hierarchy(&db)?;
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
    assert_eq!(
        list_tasks_in(&db, Filter::All, None, Sort::Manual)?.len(),
        1
    );
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
    let top_level = list_tasks_in(&db, Filter::All, None, Sort::Manual)?;
    assert_eq!(top_level.len(), 2);
    assert!(top_level.iter().all(|task| task.parent_id.is_none()));
    assert_eq!(
        top_level
            .iter()
            .filter(|task| task.project_id.is_none())
            .count(),
        1
    );
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
    let tasks = list_tasks_in(&db, Filter::All, None, Sort::Manual)?;
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].title, "existing");
    assert!(tasks[0].done);
    assert_eq!(tasks[0].priority, None);
    save_project(&db, None, "Work")?;
    let project = db.last_insert_rowid();
    create_task(&db, "project task", Some(project))?;
    drop(db);
    let db = open(&path)?;
    assert_eq!(list_projects(&db)?[0].name, "Work");
    assert_eq!(
        list_tasks_in(&db, Filter::All, Some(project), Sort::Manual)?[0].title,
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
    assert_eq!(
        list_tasks_in(&db, Filter::All, None, Sort::Manual)?.len(),
        2,
        "Inbox lists every project"
    );
    assert_eq!(
        list_tasks_in(&db, Filter::All, Some(project), Sort::Manual)?.len(),
        1
    );
    assert_eq!(
        list_tasks_in(&db, Filter::All, Some(project), Sort::Manual)?[0].id,
        task
    );
    move_task(&db, task, None)?;
    assert_eq!(
        list_tasks_in(&db, Filter::All, None, Sort::Manual)?.len(),
        2
    );
    assert!(list_tasks_in(&db, Filter::All, Some(project), Sort::Manual)?.is_empty());
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
fn deleting_completed_tasks_removes_done_roots_with_descendants_only() -> Result<()> {
    let db = open(Path::new(":memory:"))?;
    save_task_tree(&db, None, &nested_draft(), None)?;
    save_task_tree(&db, None, &nested_draft(), None)?;
    let tasks = list_tasks(&db, Filter::All)?;
    toggle_task(&db, tasks[0].id)?;
    toggle_task(&db, tasks[5].id)?;
    delete_completed_tasks(&db)?;
    let remaining = list_tasks(&db, Filter::All)?;
    assert_eq!(
        remaining.iter().map(|task| task.id).collect::<Vec<_>>(),
        tasks[4..].iter().map(|task| task.id).collect::<Vec<_>>(),
        "pending roots keep their completed subtasks"
    );
    assert!(remaining[1].done);
    Ok(())
}

#[test]
fn completion_time_is_stamped_kept_for_done_descendants_and_cleared_on_reopen() -> Result<()> {
    let db = open(Path::new(":memory:"))?;
    let done_child = SubtaskDraft {
        id: None,
        done: true,
        draft: TaskDraft {
            title: "done child".into(),
            ..TaskDraft::default()
        },
    };
    save_task_tree(
        &db,
        None,
        &TaskDraft {
            title: "parent".into(),
            subtasks: vec![done_child],
            ..TaskDraft::default()
        },
        None,
    )?;
    let completed_at = |title: &str| -> Result<Option<String>> {
        Ok(list_tasks(&db, Filter::All)?
            .into_iter()
            .find(|task| task.title == title)
            .ok_or("missing task")?
            .completed_at)
    };
    assert!(completed_at("parent")?.is_none());
    assert!(completed_at("done child")?.is_some(), "saved as done");
    db.execute(
        "UPDATE tasks SET completed_at='2000-01-01 00:00:00.000' WHERE title='done child'",
        [],
    )?;
    let parent = list_tasks(&db, Filter::All)?[0].id;
    toggle_task(&db, parent)?;
    assert!(completed_at("parent")?.is_some());
    assert_eq!(
        completed_at("done child")?.as_deref(),
        Some("2000-01-01 00:00:00.000"),
        "cascading completion keeps earlier stamps"
    );
    toggle_task(&db, parent)?;
    assert!(completed_at("parent")?.is_none());
    Ok(())
}

#[test]
fn lists_put_completed_tasks_last_newest_first_and_sort_pending_by_creation() -> Result<()> {
    let db = open(Path::new(":memory:"))?;
    for title in ["a", "b", "c", "old done", "early done", "late done"] {
        create_task(&db, title, None)?;
    }
    swap_tasks(&db, 1, 3)?;
    db.execute_batch(
        "UPDATE tasks SET done=1 WHERE id IN (4,5,6);
         UPDATE tasks SET completed_at='2026-01-01 00:00:00.000' WHERE id=5;
         UPDATE tasks SET completed_at='2026-02-01 00:00:00.000' WHERE id=6;
         UPDATE tasks SET position=0 WHERE id=6;",
    )?;
    let titles = |sort| -> Result<Vec<String>> {
        Ok(list_tasks_in(&db, Filter::All, None, sort)?
            .into_iter()
            .map(|task| task.title)
            .collect())
    };
    let completed = ["late done", "early done", "old done"];
    assert_eq!(
        titles(Sort::Manual)?,
        [&["c", "b", "a"][..], &completed].concat()
    );
    assert_eq!(
        titles(Sort::CreatedAsc)?,
        [&["a", "b", "c"][..], &completed].concat()
    );
    assert_eq!(
        titles(Sort::CreatedDesc)?,
        [&["c", "b", "a"][..], &completed].concat()
    );
    Ok(())
}

#[test]
fn sorts_are_saved_per_project_and_inbox_filter_and_survive_reopening() -> Result<()> {
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_nanos();
    let path = env::temp_dir().join(format!("cli-todo-sort-{stamp}.sqlite3"));
    let db = open(&path)?;
    for name in ["Work", "Home"] {
        save_project(&db, None, name)?;
    }
    let (work, home) = (1, 2);
    save_sort(&db, Some(work), Filter::All, Sort::CreatedDesc)?;
    save_sort(&db, None, Filter::Today, Sort::CreatedAsc)?;
    assert_eq!(
        load_sort(&db, Some(home), Filter::All)?,
        Sort::Manual,
        "other projects keep Manual"
    );
    for filter in [Filter::All, Filter::Pending, Filter::Completed] {
        assert_eq!(
            load_sort(&db, None, filter)?,
            Sort::Manual,
            "{filter:?} is not sorted by Today's choice"
        );
    }
    drop(db);
    let db = open(&path)?;
    assert_eq!(
        load_sort(&db, Some(work), Filter::Completed)?,
        Sort::CreatedDesc,
        "a project shares one sort across filters"
    );
    assert_eq!(load_sort(&db, None, Filter::Today)?, Sort::CreatedAsc);
    save_sort(&db, None, Filter::All, Sort::CreatedDesc)?;
    assert_eq!(load_sort(&db, None, Filter::All)?, Sort::CreatedDesc);
    assert_eq!(load_sort(&db, None, Filter::Today)?, Sort::CreatedAsc);
    save_sort(&db, Some(work), Filter::Pending, Sort::CreatedAsc)?;
    assert_eq!(
        load_sort(&db, Some(work), Filter::All)?,
        Sort::CreatedAsc,
        "saving replaces"
    );
    save_sort(&db, None, Filter::Today, Sort::Manual)?;
    assert_eq!(load_sort(&db, None, Filter::Today)?, Sort::Manual);
    delete_project(&db, work)?;
    let rows: i64 = db.query_row(
        "SELECT COUNT(*) FROM list_sorts WHERE project_id IS NOT NULL",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(rows, 0, "deleting a project drops its sort");
    nuke(&db)?;
    assert_eq!(
        load_sort(&db, None, Filter::All)?,
        Sort::Manual,
        "nuke resets sorts"
    );
    drop(db);
    fs::remove_file(path)?;
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

#[test]
fn project_descriptions_migrate_empty_and_enforce_one_line_limit() -> Result<()> {
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_nanos();
    let path = env::temp_dir().join(format!("cli-todo-project-description-{stamp}.sqlite3"));
    let legacy = Connection::open(&path)?;
    legacy.execute_batch("CREATE TABLE projects(id INTEGER PRIMARY KEY,name TEXT NOT NULL UNIQUE); INSERT INTO projects(name) VALUES ('Work');")?;
    // Old snapshots without the column still list.
    assert_eq!(list_projects(&legacy)?[0].description, "");
    drop(legacy);
    let db = open(&path)?;
    assert_eq!(list_projects(&db)?[0].description, "");
    set_project_description(&db, 1, "  Ship the Q4 release  ")?;
    assert_eq!(list_projects(&db)?[0].description, "Ship the Q4 release");
    // The limit counts characters, not bytes.
    let longest = "é".repeat(MAX_PROJECT_DESCRIPTION);
    set_project_description(&db, 1, &longest)?;
    assert_eq!(list_projects(&db)?[0].description, longest);
    assert_eq!(
        set_project_description(&db, 1, &format!("{longest}x"))
            .unwrap_err()
            .to_string(),
        "Project description is 101 characters; the limit is 100"
    );
    assert!(set_project_description(&db, 1, "two\nlines").is_err());
    // The schema rejects over-limit text written around the app.
    assert!(db
        .execute(
            "UPDATE projects SET description=?1 WHERE id=1",
            ["x".repeat(MAX_PROJECT_DESCRIPTION + 1)],
        )
        .is_err());
    assert_eq!(list_projects(&db)?[0].description, longest);
    set_project_description(&db, 1, "")?;
    assert_eq!(list_projects(&db)?[0].description, "");
    drop(db);
    fs::remove_file(path)?;
    Ok(())
}

#[test]
fn project_positions_migrate_in_creation_order_swap_and_append_new_projects() -> Result<()> {
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_nanos();
    let path = env::temp_dir().join(format!("cli-todo-project-position-{stamp}.sqlite3"));
    let legacy = Connection::open(&path)?;
    legacy.execute_batch("CREATE TABLE projects(id INTEGER PRIMARY KEY,name TEXT NOT NULL UNIQUE); INSERT INTO projects(name) VALUES ('first'),('second');")?;
    assert_eq!(list_projects(&legacy)?[0].name, "first");
    drop(legacy);
    let names = |db: &Connection| -> Result<Vec<String>> {
        Ok(list_projects(db)?
            .into_iter()
            .map(|project| project.name)
            .collect())
    };
    let db = open(&path)?;
    assert_eq!(names(&db)?, ["first", "second"]);
    swap_projects(&db, 1, 2)?;
    save_project(&db, None, "third")?;
    save_project(&db, Some(1), "renamed")?;
    assert_eq!(names(&db)?, ["second", "renamed", "third"]);
    drop(db);
    let db = open(&path)?;
    assert_eq!(names(&db)?, ["second", "renamed", "third"]);
    drop(db);
    fs::remove_file(path)?;
    Ok(())
}

#[test]
fn today_lists_due_tasks_and_subtasks_themselves() -> Result<()> {
    let today = Local::now().date_naive();
    let db = open(Path::new(":memory:"))?;
    let draft = |title: &str, due: String, subtasks| TaskDraft {
        title: title.into(),
        due_date: due,
        subtasks,
        ..TaskDraft::default()
    };
    let subtask = |draft, done| SubtaskDraft {
        id: None,
        done,
        draft,
    };
    let overdue = (today - chrono::Days::new(3)).to_string();
    let later = (today + chrono::Days::new(3)).to_string();
    for task in [
        draft("due today", today.to_string(), vec![]),
        draft("overdue", overdue.clone(), vec![]),
        draft("later", later.clone(), vec![]),
        draft(
            "undated parent",
            String::new(),
            vec![subtask(
                draft(
                    "undated child",
                    String::new(),
                    vec![subtask(draft("due grandchild", overdue, vec![]), false)],
                ),
                false,
            )],
        ),
        draft(
            "due parent",
            today.to_string(),
            vec![
                subtask(draft("due child", today.to_string(), vec![]), false),
                subtask(draft("done child", today.to_string(), vec![]), true),
                subtask(draft("later child", later, vec![]), false),
            ],
        ),
    ] {
        save_task_tree(&db, None, &task, None)?;
    }
    let listed: Vec<_> = list_tasks_in(&db, Filter::Today, None, Sort::Manual)?
        .into_iter()
        .map(|task| (task.title, task.parent_title))
        .collect();
    let parent = |title: &str| Some(title.to_string());
    assert_eq!(
        listed,
        [
            ("due today".into(), None),
            ("overdue".into(), None),
            ("due grandchild".into(), parent("undated child")),
            ("due parent".into(), None),
            ("due child".into(), parent("due parent")),
        ]
    );
    Ok(())
}

#[test]
fn priorities_persist_sort_p1_first_and_survive_edits_and_duplicates() -> Result<()> {
    let db = open(Path::new(":memory:"))?;
    for title in ["low", "none", "high", "mid", "high too", "done high"] {
        create_task(&db, title, None)?;
    }
    for (id, priority) in [(1, 3), (3, 1), (4, 2), (5, 1), (6, 1)] {
        set_priority(&db, id, Some(priority))?;
    }
    toggle_task(&db, 6)?;
    let titles = |sort| -> Result<Vec<String>> {
        Ok(list_tasks_in(&db, Filter::All, None, sort)?
            .into_iter()
            .map(|task| task.title)
            .collect())
    };
    assert_eq!(
        titles(Sort::Priority)?,
        ["high", "high too", "mid", "low", "none", "done high"]
    );
    assert_eq!(
        titles(Sort::Manual)?,
        ["low", "none", "high", "mid", "high too", "done high"]
    );
    assert!(
        set_priority(&db, 2, Some(4)).is_err(),
        "only P1-P3 are valid"
    );

    let mut draft = task_draft(&db, 3)?;
    assert_eq!(draft.priority, Some(1));
    draft.title = "high edited".into();
    save_task_tree(&db, Some(3), &draft, None)?;
    reword_task(&db, 3, &draft)?;
    let mut child = nested_draft();
    child.priority = Some(2);
    save_subtask_tree(&db, 3, &child)?;
    duplicate_task(&db, 3)?;
    let tasks = list_tasks(&db, Filter::All)?;
    let priority = |title: &str| {
        tasks
            .iter()
            .filter(|task| task.title == title)
            .map(|task| task.priority)
            .collect::<Vec<_>>()
    };
    assert_eq!(priority("high edited"), [Some(1), Some(1)]);
    assert_eq!(priority("parent"), [Some(2), Some(2)]);
    assert_eq!(priority("child"), [None, None], "subtasks keep their own");

    set_priority(&db, 3, None)?;
    assert_eq!(task_draft(&db, 3)?.priority, None);
    save_sort(&db, None, Filter::All, Sort::Priority)?;
    assert_eq!(load_sort(&db, None, Filter::All)?, Sort::Priority);
    Ok(())
}

#[test]
fn priority_filters_list_unfinished_tasks_and_subtasks_with_their_own_sort() -> Result<()> {
    let db = open(Path::new(":memory:"))?;
    save_project(&db, None, "Work")?;
    let project = db.last_insert_rowid();
    save_task_tree(&db, None, &nested_draft(), Some(project))?;
    for title in ["inbox p1", "done p1", "inbox p2"] {
        create_task(&db, title, None)?;
    }
    let tasks = list_tasks(&db, Filter::All)?;
    let id = |title: &str| tasks.iter().find(|task| task.title == title).unwrap().id;
    for (title, priority) in [
        ("child", 1),
        ("inbox p1", 1),
        ("done p1", 1),
        ("inbox p2", 2),
    ] {
        set_priority(&db, id(title), Some(priority))?;
    }
    toggle_task(&db, id("done p1"))?;
    let listed = |project| -> Result<Vec<(String, Option<String>)>> {
        Ok(
            list_tasks_in(&db, Filter::Priority(1), project, Sort::Manual)?
                .into_iter()
                .map(|task| (task.title, task.parent_title))
                .collect(),
        )
    };
    let child = ("child".to_string(), Some("parent".to_string()));
    assert_eq!(listed(None)?, [child.clone(), ("inbox p1".into(), None)]);
    assert_eq!(listed(Some(project))?, [child]);

    save_sort(&db, None, Filter::Priority(1), Sort::CreatedDesc)?;
    assert_eq!(
        load_sort(&db, None, Filter::Priority(1))?,
        Sort::CreatedDesc
    );
    assert_eq!(
        load_sort(&db, None, Filter::Priority(2))?,
        Sort::Manual,
        "each priority filter keeps its own sort"
    );
    Ok(())
}

#[test]
fn autoincrement_migration_keeps_rows_and_never_reuses_ids() -> Result<()> {
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_nanos();
    let path = env::temp_dir().join(format!("cli-todo-autoincrement-{stamp}.sqlite3"));
    let db = open(&path)?;
    // Recreate the pre-AUTOINCREMENT schema with a project, task, subtask, and saved sort.
    let old = |table: &str| -> Result<String> {
        let sql: String = db.query_row(
            "SELECT sql FROM sqlite_master WHERE name=?1",
            [table],
            |row| row.get(0),
        )?;
        Ok(sql.replace(" AUTOINCREMENT", ""))
    };
    let (projects, tasks) = (old("projects")?, old("tasks")?);
    db.execute_batch(&format!(
        "PRAGMA foreign_keys = OFF;
        DROP TABLE tasks; DROP TABLE projects; DELETE FROM sqlite_sequence;
        {projects}; {tasks};
        PRAGMA foreign_keys = ON;"
    ))?;
    save_project(&db, None, "Work")?;
    let project = db.last_insert_rowid();
    save_task_tree(&db, None, &nested_draft(), Some(project))?;
    save_sort(&db, Some(project), Filter::All, Sort::CreatedDesc)?;
    let before = list_tasks(&db, Filter::All)?;
    drop(db);

    let db = open(&path)?;
    let schema: String = db.query_row(
        "SELECT group_concat(sql) FROM sqlite_master WHERE name IN ('tasks','projects')",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(schema.matches("AUTOINCREMENT").count(), 2);
    assert!(list_tasks(&db, Filter::All)? == before);
    assert_eq!(list_projects(&db)?[0].id, project);
    assert_eq!(
        load_sort(&db, Some(project), Filter::All)?,
        Sort::CreatedDesc
    );
    let index: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='tasks_parent_id')",
        [],
        |row| row.get(0),
    )?;
    assert!(index);

    let newest = before.iter().map(|task| task.id).max().unwrap_or_default();
    delete_task(&db, newest)?;
    save_task_tree(
        &db,
        None,
        &TaskDraft {
            title: "next".into(),
            ..TaskDraft::default()
        },
        None,
    )?;
    assert_eq!(db.last_insert_rowid(), newest + 1);
    delete_project(&db, project)?;
    save_project(&db, None, "Home")?;
    assert_eq!(db.last_insert_rowid(), project + 1);
    drop(db);
    open(&path)?;
    fs::remove_file(path)?;
    Ok(())
}

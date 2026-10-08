
use super::*;
use crate::db;
use std::{env, time::SystemTime};

fn configure_author(history: &History) -> Result<()> {
    history.git_text(&["config", "user.name", "Todo Test"])?;
    history.git_text(&["config", "user.email", "todo@example.test"])?;
    history.git_text(&["config", "commit.gpgsign", "false"])?;
    Ok(())
}

#[test]
fn remote_sync_preserves_local_changes_and_fast_forwards_snapshots() -> Result<()> {
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_nanos();
    let root = env::temp_dir().join(format!("cli-todo-sync-{stamp}"));
    let remote = root.join("remote.git");
    fs::create_dir_all(&root)?;
    assert!(Command::new("git")
        .args(["init", "--bare", "--quiet", "--initial-branch=main"])
        .arg(&remote)
        .output()?
        .status
        .success());
    let remote = remote.to_str().unwrap();
    let one = root.join("one");
    let two = root.join("two");
    fs::create_dir_all(&one)?;
    fs::create_dir_all(&two)?;
    let first = db::open(&one.join("tasks.sqlite3"))?;
    let mut second = db::open(&two.join("tasks.sqlite3"))?;
    let a = History::new(&first).unwrap();
    let b = History::new(&second).unwrap();
    assert!(a.push().is_err());
    a.set_remote(remote)?;
    assert_eq!(a.remote()?, remote);
    configure_author(&a)?;
    db::save_project(&first, None, "Work")?;
    db::create_task(&first, "first", Some(first.last_insert_rowid()))?;
    first.execute_batch("ALTER TABLE tasks DROP COLUMN description;")?;
    a.commit(&first, "first checkpoint")?;
    a.push()?;
    b.set_remote(remote)?;
    let SyncResult::Pull(plan) = b.fetch_pull()? else {
        panic!("expected initial snapshot");
    };
    b.apply_pull(&mut second, plan)?;
    assert_eq!(db::list_tasks(&second, Filter::All)?[0].title, "first");
    assert_eq!(db::list_projects(&second)?[0].name, "Work");
    assert!(!b.has_changes(&second)?);
    assert_eq!(fs::read_dir(two.join("backups"))?.count(), 1);
    assert!(matches!(b.fetch_pull()?, SyncResult::Message(_)));

    db::migrate_description(&first)?;
    db::save_task_details(
        &first,
        Some(1),
        "second",
        "remote details\nsecond line",
        None,
    )?;
    a.commit(&first, "second checkpoint")?;
    a.push()?;
    let SyncResult::Pull(plan) = b.fetch_pull()? else {
        panic!("expected new snapshot");
    };
    db::save_task(&second, Some(1), "unsaved locally")?;
    assert!(b.apply_pull(&mut second, plan).is_err());
    assert_eq!(
        db::list_tasks(&second, Filter::All)?[0].title,
        "unsaved locally"
    );
    assert_eq!(b.list()?.len(), 1);
    db::save_task(&second, Some(1), "first")?;
    assert!(!b.has_changes(&second)?);

    let SyncResult::Pull(plan) = b.fetch_pull()? else {
        panic!("expected new snapshot");
    };
    let lock = b.directory.join(".git/index.lock");
    fs::write(&lock, "locked")?;
    let error = b.apply_pull(&mut second, plan).err().unwrap().to_string();
    assert!(error.contains("original database restored"), "{error}");
    assert_eq!(db::list_tasks(&second, Filter::All)?[0].title, "first");
    fs::remove_file(lock)?;
    let SyncResult::Pull(plan) = b.fetch_pull()? else {
        panic!("expected new snapshot");
    };
    b.apply_pull(&mut second, plan)?;
    assert_eq!(db::list_tasks(&second, Filter::All)?[0].title, "second");
    assert_eq!(
        db::list_tasks(&second, Filter::All)?[0].description,
        "remote details\nsecond line"
    );
    assert_eq!(b.list()?.len(), 2);
    assert!(!b.has_changes(&second)?);
    db::save_task_details(&second, Some(1), "second", "description-only change", None)?;
    assert!(b.has_changes(&second)?);
    db::save_task_details(
        &second,
        Some(1),
        "second",
        "remote details\nsecond line",
        None,
    )?;
    assert!(!b.has_changes(&second)?);

    configure_author(&b)?;
    db::save_task(&second, None, "local only")?;
    b.commit(&second, "local branch")?;
    assert!(matches!(b.fetch_pull()?, SyncResult::Message(_)));
    db::save_task(&first, None, "remote only")?;
    a.commit(&first, "remote branch")?;
    a.push()?;
    assert!(b
        .fetch_pull()
        .err()
        .unwrap()
        .to_string()
        .contains("diverged"));
    assert!(b.push().is_err());
    assert_eq!(db::list_tasks(&second, Filter::All)?.len(), 2);
    fs::write(b.directory.join("notes"), "untracked")?;
    assert!(b.fetch_pull().is_err());
    fs::remove_file(b.directory.join("notes"))?;
    b.set_remote("/nonexistent/todo-remote")?;
    assert!(b.push().is_err());
    b.set_remote("")?;
    assert!(b.remote()?.is_empty());
    assert!(b.fetch_pull().is_err());
    drop(first);
    drop(second);
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn pull_rejects_invalid_remote_snapshot_before_replacing_live_data() -> Result<()> {
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_nanos();
    let root = env::temp_dir().join(format!("cli-todo-invalid-pull-{stamp}"));
    fs::create_dir_all(&root)?;
    let bad = root.join("remote");
    fs::create_dir_all(&bad)?;
    let publisher = History {
        directory: bad.clone(),
    };
    publisher.git_text(&["init", "--quiet", "--initial-branch=main"])?;
    configure_author(&publisher)?;
    fs::write(bad.join("tasks.sqlite3"), "not a database")?;
    publisher.git_text(&["add", "tasks.sqlite3"])?;
    publisher.git_text(&["commit", "-m", "invalid snapshot"])?;
    let mut database = db::open(&root.join("tasks.sqlite3"))?;
    let history = History::new(&database).unwrap();
    history.set_remote(bad.to_str().unwrap())?;
    let SyncResult::Pull(plan) = history.fetch_pull()? else {
        panic!("expected download");
    };
    assert!(history.apply_pull(&mut database, plan).is_err());
    assert!(db::list_tasks(&database, Filter::All)?.is_empty());
    assert!(history.list()?.is_empty());
    assert!(!root.join("backups").exists());
    drop(database);
    fs::remove_dir_all(root)?;
    Ok(())
}

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
    assert_eq!(db::list_tasks(&database, Filter::All)?.len(), 2);
    fs::remove_file(lock)?;
    history.commit(&database, "second checkpoint")?;
    assert_eq!(history.list()?.len(), 2);
    assert!(!history.has_changes(&database)?);
    let output = history.git(&["show", &format!("{}:tasks.sqlite3", first.hash)])?;
    assert!(output.status.success());
    let old = root.join("old.sqlite3");
    fs::write(&old, output.stdout)?;
    let snapshot = db::open(&old)?;
    assert_eq!(db::list_tasks(&snapshot, Filter::All)?.len(), 1);
    assert_eq!(db::list_projects(&snapshot)?[0].name, "Work");
    assert_eq!(db::list_tasks(&database, Filter::All)?.len(), 2);
    drop(snapshot);
    drop(database);
    fs::remove_dir_all(root)?;
    Ok(())
}

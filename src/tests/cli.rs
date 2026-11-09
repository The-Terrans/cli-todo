use super::*;
use std::path::Path;

fn cli(db: &Connection, command: &str) -> Result<String> {
    let args: Vec<String> = command.split(' ').map(String::from).collect();
    let mut out = vec![];
    run(db, &args, &mut out)?;
    Ok(String::from_utf8(out)?)
}

fn error(db: &Connection, command: &str) -> String {
    cli(db, command).unwrap_err().to_string()
}

#[test]
fn adds_lists_toggles_and_deletes_tasks() -> Result<()> {
    let db = db::open(Path::new(":memory:"))?;
    assert_eq!(
        cli(&db, "add Buy milk")?,
        "Added task 1 to Inbox: Buy milk\n"
    );
    cli(&db, "add Call mom")?;
    assert_eq!(cli(&db, "done 1")?, "Completed task 1: Buy milk\n");
    assert_eq!(cli(&db, "list")?, "   2  [ ] Call mom\n");
    assert_eq!(
        cli(&db, "list --all")?,
        "   2  [ ] Call mom\n   1  [x] Buy milk\n"
    );
    assert_eq!(cli(&db, "done 1")?, "Reopened task 1: Buy milk\n");
    assert_eq!(cli(&db, "delete 2")?, "Deleted task 2: Call mom\n");
    assert_eq!(cli(&db, "list")?, "   1  [ ] Buy milk\n");
    assert_eq!(cli(&db, "delete 1")?, "Deleted task 1: Buy milk\n");
    assert_eq!(cli(&db, "list")?, "No tasks\n");
    Ok(())
}

#[test]
fn adds_tasks_to_projects_and_filters_by_project() -> Result<()> {
    let db = db::open(Path::new(":memory:"))?;
    assert_eq!(cli(&db, "project add Work")?, "Added project Work\n");
    assert_eq!(
        cli(&db, "add Ship -p Work")?,
        "Added task 1 to Work: Ship\n"
    );
    assert_eq!(
        cli(&db, "add --project Work Review")?,
        "Added task 2 to Work: Review\n"
    );
    cli(&db, "add Groceries")?;
    assert_eq!(
        cli(&db, "list")?,
        "   1  [ ] Ship  (Work)\n   2  [ ] Review  (Work)\n   3  [ ] Groceries\n"
    );
    assert_eq!(
        cli(&db, "list -p Work")?,
        "   1  [ ] Ship\n   2  [ ] Review\n"
    );
    assert_eq!(
        error(&db, "add Lost -p Home"),
        "No project named 'Home'. Run 'cli-todo project list'"
    );
    Ok(())
}

#[test]
fn manages_projects() -> Result<()> {
    let db = db::open(Path::new(":memory:"))?;
    assert_eq!(cli(&db, "project list")?, "No projects\n");
    cli(&db, "project add Work")?;
    cli(&db, "project add Home")?;
    assert_eq!(
        error(&db, "project add Work"),
        "A project named 'Work' already exists"
    );
    assert_eq!(
        cli(&db, "project rename Work Job")?,
        "Renamed project Work to Job\n"
    );
    assert_eq!(cli(&db, "project list")?, "Job\nHome\n");
    cli(&db, "add Ship -p Job")?;
    assert_eq!(
        error(&db, "project delete Job"),
        "Project Job has 1 tasks. Add --force to delete them too"
    );
    assert_eq!(
        cli(&db, "project delete Job --force")?,
        "Deleted project Job and 1 tasks\n"
    );
    assert_eq!(
        cli(&db, "project delete Home")?,
        "Deleted project Home and 0 tasks\n"
    );
    assert_eq!(cli(&db, "list --all")?, "No tasks\n");
    Ok(())
}

#[test]
fn rejects_bad_input_with_guidance() -> Result<()> {
    let db = db::open(Path::new(":memory:"))?;
    assert_eq!(
        error(&db, "todo x"),
        "Unknown command 'todo'. Run 'cli-todo help' for usage"
    );
    assert_eq!(
        error(&db, "add"),
        "Usage: cli-todo add <title> [-p <project>]"
    );
    assert_eq!(error(&db, "add x -p"), "-p needs a project name");
    assert_eq!(error(&db, "list --done"), "Unknown option '--done'");
    assert_eq!(error(&db, "done abc"), "'abc' is not a task id");
    assert_eq!(
        error(&db, "delete 9"),
        "No task with id 9. Run 'cli-todo list --all'"
    );
    assert_eq!(error(&db, "done 1 2"), "Usage: cli-todo done <id>");
    assert!(cli(&db, "help")?.starts_with("Usage:"));
    let version = format!("cli-todo {}\n", env!("CARGO_PKG_VERSION"));
    assert_eq!(cli(&db, "-v")?, version);
    assert_eq!(cli(&db, "--version")?, version);
    Ok(())
}

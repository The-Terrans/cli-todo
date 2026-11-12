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
        "Usage: cli-todo add <title> [-p <project> | --parent <id>]"
    );
    assert_eq!(error(&db, "add x -p"), "-p needs a value");
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

#[test]
fn adds_and_lists_subtasks_one_level_at_a_time() -> Result<()> {
    let db = db::open(Path::new(":memory:"))?;
    cli(&db, "project add Work")?;
    cli(&db, "add Ship -p Work")?;
    assert_eq!(
        cli(&db, "add Write docs --parent 1")?,
        "Added subtask 2 to Ship: Write docs\n"
    );
    cli(&db, "add Tests --parent 1")?;
    cli(&db, "add Unit --parent 3")?;
    assert_eq!(cli(&db, "list")?, "   1  [ ] Ship  +2 subtasks  (Work)\n");
    assert_eq!(
        cli(&db, "list --parent 1")?,
        "   2  [ ] Write docs\n   3  [ ] Tests  +1 subtask\n"
    );
    assert_eq!(cli(&db, "list --parent 3")?, "   4  [ ] Unit\n");
    assert_eq!(cli(&db, "list -p Work")?, "   1  [ ] Ship  +2 subtasks\n");
    cli(&db, "done 2")?;
    assert_eq!(
        cli(&db, "list --parent 1")?,
        "   3  [ ] Tests  +1 subtask\n"
    );
    assert_eq!(
        cli(&db, "list --parent 1 --all")?,
        "   3  [ ] Tests  +1 subtask\n   2  [x] Write docs\n"
    );
    assert_eq!(cli(&db, "list --parent 4")?, "No tasks\n");
    // Subtasks inherit the parent's project.
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM tasks WHERE project_id=1", [], |row| {
            row.get::<_, i64>(0)
        })?,
        4
    );
    assert_eq!(
        error(&db, "add x --parent 1 -p Work"),
        "Use -p or --parent, not both; subtasks stay in their parent's project"
    );
    assert_eq!(
        error(&db, "list --parent 9"),
        "No task with id 9. Run 'cli-todo list --all'"
    );
    assert_eq!(error(&db, "add x --parent"), "--parent needs a value");
    assert_eq!(error(&db, "done 1 -p Work"), "Unknown option '-p'");
    Ok(())
}

#[test]
fn describes_projects() -> Result<()> {
    let db = db::open(Path::new(":memory:"))?;
    assert_eq!(
        cli_args(&db, &["project", "add", "Work", "--description", "Day job"])?,
        "Added project Work\n"
    );
    cli(&db, "project add Home")?;
    assert_eq!(cli(&db, "project list")?, "Work — Day job\nHome\n");
    assert_eq!(
        cli_args(&db, &["project", "describe", "Home", "Chores and errands"])?,
        "Described project Home\n"
    );
    assert_eq!(
        cli_args(&db, &["project", "describe", "Work", ""])?,
        "Cleared description of project Work\n"
    );
    assert_eq!(
        cli(&db, "project list")?,
        "Work\nHome — Chores and errands\n"
    );
    let long = "x".repeat(101);
    assert_eq!(
        cli_args(&db, &["project", "add", "Side", "--description", &long])
            .unwrap_err()
            .to_string(),
        "Project description is 101 characters; the limit is 100"
    );
    // A rejected description adds no project.
    assert_eq!(
        cli(&db, "project list")?,
        "Work\nHome — Chores and errands\n"
    );
    assert!(cli_args(&db, &["project", "describe", "Home", &long]).is_err());
    assert_eq!(
        error(&db, "project rename Home House --description x"),
        "--description only works with 'project add'; use 'project describe'"
    );
    assert_eq!(
        error(&db, "project describe Nope x"),
        "No project named 'Nope'. Run 'cli-todo project list'"
    );
    Ok(())
}

/// Like `cli`, but takes pre-split arguments so values can contain spaces.
fn cli_args(db: &Connection, args: &[&str]) -> Result<String> {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut out = vec![];
    run(db, &args, &mut out)?;
    Ok(String::from_utf8(out)?)
}

#[test]
fn shows_and_edits_task_fields() -> Result<()> {
    let db = db::open(Path::new(":memory:"))?;
    cli(&db, "project add Work")?;
    cli_args(
        &db,
        &[
            "add",
            "Ship",
            "-p",
            "Work",
            "--description",
            "Tag it\nthen release",
            "--due",
            "2030-01-02",
            "--priority",
            "P1",
        ],
    )?;
    cli(&db, "add Docs --parent 1")?;
    assert_eq!(
        cli(&db, "show 1")?,
        "1  [ ] Ship\nProject:  Work\nDue:      2030-01-02\nPriority: P1\n\
         Subtasks: 1  (cli-todo list --parent 1)\n\nTag it\nthen release\n"
    );
    assert_eq!(
        cli(&db, "show 2")?,
        "2  [ ] Docs\nProject:  Work\nParent:   1  Ship\n"
    );

    assert_eq!(
        cli_args(&db, &["edit", "1", "--title", "Ship v2", "--priority", "3"])?,
        "Updated task 1: Ship v2\n"
    );
    let tomorrow = Local::now()
        .date_naive()
        .succ_opt()
        .ok_or("date overflow")?;
    cli_args(
        &db,
        &["edit", "2", "--due", "Tomorrow", "--description", "notes"],
    )?;
    assert_eq!(
        cli(&db, "show 2")?,
        format!(
            "2  [ ] Docs\nProject:  Work\nParent:   1  Ship v2\nDue:      {tomorrow}\n\nnotes\n"
        )
    );
    cli_args(
        &db,
        &[
            "edit",
            "1",
            "--due",
            "none",
            "--priority",
            "",
            "--description",
            "",
        ],
    )?;
    assert_eq!(
        cli(&db, "show 1")?,
        "1  [ ] Ship v2\nProject:  Work\nSubtasks: 1  (cli-todo list --parent 1)\n"
    );

    assert_eq!(error(&db, "edit 1"), "Nothing to change. Usage: cli-todo edit <id> [--title <title>] [--description <text>] [--due <date>] [--priority <1-3>]");
    assert_eq!(
        error(&db, "edit 1 --priority 4"),
        "Priority must be 1, 2, 3, or none, not '4'"
    );
    assert_eq!(
        error(&db, "edit 1 --due someday"),
        "Can't read due date 'someday'. Use YYYY-MM-DD, tomorrow, fri, next week, in 3 days, or 30 nov 2026"
    );
    assert_eq!(error(&db, "edit 1 --due 2030-02-30"), "Can't read due date '2030-02-30'. Use YYYY-MM-DD, tomorrow, fri, next week, in 3 days, or 30 nov 2026");
    // A rejected edit changes nothing, even fields that were valid.
    assert_eq!(
        error(&db, "edit 1 --title  --priority 1"),
        "Title cannot be empty"
    );
    assert_eq!(
        cli(&db, "show 1")?,
        "1  [ ] Ship v2\nProject:  Work\nSubtasks: 1  (cli-todo list --parent 1)\n"
    );
    assert_eq!(
        error(&db, "show 9"),
        "No task with id 9. Run 'cli-todo list --all'"
    );
    assert_eq!(error(&db, "edit 1 -p Work"), "Unknown option '-p'");
    Ok(())
}

#[test]
fn moves_tasks_with_subtasks_between_projects() -> Result<()> {
    let db = db::open(Path::new(":memory:"))?;
    cli(&db, "project add Work")?;
    cli(&db, "project add Home")?;
    cli(&db, "add Ship -p Work")?;
    cli(&db, "add Docs --parent 1")?;
    cli(&db, "add Draft --parent 2")?;
    assert_eq!(cli(&db, "move 1 -p Home")?, "Moved task 1 to Home: Ship\n");
    assert_eq!(cli(&db, "list -p Home")?, "   1  [ ] Ship  +1 subtask\n");
    assert_eq!(cli(&db, "show 3")?.lines().nth(1), Some("Project:  Home"));
    assert_eq!(
        cli(&db, "move 2 --inbox")?,
        "Moved task 2 to Inbox as a top-level task: Docs\n"
    );
    assert_eq!(
        cli(&db, "list")?,
        "   1  [ ] Ship  (Home)\n   2  [ ] Docs  +1 subtask\n"
    );
    assert_eq!(cli(&db, "show 3")?.lines().nth(1), Some("Project:  Inbox"));
    assert_eq!(
        error(&db, "move 1"),
        "Usage: cli-todo move <id> (-p <project> | --inbox)"
    );
    assert_eq!(
        error(&db, "move 1 -p Home --inbox"),
        "Usage: cli-todo move <id> (-p <project> | --inbox)"
    );
    assert_eq!(
        error(&db, "move 1 -p Nope"),
        "No project named 'Nope'. Run 'cli-todo project list'"
    );
    Ok(())
}

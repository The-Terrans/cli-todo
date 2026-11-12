use crate::{
    dates::{date_phrases, parse_due_date},
    db,
    types::{Filter, Project, Result, TaskDraft},
};
use chrono::Local;
use rusqlite::Connection;
use std::{collections::HashMap, io::Write};

pub const USAGE: &str = "Usage:
  cli-todo                              Open the interactive app
  cli-todo add <title> [-p <project>]   Add a task, to Inbox unless a project is given
  cli-todo add <title> --parent <id>    Add a subtask to the task with that id
      [--description <text>] [--due <date>] [--priority <1-3>]
                                        Optional fields for add and edit
  cli-todo list [-p <project>] [--all]  List pending tasks; --all includes completed
  cli-todo list --parent <id> [--all]   List the subtasks of the task with that id
  cli-todo show <id>                    Show a task's details and description
  cli-todo edit <id> [--title <title>] [--description <text>] [--due <date>] [--priority <1-3>]
                                        Change fields; \"\" or none clears due and priority
  cli-todo move <id> -p <project>       Move a task and its subtasks to a project
  cli-todo move <id> --inbox            Move a task and its subtasks to Inbox
  cli-todo done <id>                    Complete a task, or reopen a completed one
  cli-todo delete <id>                  Delete a task and its subtasks
  cli-todo project add <name> [--description <text>]
                                        Add a project
  cli-todo project describe <name> <text>
                                        Set a project's one-line description; \"\" clears
  cli-todo project list                 List projects and their descriptions
  cli-todo project rename <name> <new>  Rename a project
  cli-todo project delete <name> [--force]
                                        Delete a project; --force also deletes its tasks
  cli-todo help                         Show this help
  cli-todo -v, --version                Show the version

Dates: YYYY-MM-DD, today, tomorrow, fri, next fri, next week, next month,
       in 3 days, in 2 weeks, 30 nov 2026, 30-11-2026";

/// Options followed by a value; any other option is a flag.
const VALUE_OPTIONS: [&str; 6] = [
    "--project",
    "--parent",
    "--title",
    "--description",
    "--due",
    "--priority",
];

/// Task fields `add` and `edit` accept as options.
const FIELDS: [&str; 3] = ["--description", "--due", "--priority"];

/// Runs one non-interactive command; `args` excludes the program name.
pub fn run(db: &Connection, args: &[String], out: &mut impl Write) -> Result<()> {
    let (command, rest) = args.split_first().ok_or(USAGE)?;
    match command.as_str() {
        "add" => add(db, rest, out),
        "list" => list(db, rest, out),
        "show" => show(db, rest, out),
        "edit" => edit(db, rest, out),
        "move" => move_to(db, rest, out),
        "done" => done(db, rest, out),
        "delete" => delete(db, rest, out),
        "project" => project(db, rest, out),
        "help" | "-h" | "--help" => Ok(writeln!(out, "{USAGE}")?),
        "-v" | "--version" => Ok(writeln!(out, "cli-todo {}", env!("CARGO_PKG_VERSION"))?),
        other => Err(format!("Unknown command '{other}'. Run 'cli-todo help' for usage").into()),
    }
}

/// Splits `args` into words and the `allowed` options, mapped to their value ("" for flags).
/// `-p` is short for `--project`.
fn parse<'a>(
    args: &'a [String],
    allowed: &[&'static str],
) -> Result<(Vec<&'a str>, HashMap<&'static str, &'a str>)> {
    let mut words = vec![];
    let mut options = HashMap::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let option = if arg == "-p" {
            "--project"
        } else {
            arg.as_str()
        };
        if !option.starts_with('-') {
            words.push(option);
            continue;
        }
        let option = *allowed
            .iter()
            .find(|known| **known == option)
            .ok_or_else(|| format!("Unknown option '{arg}'"))?;
        let value = if VALUE_OPTIONS.contains(&option) {
            args.next()
                .ok_or_else(|| format!("{arg} needs a value"))?
                .as_str()
        } else {
            ""
        };
        options.insert(option, value);
    }
    Ok((words, options))
}

fn find_project(db: &Connection, name: &str) -> Result<Project> {
    db::list_projects(db)?
        .into_iter()
        .find(|project| project.name == name.trim())
        .ok_or_else(|| format!("No project named '{name}'. Run 'cli-todo project list'").into())
}

fn task_title(db: &Connection, id: &str) -> Result<(i64, String)> {
    let id: i64 = id.parse().map_err(|_| format!("'{id}' is not a task id"))?;
    let title = db
        .query_row("SELECT title FROM tasks WHERE id=?1", [id], |row| {
            row.get(0)
        })
        .map_err(|_| format!("No task with id {id}. Run 'cli-todo list --all'"))?;
    Ok((id, title))
}

fn one_word<'a>(words: &[&'a str], usage: &str) -> Result<&'a str> {
    match words {
        [word] => Ok(word),
        _ => Err(format!("Usage: cli-todo {usage}").into()),
    }
}

/// Rejects `-p` with `--parent`: subtasks always live in their parent's project.
fn project_or_parent<'a>(
    options: &HashMap<&str, &'a str>,
) -> Result<(Option<&'a str>, Option<&'a str>)> {
    match (options.get("--project"), options.get("--parent")) {
        (Some(_), Some(_)) => {
            Err("Use -p or --parent, not both; subtasks stay in their parent's project".into())
        }
        (project, parent) => Ok((project.copied(), parent.copied())),
    }
}

/// Accepts YYYY-MM-DD or one date phrase the task editor understands; "" or none clears.
fn due_date(input: &str) -> Result<String> {
    let input = input.trim();
    if input.is_empty() || input == "none" {
        return Ok(String::new());
    }
    if let Ok(Some(date)) = parse_due_date(input) {
        return Ok(date.to_string());
    }
    match date_phrases(input, Local::now().date_naive()).as_slice() {
        [phrase] if phrase.range == (0..input.len()) => Ok(phrase.date.to_string()),
        _ => Err(format!(
            "Can't read due date '{input}'. Use YYYY-MM-DD, tomorrow, fri, next week, in 3 days, or 30 nov 2026"
        )
        .into()),
    }
}

/// Accepts 1-3, optionally as P1-P3; "" or none clears.
fn priority(input: &str) -> Result<Option<u8>> {
    let input = input.trim().to_ascii_lowercase();
    match input.strip_prefix('p').unwrap_or(&input) {
        "" | "none" => Ok(None),
        "1" => Ok(Some(1)),
        "2" => Ok(Some(2)),
        "3" => Ok(Some(3)),
        _ => Err(format!("Priority must be 1, 2, 3, or none, not '{input}'").into()),
    }
}

/// Applies the `FIELDS` options to `draft`.
fn apply_fields(draft: &mut TaskDraft, options: &HashMap<&str, &str>) -> Result<()> {
    if let Some(description) = options.get("--description") {
        draft.description = description.to_string();
    }
    if let Some(due) = options.get("--due") {
        draft.due_date = due_date(due)?;
    }
    if let Some(value) = options.get("--priority") {
        draft.priority = priority(value)?;
    }
    Ok(())
}

fn add(db: &Connection, args: &[String], out: &mut impl Write) -> Result<()> {
    let (words, options) = parse(args, &[&["--project", "--parent"], &FIELDS[..]].concat())?;
    let (project, parent) = project_or_parent(&options)?;
    let mut draft = TaskDraft {
        title: words.join(" ").trim().into(),
        ..TaskDraft::default()
    };
    apply_fields(&mut draft, &options)?;
    if draft.title.is_empty() {
        return Err("Usage: cli-todo add <title> [-p <project> | --parent <id>]".into());
    }
    if let Some(parent) = parent {
        let (parent, parent_title) = task_title(db, parent)?;
        db::save_subtask_tree(db, parent, &draft)?;
        let id = db.last_insert_rowid();
        writeln!(out, "Added subtask {id} to {parent_title}: {}", draft.title)?;
        return Ok(());
    }
    let project = project.map(|name| find_project(db, name)).transpose()?;
    db::save_task_tree(db, None, &draft, project.as_ref().map(|project| project.id))?;
    let place = project.map_or("Inbox".into(), |project| project.name);
    writeln!(
        out,
        "Added task {} to {place}: {}",
        db.last_insert_rowid(),
        draft.title
    )?;
    Ok(())
}

fn list(db: &Connection, args: &[String], out: &mut impl Write) -> Result<()> {
    let (words, options) = parse(args, &["--project", "--parent", "--all"])?;
    if !words.is_empty() {
        return Err("Usage: cli-todo list [-p <project> | --parent <id>] [--all]".into());
    }
    let (project, parent) = project_or_parent(&options)?;
    let filter = if options.contains_key("--all") {
        Filter::All
    } else {
        Filter::Pending
    };
    let tasks = if let Some(parent) = parent {
        let (parent, _) = task_title(db, parent)?;
        let project = db.query_row(
            "SELECT project_id FROM tasks WHERE id=?1",
            [parent],
            |row| row.get(0),
        )?;
        let sort = db::load_sort(db, project, filter)?;
        db::subtasks_of(&db::list_tasks(db, filter)?, parent, sort)
    } else {
        let project = project
            .map(|name| find_project(db, name))
            .transpose()?
            .map(|project| project.id);
        let sort = db::load_sort(db, project, filter)?;
        db::list_tasks_in(db, filter, project, sort)?
    };
    if tasks.is_empty() {
        writeln!(out, "No tasks")?;
    }
    // Only top-level Inbox lists mix projects; elsewhere the name would repeat on every line.
    let projects = if project.is_none() && parent.is_none() {
        db::list_projects(db)?
    } else {
        vec![]
    };
    for task in tasks {
        let mark = if task.done { 'x' } else { ' ' };
        let subtasks = match task.subtask_count {
            0 => String::new(),
            1 => "  +1 subtask".into(),
            count => format!("  +{count} subtasks"),
        };
        let place = projects
            .iter()
            .find(|candidate| task.project_id == Some(candidate.id))
            .map(|candidate| format!("  ({})", candidate.name))
            .unwrap_or_default();
        writeln!(
            out,
            "{:>4}  [{mark}] {}{subtasks}{place}",
            task.id, task.title
        )?;
    }
    Ok(())
}

fn show(db: &Connection, args: &[String], out: &mut impl Write) -> Result<()> {
    let (words, _) = parse(args, &[])?;
    let (id, _) = task_title(db, one_word(&words, "show <id>")?)?;
    let task = db::list_tasks(db, Filter::All)?
        .into_iter()
        .find(|task| task.id == id)
        .ok_or("Task disappeared while reading it")?;
    let mark = if task.done { 'x' } else { ' ' };
    let project = db::list_projects(db)?
        .into_iter()
        .find(|project| task.project_id == Some(project.id))
        .map_or("Inbox".into(), |project| project.name);
    writeln!(out, "{id}  [{mark}] {}", task.title)?;
    writeln!(out, "Project:  {project}")?;
    if let (Some(parent), Some(title)) = (task.parent_id, &task.parent_title) {
        writeln!(out, "Parent:   {parent}  {title}")?;
    }
    if let Some(due) = task.due_date {
        writeln!(out, "Due:      {due}")?;
    }
    if let Some(priority) = task.priority {
        writeln!(out, "Priority: P{priority}")?;
    }
    if task.subtask_count > 0 {
        writeln!(
            out,
            "Subtasks: {}  (cli-todo list --parent {id})",
            task.subtask_count
        )?;
    }
    if !task.description.is_empty() {
        writeln!(out, "\n{}", task.description)?;
    }
    Ok(())
}

fn edit(db: &Connection, args: &[String], out: &mut impl Write) -> Result<()> {
    let (words, options) = parse(args, &[&["--title"], &FIELDS[..]].concat())?;
    let usage =
        "edit <id> [--title <title>] [--description <text>] [--due <date>] [--priority <1-3>]";
    let (id, _) = task_title(db, one_word(&words, usage)?)?;
    if options.is_empty() {
        return Err(format!("Nothing to change. Usage: cli-todo {usage}").into());
    }
    let mut draft = db::task_draft(db, id)?;
    if let Some(title) = options.get("--title") {
        draft.title = title.trim().into();
    }
    apply_fields(&mut draft, &options)?;
    // reword_task validates the title before anything is written; priority is saved separately.
    db::reword_task(db, id, &draft)?;
    db::set_priority(db, id, draft.priority)?;
    writeln!(out, "Updated task {id}: {}", draft.title)?;
    Ok(())
}

fn move_to(db: &Connection, args: &[String], out: &mut impl Write) -> Result<()> {
    let (words, options) = parse(args, &["--project", "--inbox"])?;
    let usage = "move <id> (-p <project> | --inbox)";
    let (id, title) = task_title(db, one_word(&words, usage)?)?;
    let project = match (options.get("--project"), options.contains_key("--inbox")) {
        (Some(name), false) => Some(find_project(db, name)?),
        (None, true) => None,
        _ => return Err(format!("Usage: cli-todo {usage}").into()),
    };
    let was_subtask: bool = db.query_row(
        "SELECT parent_id IS NOT NULL FROM tasks WHERE id=?1",
        [id],
        |row| row.get(0),
    )?;
    db::move_task(db, id, project.as_ref().map(|project| project.id))?;
    let place = project.map_or("Inbox".into(), |project| project.name);
    // A subtask cannot sit in another project than its parent, so moving detaches it.
    let detached = if was_subtask {
        " as a top-level task"
    } else {
        ""
    };
    writeln!(out, "Moved task {id} to {place}{detached}: {title}")?;
    Ok(())
}

fn done(db: &Connection, args: &[String], out: &mut impl Write) -> Result<()> {
    let (words, _) = parse(args, &[])?;
    let (id, title) = task_title(db, one_word(&words, "done <id>")?)?;
    db::toggle_task(db, id)?;
    let done: bool = db.query_row("SELECT done FROM tasks WHERE id=?1", [id], |row| row.get(0))?;
    let verb = if done { "Completed" } else { "Reopened" };
    writeln!(out, "{verb} task {id}: {title}")?;
    Ok(())
}

fn delete(db: &Connection, args: &[String], out: &mut impl Write) -> Result<()> {
    let (words, _) = parse(args, &[])?;
    let (id, title) = task_title(db, one_word(&words, "delete <id>")?)?;
    db::delete_task(db, id)?;
    writeln!(out, "Deleted task {id}: {title}")?;
    Ok(())
}

fn project(db: &Connection, args: &[String], out: &mut impl Write) -> Result<()> {
    let (words, options) = parse(args, &["--force", "--description"])?;
    if options.contains_key("--description") && words.first() != Some(&"add") {
        return Err("--description only works with 'project add'; use 'project describe'".into());
    }
    match words.as_slice() {
        ["add", name] => {
            let description =
                db::validate_project_description(options.get("--description").unwrap_or(&""))?;
            let transaction = db.unchecked_transaction()?;
            db::save_project(&transaction, None, name).map_err(|error| unique_name(error, name))?;
            let id = transaction.last_insert_rowid();
            db::set_project_description(&transaction, id, description)?;
            transaction.commit()?;
            writeln!(out, "Added project {}", name.trim())?;
        }
        ["describe", name, description] => {
            let project = find_project(db, name)?;
            db::set_project_description(db, project.id, description)?;
            if description.trim().is_empty() {
                writeln!(out, "Cleared description of project {}", project.name)?;
            } else {
                writeln!(out, "Described project {}", project.name)?;
            }
        }
        ["list"] => {
            let projects = db::list_projects(db)?;
            if projects.is_empty() {
                writeln!(out, "No projects")?;
            }
            for project in projects {
                if project.description.is_empty() {
                    writeln!(out, "{}", project.name)?;
                } else {
                    writeln!(out, "{} — {}", project.name, project.description)?;
                }
            }
        }
        ["rename", name, new] => {
            let project = find_project(db, name)?;
            db::save_project(db, Some(project.id), new).map_err(|error| unique_name(error, new))?;
            writeln!(out, "Renamed project {} to {}", project.name, new.trim())?;
        }
        ["delete", name] => {
            let project = find_project(db, name)?;
            let tasks: i64 = db.query_row(
                "SELECT COUNT(*) FROM tasks WHERE project_id=?1",
                [project.id],
                |row| row.get(0),
            )?;
            if tasks > 0 && !options.contains_key("--force") {
                return Err(format!(
                    "Project {} has {tasks} tasks. Add --force to delete them too",
                    project.name
                )
                .into());
            }
            db::delete_project(db, project.id)?;
            writeln!(out, "Deleted project {} and {tasks} tasks", project.name)?;
        }
        _ => {
            return Err(
                "Usage: cli-todo project add|describe|list|rename|delete. Run 'cli-todo help'"
                    .into(),
            )
        }
    }
    Ok(())
}

/// Replaces SQLite's UNIQUE constraint message with one naming the duplicate project.
fn unique_name(error: Box<dyn std::error::Error>, name: &str) -> Box<dyn std::error::Error> {
    if error.to_string().contains("UNIQUE") {
        format!("A project named '{}' already exists", name.trim()).into()
    } else {
        error
    }
}

#[cfg(test)]
#[path = "tests/cli.rs"]
mod tests;

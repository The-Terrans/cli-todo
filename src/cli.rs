use crate::{
    db,
    types::{Filter, Project, Result, TaskDraft},
};
use rusqlite::Connection;
use std::{collections::HashMap, io::Write};

pub const USAGE: &str = "Usage:
  cli-todo                              Open the interactive app
  cli-todo add <title> [-p <project>]   Add a task, to Inbox unless a project is given
  cli-todo add <title> --parent <id>    Add a subtask to the task with that id
  cli-todo list [-p <project>] [--all]  List pending tasks; --all includes completed
  cli-todo list --parent <id> [--all]   List the subtasks of the task with that id
  cli-todo done <id>                    Complete a task, or reopen a completed one
  cli-todo delete <id>                  Delete a task and its subtasks
  cli-todo project add <name>           Add a project
  cli-todo project list                 List projects
  cli-todo project rename <name> <new>  Rename a project
  cli-todo project delete <name> [--force]
                                        Delete a project; --force also deletes its tasks
  cli-todo help                         Show this help
  cli-todo -v, --version                Show the version";

/// Options followed by a value; any other option is a flag.
const VALUE_OPTIONS: [&str; 2] = ["--project", "--parent"];

/// Runs one non-interactive command; `args` excludes the program name.
pub fn run(db: &Connection, args: &[String], out: &mut impl Write) -> Result<()> {
    let (command, rest) = args.split_first().ok_or(USAGE)?;
    match command.as_str() {
        "add" => add(db, rest, out),
        "list" => list(db, rest, out),
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

fn add(db: &Connection, args: &[String], out: &mut impl Write) -> Result<()> {
    let (words, options) = parse(args, &["--project", "--parent"])?;
    let (project, parent) = project_or_parent(&options)?;
    let draft = TaskDraft {
        title: words.join(" ").trim().into(),
        ..TaskDraft::default()
    };
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
    let (words, options) = parse(args, &["--force"])?;
    match words.as_slice() {
        ["add", name] => {
            db::save_project(db, None, name).map_err(|error| unique_name(error, name))?;
            writeln!(out, "Added project {}", name.trim())?;
        }
        ["list"] => {
            let projects = db::list_projects(db)?;
            if projects.is_empty() {
                writeln!(out, "No projects")?;
            }
            for project in projects {
                writeln!(out, "{}", project.name)?;
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
                "Usage: cli-todo project add|list|rename|delete. Run 'cli-todo help'".into(),
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

---
name: cli-todo
description: Manages the user's todo list with the `cli-todo` command - add, list, complete, reopen, and delete tasks and subtasks, and add, list, rename, and delete projects. Use when the user asks to add a todo or task, check what is pending, mark something done, organize tasks into projects, or break a task into subtasks. Before reading further, run `cli-todo -v` with a 5-second timeout. Continue only if it prints `cli-todo <version>`. If the command is missing, errors, or times out, cli-todo is not installed or predates these commands, so do not read this skill and tell the user to install or update cli-todo. Do NOT run bare `cli-todo`, which opens an interactive terminal UI that blocks the agent.
---

# cli-todo

Run `cli-todo help` for the exact usage of the installed version.

## Commands

```bash
cli-todo add "Buy milk"                      # task in Inbox (no project)
cli-todo add "Ship v1" -p Work               # task in project Work
cli-todo add "Write docs" --parent 12        # subtask of task 12
cli-todo list                                # pending top-level tasks
cli-todo list --all                          # include completed tasks
cli-todo list -p Work                        # one project only
cli-todo list --parent 12 [--all]            # direct subtasks of task 12
cli-todo done 12                             # toggles: completes, or reopens if already done
cli-todo delete 12                           # deletes task 12 and all its subtasks
cli-todo project add Work
cli-todo project list
cli-todo project rename Work Job
cli-todo project delete Job [--force]        # --force required if it has tasks; deletes them
cli-todo -v                                  # version
```

## Reading output

`list` prints one task per line:

```text
  29  [ ] migrate service repos  +2 subtasks  (DT pending)
  31  [x] update README
```

- First column: task id. Use it with `done`, `delete`, and `--parent`.
- `[ ]` pending, `[x]` completed. Pending tasks come first.
- `+N subtasks`: the task has N direct subtasks, completed ones included. See them with `list --parent <id>`.
- `(Name)`: the project, shown only in the unfiltered top-level list. No suffix means Inbox.
- `No tasks` means the list is empty.

Every command prints one confirmation line, such as `Added task 42 to Work: Ship v1`. Read the new id from it instead of listing again.

On failure the command prints `cli-todo: <reason>` to stderr and exits with status 1. The reason says what to do next, for example `No project named 'Wrok'. Run 'cli-todo project list'`.

## Rules

1. **Find ids before acting.** Titles are not unique; ids are. Run `list` (add `--all` for completed tasks) and match the title, then act on the id.
2. **`done` is a toggle.** On a completed task it reopens it. Check the `[ ]`/`[x]` mark first and confirm from the output line (`Completed …` or `Reopened …`).
3. **Completing a parent completes all its subtasks.** Reopening a parent reopens only the parent.
4. **Confirm with the user before destructive commands:** `delete` on a task with subtasks, and `project delete --force`. Neither can be undone from the CLI.
5. **Project names are exact and case-sensitive.** Run `project list` when unsure. `add -p` does not create missing projects; run `project add` first.
6. **Always quote titles and names:** `add "Fix -x flag" -p "pi config"`. An unquoted word starting with `-` is read as an option and fails.
7. **Subtasks stay in their parent's project**, so `-p` and `--parent` cannot be combined. Nesting is limited to 64 levels.
8. **Ids are never reused.** A deleted id stays unused, so a stored id never points at a different task.

## Not available from the CLI

Due dates, priorities, descriptions, reordering, moving tasks between projects, and history commits or sync exist only in the interactive UI. If the user asks for one, say so and suggest they run `cli-todo` themselves.

## Data

Tasks live in `$XDG_DATA_HOME/cli-todo/tasks.sqlite3`, defaulting to `~/.local/share/cli-todo/tasks.sqlite3`. Use the commands above rather than editing the database directly. To experiment without touching the user's real list, prefix commands with `XDG_DATA_HOME=$(mktemp -d)`.

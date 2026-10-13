# cli-todo

A local Rust terminal todo app using Ratatui and SQLite. Inbox, projects, and todo commits in the left panel; task or commit previews in the right panel, keyboard-only controls.

## Run

```sh
nix develop path:./.nix
cargo run
```

If flakes are not enabled, use `nix --extra-experimental-features 'nix-command flakes' develop path:./.nix` instead. The explicit `path:` snapshots only `.nix/`, not the surrounding Git repository. First use downloads dependencies. The flake provides a development shell (not a `nix run` package), with Rust, Cargo, rustfmt, Clippy, pkg-config, and SQLite. Install Git separately for todo checkpoints. `.nix/flake.lock` pins Nix dependencies; `Cargo.lock` pins Rust dependencies.

Without Nix, install Git, a Rust toolchain, pkg-config, and SQLite development libraries, then run `cargo run`. Build without launching with `cargo build`.

## Controls

Press **?** while browsing for a scrollable list of all keybindings. **Up/Down** scroll, **PageUp/PageDown** move a page, and **Home/End** jump to the beginning/end. **?** or **Esc** closes help without changing focus. In text entry and search, `?` remains ordinary input. The app footer shows only `?: keybindings` plus any notification.

| Key                          | Action                                                   |
| ---------------------------- | -------------------------------------------------------- |
| ? while browsing             | Open keybindings help; ?/Esc closes it                    |
| Esc while browsing           | Return from Tasks to its navigation section; no effect on left           |
| 0 / 1 / 2 / 3 while browsing | Focus right preview / Inbox / Projects / Commits          |
| Left / Right in left panel   | Cycle Inbox → Projects → Commits with wraparound; left reverses                |
| Up / Down                    | Choose Inbox filter in navigation, or task in main panel |
| Enter                        | Focus task list; open task/subtask details               |
| a                            | Add task; add child inside an opened task; add project in Projects |
| r in Tasks                   | Reword task in a Title / Description / Due date popup     |
| e in Projects                | Rename project                                           |
| Space                        | Toggle selected row (opened leaf: toggle that task)       |
| x                            | Toggle opened/selected task between completed and pending |
| d                            | Request task/project deletion in its section              |
| Shift+D while browsing       | Confirm deletion of tasks, projects, local history and remote settings |
| m in Tasks                   | Move task to Inbox or a project; arrows and Enter choose  |
| c while browsing             | Commit a safe todo SQLite snapshot from any panel         |
| r in Commits                 | Set/change origin URL; empty input removes it             |
| p / P in Commits             | Push checkpoints / safely pull and apply a snapshot       |
| Enter in deletion dialog     | Confirm permanent deletion                               |
| Esc in deletion dialog       | Cancel deletion                                          |
| Enter in text entry          | Save title; in task Description, insert newline           |
| Tab / Shift+Tab in editor    | Cycle Title / Description / Due date                      |
| Ctrl+S outside Title         | Save the current task editor                              |
| Backspace in text entry      | Remove last character                                    |
| Esc in text entry            | Discard changes                                          |
| q while browsing             | Quit                                                     |
| Ctrl+K while browsing        | Open searchable command palette                          |

All deletion confirmations use only **Enter** to confirm and **Esc** to cancel; y/n and q do nothing in these dialogs.

Navigation offers All, Pending, and Completed filters. Arrows apply the filter immediately; Enter focuses its tasks. Editing is intentionally append/backspace only. Project input dialogs use 3 rows and 70% of the terminal width (minimum 60 columns, reduced to fit smaller terminals), with Enter/Esc in the bottom border; errors add one row. Long title input scrolls horizontally to keep its end visible. Long titles remain stored but may be clipped in the task list. Use a terminal at least 80 columns wide for readable help; the help panel scrolls to fit short terminals.

## Opened tasks

Press **Enter** on a task to replace the right-hand list with its title, description, status, and due date—no popup. A second **Subtasks** section appears only when children exist. Details uses one third of the right-panel height, with a 6-row minimum; Subtasks gets the rest. Small terminals shrink Details safely, retaining space for Subtasks. Without children, Details fills the panel. Within Details, the title (with completion checkbox) and date stay pinned at the top; a divider separates the scrollable Description below. This split does not change the overall detail height.

1. Details are focused first. **Left/Right** cycles Details ↔ Subtasks when children exist, like the sidebar. **Tab/Shift+Tab** also cycles these two panes. In Subtasks, **Up/Down** selects a child and **Enter** opens its details.
2. **a** opens a creation form for a child of the opened task. Saving returns to the parent view; Esc cancels.
3. **Enter** while Details is focused opens the same three-field reword popup as **r**. In Subtasks, Enter still opens the selected child's details. **x** toggles that task between completed and pending. Completing also completes every descendant; reopening leaves descendants unchanged.
4. **PageUp/PageDown** scroll Description; **Home/End** jump. Up/Down scroll Description while Details is focused. Title and date remain visible.
5. **Esc** returns to the parent, then the main task list, then the sidebar. Space toggles the selected child in Subtasks, or the opened task in Details.

## Task descriptions

**a** creates a task and **r** rewords one using the same three-field popup: **Title**, **Description**, and **Due date** only. Subtasks are managed in the right panel, not inside the popup. The popup uses 70% of the terminal width (minimum 60 columns) and 70% of its height (minimum 15 rows), centered and reduced to fit small terminals. Title and Due date stay compact; Description takes the remaining height. Title is focused initially: **Enter** saves and **Esc** cancels. **Tab** cycles Title → Description → Due date → Title; **Shift+Tab** cycles backward. Description's top-right border shows the focus hint. Rewording preserves existing subtasks and their details.

In Description, **Enter** inserts a newline and **Ctrl+S** saves the entire task. **Esc** cancels all draft changes. Only the focused field shows its save/cancel footer and cursor. Description text wraps and scrolls to keep the input end visible. Editing remains append/backspace only; descriptions are optional and preserve whitespace.

The task list shows the first nonblank description line beneath its title, indented in dim dark gray. An ellipsis indicates additional lines; press Enter for full details, or r to reword. Empty descriptions take no extra row. Terminal font size is fixed, so subdued styling provides the visual hierarchy.

Existing tasks get empty descriptions automatically. Descriptions persist across restarts and are included in Git snapshots and safe Pull. Older snapshots without descriptions remain readable and are upgraded when applied.

## Due dates

From Title, press **Tab twice** to focus **Due date**. Enter an optional **YYYY-MM-DD** date (for example `2028-02-29`); blank input clears it. **Enter** or **Ctrl+S** saves the current editor. Invalid calendar dates keep the draft open and focus the date field.

Tasks and subtasks have independent deadlines. Titles show a `[due: YYYY-MM-DD]` badge without changing task order. Unfinished tasks due **before today in your local timezone** have red badges; dates due today are not overdue, and completed tasks use muted badges. Deadlines persist in SQLite and Git snapshots; older databases/snapshots get no deadline automatically.

## Subtasks

Press **Enter** to open a task's details, then **a** to create a child using the three-field popup. Nesting supports up to 64 levels.

1. **Right** focuses Subtasks; **Up/Down** selects a child and **Enter** opens its details with Details focused. **r** rewords the opened task/subtask.
2. **Space** toggles the selected child; **x** toggles the opened task. Completing cascades to descendants; reopening affects only that task.
3. **d** deletes the opened task and descendants after Enter/Esc confirmation; Esc cancels.
4. Saving a new child persists it immediately and returns to its parent view. Esc cancels only the current popup, never previously saved children.
5. **Esc** in task details returns to the parent; keep pressing to return to the main list and sidebar.

Only top-level tasks appear in Inbox/project lists; manage children in the right panel. Titles show their direct-child count, such as `Task (2 subtasks)`, in lists and opened views. Completed children count too; tasks without children show no count. Ctrl+K searches subtasks too and opens their details directly; r rewords them. Subtasks inherit their parent's project; moving or deleting a task affects its entire subtree. Descriptions, completion state, and parent relationships persist across restarts and Git snapshots. Legacy databases and snapshots migrate automatically; safe Pull rejects cycles, missing parents, and cross-project child references.

## Projects

The compact **[2] Projects** box appears below Inbox. Press **2**, then **a** to create a project. Use arrows to select a project: its tasks preview immediately on the right. **Enter** focuses those tasks. The focused panel has a bold bright-yellow border and a black-on-yellow title, even when its list is empty. Only its row cursor is highlighted; inactive panels have dark-gray borders and no highlighted cursor. **e** renames it. Names must be nonblank and unique.

New tasks belong to the open project; press **1** to return to Inbox for unassigned tasks. In Tasks, press **m** (or search Move via Ctrl+K) to move a task between Inbox and projects. **Esc** returns through any opened task ancestry to the main list, then to its navigation section. Tab/Shift+Tab cycles Details/Subtasks in opened tasks and fields inside task popups. It remains disabled in main lists.

**Deleting a project permanently deletes all its tasks**, after explicit confirmation. Esc cancels. Existing databases migrate automatically, preserving old tasks in Inbox.

## Nuke todo data

Press **Shift+D** from any browsing panel to open a confirmation showing task, project, and local commit counts. **Enter** deletes **all tasks, projects, local Git checkpoints, and remote settings**, including tasks outside the current view. **Esc** cancels; y/n have no effect. The confirmation keys appear in the dialog's bottom border. This removes only the dedicated todo-history repository, never the app source repository or global Git configuration.

**Existing backups and the remote repository itself remain untouched.** Nuke does not create a backup automatically. If you need a fresh backup, quit and copy the live database before nuking; local checkpoints will also be deleted. Task/project deletion runs in one SQLite transaction, with history staged until that succeeds. Errors stay in the dialog; filesystem cleanup failures identify any remaining history files. After confirmation, focus returns to an empty Inbox with no commits or configured remote. Nuke is disabled while syncing, and Shift+D remains ordinary text in editing dialogs.

## Todo commits

**[3] Commits** is below Projects. It tracks todo data, not the app source or current working directory. Press **c** from any browsing panel, type a message, and press **Enter**. Closing or saving the dialog preserves the previous panel focus; `c` remains ordinary input in text entry and search. Esc cancels; blank messages are rejected. A **\*** beside Commits means there are uncommitted todo changes. The marker updates after task/project changes, when opening Commits, and on restart; it disappears after a successful checkpoint. Pressing **c** without changes opens a **“No todo changes to commit”** popup. **Enter** or **Esc** closes it and preserves the previous panel focus; no checkpoint is created. An empty database with no history is considered unchanged.

The commit message dialog follows the compact input layout: 3 rows (4 with an error), 70% width with a 60-column minimum, and Enter/Esc in the bottom-right border. Long messages scroll horizontally.

Git must have a configured author identity (`user.name` and `user.email`). Git errors stay in the dialog so you can cancel or retry; the live database is not modified by committing.

Each checkpoint uses SQLite `VACUUM INTO` to make a consistent snapshot while the app runs. Only the snapshot is committed to a dedicated repository at `$XDG_DATA_HOME/cli-todo/history/` (or `$HOME/.local/share/cli-todo/history/`). Its tracked file is `tasks.sqlite3`. All tasks and projects are included; unchanged snapshots do not create another commit. This repository is initialized on the first checkpoint or remote setup. New repositories use the main branch.

Up/Down selects a commit and immediately previews its Git details on the right: hash, author, timestamps, message, and binary file statistics. Enter or **0** focuses the details; Up/Down scrolls, Esc returns to Commits. History remains available across app restarts. There is no JSON conversion, readable task diff, automatic commit, or arbitrary historical restore action. Snapshots contain your private todo data; review before sharing the history repository.

## Remote backup and sync

Create an empty **private** GitHub repository, then press **3 → r**, enter its SSH or HTTPS URL, and press Enter. **r** also changes origin; erase the URL and save to remove it. Removing origin does not delete local checkpoints. The configured URL appears in the right preview. The remote input uses the compact layout: 3 rows (4 with an error), 70% width with a 60-column minimum, and Enter/Esc in the bottom-right border. Long URLs scroll horizontally; the title reminds you that blank input removes origin.

- **p: Push** uploads committed snapshots only, even if newer tasks are uncommitted. It never force-pushes.
- **P: Pull** fetches remote history and applies its latest SQLite snapshot only when local todo data and Git files are clean and history can fast-forward. Local-ahead history is left alone; divergent history is refused.
- Before replacement, the snapshot is checked for SQLite integrity, compatible tables, and project references. The live database is backed up to `cli-todo/backups/before-pull-<timestamp>.sqlite3`. If Git integration fails, the original database is restored from that backup.
- Git authentication must already work outside the app (SSH keys or your Git credential manager). The app does not store tokens or prompt for passwords. Do not embed tokens in the remote URL.

Sync runs in the background; progress and errors appear in the footer. You can navigate while syncing, but checkpoints, remote changes, and quitting wait for completion. Pull checks again before applying: task changes made during download, or an open editing dialog, prevent replacement. Close other app instances before pulling. A fresh empty database can pull existing history without creating a local checkpoint first. GitHub access is optional; normal task operations remain local.

## Command palette

Press **Ctrl+K**, type a task title or action name, use **Up/Down** to choose, and press **Enter**. **Esc** or **Ctrl+K** closes it. Search uses case-insensitive substring matching across all tasks, including tasks outside the current status filter. Results are labeled Action or Task. Selecting a task opens its project or Inbox with the All filter and focuses that task; use the usual shortcuts to edit, complete, or delete it.

Actions: add, edit selected task, complete/reopen selected task, delete selected task, move selected task, show All/Pending/Completed, and quit. Task actions work from either panel and are hidden when no task is selected. Delete still asks for confirmation. Ctrl+K does not interrupt editing or deletion confirmation. Typing `q` in the palette searches for Quit; Enter is required to execute it.

## Storage

Tasks save immediately to `$XDG_DATA_HOME/cli-todo/tasks.sqlite3` when `XDG_DATA_HOME` is absolute. Otherwise they use `$HOME/.local/share/cli-todo/tasks.sqlite3`. The directory is created automatically. No app accounts, tags, priorities, or dates. Network access occurs only for explicit Git Push/Pull.

Quit before copying the database for backup. Database errors exit with an error after restoring the terminal; failed changes are not reported as saved.

## Verify

Inside `nix develop path:./.nix`:

```sh
cargo fmt --check
cargo test
cargo clippy -- -D warnings
cargo build
python3 tests/smoke.py
```

Unit tests verify Push/Pull against temporary local Git remotes, including dirty-data refusal, divergence, malformed snapshots, and rollback after an injected Git failure. These checks do not contact GitHub.

The optional smoke test requires Python 3 on Unix. It uses a real pseudo-terminal and a temporary database, not your Inbox. It checks multiline descriptions, Tab/Ctrl+S saving, keybindings help, keyboard navigation, command palette search and actions, project creation/rename/deletion, task moves, SQLite checkpoints and persisted Git history, add/edit/complete/delete, confirmed nuke and cancellation, restart persistence, and terminal restoration on normal exit and an injected SQLite error.

Verified in this workspace: Nix shell (Cargo 1.98.0, Rust 1.98.1, SQLite 3.53.3), formatting, unit test, Clippy, build, and PTY smoke test passed.

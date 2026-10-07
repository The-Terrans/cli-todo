# cli-todo

A local Rust terminal todo app using Ratatui and SQLite. Inbox and projects in the left panel, tasks in the right panel, keyboard-only controls.

## Run

```sh
nix develop path:./.nix
cargo run
```

If flakes are not enabled, use `nix --extra-experimental-features 'nix-command flakes' develop path:./.nix` instead. The explicit `path:` snapshots only `.nix/`, not the surrounding Git repository. First use downloads dependencies. The flake provides a development shell (not a `nix run` package), with Rust, Cargo, rustfmt, Clippy, pkg-config, and SQLite. `.nix/flake.lock` pins Nix dependencies; `Cargo.lock` pins Rust dependencies.

Without Nix, install a Rust toolchain, pkg-config, and SQLite development libraries, then run `cargo run`. Build without launching with `cargo build`.

## Controls

| Key                          | Action                                                   |
| ---------------------------- | -------------------------------------------------------- |
| Esc while browsing           | Return from Tasks to its navigation section; no effect on left           |
| 0 / 1 / 2 while browsing     | Focus Tasks / Inbox / Projects directly                   |
| Left / Right in left panel   | Cycle Inbox ↔ Projects with wraparound; no effect in Tasks                |
| Up / Down                    | Choose Inbox filter in navigation, or task in main panel |
| Enter                        | Open Inbox filter / edit selected task                   |
| a                            | Add task; in Projects section, add project                |
| e                            | Edit task; in Projects section, rename project            |
| Space                        | Toggle completion in main panel                          |
| d                            | Request task/project deletion in its section              |
| m in Tasks                   | Move task to Inbox or a project; arrows and Enter choose  |
| y / Enter in deletion dialog | Confirm permanent deletion                               |
| n / Esc in deletion dialog   | Cancel deletion                                          |
| Enter in text entry          | Save title (empty/whitespace-only titles rejected)       |
| Backspace in text entry      | Remove last character                                    |
| Esc in text entry            | Discard changes                                          |
| q outside text entry/palette | Quit                                                     |
| Ctrl+K while browsing        | Open searchable command palette                          |

Navigation offers All, Pending, and Completed filters. Arrows apply the filter immediately; Enter focuses its tasks. Editing is intentionally append/backspace only. Long titles remain stored but may be clipped by terminal width. Use a terminal at least 80 columns wide for the full help text.

## Projects

The compact **[2] Projects** box appears below Inbox. Press **2**, then **a** to create a project. Use arrows to select a project: its tasks preview immediately on the right. **Enter** focuses those tasks. Only the focused section's cursor is highlighted; other sections retain selection without highlighting. **e** renames it. Names must be nonblank and unique.

New tasks belong to the open project; press **1** to return to Inbox for unassigned tasks. In Tasks, press **m** (or search Move via Ctrl+K) to move a task between Inbox and projects. **Esc** from Tasks returns to its navigation section. Tab remains disabled.

**Deleting a project permanently deletes all its tasks**, after explicit confirmation. Esc/n cancels. Existing databases migrate automatically, preserving old tasks in Inbox.

## Command palette

Press **Ctrl+K**, type a task title or action name, use **Up/Down** to choose, and press **Enter**. **Esc** or **Ctrl+K** closes it. Search uses case-insensitive substring matching across all tasks, including tasks outside the current status filter. Results are labeled Action or Task. Selecting a task opens its project or Inbox with the All filter and focuses that task; use the usual shortcuts to edit, complete, or delete it.

Actions: add, edit selected task, complete/reopen selected task, delete selected task, move selected task, show All/Pending/Completed, and quit. Task actions work from either panel and are hidden when no task is selected. Delete still asks for confirmation. Ctrl+K does not interrupt editing or deletion confirmation. Typing `q` in the palette searches for Quit; Enter is required to execute it.

## Storage

Tasks save immediately to `$XDG_DATA_HOME/cli-todo/tasks.sqlite3` when `XDG_DATA_HOME` is absolute. Otherwise they use `$HOME/.local/share/cli-todo/tasks.sqlite3`. The directory is created automatically. No network, accounts, tags, priorities, or dates.

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

The optional smoke test requires Python 3 on Unix. It uses a real pseudo-terminal and a temporary database, not your Inbox. It checks keyboard navigation, command palette search and actions, project creation/rename/deletion, task moves, add/edit/complete/delete, cancellation, restart persistence, and terminal restoration on normal exit and an injected SQLite error.

Verified in this workspace: Nix shell (Cargo 1.98.0, Rust 1.98.1, SQLite 3.53.3), formatting, unit test, Clippy, build, and PTY smoke test passed.

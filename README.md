# cli-todo

A local Rust terminal todo app using Ratatui and SQLite. One Inbox, two panels, keyboard-only controls.

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
| Esc while browsing           | Return from Tasks to Inbox; no effect in Inbox           |
| 0 / 1 while browsing         | Focus Tasks / Inbox directly                             |
| Up / Down                    | Choose Inbox filter in navigation, or task in main panel |
| Enter                        | Open Inbox filter / edit selected task                   |
| a                            | Add a task from either panel                             |
| e                            | Edit selected task in main panel                         |
| Space                        | Toggle completion in main panel                          |
| d                            | Request deletion in main panel                           |
| y / Enter in deletion dialog | Confirm permanent deletion                               |
| n / Esc in deletion dialog   | Cancel deletion                                          |
| Enter in text entry          | Save title (empty/whitespace-only titles rejected)       |
| Backspace in text entry      | Remove last character                                    |
| Esc in text entry            | Discard changes                                          |
| q outside text entry/palette | Quit                                                     |
| Ctrl+K while browsing        | Open searchable command palette                          |

Navigation offers All, Pending, and Completed filters. Arrows apply the filter immediately; Enter focuses its tasks. Editing is intentionally append/backspace only. Long titles remain stored but may be clipped by terminal width. Use a terminal at least 80 columns wide for the full help text.

## Command palette

Press **Ctrl+K**, type a task title or action name, use **Up/Down** to choose, and press **Enter**. **Esc** or **Ctrl+K** closes it. Search uses case-insensitive substring matching across all tasks, including tasks outside the current status filter. Results are labeled Action or Task. Selecting a task switches to All and focuses that task; use the usual shortcuts to edit, complete, or delete it.

Actions: add, edit selected task, complete/reopen selected task, delete selected task, show All/Pending/Completed, and quit. Task actions work from either panel and are hidden when no task is selected. Delete still asks for confirmation. Ctrl+K does not interrupt editing or deletion confirmation. Typing `q` in the palette searches for Quit; Enter is required to execute it.

## Storage

Tasks save immediately to `$XDG_DATA_HOME/cli-todo/tasks.sqlite3` when `XDG_DATA_HOME` is absolute. Otherwise they use `$HOME/.local/share/cli-todo/tasks.sqlite3`. The directory is created automatically. No network, accounts, project management, tags, priorities, or dates.

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

The optional smoke test requires Python 3 on Unix. It uses a real pseudo-terminal and a temporary database, not your Inbox. It checks keyboard navigation, command palette search and actions, add/edit/complete/delete, cancellation, restart persistence, and terminal restoration on normal exit and an injected SQLite error.

Verified in this workspace: Nix shell (Cargo 1.98.0, Rust 1.98.1, SQLite 3.53.3), formatting, unit test, Clippy, build, and PTY smoke test passed.

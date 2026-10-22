# cli-todo

A local Rust terminal todo app using Ratatui and SQLite. Inbox, projects, and todo commits in the left panel; task or commit previews in the right panel, keyboard-only controls.

## Install with Nix

After the root flake is pushed, run from the default branch:

```sh
nix run github:The-Terrans/cli-todo
```

Or install into your Nix profile:

```sh
nix profile install github:The-Terrans/cli-todo
cli-todo
```

Nix builds the app from source unless an exact matching build is available in your configured caches. Dependencies can be downloaded from Nix's standard cache. Linux and macOS, x86_64 and ARM64, are supported. Use `github:The-Terrans/cli-todo/vVERSION` to select a release tag containing the root flake; the bare URL selects the default branch, not the latest release. Install Git separately for checkpoints and sync.

### Home Manager

Add the input to your existing flake and include `cli-todo` in its `outputs` arguments:

```nix
inputs.cli-todo.url = "github:The-Terrans/cli-todo";
```

With that input in scope, register the overlay in your Home Manager modules:

```nix
nixpkgs.overlays = [ cli-todo.overlays.default ];
```

Then use the package normally in `home.nix`:

```nix
home.packages = with pkgs; [ cli-todo ];
```

If Home Manager uses `useGlobalPkgs`, register the overlay in your NixOS/nix-darwin configuration instead. Alternatively, without an overlay, use `cli-todo.packages.${pkgs.stdenv.hostPlatform.system}.default` in `home.packages`. Your configuration's `flake.lock` pins the input; run `nix flake update cli-todo` and apply Home Manager to upgrade. The overlay builds against your own Nixpkgs; standalone packages use the root lock's Nixpkgs 26.05 pin for Intel macOS compatibility.

## Install a prebuilt release (without Nix)

### Quick install or update

After `install.sh` is pushed, download it and run it:

```sh
curl --proto '=https' --proto-redir '=https' -fsSL https://raw.githubusercontent.com/The-Terrans/cli-todo/main/install.sh -o install.sh
sh install.sh
```

You can inspect `install.sh` before running it. **Run the same commands again to update** to the latest published non-prerelease version. The script detects Linux/macOS and x86_64/ARM64, verifies the archive against the release's SHA-256 checksum, and atomically replaces `~/.local/bin/cli-todo`. No sudo is needed. Task databases, history, backups, and settings are untouched; restart any running app to use the new binary. A failed download or verification keeps the old binary intact. Checksums detect corruption but are not an independent signature of the publisher.

Choose an exact release tag or custom directory:

```sh
sh install.sh --version v0.1.0
sh install.sh --dir "$HOME/bin"
```

Use the exact tag shown on GitHub (`0.1.0` and `v0.1.0` are different). `--version` can also install an older release; backwards database compatibility is not guaranteed. Use the same `--dir` on future updates, or set `CLI_TODO_INSTALL_DIR`. The script prints a reminder if the directory is missing from PATH or another installation takes precedence. It refuses to overwrite symlinks; update Nix/Home Manager-managed installations through Nix instead. Updates are explicit, not automatic background checks.

Requires `curl`, `tar`, and either `sha256sum` (Linux) or `shasum` (macOS), plus standard shell tools. A release must have finished uploading its binary and `SHA256SUMS` assets.

### Manual download

Download your platform's `cli-todo-TAG-TARGET.tar.gz` from [Releases](https://github.com/The-Terrans/cli-todo/releases), extract it, and put `cli-todo` on your PATH. Linux releases are static musl binaries; SQLite is bundled on every platform. `SHA256SUMS` covers all four binary archives. Install Git separately for checkpoints and sync.

### Publish a release

1. Update the package version in `Cargo.toml` and refresh `Cargo.lock` with `cargo check`.
2. Commit and push the version and workflow files; tag that commit (for example `0.1.0` or `v0.1.0`).
3. Publish a GitHub release for that tag. The **Release binaries** workflow tests and builds all four platforms, then attaches the binary archives and checksums. Nix users build directly from the repository flake; no generated flake archive is needed.

The workflow does not validate release tags against `Cargo.toml`. Draft releases do not build until published. Keep GitHub **immutable releases disabled**: this workflow attaches assets after publication. It uses the repository's built-in `GITHUB_TOKEN` with write access only in the publish job; no extra secret is needed. If Actions permissions are restricted by your organization, allow release asset uploads. Rerunning a successful workflow replaces its assets; prefer a new version rather than changing an existing release to keep published downloads stable.

## Develop from source

```sh
nix-shell
cargo run
```

`shell.nix` provides rustfmt, Clippy, pkg-config, and SQLite without requiring flakes. Install Rust, Cargo, and Git separately. First use downloads dependencies. The nixpkgs commit in `shell.nix` pins the development tools; `Cargo.lock` pins Rust dependencies. The root `flake.nix` and `flake.lock` are used for packaging, not the development shell.

Without Nix, install Git, a Rust toolchain, pkg-config, and SQLite development libraries, then run `cargo run`. Build without launching with `cargo build`.

## Controls

Press **?** while browsing for a scrollable list of all keybindings. **Up/Down** scroll, **PageUp/PageDown** move a page, and **Home/End** jump to the beginning/end. **?** or **Esc** closes help without changing focus. In text entry and search, `?` remains ordinary input. The app footer shows only `?: keybindings` plus any notification.

| Key                          | Action                                                   |
| ---------------------------- | -------------------------------------------------------- |
| ? while browsing             | Open keybindings help; ?/Esc closes it                    |
| Tab / Shift+Tab while browsing | Switch left/right panel focus, preserving position      |
| Esc while browsing           | Return from Tasks to its navigation section; no effect on left           |
| 0 / 1 / 2 / 3 while browsing | Focus right preview / Inbox / Projects / Commits          |
| Left / Right in left panel   | Cycle Inbox → Projects → Commits with wraparound; left reverses                |
| Up / Down                    | Choose Inbox filter in navigation, or task in main panel |
| Enter                        | Focus task list; open task/subtask details               |
| a                            | Add task; add child inside an opened task; add project in Projects |
| e in Tasks                   | Edit task in a Title / Description / Due date popup       |
| e in Projects                | Rename project                                           |
| Space                        | Toggle selected row (opened leaf: toggle that task)       |
| x                            | Toggle opened/selected task between completed and pending |
| d                            | Request task/project deletion in its section              |
| Shift+D while browsing       | Confirm deletion of tasks, projects, local history and remote settings |
| m in Tasks                   | Move task to a project or No project; arrows and Enter choose |
| Alt+J / Alt+K in Tasks       | Move selected task or focused subtask down/up; order is saved |
| c while browsing             | Commit a safe todo SQLite snapshot from any panel         |
| r in Commits                 | Set/change origin URL; empty input removes it             |
| p / P in Commits             | Push checkpoints / safely pull and apply a snapshot       |
| Enter in deletion dialog     | Confirm permanent deletion                               |
| Esc in deletion dialog       | Cancel deletion                                          |
| Enter in text entry          | Save title; in task Description, insert newline           |
| Tab / Shift+Tab in editor    | Cycle Title / Description / Due date                      |
| Ctrl+S outside Title         | Save the current task editor                              |
| Ctrl+G in task editor        | Edit the whole draft in an external editor                |
| Left / Right in editors      | Move the cursor; typing inserts at its position           |
| Up / Down in Description     | Move between explicit newline-separated lines             |
| Home / End in editors        | Move to the beginning/end of the current line             |
| Backspace / Delete in editors | Delete the character before/after the cursor             |
| Esc in text entry            | Discard changes                                          |
| q while browsing             | Quit                                                     |
| Ctrl+K while browsing        | Open searchable command palette                          |

All deletion confirmations use only **Enter** to confirm and **Esc** to cancel; y/n and q do nothing in these dialogs.

Navigation offers All, Pending, and Completed filters. Inbox lists tasks from every project plus tasks with no project, in your manual order; project tasks show a `[Project]` tag. Arrows apply the filter immediately; Enter focuses its tasks. Input dialogs support Left/Right, Home/End, insertion, Backspace, and Delete. Search retains its existing append/backspace input and arrow selection. Project input dialogs use 3 rows and 70% of the terminal width (minimum 60 columns, reduced to fit smaller terminals), with Enter/Esc in the bottom border; errors add one row. Long title input scrolls horizontally to keep the cursor visible. Long titles remain stored but may be clipped in the task list. Use a terminal at least 80 columns wide for readable help; the help panel scrolls to fit short terminals.

## Opened tasks

Press **Enter** on a task to replace the right-hand list with its title, description, status, and due date—no popup. The **Task details** header shows the task's project, like the task list. A second **Subtasks** section appears only when children exist. Details uses one third of the right-panel height, with a 6-row minimum (7 for subtasks); Subtasks gets the rest. Small terminals shrink Details safely, retaining space for Subtasks. Without children, Details fills the panel. Within Details, the title (with completion checkbox), a **Parent:** line naming the parent task (subtasks only), and date stay pinned at the top; a divider separates the scrollable Description below. This split does not change the overall detail height.

1. Details are focused first. **Left/Right** cycles Details ↔ Subtasks when children exist, like the sidebar. **Tab/Shift+Tab** switches between the whole left and right panels, preserving the opened task, selected child, scroll position, and Details/Subtasks focus. In Subtasks, **Up/Down** selects a child and **Enter** opens its details.
2. **a** opens a creation form for a child of the opened task. Saving returns to the parent view; Esc cancels.
3. **Enter** while Details is focused opens the same three-field edit popup as **e**. In Subtasks, Enter still opens the selected child's details. **x** toggles that task between completed and pending. Completing also completes every descendant; reopening leaves descendants unchanged.
4. **PageUp/PageDown** scroll Description; **Home/End** jump. Up/Down scroll Description while Details is focused. Title and date remain visible.
5. **Esc** returns to the parent, then the main task list, then the sidebar. Space toggles the selected child in Subtasks, or the opened task in Details.

## Task descriptions

**a** creates a task and **e** edits one using the same three-field popup: **Title**, **Description**, and **Due date** only. Subtasks are managed in the right panel, not inside the popup. The popup uses 70% of the terminal width (minimum 60 columns) and 70% of its height (minimum 15 rows), centered and reduced to fit small terminals. Title and Due date stay compact; Description takes the remaining height. Title is focused initially: **Enter** saves and **Esc** cancels. **Tab** cycles Title → Description → Due date → Title; **Shift+Tab** cycles backward. Description's top-right border shows the focus hint. Editing preserves existing subtasks and their details.

In Description, **Enter** inserts a newline and **Ctrl+S** saves the entire task. **Esc** cancels all draft changes. Only the focused field shows its save/cancel footer and cursor. Description text wraps and scrolls to keep the cursor visible. **Left/Right** moves through text; **Up/Down** moves between explicit newline-separated lines, not soft-wrapped display rows, preserving the horizontal position where possible. **Home/End** moves to the current line's edges. Typing and Enter insert at the cursor; **Backspace/Delete** removes the character before/after it. Each field starts at its text end and remembers its cursor when switching with Tab/Shift+Tab. Editors and command-palette search use a solid native terminal block cursor, not an inserted glyph: moving it does not push letters or change word wrapping. The viewport scrolls only when needed to keep the cursor visible. Browsing hides it; exiting resets the cursor style to the terminal default. Cursor appearance depends on terminal support. Descriptions are optional and preserve whitespace.

The task list shows the first nonblank description line beneath its title, indented in dim dark gray. An ellipsis indicates additional lines; press Enter for full details, or e to edit. Empty descriptions take no extra row. Terminal font size is fixed, so subdued styling provides the visual hierarchy.

Existing tasks get empty descriptions automatically. Descriptions persist across restarts and are included in Git snapshots and safe Pull. Older snapshots without descriptions remain readable and are upgraded when applied.

## External editor

1. Set **VISUAL** or **EDITOR** to an installed editor, for example `export EDITOR=nvim` or `export VISUAL='code --wait'`. The first nonempty setting wins: VISUAL, then EDITOR. There is no fallback editor or automatic installation.
2. Open a task creation/edit popup and press **Ctrl+G** from any field.
3. Edit the private temporary file, preserving its two headers and blank separator:

   ```text
   Title: Buy groceries
   Due date: YYYY-MM-DD

   Milk
   Bread
   ```

4. Save and exit the external editor. Changes return to the popup **without saving to the database**. Use the app's usual save controls, or Esc to discard.

When no date is set, the template shows `Due date: YYYY-MM-DD`. Existing dates remain unchanged in the template. Leave the placeholder unchanged or clear it for no deadline; replace it with a valid date such as `2028-02-29` to set one. Title must be nonblank. The description is everything after the blank separator, including its whitespace and final newline. Imported date phrases stay literal: the Due date header is authoritative. Subtasks, completion, project, and parent relationships are not edited by this file.

Editor settings accept quoted executable paths and arguments, but never run through a shell: variables, pipes, and command substitutions are not expanded. Choose an editor/option that waits until editing finishes; a GUI editor that exits immediately cannot return later changes.

Ctrl+G works only inside task/subtask popups, not browsing, search, or project/commit/remote dialogs. The app temporarily releases the terminal and restores its UI and block cursor afterward. A missing editor setting leaves the draft untouched. A failed editor or invalid file also leaves the draft untouched and retains the temporary file; the error starts with its recovery path. Successful imports remove the temporary directory. Recovery files contain todo text—remove them manually when no longer needed. On Linux/macOS, temporary directories and files are created with owner-only permissions (700/600).

## Due dates

From Title, press **Tab twice** to focus **Due date**. Enter an optional **YYYY-MM-DD** date (for example `2028-02-29`); blank input clears it. **Enter** or **Ctrl+S** saves the current editor. Invalid calendar dates keep the draft open and focus the date field.

While typing, recognized English date phrases in **Title or Description** are highlighted in yellow with an underline and immediately fill Due date, **replacing its existing value**. Saving removes only the active highlighted phrase from the text. Dates are relative to your local day. Supported phrases:

- `today` / `tod`, `tomorrow` / `tom`
- `next week` (+7 days), `next month` (same day next calendar month, clamped to month end), `next Monday` (or another weekday; full/abbreviated names)
- `in 3 days`, `in 2 weeks` (any nonnegative whole number)
- `12 oct 2026`, `12 October 2026`, `2026-10-12`, `12-12-2000` (**DD-MM-YYYY**)

For example, `do chores tomorrow` saves as `do chores` with tomorrow's deadline. Matching uses whole words and is case-insensitive; invalid dates stay as ordinary text. The first non-dismissed phrase wins, checking Title before Description. Saving a date-only title is refused without discarding your draft.

**Esc** clears an active automatic date and its highlight, leaves the keyword as normal text, and keeps the popup open. Esc again cancels the popup. **Backspace/Delete at the keyword** deletes a character and clears the automatic date/highlight; deleting unrelated text keeps the guess. Neither deletion key creates a new guess from a shortened alias. Moving the cursor does not change automatic dates. Dismissed keywords stay normal text for the current edit and are not stripped on save. Clearing a guess leaves Due date blank, not its old manual value. Editing Due date manually also dismisses detection without erasing your manual input. Ordinary words such as `tomatoes` do not replace a manual date once typing is complete.

Tasks and subtasks have independent deadlines. Titles show a `[due: YYYY-MM-DD]` badge without changing task order. Unfinished tasks due **before today in your local timezone** have red badges; dates due today are not overdue, and completed tasks use muted badges. Deadlines persist in SQLite and Git snapshots; older databases/snapshots get no deadline automatically.

## Subtasks

Press **Enter** to open a task's details, then **a** to create a child using the three-field popup. Nesting supports up to 64 levels.

1. **Right** focuses Subtasks; **Up/Down** selects a child and **Enter** opens its details with Details focused. **e** edits the opened task/subtask.
2. **Space** toggles the selected child; **x** toggles the opened task. Completing cascades to descendants; reopening affects only that task.
3. **d** deletes the highlighted child when Subtasks is focused, or the opened task when Details is focused, including the target's descendants. Enter confirms; Esc cancels. Deleting a child preserves its parent and siblings.
4. Saving a new child persists it immediately and returns to its parent view. Esc cancels only the current popup, never previously saved children.
5. **Esc** in task details returns to the parent; keep pressing to return to the main list and sidebar.

Only top-level tasks appear in Inbox/project lists; manage children in the right panel. Titles show their direct-child count, such as `Task (2 subtasks)`, in lists and opened views. Completed children count too; tasks without children show no count. Ctrl+K searches subtasks too and opens their details directly; e edits them. Subtasks inherit their parent's project; moving or deleting a task affects its entire subtree. Descriptions, completion state, and parent relationships persist across restarts and Git snapshots. Legacy databases and snapshots migrate automatically; safe Pull rejects cycles, missing parents, and cross-project child references.

## Projects

The compact **[2] Projects** box appears below Inbox. Press **2**, then **a** to create a project. Use arrows to select a project: its tasks preview immediately on the right. **Enter** focuses those tasks. The focused panel has a bold bright-yellow border and a black-on-yellow title, even when its list is empty. Only its row cursor is highlighted; inactive panels have dark-gray borders and no highlighted cursor. **e** renames it. Names must be nonblank and unique.

New tasks belong to the open project; tasks added from Inbox have no project. In Tasks, press **m** (or search Move via Ctrl+K) to move a task to a project or to **No project**. Alt+J/Alt+K in Inbox reorders the shared list; a project's own order changes only if the active filter hides its tasks between the swapped pair. **Esc** returns through any opened task ancestry to the main list, then to its navigation section. Tab/Shift+Tab switches left/right panel focus throughout browsing, including main lists, opened tasks, and Commits. Switching back resumes the same position unless sidebar navigation changes the displayed content. Inside task popups, Tab/Shift+Tab still cycles editor fields.

**Deleting a project permanently deletes all its tasks**, after explicit confirmation. Esc cancels. Existing databases migrate automatically, preserving old tasks with no project.

## Nuke todo data

Press **Shift+D** from any browsing panel to open a confirmation showing task, project, and local commit counts. **Enter** deletes **all tasks, projects, local Git checkpoints, and remote settings**, including tasks outside the current view. **Esc** cancels; y/n have no effect. The confirmation keys appear in the dialog's bottom border. This removes only the dedicated todo-history repository, never the app source repository or global Git configuration.

**Existing backups and the remote repository itself remain untouched.** Nuke does not create a backup automatically. If you need a fresh backup, quit and copy the live database before nuking; local checkpoints will also be deleted. Task/project deletion runs in one SQLite transaction, with history staged until that succeeds. Errors stay in the dialog; filesystem cleanup failures identify any remaining history files. After confirmation, focus returns to an empty Inbox with no commits or configured remote. Nuke is disabled while syncing, and Shift+D remains ordinary text in editing dialogs.

## Todo commits

**[3] Commits** is below Projects. It tracks todo data, not the app source or current working directory. Press **c** from any browsing panel, type a message, and press **Enter**. Closing or saving the dialog preserves the previous panel focus; `c` remains ordinary input in text entry and search. Esc cancels; blank messages are rejected. A **\*** beside Commits means there are uncommitted todo changes. The marker updates after task/project changes, when opening Commits, and on restart; it disappears after a successful checkpoint. Pressing **c** without changes opens a **“No todo changes to commit”** popup. **Enter** or **Esc** closes it and preserves the previous panel focus; no checkpoint is created. An empty database with no history is considered unchanged.

The commit message dialog follows the compact input layout: 3 rows (4 with an error), 70% width with a 60-column minimum, and Enter/Esc in the bottom-right border. Long messages scroll horizontally.

Git must have a configured author identity (`user.name` and `user.email`). Git errors stay in the dialog so you can cancel or retry; the live database is not modified by committing.

Each checkpoint uses SQLite `VACUUM INTO` to make a consistent snapshot while the app runs. Only the snapshot is committed to a dedicated repository at `$XDG_DATA_HOME/cli-todo/history/` (or `$HOME/.local/share/cli-todo/history/`). Its tracked file is `tasks.sqlite3`. All tasks and projects are included; unchanged snapshots do not create another commit. This repository is initialized on the first checkpoint or remote setup. New repositories use the main branch.

Up/Down selects a commit and immediately previews its Git details on the right: hash, author, timestamps, message, and binary file statistics. Enter or **0** focuses the details; Up/Down scrolls, Esc returns to Commits. History remains available across app restarts. There is no JSON conversion, readable task diff, automatic commit, or arbitrary historical restore action. Snapshots contain your private todo data; review before sharing the history repository.

## Reset todo history without deleting todos

1. Press **Ctrl+K**, search **Reset todo history**, and press **Enter**.
2. Review the warning. **Enter** permanently deletes local checkpoints and disconnects the old remote; **Esc** cancels. No new backup is created.
3. Press **3 → r** to connect an empty private repository.
4. Press **c** to create a fresh first checkpoint, then **p** in Commits to push it.

All todos, subtasks, descriptions, due dates, completion states, and projects are preserved. Existing backups and the old remote repository are untouched. Reset leaves zero local checkpoints and is blocked during sync. Do not use **Shift+D** for this—it deletes todos too. Close other app instances before resetting history.

## Remote backup and sync

Create an empty **private** GitHub repository, then press **3 → r**, enter its SSH or HTTPS URL, and press Enter. **r** also changes origin; erase the URL and save to remove it. Removing origin does not delete local checkpoints. The configured URL appears in the right preview. The remote input uses the compact layout: 3 rows (4 with an error), 70% width with a 60-column minimum, and Enter/Esc in the bottom-right border. Long URLs scroll horizontally; the title reminds you that blank input removes origin.

- **p: Push** uploads committed snapshots only, even if newer tasks are uncommitted. It never force-pushes.
- **P: Pull** fetches remote history and applies its latest SQLite snapshot only when local todo data and Git files are clean and history can fast-forward. Local-ahead history is left alone; divergent history is refused.
- Before replacement, the snapshot is checked for SQLite integrity, compatible tables, and project references. The live database is backed up to `cli-todo/backups/before-pull-<timestamp>.sqlite3`. If Git integration fails, the original database is restored from that backup.
- Git authentication must already work outside the app (SSH keys or your Git credential manager). The app does not store tokens or prompt for passwords. Do not embed tokens in the remote URL.

Sync runs in the background; progress and errors appear in the footer. You can navigate while syncing, but checkpoints, remote changes, and quitting wait for completion. Pull checks again before applying: task changes made during download, or an open editing dialog, prevent replacement. Close other app instances before pulling. A fresh empty database can pull existing history without creating a local checkpoint first. GitHub access is optional; normal task operations remain local.

## Command palette

Press **Ctrl+K**, type a task title or action name, use **Up/Down** to choose, and press **Enter**. **Esc** or **Ctrl+K** closes it. Search uses case-insensitive substring matching across all tasks, including tasks outside the current status filter. Results are labeled Action or Task. Selecting a task opens its project or Inbox with the All filter and focuses that task; use the usual shortcuts to edit, complete, or delete it.

Actions: add, edit selected task, complete/reopen selected task, delete selected task, move selected task, show All/Pending/Completed, reset todo history, and quit. Task actions work from either panel and are hidden when no task is selected. Delete still asks for confirmation. Ctrl+K does not interrupt editing or deletion confirmation. Typing `q` in the palette searches for Quit; Enter is required to execute it.

## Storage

Tasks save immediately to `$XDG_DATA_HOME/cli-todo/tasks.sqlite3` when `XDG_DATA_HOME` is absolute. Otherwise they use `$HOME/.local/share/cli-todo/tasks.sqlite3`. The directory is created automatically. No app accounts, tags, priorities, or dates. Network access occurs only for explicit Git Push/Pull.

Quit before copying the database for backup. Database errors exit with an error after restoring the terminal; failed changes are not reported as saved.

## Verify

Inside `nix-shell`:

```sh
cargo fmt --check
cargo test
cargo clippy -- -D warnings
cargo build
python3 tests/smoke.py
python3 tests/external_editor.py
```

Unit tests verify Push/Pull against temporary local Git remotes, including dirty-data refusal, divergence, malformed snapshots, and rollback after an injected Git failure. These checks do not contact GitHub.

The optional smoke test requires Python 3 on Unix. It uses a real pseudo-terminal and a temporary database, not your Inbox. It checks multiline descriptions, Tab/Ctrl+S saving, keybindings help, keyboard navigation, command palette search and actions, project creation/rename/deletion, task moves, SQLite checkpoints and persisted Git history, add/edit/complete/delete, confirmed nuke and cancellation, restart persistence, and terminal restoration on normal exit and an injected SQLite error.

Run the checks above to verify your local toolchain and development environment.

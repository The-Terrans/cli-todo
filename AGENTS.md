# Instructions

## Nix shell

Run build, test, and lint commands through `nix-shell` unless `$IN_NIX_SHELL` is already set:

- `nix-shell --run 'cargo test'`
- `nix-shell --run 'cargo clippy --all-targets -- -D warnings'`
- `nix-shell --run 'python3 tests/smoke.py'`

Plain `cargo` fails to link without the shell (`cannot find -lsqlite3`).

## Commit messages

Use Conventional Commits for every commit message:

```text
<type>[optional scope]: <description>
```

Common types: `feat`, `fix`, `refactor`, `docs`, `test`, `chore`.
Keep descriptions concise and imperative. Mark breaking changes with `!` after the type or scope.

Examples:

```text
feat: add command palette
fix(ui): restore terminal after errors
docs: document keyboard shortcuts
```

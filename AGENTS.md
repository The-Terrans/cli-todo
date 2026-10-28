# Instructions

## Nix shell

Run build, test, and lint commands through `nix develop -f shell.nix` unless `$IN_NIX_SHELL` is already set:

- `nix develop -f shell.nix -c cargo test`
- `nix develop -f shell.nix -c cargo clippy --all-targets -- -D warnings`
- `nix develop -f shell.nix -c python3 tests/smoke.py`

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

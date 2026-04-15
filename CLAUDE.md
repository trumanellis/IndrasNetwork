# Claude Code Instructions for IndrasNetwork

## Working Directory

**Always run commands from the repository root directory** (`/Users/truman/Code/IndrasNetwork`).

Do not `cd` into subdirectories to run commands. Use full paths or package flags instead:

```bash
# Good - run from root
cargo build -p indras-home-viewer
cargo test -p indras-network

# Bad - don't cd into subdirectories
cd crates/indras-home-viewer && cargo build
```

## Scripts for Complex Commands

When a command requires multiple flags or environment variables, create a simple shell script in `scripts/` with sensible defaults rather than asking the user to remember complex invocations.

### Example: Running Lua Scenarios

Instead of:
```bash
cd simulation/scripts && STRESS_LEVEL=quick cargo run --bin lua_runner -- scenarios/sync_engine_home_realm_stress.lua | cargo run -p indras-home-viewer
```

Create `scripts/run-home-viewer.sh`:
```bash
#!/bin/bash
STRESS_LEVEL="${STRESS_LEVEL:-quick}" cargo run --bin lua_runner --manifest-path simulation/Cargo.toml -- scripts/scenarios/sync_engine_home_realm_stress.lua | cargo run -p indras-home-viewer -- "$@"
```

Then the user can simply run:
```bash
./scripts/run-home-viewer.sh
./scripts/run-home-viewer.sh -m A  # with member filter
```

### Script Guidelines

1. **Location**: Place scripts in `scripts/` directory
2. **Defaults**: Use environment variable defaults (`${VAR:-default}`)
3. **Pass-through args**: Forward `"$@"` to the main command
4. **Executable**: Make scripts executable with `chmod +x`
5. **Documentation**: Add a brief comment at the top explaining usage

## Examples and Naming

When using names for peer/node instances in examples, simulations, or documentation, use single-letter identifiers: A, B, C, D, E, F, G, etc.

## Peer sync workflow (syncgit)

This repo uses **syncgit** — a peer-to-peer VCS where each git worktree is an equal peer with its own agent. There is no `main` and no central hub. Peers broadcast work to each other as PRs over `refs/pr/*`.

**When you finish a slice of work, run `/sync`.** Do not `git commit` / `git push` manually. The `/sync` slash command orchestrates the full flow:

1. `syncgit stage` — review the diff and `git add` only real work. Never stage logs, `node_modules`, `dist`, `.env*`, or anything listed in `.syncgit/ignore`.
2. Commit with a short imperative message scoped to this slice.
3. `syncgit merge` — rebase through every pending peer PR, oldest first.
4. On conflict: resolve in-file, `git rebase --continue`, re-run `syncgit merge`. After 3 failed attempts, abort and halt with a summary in `.syncgit/last-halt.md` rather than guessing.
5. `syncgit verify` — must pass before broadcast if `.syncgit/verify.sh` exists.
6. `syncgit push` — broadcasts `HEAD` to every peer.

**Rules:**
- Always merge inbound peer PRs *before* broadcasting your own.
- Rebase, never merge-commit — history stays linear across peers.
- If you can't make the tree clean, **halt** and write `.syncgit/last-halt.md`. Do not force-push, do not `--no-verify`, do not skip `syncgit verify`.
- Check peer state with `syncgit status` before starting non-trivial work so you don't duplicate a sibling's effort.
- Your peer identity is the worktree directory name; treat sibling worktrees as independent collaborators, not as backups.

## Terminology

- **syncengine** (or "sync engine") refers to the **Synchronicity Engine** app as a whole — the product we're building — not any specific module or crate.

## Frontend Design Philosophy

**Make everything as frictionless as possible.**

Every interaction should remove a step, not add one. Concretely:

- **Prefer inline editing** over edit-mode toggles. Profiles, content, names, descriptions, and other user-owned data should be editable in place — click the text, type, done. No separate edit buttons or modal dialogs when inline editing is feasible.
- **No confirmation dialogs** for reversible actions. Use undo instead.
- **Autosave** over explicit save buttons. Persist on blur or debounce.
- **Sensible defaults** over required fields. Let the user start working immediately and refine later.
- **Direct manipulation** (drag, click-to-edit, keyboard shortcuts) over nested menus.

When designing new UI, the default question is: "Can the user accomplish this without leaving the view they're already in?" If yes, do that.

## Greenfield Project

This entire project is greenfield. Do not maintain backward compatibility unless explicitly told
to. Feel free to delete, replace, and rewrite modules without preserving old interfaces.

## Documentation Convention

- Every public type and function must have a `///` doc comment
- Every `lib.rs` should have `//!` module docs with purpose, key types, and architecture
- When adding a new module to a crate, update that crate's `AGENTS.md`
- When changing `indras-network` public API, update the developer guide (`articles/indras-network-developers-guide.md`)
- `AGENTS.md` files should be 50–150 lines — architectural context, not API reference

## Cargo Commands Reference

```bash
# Build specific package
cargo build -p <package-name>

# Run specific binary
cargo run -p <package-name>
cargo run --bin <binary-name>

# Test specific package
cargo test -p <package-name>

# Run Lua scenarios
cargo run --bin lua_runner --manifest-path simulation/Cargo.toml -- scripts/scenarios/<scenario>.lua
```

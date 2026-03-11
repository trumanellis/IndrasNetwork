# Claude Code Instructions for IndrasNetwork

## Session Start

On every new session, run `jj workspace list` and `jj log --limit 5` silently, then greet the user with:
1. Current workspace and change (from `jj st`)
2. Any other active workspaces (from `jj workspace list`)
3. Recent changes with open (non-main) work (from `jj log`)
4. Offer: "Continue an existing change, start new work, or create a parallel workspace?"

Keep the welcome concise — a short table or list, not a wall of text.

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

## Version Control (jj)

This repo uses **jj (Jujutsu)** colocated with git. Never use raw git commands.

### Core Rules
- Every change gets a descriptive message via `jj describe -m "..."`
- Use `jj new main` to start work, never bookmark creation
- Use `jj undo` as first response to any mistake
- Working copy is always a commit — no staging needed

### Single-Session Workflow
- `jj new main -m "task"` → work → `jj describe` → land
- Switch between tasks: `jj edit <change-id>`
- Stack dependent work: `jj new <parent-change> -m "next task"`

### Parallel Sessions (Multiple Claude Code instances)
**Always ask the user before creating workspaces.**
1. Create workspaces: `jj workspace add ../IndrasNetwork-ws-{name} --rev main -m "task"`
2. Launch Claude Code in each workspace directory
3. Track workspace→change mapping in notepad
4. Integrate when done (see Landing Changes)
5. Clean up: `jj workspace forget {name}` + remove directory

### Curating Output Before Landing
Self-curate all work into clean commits before landing:
1. `jj new @- -m "feat: clean description"` (clean target)
2. `jj squash -i --from <scratch>` (cherry-pick good parts)
3. `jj abandon <scratch>` (discard the scratch revision)
Or split a large change: `jj split` to separate concerns interactively.

### Landing Changes
1. `jj rebase -d main` (catch up with main)
2. `jj bookmark set main -r @`
3. `jj git push`
4. Other active changes auto-rebase onto new main

### Key Commands
```bash
jj st                        # status
jj log                       # commit graph
jj diff                      # working copy diff
jj new main -m "task"        # start new work
jj edit <change-id>          # switch to existing change
jj describe -m "msg"         # set change description
jj rebase -d main            # rebase onto main
jj squash                    # fold into parent
jj squash -i --from <id>     # cherry-pick from another change
jj split                     # split change interactively
jj abandon                   # discard a change
jj undo                      # undo last operation
jj bookmark list             # list bookmarks
jj bookmark set main -r @    # point main at current change
jj git fetch                 # pull from remote
jj git push                  # push to remote
jj workspace add <path>      # create parallel workspace
jj workspace forget <name>   # remove workspace
jj workspace list            # list workspaces
```

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

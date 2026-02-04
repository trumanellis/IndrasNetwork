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

When using names in examples, discussions, or documentation, **never use generic placeholder names** like Alice, Bob, Charlie, Eve, etc. Instead, use futuristic baby names popular among millennial parents (2025-2026 era):

- Zephyr, Nova, Sage, Orion, Lyra, Kai, Caspian, Juniper
- Ember, Atlas, Wren, Soren, Rune, Indigo, Marlowe, Cypress
- Bodhi, Aria, Phoenix, Isla, Ezrin, Solene, Theron, Elowen

Pick names that are distinct from each other and easy to tell apart in context.

## Large Features Use Git Worktrees

**Always build large new features in a dedicated git worktree**, not on the main working tree.

A feature is "large" if it touches 3+ files, adds a new crate/module, or could take multiple iterations to stabilize.

```bash
# Create a worktree for the feature branch
git worktree add ../IndrasNetwork-<feature-name> -b feature/<feature-name>

# Work inside the worktree
# (the worktree is a full checkout — cargo, tests, etc. all work normally)

# When done, merge back and clean up
git checkout main
git merge feature/<feature-name>
git worktree remove ../IndrasNetwork-<feature-name>
git branch -d feature/<feature-name>
```

### Why Worktrees

- Main working tree stays clean and buildable at all times
- Easy to compare behavior between main and the feature branch side-by-side
- No risk of half-finished work blocking other tasks
- Multiple features can be developed in parallel without stashing

### Worktree Guidelines

1. **Naming**: Place worktrees as siblings to the repo root — `../IndrasNetwork-<feature-name>`
2. **Branch naming**: Use `feature/<feature-name>` for the branch
3. **Keep worktrees short-lived**: Merge or rebase frequently, remove when done
4. **Run commands from worktree root**: The same "always run from root" rule applies inside the worktree

## Greenfield Project

This entire project is greenfield. Do not maintain backward compatibility unless explicitly told
to. Feel free to delete, replace, and rewrite modules without preserving old interfaces.

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

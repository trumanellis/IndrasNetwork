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

## Git Workflow (GitButler)

This repo uses **GitButler virtual branches**. The workspace branch (`gitbutler/workspace`) is a synthetic merge of all active virtual branches. Follow these rules:

1. **Never use `git commit`** — it will be blocked by the pre-commit hook. Use `but commit <branch> -m "msg"` instead
2. **Check `but status`** before committing to understand which branch owns which files
3. **Stage files explicitly** with `but stage -b <branch> <file>` when changes span multiple branches
4. **Use `-o` flag** (`but commit -o`) to commit only staged changes, not everything
5. **Don't touch `gitbutler/workspace`** — it's managed by GitButler

### Key Commands

| Task | Command |
|------|---------|
| Commit to a branch | `but commit <branch-name> -m "message"` |
| Commit only staged changes | `but commit -o <branch-name> -m "message"` |
| Stage file to a branch | `but stage -b <branch-name> <file>` |
| View status | `but status` |
| View diff | `but diff` |
| List branches | `but branch list` |

### Workflow: Changes Spanning Multiple Branches

```bash
# Check what's staged where
but status
# Stage specific files to the right branch
but stage -b feature-a src/module_a.rs
but stage -b feature-b src/module_b.rs
# Commit each branch separately
but commit -o feature-a -m "feat: update module A"
but commit -o feature-b -m "feat: update module B"
```

### Gotchas

| Issue | Cause | Fix |
|-------|-------|-----|
| `git commit` fails with GITBUTLER_ERROR | Pre-commit hook blocks direct commits | Use `but commit` instead |
| Wrong files in commit | `but commit` without `-o` grabs everything | Use `but commit -o` for staged-only |
| File locked to another branch | GitButler assigned the file to a different branch | Use `but stage -b <target> <file>` to reassign |

### When to Use Git Worktrees Instead

Virtual branches handle most parallel development. Only use worktrees for major architectural isolation that requires a **separate build directory** (e.g., incompatible dependency changes that would break the main workspace build).

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

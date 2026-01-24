# Contributing to Indra's Network

Thank you for your interest in contributing to Indra's Network! This document provides guidelines and instructions for contributing to the project.

## Table of Contents

1. [How to Contribute](#how-to-contribute)
2. [Development Setup](#development-setup)
3. [Code Style](#code-style)
4. [Testing Requirements](#testing-requirements)
5. [Commit Message Conventions](#commit-message-conventions)
6. [Pull Request Guidelines](#pull-request-guidelines)
7. [Code Review Process](#code-review-process)
8. [Architecture Notes](#architecture-notes)

## How to Contribute

### Fork and Branch Workflow

1. **Fork the Repository**
   ```bash
   # Go to https://github.com/indras-network/indras-network and click "Fork"
   ```

2. **Clone Your Fork**
   ```bash
   git clone https://github.com/YOUR_USERNAME/indras-network.git
   cd indras-network
   ```

3. **Add Upstream Remote**
   ```bash
   git remote add upstream https://github.com/indras-network/indras-network.git
   git remote -v  # verify both origin and upstream are listed
   ```

4. **Create a Feature Branch**
   ```bash
   # Update main from upstream
   git fetch upstream
   git checkout main
   git merge upstream/main

   # Create your feature branch from main
   git checkout -b feature/your-feature-name
   # or for bugfixes:
   git checkout -b fix/bug-description
   ```

5. **Make Your Changes**
   - Keep changes focused and atomic
   - One logical change per commit
   - Update related tests and documentation

6. **Push to Your Fork**
   ```bash
   git push origin feature/your-feature-name
   ```

7. **Create a Pull Request**
   - Go to https://github.com/indras-network/indras-network/pulls
   - Click "New Pull Request"
   - Select your fork and branch
   - Fill out the PR template completely
   - Request review from maintainers

### Branch Naming Conventions

- `feature/description` - New features or enhancements
- `fix/description` - Bug fixes
- `docs/description` - Documentation updates
- `refactor/description` - Code refactoring without behavioral changes
- `test/description` - Test additions or improvements
- `chore/description` - Dependencies, tooling, CI/CD updates

## Development Setup

### Prerequisites

- **Rust 1.70+** (MSRV may be enforced in CI)
- **Git** for version control
- **macOS, Linux, or Windows** (WSL recommended for Windows)

### Installation

1. **Install Rust**
   ```bash
   # Using rustup (recommended)
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   source $HOME/.cargo/env

   # Verify installation
   rustc --version
   cargo --version
   ```

2. **Clone the Repository**
   ```bash
   git clone https://github.com/indras-network/indras-network.git
   cd indras-network
   ```

3. **Build the Project**
   ```bash
   # Debug build (faster compilation, slower runtime)
   cargo build

   # Release build (slower compilation, optimized runtime)
   cargo build --release

   # Build with all features
   cargo build --all-features
   ```

4. **Verify Setup**
   ```bash
   # Run a quick scenario to verify everything works
   cargo run --example abc

   # Run the test suite
   cargo test
   ```

### Project Structure

The project is organized as a Rust workspace with multiple crates:

- **crates/indras-core** - Core traits, types, and abstractions
- **crates/indras-crypto** - Cryptographic operations and key management
- **crates/indras-transport** - Transport abstractions (TCP, UDP, etc.)
- **crates/indras-routing** - Message routing logic
- **crates/indras-storage** - Storage abstractions and implementations
- **crates/indras-gossip** - Gossip protocol implementation
- **crates/indras-sync** - CRDT synchronization using Automerge
- **crates/indras-messaging** - Message formatting and handling
- **crates/indras-dtn** - Delay-tolerant networking module
- **crates/indras-iot** - IoT device integration
- **crates/indras-logging** - Structured logging infrastructure
- **crates/indras-node** - P2P node coordinator
- **crates/indras-dashboard** - Web dashboard UI
- **simulation** - Discrete-time simulation engine
- **examples/** - Example applications (chat-app, sync-demo, indras-notes)

### Workspace Commands

```bash
# Build all crates
cargo build --workspace

# Test all crates
cargo test --workspace

# Test a specific crate
cargo test -p indras-core

# Build documentation for all crates
cargo doc --workspace --open

# Format all crates
cargo fmt --all

# Lint all crates
cargo clippy --workspace --all-features --all-targets -- -D warnings
```

### Dependencies

The project uses:

- **iroh** (0.95) - P2P networking stack
- **automerge** (0.7) - CRDT for data synchronization
- **tokio** - Async runtime
- **serde** / **postcard** - Serialization
- **tracing** - Structured logging
- **chacha20poly1305** - AEAD encryption

Check `Cargo.toml` for the complete dependency list.

## Code Style

### Formatting

All code must be formatted with `rustfmt`:

```bash
# Format all code
cargo fmt --all

# Check formatting without modifying
cargo fmt --all -- --check

# Format a specific file
cargo fmt -- src/lib.rs
```

Formatting is enforced in CI. PRs with formatting issues will not be merged.

### Linting

All code must pass `cargo clippy`:

```bash
# Run clippy on all crates with all features
cargo clippy --all-features --all-targets -- -D warnings

# Run clippy on a specific crate
cargo clippy -p indras-core -- -D warnings

# Fix common clippy suggestions automatically
cargo clippy --fix --allow-dirty
```

Clippy warnings are treated as errors in CI. Fix all warnings before submitting a PR.

### Code Style Guidelines

1. **Documentation**
   - All public items must have doc comments
   - Use `///` for item documentation
   - Use `//!` for module-level documentation
   - Include examples in doc comments for public APIs
   - Document invariants and safety considerations

   ```rust
   /// Sends a message to a peer in the network.
   ///
   /// # Arguments
   ///
   /// * `from` - Source peer identifier
   /// * `to` - Destination peer identifier
   /// * `message` - Message bytes to send
   ///
   /// # Returns
   ///
   /// Returns a `Result` with the packet ID on success, or `NetworkError` on failure.
   ///
   /// # Example
   ///
   /// ```rust
   /// # use indras_network::*;
   /// # async fn example() -> Result<()> {
   /// let net = Network::new()?;
   /// let packet_id = net.send_message(
   ///     PeerId::from("alice"),
   ///     PeerId::from("bob"),
   ///     b"hello".to_vec()
   /// ).await?;
   /// # Ok(())
   /// # }
   /// ```
   pub fn send_message(&mut self, from: PeerId, to: PeerId, message: Vec<u8>) -> Result<PacketId> {
       // implementation
   }
   ```

2. **Naming Conventions**
   - Constants: `SCREAMING_SNAKE_CASE`
   - Types: `PascalCase`
   - Variables/functions: `snake_case`
   - Private items: prefix with `_` if unused
   - Trait names ending in `-able`: use `CanX` or `XTrait` pattern

3. **Error Handling**
   - Use `Result<T>` for fallible operations
   - Define custom error types using `thiserror` or `anyhow`
   - Provide context with `anyhow::Context`
   - Avoid panicking in library code; use `expect()` only in tests/examples

   ```rust
   use anyhow::{Context, Result};

   pub fn load_config(path: &Path) -> Result<Config> {
       let contents = std::fs::read_to_string(path)
           .context("failed to read config file")?;
       serde_json::from_str(&contents)
           .context("failed to parse config JSON")
   }
   ```

4. **Comments**
   - Explain the "why", not the "what" (code shows the what)
   - Use `//` for single-line comments
   - Use `/* */` for multi-line comments
   - Avoid commented-out code; use git history instead

5. **Imports**
   - Use explicit imports; avoid glob imports except in tests
   - Group imports: std, external crates, internal modules
   - Organize alphabetically within groups

   ```rust
   use std::collections::HashMap;
   use std::path::Path;

   use anyhow::Result;
   use serde::Deserialize;

   use crate::config::Config;
   use crate::network::Network;
   ```

6. **Async Code**
   - Use `async`/`await` for async functions
   - Avoid `.spawn()` for long-running tasks; use structured concurrency
   - Always handle errors in spawned tasks
   - Use `tokio::select!` for cancellation-safe code

## Testing Requirements

### Test Organization

Tests should be:
1. Located alongside the code they test
2. Named with `#[test]` or `#[tokio::test]` for async tests
3. Grouped in a `tests` module at the end of files
4. Organized logically with descriptive names

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_serialization() {
        // test implementation
    }

    #[tokio::test]
    async fn test_network_send() {
        // async test
    }
}
```

### Running Tests

```bash
# Run all tests
cargo test

# Run tests for a specific crate
cargo test -p indras-core

# Run tests with output printed
cargo test -- --nocapture

# Run a specific test
cargo test test_message_serialization

# Run ignored tests only
cargo test -- --ignored

# Run tests in release mode
cargo test --release

# Generate test coverage
cargo tarpaulin --out Html --skip-clean
```

### Test Requirements

1. **Unit Tests**
   - Test individual functions and methods
   - Cover normal cases and edge cases
   - Mock external dependencies

2. **Integration Tests**
   - Test multiple components together
   - Located in `tests/` directory for library crates
   - Should be able to run independently

3. **Property-Based Tests**
   - Use for complex logic or algorithms
   - Consider using `quickcheck` or `proptest`

4. **Minimum Coverage**
   - Aim for >80% code coverage
   - Critical paths should have >95% coverage
   - Use `cargo tarpaulin` to measure coverage

### Test Examples

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_packet_serialization_roundtrip() {
        let original = Packet {
            sender: PeerId('A'),
            recipient: PeerId('B'),
            payload: vec![1, 2, 3, 4, 5],
        };

        let serialized = postcard::to_allocvec(&original).unwrap();
        let deserialized: Packet = postcard::from_bytes(&serialized).unwrap();

        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_invalid_peer_id() {
        // Test error cases
        let result = PeerId::from_str("invalid");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_concurrent_message_sending() {
        let network = Network::new().await.unwrap();

        let handles: Vec<_> = (0..10)
            .map(|i| {
                let net = network.clone();
                tokio::spawn(async move {
                    net.send_message(
                        PeerId::from(char::from_u32(65 + i).unwrap()),
                        PeerId('Z'),
                        format!("message {}", i).into_bytes(),
                    ).await
                })
            })
            .collect();

        for handle in handles {
            assert!(handle.await.unwrap().is_ok());
        }
    }
}
```

### CI Test Pipeline

The CI pipeline runs:
1. `cargo test --all-features` - All unit and integration tests
2. Network integration tests (marked with `#[ignore]`)
3. Docker-based multi-node tests
4. Code coverage with `cargo tarpaulin`

All tests must pass before PR approval.

## Commit Message Conventions

We follow a simplified version of [Conventional Commits](https://www.conventionalcommits.org/):

### Format

```
<type>(<scope>): <subject>

<body>

<footer>
```

### Type

- `feat`: A new feature
- `fix`: A bug fix
- `docs`: Documentation changes
- `style`: Code style changes (formatting, etc.)
- `refactor`: Code refactoring without new features or bug fixes
- `perf`: Performance improvements
- `test`: Test additions or improvements
- `chore`: Dependency updates, build system changes
- `ci`: CI/CD configuration changes

### Scope

The scope specifies what part of the codebase is affected:
- `core` - Core module
- `routing` - Routing module
- `crypto` - Cryptography module
- `transport` - Transport abstraction
- `dashboard` - Dashboard UI
- `sync` - Synchronization/CRDT
- `gossip` - Gossip protocol
- `dtn` - Delay-tolerant networking

### Subject

- Use imperative mood ("add" not "added" or "adds")
- Don't capitalize the first letter
- No period at the end
- Keep it concise (50 characters or less)

### Body

- Explain what and why, not how
- Wrap at 72 characters
- Separate from subject with a blank line
- Use bullet points for multiple changes

### Footer

- Reference related issues: `Fixes #123`, `Related to #456`
- Break changes: `BREAKING CHANGE: description`

### Examples

```
feat(routing): implement dijkstra's algorithm for path finding

Replace the current breadth-first search with Dijkstra's algorithm
to find optimal paths. This improves message delivery efficiency by
15% in large networks.

Fixes #234
Related to #200

feat(sync): add automerge document persistence

- Implement document serialization to disk
- Add database schema migrations
- Update sync protocol to include persistence metadata

BREAKING CHANGE: Changed SyncMessage format, requires client update
```

## Pull Request Guidelines

### Before Submitting

1. **Update Your Branch**
   ```bash
   git fetch upstream
   git rebase upstream/main
   ```

2. **Run All Checks Locally**
   ```bash
   # Format check
   cargo fmt --all -- --check

   # Lint check
   cargo clippy --all-features --all-targets -- -D warnings

   # Run all tests
   cargo test --all-features

   # Build documentation
   cargo doc --no-deps --all-features
   ```

3. **Commit with Proper Messages**
   - Use conventional commit format
   - Organize commits logically
   - Consider using `git rebase -i` to organize commits before pushing

### PR Template

When creating a PR, include:

```markdown
## Description
Brief description of the changes.

## Type of Change
- [ ] Bug fix
- [ ] New feature
- [ ] Breaking change
- [ ] Documentation update

## Related Issues
Closes #123

## Changes Made
- Change 1
- Change 2
- Change 3

## Testing
Describe how these changes were tested:
- [ ] Unit tests added/updated
- [ ] Integration tests added
- [ ] Manual testing performed

## Checklist
- [ ] Code follows project style guidelines
- [ ] Self-review completed
- [ ] Comments added for complex logic
- [ ] Documentation updated
- [ ] All tests pass locally
- [ ] No new warnings from clippy
```

### PR Title

- Use conventional commit format: `feat(scope): description`
- Keep it concise and descriptive
- Link to issues if applicable

### PR Description

- Clearly explain the purpose and changes
- Link related issues with `Closes #123` or `Fixes #456`
- Describe how to test the changes
- Highlight any breaking changes
- Note any performance implications

## Code Review Process

### Review Criteria

Reviewers will check:

1. **Correctness**
   - Does the code do what it claims?
   - Are error cases handled?
   - Are there any logic errors or edge cases missed?

2. **Design**
   - Does the solution fit the architecture?
   - Are abstractions appropriate?
   - Could the approach be simpler?

3. **Performance**
   - Are there unnecessary allocations?
   - Are loops efficient?
   - Are there potential bottlenecks?

4. **Testing**
   - Are tests comprehensive?
   - Do tests cover edge cases?
   - Are tests maintainable?

5. **Documentation**
   - Are public APIs documented?
   - Is the documentation clear and accurate?
   - Are there sufficient examples?

6. **Code Style**
   - Does code pass `cargo fmt`?
   - Does code pass `cargo clippy`?
   - Are naming conventions followed?

### Responding to Reviews

1. **Address All Comments**
   - Respond to every comment, even if just to acknowledge
   - Ask for clarification if feedback is unclear
   - Explain your reasoning if you disagree

2. **Update Your PR**
   - Make requested changes in new commits
   - Don't force-push during active review
   - Resolve conversation threads only after making changes

3. **Request Re-Review**
   - After making changes, request re-review
   - Reply to comments with "Done" or explain changes

4. **Disagreement**
   - Discuss respectfully
   - Seek maintainer guidance if consensus can't be reached
   - Final decision rests with project maintainers

### Approval and Merge

- PRs require at least 1 approval from a maintainer
- All CI checks must pass
- Branch must be up-to-date with `main`
- No unresolved conversations

Maintainers may merge with:
- "Squash and merge" for single-commit PRs
- "Create a merge commit" for multi-commit PRs
- "Rebase and merge" for cleanup when needed

## Architecture Notes

### Key Design Principles

1. **Abstraction Over Simulation**
   - Core logic uses traits to work with both simulation (char-based peers) and real networking (public keys)
   - See `PeerIdentity` trait in `indras-core`

2. **Event-Driven Architecture**
   - Peer interfaces maintain append-only event logs
   - Enables CRDT-style synchronization
   - Provides auditable history

3. **Store-and-Forward**
   - Packets sealed for destination, intermediate peers cannot read content
   - Enables reliable delivery in DTN scenarios
   - Back-propagation confirms delivery

4. **Modular Stack**
   - Each crate has single responsibility
   - Crates can be used independently
   - Dependencies flow upward (core -> crypto -> routing -> etc.)

### Before Adding New Features

1. Check if similar logic exists in other crates
2. Consider if the feature belongs in a new crate or existing one
3. Update relevant documentation and examples
4. Add tests at unit and integration level
5. Update ARCHITECTURE.md if adding new crate or major design change

### Workspace Dependency Policy

- Prefer workspace dependencies for internal crates
- Pin external crate versions in workspace
- Update dependencies carefully; test all crates after updates
- Document reasons for specific version constraints

## Getting Help

- **Questions**: Open a discussion or ask in issues
- **Bug Reports**: Open an issue with reproducible example
- **Feature Requests**: Open an issue describing use case
- **Design Discussions**: Start in discussions before coding

## License

By contributing to Indra's Network, you agree that your contributions will be licensed under the MIT OR Apache-2.0 license.

---

**Thank you for contributing to Indra's Network!**

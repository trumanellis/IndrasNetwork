//! Quality gate runner: shells out to `cargo` and produces signed `Evidence`.
//!
//! # Usage
//!
//! Build a [`VerificationRequest`] specifying which crates to check, then call
//! [`run`] to execute `cargo build`, `cargo test -p <crate>`, and
//! `cargo clippy -p <crate>` per crate.
//!
//! # Safety rule
//!
//! This module **never** invokes bare `cargo test` or `cargo test --workspace`.
//! Every test invocation is scoped to `-p <crate>` to avoid workspace-level
//! hangs.

use std::path::{Path, PathBuf};
use std::time::Instant;

use tokio::process::Command;

use super::changeset::Evidence;
use crate::vault::vault_file::UserId;

/// Specifies which crates to verify and how.
#[derive(Debug, Clone)]
pub struct VerificationRequest {
    /// Crate names (as they appear in `Cargo.toml` `[package] name`) to build,
    /// test, and lint.
    pub crates: Vec<String>,
    /// Root directory of the Cargo workspace.
    pub workspace_root: PathBuf,
    /// Agent identity that will sign the resulting [`Evidence`].
    pub agent: UserId,
    /// Whether to run `cargo clippy -p <crate> --all-targets -- -D warnings`
    /// for each crate.
    pub run_clippy: bool,
    /// Whether to run `cargo test -p <crate>` for each crate.
    pub run_tests: bool,
}

impl VerificationRequest {
    /// Construct a new request with `run_clippy` and `run_tests` both `true`.
    pub fn new(crates: Vec<String>, workspace_root: PathBuf, agent: UserId) -> Self {
        Self {
            crates,
            workspace_root,
            agent,
            run_clippy: true,
            run_tests: true,
        }
    }

    /// Disable clippy (useful for fast smoke-check builds).
    pub fn without_clippy(mut self) -> Self {
        self.run_clippy = false;
        self
    }

    /// Disable tests (useful when only compilation evidence is needed).
    pub fn without_tests(mut self) -> Self {
        self.run_tests = false;
        self
    }
}

/// Errors that can occur while running the verification suite.
#[derive(Debug, thiserror::Error)]
pub enum VerificationFailure {
    /// `cargo build -p <crate>` exited non-zero.
    #[error("build failed for crate `{crate_name}`: {stderr}")]
    BuildFailed {
        /// Name of the crate whose build failed.
        crate_name: String,
        /// Captured stderr from the failing `cargo build` invocation.
        stderr: String,
    },

    /// `cargo test -p <crate>` exited non-zero.
    #[error("tests failed for crate `{crate_name}`: {stdout}")]
    TestFailed {
        /// Name of the crate whose tests failed.
        crate_name: String,
        /// Captured stdout from the failing `cargo test` invocation.
        stdout: String,
    },

    /// `cargo clippy -p <crate>` exited non-zero.
    #[error("clippy failed for crate `{crate_name}`: {stderr}")]
    ClippyFailed {
        /// Name of the crate where clippy reported warnings/errors.
        crate_name: String,
        /// Captured stderr from the failing `cargo clippy` invocation.
        stderr: String,
    },

    /// Could not spawn the `cargo` subprocess.
    #[error("failed to spawn cargo: {0}")]
    SpawnError(#[from] std::io::Error),
}

/// Run the full quality gate defined by `req` and return signed [`Evidence`].
///
/// Per crate, in order: `cargo build -p <crate>`, then (if enabled) `cargo
/// test -p <crate>`, then (if enabled) `cargo clippy -p <crate> --all-targets
/// -- -D warnings`. Returns the first failure encountered.
pub async fn run(req: &VerificationRequest) -> Result<Evidence, VerificationFailure> {
    let start = Instant::now();
    let mut tests_passed: Vec<String> = Vec::new();
    let mut lints_clean = true;

    if !req.run_clippy {
        lints_clean = false;
    }

    for crate_name in &req.crates {
        let build_out = Command::new("cargo")
            .args(["build", "-p", crate_name])
            .current_dir(&req.workspace_root)
            .output()
            .await?;

        if !build_out.status.success() {
            return Err(VerificationFailure::BuildFailed {
                crate_name: crate_name.clone(),
                stderr: String::from_utf8_lossy(&build_out.stderr).into_owned(),
            });
        }

        if req.run_tests {
            let test_out = Command::new("cargo")
                .args(["test", "-p", crate_name])
                .current_dir(&req.workspace_root)
                .output()
                .await?;

            if test_out.status.success() {
                tests_passed.push(crate_name.clone());
            } else {
                return Err(VerificationFailure::TestFailed {
                    crate_name: crate_name.clone(),
                    stdout: String::from_utf8_lossy(&test_out.stdout).into_owned(),
                });
            }
        }

        if req.run_clippy {
            let clippy_out = Command::new("cargo")
                .args([
                    "clippy",
                    "-p",
                    crate_name,
                    "--all-targets",
                    "--",
                    "-D",
                    "warnings",
                ])
                .current_dir(&req.workspace_root)
                .output()
                .await?;

            if !clippy_out.status.success() {
                return Err(VerificationFailure::ClippyFailed {
                    crate_name: crate_name.clone(),
                    stderr: String::from_utf8_lossy(&clippy_out.stderr).into_owned(),
                });
            }
        }
    }

    if req.run_clippy && req.crates.is_empty() {
        lints_clean = true;
    }

    let runtime_ms = start.elapsed().as_millis() as u64;

    Ok(Evidence::Agent {
        compiled: true,
        tests_passed,
        lints_clean,
        runtime_ms,
        signed_by: req.agent,
    })
}

/// Map edited file paths to the names of their owning Cargo crates.
///
/// For each path, walk upward from the file's directory until a `Cargo.toml`
/// is found. Workspace-root `Cargo.toml`s are omitted.
pub fn crates_touched_by_paths(paths: &[&Path], workspace_root: &Path) -> Vec<String> {
    let workspace_root = match workspace_root.canonicalize() {
        Ok(p) => p,
        Err(_) => workspace_root.to_path_buf(),
    };

    let mut crate_names: Vec<String> = Vec::new();

    for &path in paths {
        let abs = match path.canonicalize() {
            Ok(p) => p,
            Err(_) => continue,
        };

        let start_dir = if abs.is_dir() {
            abs.clone()
        } else {
            match abs.parent() {
                Some(p) => p.to_path_buf(),
                None => continue,
            }
        };

        let mut current = start_dir.as_path();
        loop {
            let candidate = current.join("Cargo.toml");
            if candidate.exists() {
                if current == workspace_root {
                    break;
                }
                if let Some(name) = read_package_name(&candidate) {
                    crate_names.push(name);
                }
                break;
            }
            match current.parent() {
                Some(parent) => {
                    if !current.starts_with(&workspace_root) {
                        break;
                    }
                    current = parent;
                }
                None => break,
            }
        }
    }

    crate_names.sort();
    crate_names.dedup();
    crate_names
}

/// Read the `[package] name` value from a `Cargo.toml` file.
fn read_package_name(cargo_toml: &Path) -> Option<String> {
    let contents = std::fs::read_to_string(cargo_toml).ok()?;
    let mut in_package = false;
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed == "[package]" {
            in_package = true;
            continue;
        }
        if trimmed.starts_with('[') && trimmed != "[package]" {
            if in_package {
                break;
            }
            continue;
        }
        if in_package
            && trimmed.starts_with("name")
            && let Some(eq_pos) = trimmed.find('=')
        {
            let value_part = trimmed[eq_pos + 1..].trim();
            let name = value_part.trim_matches('"').trim_matches('\'');
            if !name.is_empty() {
                return Some(name.to_owned());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace_root() -> PathBuf {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest
            .parent()
            .expect("manifest has parent")
            .parent()
            .expect("crates/ has parent")
            .to_path_buf()
    }

    #[test]
    fn crates_touched_single() {
        let root = workspace_root();
        let file = root.join("crates/indras-sync-engine/src/lib.rs");
        let result = crates_touched_by_paths(&[file.as_path()], &root);
        assert_eq!(result, vec!["indras-sync-engine".to_string()]);
    }

    #[test]
    fn crates_touched_multiple_dedup() {
        let root = workspace_root();
        let file1 = root.join("crates/indras-sync-engine/src/lib.rs");
        let file2 = root.join("crates/indras-sync-engine/src/realm_vault.rs");
        let result = crates_touched_by_paths(&[file1.as_path(), file2.as_path()], &root);
        assert_eq!(result, vec!["indras-sync-engine".to_string()]);
    }

    #[test]
    fn crates_touched_two_crates() {
        let root = workspace_root();
        let file1 = root.join("crates/indras-sync-engine/src/lib.rs");
        let file2 = root.join("crates/indras-network/src/lib.rs");
        let mut result = crates_touched_by_paths(&[file1.as_path(), file2.as_path()], &root);
        result.sort();
        assert!(result.contains(&"indras-sync-engine".to_string()));
        assert!(result.contains(&"indras-network".to_string()));
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn crates_touched_root_file_omitted() {
        let root = workspace_root();
        let readme = root.join("README.md");
        let result = crates_touched_by_paths(&[readme.as_path()], &root);
        assert!(
            result.is_empty(),
            "expected empty vec for workspace root file, got: {result:?}"
        );
    }

    #[test]
    fn crates_touched_nonexistent_path() {
        let root = workspace_root();
        let nonexistent = root.join("crates/does-not-exist/src/lib.rs");
        let result = crates_touched_by_paths(&[nonexistent.as_path()], &root);
        assert!(result.is_empty());
    }
}

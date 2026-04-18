//! Heal-detection and repair-task-emission for the braided VCS.
//!
//! # Trust model
//!
//! Incoming changesets are **not** re-verified on receipt. Instead, after
//! absorbing peer changesets via DAG merge, the *local* working tree may
//! break: a peer changed `foo.rs` in a way that makes the local agent's
//! `bar.rs` edit fail to compile, or local tests now fail because of the
//! combined state.
//!
//! This module detects that situation and emits a [`RepairTask`] so an
//! upstream agent loop can produce a new verified merge changeset.

use super::{
    changeset::{ChangeId, Evidence},
    gate::LocalRepo,
    verification::VerificationFailure,
};

/// Description of local breakage caused by absorbing peer changesets.
#[derive(Debug, Clone, PartialEq)]
pub struct RepairTask {
    /// The DAG heads that are now combined in the local tree. These become
    /// the parent pointers for the eventual repair changeset.
    pub merged_heads: Vec<ChangeId>,
    /// Human-readable summary of what broke.
    pub failure: String,
    /// Names of the Cargo crates whose build, test, or lint step failed.
    /// Empty when the failure is infrastructural.
    pub failing_crates: Vec<String>,
    /// Pre-filled commit intent for the repair changeset.
    pub suggested_intent: String,
}

/// Decide whether a heal repair task is needed after merging incoming peer
/// changesets, and if so, return a [`RepairTask`] describing what broke.
pub fn detect_heal_needed(
    pre_merge_heads: &[ChangeId],
    post_merge_heads: &[ChangeId],
    local_verification: &Result<Evidence, VerificationFailure>,
) -> Option<RepairTask> {
    if local_verification.is_ok() {
        return None;
    }

    let mut pre_sorted = pre_merge_heads.to_vec();
    pre_sorted.sort();
    let mut post_sorted = post_merge_heads.to_vec();
    post_sorted.sort();
    if pre_sorted == post_sorted {
        return None;
    }

    let failure_ref = local_verification.as_ref().unwrap_err();

    let (failure, failing_crates) = match failure_ref {
        VerificationFailure::BuildFailed { crate_name, stderr } => {
            let summary = format!(
                "build failed for crate `{crate_name}`: {}",
                first_line(stderr)
            );
            (summary, vec![crate_name.clone()])
        }
        VerificationFailure::TestFailed { crate_name, stdout } => {
            let summary = format!(
                "tests failed for crate `{crate_name}`: {}",
                first_line(stdout)
            );
            (summary, vec![crate_name.clone()])
        }
        VerificationFailure::ClippyFailed { crate_name, stderr } => {
            let summary = format!(
                "clippy failed for crate `{crate_name}`: {}",
                first_line(stderr)
            );
            (summary, vec![crate_name.clone()])
        }
        VerificationFailure::SpawnError(io_err) => {
            let summary = format!("spawn error running cargo: {io_err}");
            (summary, vec![])
        }
    };

    let suggested_intent = if failing_crates.is_empty() {
        "heal: reconcile after merging from peers".to_owned()
    } else {
        format!(
            "heal: reconcile {} after merging from peers",
            failing_crates.join(", ")
        )
    };

    Some(RepairTask {
        merged_heads: post_merge_heads.to_vec(),
        failure,
        failing_crates,
        suggested_intent,
    })
}

fn first_line(s: &str) -> &str {
    s.lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or(s)
        .trim()
}

impl LocalRepo {
    /// Describe a repair task if the local working tree failed verification
    /// after absorbing incoming peer changesets.
    ///
    /// Reads `post_merge_heads` from `self.head_ids()`.
    pub fn describe_heal(
        &self,
        pre_merge_heads: &[ChangeId],
        local_verification: &Result<Evidence, VerificationFailure>,
    ) -> Option<RepairTask> {
        let post_merge_heads = self.head_ids();
        detect_heal_needed(pre_merge_heads, &post_merge_heads, local_verification)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use super::super::changeset::Evidence;
    use crate::vault::vault_file::UserId;

    fn change_id(byte: u8) -> ChangeId {
        ChangeId([byte; 32])
    }

    fn agent(byte: u8) -> UserId {
        [byte; 32]
    }

    fn green_evidence() -> Evidence {
        Evidence::Agent {
            compiled: true,
            tests_passed: vec!["some-crate".to_owned()],
            lints_clean: true,
            runtime_ms: 42,
            signed_by: [0u8; 32],
        }
    }

    fn build_err(crate_name: &str) -> VerificationFailure {
        VerificationFailure::BuildFailed {
            crate_name: crate_name.to_owned(),
            stderr: "error[E0308]: mismatched types\n  --> src/lib.rs:5:9".to_owned(),
        }
    }

    fn test_err(crate_name: &str) -> VerificationFailure {
        VerificationFailure::TestFailed {
            crate_name: crate_name.to_owned(),
            stdout: "test foo ... FAILED\nfailures: foo".to_owned(),
        }
    }

    fn clippy_err(crate_name: &str) -> VerificationFailure {
        VerificationFailure::ClippyFailed {
            crate_name: crate_name.to_owned(),
            stderr: "warning: unused variable `x`\n  --> src/lib.rs:3:9".to_owned(),
        }
    }

    fn spawn_err() -> VerificationFailure {
        VerificationFailure::SpawnError(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "cargo not found",
        ))
    }

    #[test]
    fn no_heal_when_verification_green() {
        let pre = vec![change_id(1)];
        let post = vec![change_id(1), change_id(2)];
        let result = detect_heal_needed(&pre, &post, &Ok(green_evidence()));
        assert!(result.is_none());
    }

    #[test]
    fn no_heal_when_no_incoming_work() {
        let heads = vec![change_id(1), change_id(2)];
        let result = detect_heal_needed(&heads, &heads, &Err(build_err("foo")));
        assert!(result.is_none());
    }

    #[test]
    fn heal_on_build_failure() {
        let pre = vec![change_id(1)];
        let post = vec![change_id(1), change_id(2)];
        let task = detect_heal_needed(&pre, &post, &Err(build_err("foo")))
            .expect("should return Some(RepairTask) for BuildFailed");

        assert_eq!(task.failing_crates, vec!["foo".to_owned()]);
        assert!(task.suggested_intent.contains("foo"));
        assert!(task.failure.contains("build failed"));
    }

    #[test]
    fn heal_on_test_failure() {
        let pre = vec![change_id(1)];
        let post = vec![change_id(1), change_id(3)];
        let task = detect_heal_needed(&pre, &post, &Err(test_err("bar")))
            .expect("should return Some(RepairTask) for TestFailed");

        assert_eq!(task.failing_crates, vec!["bar".to_owned()]);
        assert!(task.suggested_intent.contains("bar"));
        assert!(task.failure.contains("tests failed"));
    }

    #[test]
    fn heal_on_clippy_failure() {
        let pre = vec![change_id(1)];
        let post = vec![change_id(1), change_id(4)];
        let task = detect_heal_needed(&pre, &post, &Err(clippy_err("baz")))
            .expect("should return Some(RepairTask) for ClippyFailed");

        assert_eq!(task.failing_crates, vec!["baz".to_owned()]);
        assert!(task.suggested_intent.contains("baz"));
        assert!(task.failure.contains("clippy failed"));
    }

    #[test]
    fn heal_on_spawn_error_no_crates() {
        let pre = vec![change_id(1)];
        let post = vec![change_id(1), change_id(5)];
        let task = detect_heal_needed(&pre, &post, &Err(spawn_err()))
            .expect("should return Some(RepairTask) for SpawnError");

        assert!(task.failing_crates.is_empty());
        assert!(task.failure.contains("spawn"));
    }

    #[test]
    fn heal_merged_heads_equals_post_merge() {
        let pre = vec![change_id(1)];
        let post = vec![change_id(2), change_id(3), change_id(4)];
        let task = detect_heal_needed(&pre, &post, &Err(build_err("qux")))
            .expect("should return Some(RepairTask)");

        assert_eq!(task.merged_heads, post);
    }

    #[test]
    fn describe_heal_uses_current_head_ids() {
        let repo = LocalRepo::new(PathBuf::from("/tmp/test"), agent(1));
        // Fresh repo has no heads → pre == post → no task.
        let task = repo.describe_heal(&[], &Err(build_err("x")));
        assert!(task.is_none());
    }
}

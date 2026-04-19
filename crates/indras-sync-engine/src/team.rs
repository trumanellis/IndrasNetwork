//! Team types — roster of logical AI agents for a vault.
//!
//! The [`Team`] struct tracks which logical agents belong to a vault.
//! Device-local types (folder bindings, registries, membership views)
//! live in `synchronicity_engine::team`.

use serde::{Deserialize, Serialize};

use crate::braid::changeset::ChangeId;
use crate::content_addr::SymlinkIndex;
/// Stable identifier for an AI-agent participant on a team.
///
/// Human-readable (e.g. `"agent1"`, `"researcher"`). Uniqueness scope is the
/// enclosing [`Team::roster`].
#[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct LogicalAgentId(pub String);

impl LogicalAgentId {
    /// Construct a new logical agent id from any string-like value.
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Borrow the underlying name as a `&str`.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for LogicalAgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The team associated with a vault — a roster of logical AI agents.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Team {
    /// Roster of logical agents belonging to the team. Kept sorted after
    /// merges so peers converge to the same order.
    pub roster: Vec<LogicalAgentId>,
    /// The current published HEAD — the latest committed changeset.
    #[serde(default)]
    pub head: Option<ChangeId>,
    /// The [`SymlinkIndex`] of `head` — carried alongside so non-hosting
    /// devices can materialize the HEAD state from blobs without joining
    /// the team realm or reading the DAG.
    #[serde(default)]
    pub head_index: Option<SymlinkIndex>,
}

impl Team {
    /// Construct an empty team.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Whether the given logical agent is part of this team's roster.
    pub fn contains(&self, agent: &LogicalAgentId) -> bool {
        self.roster.iter().any(|a| a == agent)
    }

    /// Merge with another team replica via set-union on the roster.
    pub fn merge(&mut self, remote: Self) {
        for agent in remote.roster {
            if !self.roster.contains(&agent) {
                self.roster.push(agent);
            }
        }
        self.roster.sort();

        // HEAD: take whichever side has a head; if both do, the higher
        // byte-value ChangeId wins (deterministic tiebreak). The
        // index travels with its head.
        match (&self.head, &remote.head) {
            (None, Some(_)) => {
                self.head = remote.head;
                self.head_index = remote.head_index;
            }
            (Some(local), Some(remote_head)) if remote_head > local => {
                self.head = Some(*remote_head);
                self.head_index = remote.head_index;
            }
            _ => {} // local wins or both None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(name: &str) -> LogicalAgentId {
        LogicalAgentId::new(name)
    }

    #[test]
    fn merge_union_roster_and_sort() {
        let mut a = Team {
            roster: vec![agent("b"), agent("a")],
            ..Default::default()
        };
        let b = Team {
            roster: vec![agent("c"), agent("a")],
            ..Default::default()
        };
        a.merge(b);
        assert_eq!(a.roster, vec![agent("a"), agent("b"), agent("c")]);
    }

    #[test]
    fn merge_identity() {
        let mut a = Team::empty();
        a.merge(Team::empty());
        assert_eq!(a, Team::empty());
    }
}

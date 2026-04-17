//! Team types synced alongside the vault.
//!
//! The [`Team`] struct is embedded in [`crate::vault::VaultFileDocument`] so
//! every device and connection on a synced vault learns the team roster and,
//! once established, the id of the team realm where the braid DAG gossips.
//! These types are the synced half of the architecture; device-local types
//! (folder bindings, registries, membership views) live in
//! `synchronicity_engine::team`.

use indras_network::RealmId;
use serde::{Deserialize, Serialize};

use crate::braid::changeset::{ChangeId, PatchManifest};

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

/// The team associated with a synced vault.
///
/// Synced across all devices and all connections on the vault. The
/// `team_realm_id` is `None` until the first agent-hosting device creates
/// the team realm (subtask 0.4); before that point, the roster is declared
/// but no braid DAG channel exists yet.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Team {
    /// Roster of logical agents belonging to the team. Kept sorted after
    /// merges so peers converge to the same order.
    pub roster: Vec<LogicalAgentId>,
    /// Id of the team realm hosting this team's braid DAG. Set once.
    pub team_realm_id: Option<RealmId>,
    /// The current published HEAD — the latest committed changeset that
    /// this vault considers "the tip." Updated by the committing device
    /// after a successful `try_land`; synced to all devices via the
    /// VaultFileDocument CRDT so non-hosting devices can see (and
    /// eventually checkout) the latest agreed-upon state.
    #[serde(default)]
    pub head: Option<ChangeId>,
    /// The `PatchManifest` of `head` — carried alongside so non-hosting
    /// devices can materialize the HEAD state from blobs without joining
    /// the team realm or reading the DAG.
    #[serde(default)]
    pub head_manifest: Option<PatchManifest>,
}

impl Team {
    /// Construct an empty team (no roster, no team realm).
    pub fn empty() -> Self {
        Self::default()
    }

    /// Whether the given logical agent is part of this team's roster.
    pub fn contains(&self, agent: &LogicalAgentId) -> bool {
        self.roster.iter().any(|a| a == agent)
    }

    /// CRDT merge with another team replica.
    ///
    /// Roster: set-union by agent id, result kept sorted for deterministic
    /// convergence across peers. Team realm id: first-set-wins, with
    /// byte-lexicographic tiebreak if both sides concurrently set different
    /// ids. The tiebreak guarantees all peers converge to the same id.
    pub fn merge(&mut self, remote: Self) {
        for agent in remote.roster {
            if !self.roster.contains(&agent) {
                self.roster.push(agent);
            }
        }
        self.roster.sort();

        self.team_realm_id = match (self.team_realm_id, remote.team_realm_id) {
            (None, None) => None,
            (Some(local), None) => Some(local),
            (None, Some(remote)) => Some(remote),
            (Some(local), Some(remote)) if local == remote => Some(local),
            (Some(local), Some(remote)) => {
                if local.0 <= remote.0 {
                    Some(local)
                } else {
                    Some(remote)
                }
            }
        };

        // HEAD: take whichever side has a head; if both do, the higher
        // byte-value ChangeId wins (deterministic tiebreak). The
        // manifest travels with its head.
        match (&self.head, &remote.head) {
            (None, Some(_)) => {
                self.head = remote.head;
                self.head_manifest = remote.head_manifest;
            }
            (Some(local), Some(remote_head)) if remote_head > local => {
                self.head = Some(*remote_head);
                self.head_manifest = remote.head_manifest;
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
            team_realm_id: None,
            ..Default::default()
        };
        let b = Team {
            roster: vec![agent("c"), agent("a")],
            team_realm_id: None,
            ..Default::default()
        };
        a.merge(b);
        assert_eq!(a.roster, vec![agent("a"), agent("b"), agent("c")]);
    }

    #[test]
    fn merge_takes_single_set_team_realm_id() {
        let realm = RealmId::new([7u8; 32]);
        let mut a = Team {
            roster: vec![],
            team_realm_id: None,
            ..Default::default()
        };
        let b = Team {
            roster: vec![],
            team_realm_id: Some(realm),
            ..Default::default()
        };
        a.merge(b);
        assert_eq!(a.team_realm_id, Some(realm));
    }

    #[test]
    fn merge_races_resolve_deterministically() {
        let lower = RealmId::new([0u8; 32]);
        let higher = RealmId::new([1u8; 32]);
        let mut a = Team {
            roster: vec![],
            team_realm_id: Some(higher),
            ..Default::default()
        };
        let b = Team {
            roster: vec![],
            team_realm_id: Some(lower),
            ..Default::default()
        };
        a.merge(b);
        assert_eq!(
            a.team_realm_id,
            Some(lower),
            "lower byte id should always win"
        );

        // Symmetric case: same result regardless of merge direction.
        let mut c = Team {
            roster: vec![],
            team_realm_id: Some(lower),
            ..Default::default()
        };
        c.merge(Team {
            roster: vec![],
            team_realm_id: Some(higher),
            ..Default::default()
        });
        assert_eq!(c.team_realm_id, Some(lower));
    }

    #[test]
    fn merge_identity() {
        let mut a = Team::empty();
        a.merge(Team::empty());
        assert_eq!(a, Team::empty());
    }
}

//! Team binding types — the model for which AI-agent folders on this device
//! participate in a given synced vault's team.
//!
//! # Layers
//!
//! - [`Team`] lives inside a synced-vault document and is replicated across
//!   every device and every connection on that vault. It names the roster of
//!   logical agents and carries the id of the team realm where the braid DAG
//!   is gossiped.
//! - [`TeamBindingRegistry`] is **device-local**. It maps [`LogicalAgentId`]s
//!   to on-disk folders hosted by this device. It is never synced.
//! - [`DeviceTeamMembership`] is a computed view: given a [`Team`] and the
//!   device's [`TeamBindingRegistry`], it names the subset of roster agents
//!   this device actually hosts.
//!
//! Persistence (load/save of the registry) lives in a later subtask; this
//! module defines only the types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::state::RealmId;

/// Stable identifier for an AI-agent participant on a team.
///
/// Human-readable (e.g. `"agent1"`, `"researcher"`) so bindings and logs stay
/// legible. Uniqueness scope is the enclosing [`Team`]'s `roster`.
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

/// A device-local binding of a logical agent to a filesystem folder.
///
/// The folder is what the AI agent edits; the syncengine mirrors edits from
/// the folder into the team realm's braid DAG on the agent's behalf.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FolderBinding {
    /// Which logical agent owns edits from this folder.
    pub agent: LogicalAgentId,
    /// Absolute path to the folder on this device.
    pub folder: PathBuf,
}

impl FolderBinding {
    /// Build a new binding from a logical agent and an absolute folder path.
    pub fn new(agent: LogicalAgentId, folder: PathBuf) -> Self {
        Self { agent, folder }
    }
}

/// The team associated with a synced vault. Synced across devices/connections.
///
/// `team_realm_id` is `None` until the first time the team realm is created
/// (see subtask 0.4). Before that point, the vault knows its roster but has
/// no DAG gossip channel yet.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Team {
    /// Ordered roster of logical agents belonging to the team. Order is
    /// cosmetic; membership is what matters.
    pub roster: Vec<LogicalAgentId>,
    /// Id of the team realm that hosts this team's braid DAG. `None` until
    /// the first agent-hosting device creates the realm.
    pub team_realm_id: Option<RealmId>,
}

impl Team {
    /// Construct an empty team with no roster and no team realm.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Whether the given logical agent is part of this team's roster.
    pub fn contains(&self, agent: &LogicalAgentId) -> bool {
        self.roster.iter().any(|a| a == agent)
    }
}

/// Device-local map from logical agent id to bound folder path.
///
/// Persisted as JSON at `{data_dir}/team_bindings.json`. Load/save logic
/// lives in subtask 0.7; this type just models the in-memory shape.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TeamBindingRegistry {
    /// All agent → folder bindings this device hosts, flattened across teams.
    /// An agent id is unique within this registry — a device can only host
    /// a given logical agent in one folder at a time.
    pub bindings: HashMap<LogicalAgentId, PathBuf>,
}

impl TeamBindingRegistry {
    /// Construct an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or replace the binding for an agent.
    pub fn bind(&mut self, agent: LogicalAgentId, folder: PathBuf) {
        self.bindings.insert(agent, folder);
    }

    /// Remove the binding for an agent, if any. Returns the previous path.
    pub fn unbind(&mut self, agent: &LogicalAgentId) -> Option<PathBuf> {
        self.bindings.remove(agent)
    }

    /// Look up the folder path for a given agent.
    pub fn folder_for(&self, agent: &LogicalAgentId) -> Option<&PathBuf> {
        self.bindings.get(agent)
    }

    /// Compute the subset of a team's roster this device hosts.
    pub fn membership_for(&self, team: &Team) -> DeviceTeamMembership {
        let hosted = team
            .roster
            .iter()
            .filter_map(|agent| {
                self.bindings
                    .get(agent)
                    .map(|path| (agent.clone(), path.clone()))
            })
            .collect();
        DeviceTeamMembership { hosted }
    }
}

/// The subset of a team's roster actually hosted on this device, with folders.
///
/// Derived from [`Team`] + [`TeamBindingRegistry`]. Used to decide whether
/// this device should join the team realm (non-empty `hosted` ⇒ join).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeviceTeamMembership {
    /// Logical agents this device hosts, with their bound folders.
    pub hosted: HashMap<LogicalAgentId, PathBuf>,
}

impl DeviceTeamMembership {
    /// Whether the device hosts at least one agent for the team.
    pub fn is_participating(&self) -> bool {
        !self.hosted.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(name: &str) -> LogicalAgentId {
        LogicalAgentId::new(name)
    }

    #[test]
    fn team_contains_roster_member() {
        let team = Team {
            roster: vec![agent("a"), agent("b")],
            team_realm_id: None,
        };
        assert!(team.contains(&agent("a")));
        assert!(!team.contains(&agent("c")));
    }

    #[test]
    fn registry_bind_and_lookup() {
        let mut reg = TeamBindingRegistry::new();
        reg.bind(agent("a"), PathBuf::from("/tmp/a"));
        assert_eq!(reg.folder_for(&agent("a")), Some(&PathBuf::from("/tmp/a")));
        assert_eq!(reg.folder_for(&agent("b")), None);
    }

    #[test]
    fn membership_intersects_roster_with_bindings() {
        let mut reg = TeamBindingRegistry::new();
        reg.bind(agent("a"), PathBuf::from("/tmp/a"));
        reg.bind(agent("unrelated"), PathBuf::from("/tmp/other"));

        let team = Team {
            roster: vec![agent("a"), agent("b")],
            team_realm_id: None,
        };
        let membership = reg.membership_for(&team);
        assert_eq!(membership.hosted.len(), 1);
        assert!(membership.hosted.contains_key(&agent("a")));
        assert!(!membership.hosted.contains_key(&agent("b")));
        assert!(!membership.hosted.contains_key(&agent("unrelated")));
        assert!(membership.is_participating());
    }

    #[test]
    fn empty_membership_not_participating() {
        let reg = TeamBindingRegistry::new();
        let team = Team::empty();
        assert!(!reg.membership_for(&team).is_participating());
    }
}

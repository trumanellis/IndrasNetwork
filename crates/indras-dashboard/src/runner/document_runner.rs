//! Runs document sync scenarios and emits state updates
//!
//! This module provides in-memory document sync scenarios that demonstrate
//! Automerge CRDT synchronization between multiple simulated peers.

use crate::state::document::{DocumentEvent, NoteSnapshot, PeerDocumentState};
use std::collections::HashMap;

/// Update types from document scenario execution
#[allow(dead_code)] // Some variants reserved for future features
#[derive(Debug, Clone)]
pub enum DocumentUpdate {
    PeerState {
        peer_name: String,
        notebook_name: String,
        notes: Vec<NoteInfo>,
        heads: Vec<String>,
    },
    Event(DocumentEvent),
    StepComplete {
        current: usize,
        total: usize,
    },
    ScenarioLoaded {
        name: String,
        total_steps: usize,
    },
    Complete {
        success: bool,
    },
    Error(String),
}

#[derive(Debug, Clone)]
pub struct NoteInfo {
    pub id: String,
    pub title: String,
    pub content_preview: String,
    pub author: String,
}

impl From<NoteInfo> for NoteSnapshot {
    fn from(info: NoteInfo) -> Self {
        NoteSnapshot {
            id: info.id,
            title: info.title,
            content_preview: info.content_preview,
            author: info.author,
        }
    }
}

/// Document scenario definitions (in-memory, no Lua)
#[allow(dead_code)] // Fields used in scenario definitions
#[derive(Clone)]
pub struct DocumentScenario {
    pub name: &'static str,
    pub description: &'static str,
    pub steps: Vec<ScenarioStep>,
}

#[allow(dead_code)] // Some variants reserved for future scenarios
#[derive(Clone, Debug)]
pub enum ScenarioStep {
    CreatePeer {
        name: String,
    },
    ForkFrom {
        source: String,
        target: String,
    },
    CreateNote {
        peer: String,
        title: String,
        content: String,
    },
    UpdateNote {
        peer: String,
        note_idx: usize,
        content: String,
    },
    Sync {
        from: String,
        to: String,
    },
    CheckConvergence,
    Phase {
        name: String,
    },
}

pub fn get_scenarios() -> Vec<DocumentScenario> {
    vec![
        DocumentScenario {
            name: "full_sync",
            description: "Basic sync between Alice and Bob",
            steps: vec![
                ScenarioStep::Phase {
                    name: "Setup".into(),
                },
                ScenarioStep::CreatePeer {
                    name: "Alice".into(),
                },
                ScenarioStep::ForkFrom {
                    source: "Alice".into(),
                    target: "Bob".into(),
                },
                ScenarioStep::Phase {
                    name: "Alice creates note".into(),
                },
                ScenarioStep::CreateNote {
                    peer: "Alice".into(),
                    title: "First Note".into(),
                    content: "Hello world".into(),
                },
                ScenarioStep::Phase {
                    name: "Sync to Bob".into(),
                },
                ScenarioStep::Sync {
                    from: "Alice".into(),
                    to: "Bob".into(),
                },
                ScenarioStep::CheckConvergence,
                ScenarioStep::Phase {
                    name: "Bob creates note".into(),
                },
                ScenarioStep::CreateNote {
                    peer: "Bob".into(),
                    title: "Bob's Note".into(),
                    content: "Hi Alice!".into(),
                },
                ScenarioStep::Phase {
                    name: "Sync to Alice".into(),
                },
                ScenarioStep::Sync {
                    from: "Bob".into(),
                    to: "Alice".into(),
                },
                ScenarioStep::CheckConvergence,
            ],
        },
        DocumentScenario {
            name: "concurrent",
            description: "Concurrent edits from multiple peers",
            steps: vec![
                ScenarioStep::Phase {
                    name: "Setup".into(),
                },
                ScenarioStep::CreatePeer {
                    name: "Alice".into(),
                },
                ScenarioStep::ForkFrom {
                    source: "Alice".into(),
                    target: "Bob".into(),
                },
                ScenarioStep::Phase {
                    name: "Concurrent edits".into(),
                },
                // Both create notes concurrently
                ScenarioStep::CreateNote {
                    peer: "Alice".into(),
                    title: "Alice's Work".into(),
                    content: "Draft 1".into(),
                },
                ScenarioStep::CreateNote {
                    peer: "Bob".into(),
                    title: "Bob's Work".into(),
                    content: "Ideas".into(),
                },
                ScenarioStep::Phase {
                    name: "Bidirectional sync".into(),
                },
                // Bidirectional sync
                ScenarioStep::Sync {
                    from: "Alice".into(),
                    to: "Bob".into(),
                },
                ScenarioStep::Sync {
                    from: "Bob".into(),
                    to: "Alice".into(),
                },
                ScenarioStep::CheckConvergence,
            ],
        },
        DocumentScenario {
            name: "offline",
            description: "Offline peer catches up after changes",
            steps: vec![
                ScenarioStep::Phase {
                    name: "Setup with 3 peers".into(),
                },
                ScenarioStep::CreatePeer {
                    name: "Alice".into(),
                },
                ScenarioStep::ForkFrom {
                    source: "Alice".into(),
                    target: "Bob".into(),
                },
                ScenarioStep::ForkFrom {
                    source: "Alice".into(),
                    target: "Carol".into(),
                },
                ScenarioStep::Phase {
                    name: "Carol goes offline".into(),
                },
                // Alice and Bob sync while Carol is "offline"
                ScenarioStep::CreateNote {
                    peer: "Alice".into(),
                    title: "Update 1".into(),
                    content: "Content".into(),
                },
                ScenarioStep::Sync {
                    from: "Alice".into(),
                    to: "Bob".into(),
                },
                ScenarioStep::Phase {
                    name: "Bob edits while Carol offline".into(),
                },
                ScenarioStep::CreateNote {
                    peer: "Bob".into(),
                    title: "Update 2".into(),
                    content: "More".into(),
                },
                ScenarioStep::Sync {
                    from: "Bob".into(),
                    to: "Alice".into(),
                },
                ScenarioStep::Phase {
                    name: "Carol comes online".into(),
                },
                // Carol comes online and syncs
                ScenarioStep::Sync {
                    from: "Alice".into(),
                    to: "Carol".into(),
                },
                ScenarioStep::CheckConvergence,
            ],
        },
    ]
}

pub fn get_scenario(name: &str) -> Option<DocumentScenario> {
    get_scenarios().into_iter().find(|s| s.name == name)
}

/// Simple in-memory notebook simulation for demonstration
/// This is a simplified version that doesn't use actual Automerge
/// but demonstrates the sync patterns
#[derive(Clone, Debug)]
pub struct SimulatedNotebook {
    pub name: String,
    pub peer_name: String,
    pub notes: Vec<SimulatedNote>,
    /// Simulated document version (increments with changes)
    pub version: u64,
    /// Simulated heads (just version as hex for demo)
    heads: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct SimulatedNote {
    pub id: String,
    pub title: String,
    pub content: String,
    pub author: String,
}

impl SimulatedNotebook {
    pub fn new(peer_name: &str) -> Self {
        let version = 0;
        Self {
            name: "Shared Notebook".into(),
            peer_name: peer_name.into(),
            notes: Vec::new(),
            version,
            heads: vec![format!("{:08x}", version)],
        }
    }

    pub fn fork(&self, new_peer: &str) -> Self {
        Self {
            name: self.name.clone(),
            peer_name: new_peer.into(),
            notes: self.notes.clone(),
            version: self.version,
            heads: self.heads.clone(),
        }
    }

    pub fn create_note(&mut self, title: &str, content: &str) -> String {
        let id = format!(
            "note-{}-{}",
            self.peer_name.to_lowercase(),
            self.notes.len() + 1
        );
        self.notes.push(SimulatedNote {
            id: id.clone(),
            title: title.into(),
            content: content.into(),
            author: self.peer_name.clone(),
        });
        self.version += 1;
        self.heads = vec![format!("{:08x}", self.version)];
        id
    }

    pub fn update_note(&mut self, idx: usize, content: &str) {
        if let Some(note) = self.notes.get_mut(idx) {
            note.content = content.into();
            self.version += 1;
            self.heads = vec![format!("{:08x}", self.version)];
        }
    }

    /// Simulate generating a sync message
    pub fn generate_sync_message(&self, _their_heads: &[String]) -> Vec<u8> {
        // Simplified: just serialize our notes
        let mut data = Vec::new();
        for note in &self.notes {
            data.extend(note.id.as_bytes());
            data.push(b'|');
            data.extend(note.title.as_bytes());
            data.push(b'|');
            data.extend(note.content.as_bytes());
            data.push(b'|');
            data.extend(note.author.as_bytes());
            data.push(b'\n');
        }
        data
    }

    /// Simulate applying a sync message
    pub fn apply_sync_message(&mut self, source: &SimulatedNotebook) -> bool {
        let mut changed = false;
        for note in &source.notes {
            if !self.notes.iter().any(|n| n.id == note.id) {
                self.notes.push(note.clone());
                changed = true;
            }
        }

        // After merging, both peers should converge to the same version
        // if they have the same content
        if changed {
            // We received new content, merge versions
            // Use max of both versions to indicate we've seen both histories
            self.version = self.version.max(source.version);
            self.heads = vec![format!("{:08x}", self.version)];
        } else if source.version > self.version {
            // No new notes, but source has a higher version (we're catching up)
            // Adopt the source version since we now have all its content
            self.version = source.version;
            self.heads = vec![format!("{:08x}", self.version)];
        }
        changed
    }

    pub fn heads(&self) -> Vec<String> {
        self.heads.clone()
    }

    pub fn to_peer_state(&self) -> PeerDocumentState {
        PeerDocumentState {
            peer_name: self.peer_name.clone(),
            notebook_name: self.name.clone(),
            notes: self
                .notes
                .iter()
                .map(|n| NoteSnapshot {
                    id: n.id.clone(),
                    title: n.title.clone(),
                    content_preview: if n.content.len() > 50 {
                        format!("{}...", &n.content[..47])
                    } else {
                        n.content.clone()
                    },
                    author: n.author.clone(),
                })
                .collect(),
            heads: self.heads.clone(),
            note_count: self.notes.len(),
        }
    }
}

/// Executes a document scenario step by step
pub struct DocumentRunner {
    pub notebooks: HashMap<String, SimulatedNotebook>,
    pub scenario: Option<DocumentScenario>,
    pub current_step: usize,
}

impl DocumentRunner {
    pub fn new() -> Self {
        Self {
            notebooks: HashMap::new(),
            scenario: None,
            current_step: 0,
        }
    }

    pub fn load_scenario(&mut self, name: &str) -> Result<usize, String> {
        let scenario = get_scenario(name).ok_or_else(|| format!("Unknown scenario: {}", name))?;
        let total_steps = scenario.steps.len();
        self.scenario = Some(scenario);
        self.current_step = 0;
        self.notebooks.clear();
        Ok(total_steps)
    }

    /// Execute the next step and return the update
    pub fn step(&mut self) -> Option<DocumentUpdate> {
        let scenario = self.scenario.as_ref()?;
        if self.current_step >= scenario.steps.len() {
            return Some(DocumentUpdate::Complete { success: true });
        }

        let step = scenario.steps[self.current_step].clone();
        self.current_step += 1;

        let event = match step {
            ScenarioStep::CreatePeer { name } => {
                let nb = SimulatedNotebook::new(&name);
                self.notebooks.insert(name.clone(), nb);
                DocumentEvent::PhaseChanged {
                    phase: format!("Created peer {}", name),
                    step: self.current_step,
                }
            }
            ScenarioStep::ForkFrom { source, target } => {
                if let Some(source_nb) = self.notebooks.get(&source) {
                    let forked = source_nb.fork(&target);
                    self.notebooks.insert(target.clone(), forked);
                }
                DocumentEvent::PhaseChanged {
                    phase: format!("Forked {} from {}", target, source),
                    step: self.current_step,
                }
            }
            ScenarioStep::CreateNote {
                peer,
                title,
                content,
            } => {
                let note_id = if let Some(nb) = self.notebooks.get_mut(&peer) {
                    nb.create_note(&title, &content)
                } else {
                    "unknown".into()
                };
                DocumentEvent::NoteCreated {
                    peer,
                    note_id,
                    title,
                    step: self.current_step,
                }
            }
            ScenarioStep::UpdateNote {
                peer,
                note_idx,
                content,
            } => {
                let note_id = if let Some(nb) = self.notebooks.get_mut(&peer) {
                    nb.update_note(note_idx, &content);
                    nb.notes
                        .get(note_idx)
                        .map(|n| n.id.clone())
                        .unwrap_or_default()
                } else {
                    "unknown".into()
                };
                DocumentEvent::NoteUpdated {
                    peer,
                    note_id,
                    step: self.current_step,
                }
            }
            ScenarioStep::Sync { from, to } => {
                let size_bytes;

                // Clone source notebook to avoid borrow issues
                let source_nb = self.notebooks.get(&from).cloned();

                if let (Some(source), Some(target)) = (source_nb, self.notebooks.get_mut(&to)) {
                    let sync_msg = source.generate_sync_message(&target.heads());
                    size_bytes = sync_msg.len();
                    let _changes_applied = target.apply_sync_message(&source);
                } else {
                    size_bytes = 0;
                }

                // Return sync generated event
                return Some(DocumentUpdate::Event(DocumentEvent::SyncGenerated {
                    from: from.clone(),
                    to: to.clone(),
                    size_bytes,
                    step: self.current_step,
                }));
            }
            ScenarioStep::CheckConvergence => {
                let heads: Vec<_> = self.notebooks.values().map(|nb| nb.heads()).collect();
                let converged = if heads.len() < 2 {
                    true
                } else {
                    let first = &heads[0];
                    heads.iter().all(|h| h == first)
                };

                if converged {
                    let peers: Vec<String> = self.notebooks.keys().cloned().collect();
                    DocumentEvent::Converged {
                        peers,
                        step: self.current_step,
                    }
                } else {
                    DocumentEvent::PhaseChanged {
                        phase: "Checking convergence - peers divergent".into(),
                        step: self.current_step,
                    }
                }
            }
            ScenarioStep::Phase { name } => DocumentEvent::PhaseChanged {
                phase: name,
                step: self.current_step,
            },
        };

        Some(DocumentUpdate::Event(event))
    }

    /// Get current state of all peers
    pub fn get_peer_states(&self) -> HashMap<String, PeerDocumentState> {
        self.notebooks
            .iter()
            .map(|(name, nb)| (name.clone(), nb.to_peer_state()))
            .collect()
    }

    /// Check if all peers have converged
    pub fn check_convergence(&self) -> bool {
        let heads: Vec<_> = self.notebooks.values().map(|nb| nb.heads()).collect();
        if heads.len() < 2 {
            return true;
        }
        let first = &heads[0];
        heads.iter().all(|h| h == first)
    }

    /// Get total steps in current scenario
    #[allow(dead_code)] // Reserved for future progress tracking
    pub fn total_steps(&self) -> usize {
        self.scenario.as_ref().map(|s| s.steps.len()).unwrap_or(0)
    }

    /// Check if scenario is complete
    #[allow(dead_code)] // Reserved for future scenario control
    pub fn is_complete(&self) -> bool {
        self.scenario
            .as_ref()
            .map(|s| self.current_step >= s.steps.len())
            .unwrap_or(true)
    }
}

impl Default for DocumentRunner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scenario_loading() {
        let mut runner = DocumentRunner::new();
        let steps = runner.load_scenario("full_sync").unwrap();
        assert!(steps > 0);
    }

    #[test]
    fn test_full_sync_scenario() {
        let mut runner = DocumentRunner::new();
        runner.load_scenario("full_sync").unwrap();

        // Run all steps
        while !runner.is_complete() {
            let _ = runner.step();
        }

        // Check that both peers have the same notes
        let states = runner.get_peer_states();
        assert!(states.contains_key("Alice"));
        assert!(states.contains_key("Bob"));

        let alice = &states["Alice"];
        let bob = &states["Bob"];
        assert_eq!(alice.note_count, bob.note_count);
    }

    #[test]
    fn test_convergence() {
        let mut runner = DocumentRunner::new();
        runner.load_scenario("concurrent").unwrap();

        // Run all steps
        while !runner.is_complete() {
            let _ = runner.step();
        }

        assert!(runner.check_convergence());
    }
}

//! Home realm event types.
//!
//! These events are emitted by the home realm Lua scenario and represent
//! the lifecycle of notes, quests, artifacts, and sessions from a user's perspective.

use serde::Deserialize;

/// Events emitted by the home realm scenario.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "event_type")]
pub enum HomeRealmEvent {
    // Session lifecycle
    #[serde(rename = "session_started")]
    SessionStarted {
        member: String,
        realm_id: String,
        tick: u32,
    },

    #[serde(rename = "session_ended")]
    SessionEnded {
        member: String,
        realm_id: String,
        tick: u32,
        notes_count: u32,
        quests_count: u32,
        artifacts_count: u32,
    },

    #[serde(rename = "data_recovered")]
    DataRecovered {
        member: String,
        realm_id: String,
        consistent: bool,
        tick: u32,
    },

    // Home realm identity
    #[serde(rename = "home_realm_id_computed")]
    HomeRealmIdComputed {
        member: String,
        realm_id: String,
        latency_us: f64,
        consistent: bool,
        tick: u32,
    },

    // Notes
    #[serde(rename = "note_created")]
    NoteCreated {
        member: String,
        note_id: String,
        title: String,
        tag_count: u32,
        latency_us: f64,
        tick: u32,
    },

    #[serde(rename = "note_updated")]
    NoteUpdated {
        member: String,
        note_id: String,
        latency_us: f64,
        tick: u32,
    },

    #[serde(rename = "note_deleted")]
    NoteDeleted {
        member: String,
        note_id: String,
        tick: u32,
    },

    // Quests
    #[serde(rename = "home_quest_created")]
    HomeQuestCreated {
        member: String,
        quest_id: String,
        title: String,
        latency_us: f64,
        tick: u32,
    },

    #[serde(rename = "home_quest_completed")]
    HomeQuestCompleted {
        member: String,
        quest_id: String,
        tick: u32,
    },

    // Artifacts
    #[serde(rename = "artifact_uploaded")]
    ArtifactUploaded {
        member: String,
        artifact_id: String,
        size: u64,
        mime_type: String,
        latency_us: f64,
        tick: u32,
    },

    #[serde(rename = "artifact_retrieved")]
    ArtifactRetrieved {
        member: String,
        artifact_id: String,
        latency_us: f64,
        tick: u32,
    },

    // Sync
    #[serde(rename = "multi_device_sync")]
    MultiDeviceSync {
        member: String,
        devices_checked: u32,
        consistent: bool,
        tick: u32,
    },

    // Utility
    #[serde(rename = "info")]
    Info { message: String },

    /// Catch-all for unknown events
    #[serde(other)]
    Unknown,
}

impl HomeRealmEvent {
    /// Returns the tick for this event, if applicable.
    pub fn tick(&self) -> u32 {
        match self {
            HomeRealmEvent::SessionStarted { tick, .. } => *tick,
            HomeRealmEvent::SessionEnded { tick, .. } => *tick,
            HomeRealmEvent::DataRecovered { tick, .. } => *tick,
            HomeRealmEvent::HomeRealmIdComputed { tick, .. } => *tick,
            HomeRealmEvent::NoteCreated { tick, .. } => *tick,
            HomeRealmEvent::NoteUpdated { tick, .. } => *tick,
            HomeRealmEvent::NoteDeleted { tick, .. } => *tick,
            HomeRealmEvent::HomeQuestCreated { tick, .. } => *tick,
            HomeRealmEvent::HomeQuestCompleted { tick, .. } => *tick,
            HomeRealmEvent::ArtifactUploaded { tick, .. } => *tick,
            HomeRealmEvent::ArtifactRetrieved { tick, .. } => *tick,
            HomeRealmEvent::MultiDeviceSync { tick, .. } => *tick,
            HomeRealmEvent::Info { .. } => 0,
            HomeRealmEvent::Unknown => 0,
        }
    }

    /// Returns the member ID for this event, if applicable.
    pub fn member(&self) -> Option<&str> {
        match self {
            HomeRealmEvent::SessionStarted { member, .. } => Some(member),
            HomeRealmEvent::SessionEnded { member, .. } => Some(member),
            HomeRealmEvent::DataRecovered { member, .. } => Some(member),
            HomeRealmEvent::HomeRealmIdComputed { member, .. } => Some(member),
            HomeRealmEvent::NoteCreated { member, .. } => Some(member),
            HomeRealmEvent::NoteUpdated { member, .. } => Some(member),
            HomeRealmEvent::NoteDeleted { member, .. } => Some(member),
            HomeRealmEvent::HomeQuestCreated { member, .. } => Some(member),
            HomeRealmEvent::HomeQuestCompleted { member, .. } => Some(member),
            HomeRealmEvent::ArtifactUploaded { member, .. } => Some(member),
            HomeRealmEvent::ArtifactRetrieved { member, .. } => Some(member),
            HomeRealmEvent::MultiDeviceSync { member, .. } => Some(member),
            HomeRealmEvent::Info { .. } => None,
            HomeRealmEvent::Unknown => None,
        }
    }

    /// Returns a short description of this event for the activity feed.
    pub fn description(&self) -> String {
        match self {
            HomeRealmEvent::SessionStarted { .. } => "Session started".to_string(),
            HomeRealmEvent::SessionEnded { .. } => "Session ended".to_string(),
            HomeRealmEvent::DataRecovered { consistent, .. } => {
                if *consistent {
                    "Data recovered successfully".to_string()
                } else {
                    "Data recovery with conflicts".to_string()
                }
            }
            HomeRealmEvent::HomeRealmIdComputed { .. } => "Home realm initialized".to_string(),
            HomeRealmEvent::NoteCreated { title, .. } => format!("Created note: {}", title),
            HomeRealmEvent::NoteUpdated { note_id, .. } => {
                format!("Updated note {}", &note_id[..8.min(note_id.len())])
            }
            HomeRealmEvent::NoteDeleted { note_id, .. } => {
                format!("Deleted note {}", &note_id[..8.min(note_id.len())])
            }
            HomeRealmEvent::HomeQuestCreated { title, .. } => format!("Created quest: {}", title),
            HomeRealmEvent::HomeQuestCompleted { quest_id, .. } => {
                format!("Completed quest {}", &quest_id[..8.min(quest_id.len())])
            }
            HomeRealmEvent::ArtifactUploaded { mime_type, .. } => {
                format!("Uploaded {} artifact", mime_type)
            }
            HomeRealmEvent::ArtifactRetrieved { .. } => "Retrieved artifact".to_string(),
            HomeRealmEvent::MultiDeviceSync { consistent, .. } => {
                if *consistent {
                    "Devices synced".to_string()
                } else {
                    "Sync conflict detected".to_string()
                }
            }
            HomeRealmEvent::Info { message } => message.clone(),
            HomeRealmEvent::Unknown => "Unknown event".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_note_created() {
        let json = r#"{"event_type":"note_created","member":"A","note_id":"note-123","title":"My Note","tag_count":3,"latency_us":150.5,"tick":42}"#;
        let event: HomeRealmEvent = serde_json::from_str(json).unwrap();
        match event {
            HomeRealmEvent::NoteCreated {
                member,
                title,
                tick,
                ..
            } => {
                assert_eq!(member, "A");
                assert_eq!(title, "My Note");
                assert_eq!(tick, 42);
            }
            _ => panic!("Expected NoteCreated"),
        }
    }

    #[test]
    fn test_parse_session_started() {
        let json =
            r#"{"event_type":"session_started","member":"B","realm_id":"realm-456","tick":1}"#;
        let event: HomeRealmEvent = serde_json::from_str(json).unwrap();
        assert!(matches!(event, HomeRealmEvent::SessionStarted { .. }));
    }

    #[test]
    fn test_parse_unknown() {
        let json = r#"{"event_type":"some_future_event","data":"stuff"}"#;
        let event: HomeRealmEvent = serde_json::from_str(json).unwrap();
        assert!(matches!(event, HomeRealmEvent::Unknown));
    }
}

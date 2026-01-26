//! State tracking for session information in the home realm.

use crate::events::HomeRealmEvent;

/// Status of the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SessionStatus {
    #[default]
    Inactive,
    Active,
    Ended,
}

/// Sync status of the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyncStatus {
    #[default]
    Unknown,
    Synced,
    Conflict,
}

/// State for tracking session information.
#[derive(Debug, Clone, Default)]
pub struct SessionState {
    /// Current session status.
    pub status: SessionStatus,

    /// Current sync status.
    pub sync_status: SyncStatus,

    /// The member this session belongs to.
    pub member: Option<String>,

    /// The home realm ID.
    pub realm_id: Option<String>,

    /// Tick when session started.
    pub started_tick: Option<u32>,

    /// Tick when session ended.
    pub ended_tick: Option<u32>,

    /// Whether home realm ID has been computed.
    pub home_realm_initialized: bool,

    /// Whether data recovery has occurred.
    pub data_recovered: bool,

    /// Number of devices checked in last sync.
    pub devices_synced: u32,
}

impl SessionState {
    /// Creates a new session state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Processes a home realm event that may affect session state.
    pub fn process_event(&mut self, event: &HomeRealmEvent) {
        match event {
            HomeRealmEvent::SessionStarted {
                member,
                realm_id,
                tick,
            } => {
                self.status = SessionStatus::Active;
                self.member = Some(member.clone());
                self.realm_id = Some(realm_id.clone());
                self.started_tick = Some(*tick);
                self.ended_tick = None;
            }
            HomeRealmEvent::SessionEnded { tick, .. } => {
                self.status = SessionStatus::Ended;
                self.ended_tick = Some(*tick);
            }
            HomeRealmEvent::HomeRealmIdComputed { consistent, .. } => {
                self.home_realm_initialized = true;
                if *consistent {
                    self.sync_status = SyncStatus::Synced;
                } else {
                    self.sync_status = SyncStatus::Conflict;
                }
            }
            HomeRealmEvent::DataRecovered { consistent, .. } => {
                self.data_recovered = true;
                if *consistent {
                    self.sync_status = SyncStatus::Synced;
                } else {
                    self.sync_status = SyncStatus::Conflict;
                }
            }
            HomeRealmEvent::MultiDeviceSync {
                devices_checked,
                consistent,
                ..
            } => {
                self.devices_synced = *devices_checked;
                if *consistent {
                    self.sync_status = SyncStatus::Synced;
                } else {
                    self.sync_status = SyncStatus::Conflict;
                }
            }
            _ => {}
        }
    }

    /// Returns a display string for the session status.
    pub fn status_display(&self) -> &str {
        match self.status {
            SessionStatus::Inactive => "Inactive",
            SessionStatus::Active => "Active",
            SessionStatus::Ended => "Ended",
        }
    }

    /// Returns a display string for the sync status.
    pub fn sync_display(&self) -> &str {
        match self.sync_status {
            SyncStatus::Unknown => "Unknown",
            SyncStatus::Synced => "Synced",
            SyncStatus::Conflict => "Conflict",
        }
    }

    /// Resets the state.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

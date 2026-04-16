//! Liveness heartbeats over realm message events.
//!
//! Each peer sends a tiny `Content::Extension` message into every DM realm
//! on a fixed cadence. Receivers subscribe to `realm.messages()`, filter
//! by the heartbeat `type_id`, and maintain an in-memory map of
//! `MemberId → last_seen_secs`. The map is the source of truth for the
//! Connections-column online indicator.
//!
//! Why messages, not CRDT writes: a heartbeat is ephemeral. CRDT history
//! grows monotonically and is wrong for fire-and-forget liveness. Realm
//! events flow through gossip without touching any persisted document.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use indras_network::prelude::StreamExt;
use indras_network::{Content, IndrasNetwork};
use tokio::time::MissedTickBehavior;

/// Type identifier for heartbeat extension messages. Receivers filter by this.
pub const HEARTBEAT_TYPE_ID: &str = "indras-sync-engine/heartbeat-v1";

/// Cadence (seconds) at which each peer broadcasts its heartbeat.
pub const HEARTBEAT_TICK_SECS: u64 = 3;

/// Threshold (seconds) past which a missing heartbeat means the peer is
/// considered offline. ~4 ticks to tolerate one missed publish.
pub const HEARTBEAT_STALE_AFTER_SECS: i64 = 12;

/// In-memory map of last-seen timestamps per peer, populated by the heartbeat
/// receiver. Cheap to clone; share via `Arc`.
#[derive(Debug, Default)]
pub struct PeerLiveness {
    last_seen: Mutex<HashMap<[u8; 32], i64>>,
}

impl PeerLiveness {
    /// Record that `peer` was seen at `ts_secs`. No-op if older than the
    /// existing entry (so out-of-order events don't regress).
    pub fn record(&self, peer: [u8; 32], ts_secs: i64) {
        let mut m = self.last_seen.lock().expect("PeerLiveness mutex poisoned");
        let entry = m.entry(peer).or_insert(0);
        if ts_secs > *entry {
            *entry = ts_secs;
        }
    }

    /// Whether `peer` has heartbeated within the staleness window.
    pub fn is_recently_seen(&self, peer: &[u8; 32], now_secs: i64) -> bool {
        let m = self.last_seen.lock().expect("PeerLiveness mutex poisoned");
        match m.get(peer) {
            Some(&ts) if ts > 0 => now_secs.saturating_sub(ts) <= HEARTBEAT_STALE_AFTER_SECS,
            _ => false,
        }
    }
}

/// Spawn the heartbeat sender + receiver tasks against `network`.
///
/// The sender broadcasts a heartbeat every `HEARTBEAT_TICK_SECS` into every
/// DM realm. The receiver discovers new DM realms periodically and starts a
/// per-realm subscription that records sender→timestamp into `liveness`.
pub fn start_heartbeat_loop(network: Arc<IndrasNetwork>, liveness: Arc<PeerLiveness>) {
    spawn_sender(network.clone());
    spawn_receiver_supervisor(network, liveness);
}

fn spawn_sender(network: Arc<IndrasNetwork>) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(HEARTBEAT_TICK_SECS));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            send_heartbeats(&network).await;
        }
    });
}

fn spawn_receiver_supervisor(network: Arc<IndrasNetwork>, liveness: Arc<PeerLiveness>) {
    tokio::spawn(async move {
        let mut subscribed: HashSet<[u8; 32]> = HashSet::new();
        loop {
            for realm_id in network.conversation_realms() {
                if network.dm_peer_for_realm(&realm_id).is_none() {
                    continue;
                }
                let key = *realm_id.as_bytes();
                if !subscribed.insert(key) {
                    continue;
                }
                let Some(realm) = network.get_realm_by_id(&realm_id) else {
                    subscribed.remove(&key);
                    continue;
                };
                let liveness_for_task = liveness.clone();
                tokio::spawn(async move {
                    let mut messages = Box::pin(realm.messages());
                    while let Some(msg) = messages.next().await {
                        if let Content::Extension { type_id, payload } = &msg.content {
                            if type_id == HEARTBEAT_TYPE_ID && payload.len() == 8 {
                                let mut buf = [0u8; 8];
                                buf.copy_from_slice(payload.as_slice());
                                let ts = i64::from_le_bytes(buf);
                                liveness_for_task.record(msg.sender.id(), ts);
                            }
                        }
                    }
                });
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
}

async fn send_heartbeats(network: &Arc<IndrasNetwork>) {
    let now = now_secs();
    let payload = now.to_le_bytes().to_vec();
    for realm_id in network.conversation_realms() {
        if network.dm_peer_for_realm(&realm_id).is_none() {
            continue;
        }
        let Some(realm) = network.get_realm_by_id(&realm_id) else { continue };
        let content = Content::Extension {
            type_id: HEARTBEAT_TYPE_ID.to_string(),
            payload: payload.clone(),
        };
        if let Err(e) = realm.send(content).await {
            tracing::debug!("heartbeat send failed: {e}");
        }
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

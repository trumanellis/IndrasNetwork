//! Background tasks for the peering lifecycle within `IndrasNetwork`.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use tokio::sync::{broadcast, watch, Notify};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::member::MemberId;
use crate::network::IndrasNetwork;

use super::{PeerEvent, PeerInfo};

/// Format a short hex ID for display when no name is available.
pub(crate) fn short_id(id: &MemberId) -> String {
    let hex: String = id[..4].iter().map(|b| format!("{b:02x}")).collect();
    format!("Peer {hex}")
}

/// Polls the contacts realm every `interval`, diffs against the previous set,
/// and emits `PeerConnected` / `PeerDisconnected` / `PeersChanged` events.
///
/// Also listens on `poll_notify` to allow on-demand immediate poll cycles
/// via [`IndrasNetwork::refresh_peers()`].
pub(crate) fn spawn_contact_poller(
    network: Arc<IndrasNetwork>,
    peers_tx: watch::Sender<Vec<PeerInfo>>,
    event_tx: broadcast::Sender<PeerEvent>,
    cancel: CancellationToken,
    interval: Duration,
    poll_notify: Arc<Notify>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut known: HashMap<MemberId, PeerInfo> = HashMap::new();
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = ticker.tick() => {}
                _ = poll_notify.notified() => {}
            }

            let contacts_realm = match network.contacts_realm().await {
                Some(cr) => cr,
                None => continue,
            };

            let contact_ids = contacts_realm.contacts_list().await;
            let my_id = network.id();

            let mut current: HashMap<MemberId, PeerInfo> = HashMap::new();
            for cid in &contact_ids {
                if *cid == my_id {
                    continue;
                }

                if let Some(existing) = known.get(cid) {
                    let entry = contacts_realm.get_contact_entry(cid).await;
                    let mut updated = existing.clone();
                    updated.sentiment = entry.as_ref().map(|e| e.sentiment).unwrap_or(0);
                    updated.status = entry.as_ref().map(|e| e.status).unwrap_or_default();
                    current.insert(*cid, updated);
                } else {
                    let entry = contacts_realm.get_contact_entry(cid).await;
                    let display_name = entry
                        .as_ref()
                        .and_then(|e| e.display_name.clone())
                        .unwrap_or_else(|| short_id(cid));

                    let info = PeerInfo {
                        member_id: *cid,
                        display_name,
                        connected_at: chrono::Utc::now().timestamp(),
                        sentiment: entry.as_ref().map(|e| e.sentiment).unwrap_or(0),
                        status: entry.as_ref().map(|e| e.status).unwrap_or_default(),
                    };

                    let _ = event_tx.send(PeerEvent::PeerConnected { peer: info.clone() });
                    current.insert(*cid, info);
                }
            }

            for old_id in known.keys() {
                if !current.contains_key(old_id) {
                    let _ = event_tx.send(PeerEvent::PeerDisconnected { member_id: *old_id });
                }
            }

            if current.len() != known.len()
                || current.iter().any(|(k, v)| known.get(k) != Some(v))
            {
                let peers_vec: Vec<PeerInfo> = current.values().cloned().collect();
                let _ = event_tx.send(PeerEvent::PeersChanged {
                    peers: peers_vec.clone(),
                });
                let _ = peers_tx.send(peers_vec);
            }

            known = current;
        }

        tracing::debug!("contact poller stopped");
    })
}

/// Forwards raw `GlobalEvent`s from the network's event stream into the
/// peering broadcast channel.
pub(crate) fn spawn_event_forwarder(
    network: Arc<IndrasNetwork>,
    event_tx: broadcast::Sender<PeerEvent>,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut stream = std::pin::pin!(network.events());

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                maybe_event = stream.next() => {
                    match maybe_event {
                        Some(ge) => {
                            let _ = event_tx.send(PeerEvent::NetworkEvent(ge));
                        }
                        None => break,
                    }
                }
            }
        }

        tracing::debug!("event forwarder stopped");
    })
}

/// Periodically saves the world view to disk.
pub(crate) fn spawn_periodic_saver(
    network: Arc<IndrasNetwork>,
    event_tx: broadcast::Sender<PeerEvent>,
    cancel: CancellationToken,
    interval: Duration,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = ticker.tick() => {}
            }

            match network.save_world_view().await {
                Ok(_) => {
                    let _ = event_tx.send(PeerEvent::WorldViewSaved);
                }
                Err(e) => {
                    let _ = event_tx.send(PeerEvent::Warning(
                        format!("world view save failed: {e}"),
                    ));
                }
            }
        }

        tracing::debug!("periodic saver stopped");
    })
}

/// Lightweight supervisor that checks task health every 30 seconds.
pub(crate) fn spawn_task_supervisor(
    event_tx: broadcast::Sender<PeerEvent>,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(30));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        // Skip the first immediate tick
        ticker.tick().await;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = ticker.tick() => {}
            }

            if event_tx.receiver_count() == 0 {
                tracing::debug!("task supervisor: no event subscribers");
            }
        }

        tracing::debug!("task supervisor stopped");
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contacts::ContactStatus;

    fn make_member_id(byte: u8) -> MemberId {
        MemberId::from([byte; 32])
    }

    #[test]
    fn short_id_formats_correctly() {
        let id = make_member_id(0xab);
        let s = short_id(&id);
        assert_eq!(s, "Peer abababab");
    }

    #[test]
    fn short_id_zero_bytes() {
        let id = make_member_id(0x00);
        let s = short_id(&id);
        assert_eq!(s, "Peer 00000000");
    }

    #[test]
    fn contact_diff_detects_new_and_departed_peers() {
        let id_a = make_member_id(0x01);
        let id_b = make_member_id(0x02);
        let id_c = make_member_id(0x03);

        let mut known: HashMap<MemberId, PeerInfo> = HashMap::new();
        known.insert(id_a, PeerInfo {
            member_id: id_a,
            display_name: "Peer A".into(),
            connected_at: 100,
            sentiment: 0,
            status: ContactStatus::default(),
        });
        known.insert(id_b, PeerInfo {
            member_id: id_b,
            display_name: "Peer B".into(),
            connected_at: 100,
            sentiment: 0,
            status: ContactStatus::default(),
        });

        let current_ids = vec![id_a, id_c];

        let new_peers: Vec<_> = current_ids.iter()
            .filter(|id| !known.contains_key(*id))
            .collect();
        assert_eq!(new_peers, vec![&id_c]);

        let current_set: std::collections::HashSet<_> = current_ids.iter().collect();
        let departed: Vec<_> = known.keys()
            .filter(|id| !current_set.contains(id))
            .collect();
        assert_eq!(departed, vec![&id_b]);
    }

    #[test]
    fn contact_diff_detects_value_changes() {
        let id_a = make_member_id(0x01);

        let mut known: HashMap<MemberId, PeerInfo> = HashMap::new();
        known.insert(id_a, PeerInfo {
            member_id: id_a,
            display_name: "Peer A".into(),
            connected_at: 100,
            sentiment: 0,
            status: ContactStatus::default(),
        });

        let mut current: HashMap<MemberId, PeerInfo> = HashMap::new();
        current.insert(id_a, PeerInfo {
            member_id: id_a,
            display_name: "Peer A".into(),
            connected_at: 100,
            sentiment: 1,
            status: ContactStatus::default(),
        });

        let key_only_changed = current.len() != known.len()
            || current.keys().any(|k| !known.contains_key(k));
        assert!(!key_only_changed);

        let value_changed = current.len() != known.len()
            || current.iter().any(|(k, v)| known.get(k) != Some(v));
        assert!(value_changed);
    }

    #[test]
    fn broadcast_channel_delivers_events() {
        let (tx, _) = broadcast::channel::<PeerEvent>(16);
        let mut rx1 = tx.subscribe();
        let mut rx2 = tx.subscribe();

        let peer = PeerInfo {
            member_id: make_member_id(0x01),
            display_name: "Test".into(),
            connected_at: 0,
            sentiment: 0,
            status: ContactStatus::default(),
        };
        tx.send(PeerEvent::PeerConnected { peer }).unwrap();

        assert!(matches!(rx1.try_recv(), Ok(PeerEvent::PeerConnected { .. })));
        assert!(matches!(rx2.try_recv(), Ok(PeerEvent::PeerConnected { .. })));
    }

    #[test]
    fn watch_channel_reflects_latest_peers() {
        let (tx, rx) = watch::channel::<Vec<PeerInfo>>(Vec::new());

        assert!(rx.borrow().is_empty());

        let peers = vec![PeerInfo {
            member_id: make_member_id(0x42),
            display_name: "Alice".into(),
            connected_at: 1000,
            sentiment: 1,
            status: ContactStatus::Confirmed,
        }];
        tx.send(peers).unwrap();

        let snapshot = rx.borrow().clone();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].display_name, "Alice");
    }

    #[tokio::test]
    async fn cancellation_token_stops_tasks() {
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();

        let handle = tokio::spawn(async move {
            tokio::select! {
                _ = cancel_clone.cancelled() => "cancelled",
                _ = tokio::time::sleep(Duration::from_secs(60)) => "timeout",
            }
        });

        cancel.cancel();
        let result = handle.await.unwrap();
        assert_eq!(result, "cancelled");
    }
}

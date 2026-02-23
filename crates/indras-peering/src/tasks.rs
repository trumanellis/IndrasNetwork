//! Background tasks spawned by `PeeringRuntime`.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use tokio::sync::{broadcast, watch};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use indras_network::{IndrasNetwork, MemberId};

use crate::event::{PeerEvent, PeerInfo};

/// Format a short hex ID for display when no name is available.
pub(crate) fn short_id(id: &MemberId) -> String {
    let hex: String = id[..4].iter().map(|b| format!("{b:02x}")).collect();
    format!("Peer {hex}")
}

/// Polls the contacts realm every `interval`, diffs against the previous set,
/// and emits `PeerConnected` / `PeerDisconnected` / `PeersChanged` events.
pub(crate) fn spawn_contact_poller(
    network: Arc<IndrasNetwork>,
    peers_tx: watch::Sender<Vec<PeerInfo>>,
    event_tx: broadcast::Sender<PeerEvent>,
    cancel: CancellationToken,
    interval: Duration,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut known: HashMap<MemberId, PeerInfo> = HashMap::new();
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = ticker.tick() => {}
            }

            let contacts_realm = match network.contacts_realm().await {
                Some(cr) => cr,
                None => continue,
            };

            let contact_ids = contacts_realm.contacts_list_async().await;
            let my_id = network.id();

            // Build the current set from the contacts document
            let mut current: HashMap<MemberId, PeerInfo> = HashMap::new();
            for cid in &contact_ids {
                if *cid == my_id {
                    continue;
                }

                if let Some(existing) = known.get(cid) {
                    // Preserve existing PeerInfo (keep connected_at timestamp)
                    current.insert(*cid, existing.clone());
                } else {
                    // Use short hex ID; proper names arrive via connect_by_code flow
                    let display_name = short_id(cid);

                    let info = PeerInfo {
                        member_id: *cid,
                        display_name,
                        connected_at: chrono::Utc::now().timestamp(),
                    };

                    let _ = event_tx.send(PeerEvent::PeerConnected { peer: info.clone() });
                    current.insert(*cid, info);
                }
            }

            // Detect disconnections (peers that were known but are no longer in contacts)
            for old_id in known.keys() {
                if !current.contains_key(old_id) {
                    let _ = event_tx.send(PeerEvent::PeerDisconnected { member_id: *old_id });
                }
            }

            // Emit full peers-changed if the set actually changed
            if current.len() != known.len()
                || current.keys().any(|k| !known.contains_key(k))
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

#[cfg(test)]
mod tests {
    use super::*;

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

    /// Simulate the contact poller's diff logic in isolation.
    #[test]
    fn contact_diff_detects_new_and_departed_peers() {
        // Simulate the "known" and "current" maps from the poller
        let id_a = make_member_id(0x01);
        let id_b = make_member_id(0x02);
        let id_c = make_member_id(0x03);

        let mut known: HashMap<MemberId, PeerInfo> = HashMap::new();
        known.insert(id_a, PeerInfo {
            member_id: id_a,
            display_name: "Peer A".into(),
            connected_at: 100,
        });
        known.insert(id_b, PeerInfo {
            member_id: id_b,
            display_name: "Peer B".into(),
            connected_at: 100,
        });

        // Current poll: A stays, B departs, C is new
        let current_ids = vec![id_a, id_c];

        // New peers = in current but not in known
        let new_peers: Vec<_> = current_ids.iter()
            .filter(|id| !known.contains_key(*id))
            .collect();
        assert_eq!(new_peers, vec![&id_c]);

        // Departed peers = in known but not in current
        let current_set: std::collections::HashSet<_> = current_ids.iter().collect();
        let departed: Vec<_> = known.keys()
            .filter(|id| !current_set.contains(id))
            .collect();
        assert_eq!(departed, vec![&id_b]);
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
        };
        tx.send(PeerEvent::PeerConnected { peer }).unwrap();

        // Both receivers get the event
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

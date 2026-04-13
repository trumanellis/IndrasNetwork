//! Integration test: desktop-app-level two-instance vault sync.
//!
//! Exercises the full pipeline that was missing prior to the wiring fix:
//! `Vault::create`/`join` → `initial_scan` → VaultWatcher → SyncToDisk →
//! RelayBlobSync across two peers.
//!
//! # Scenario
//!
//! 1. Instance A creates an account and a shared `Vault`.
//! 2. A writes `hello.txt` through the vault API.
//! 3. Instance B joins via A's invite.
//! 4. Assert: `hello.txt` appears on B's disk within 10 s.
//! 5. B edits `hello.txt`; assert the update reaches A within 10 s.
//!
//! Run (requires iroh transport):
//! ```sh
//! cargo test -p synchronicity-engine --test vault_sync_wiring -- --ignored
//! ```

use std::sync::Arc;
use std::time::Duration;

use indras_network::IndrasNetwork;
use indras_storage::{BlobStore, BlobStoreConfig};
use indras_sync_engine::vault::Vault;
use synchronicity_engine::vault_manager::VaultManager;
use tempfile::TempDir;
use tokio::time::sleep;

/// Build a fresh on-disk `BlobStore` for a test instance.
async fn build_blob_store(data_dir: &std::path::Path) -> Arc<BlobStore> {
    let cfg = BlobStoreConfig {
        base_dir: data_dir.join("shared-blobs"),
        ..Default::default()
    };
    Arc::new(BlobStore::new(cfg).await.expect("BlobStore::new"))
}

// ── helpers ──────────────────────────────────────────────────────────────────

/// Spin up an `IndrasNetwork` with a fresh identity in a temp directory.
///
/// Starts the network transport so peers can discover each other.
async fn build_network(name: &str, data_dir: &std::path::Path) -> Arc<IndrasNetwork> {
    let network = IndrasNetwork::builder()
        .data_dir(data_dir)
        .display_name(name)
        .build()
        .await
        .unwrap_or_else(|e| panic!("build_network({name}): {e}"));
    network
        .start()
        .await
        .unwrap_or_else(|e| panic!("start_network({name}): {e}"));
    network
}

/// Poll `path` until it exists and contains `expected_content`, or `deadline`
/// elapses.  Returns `true` if the condition was satisfied.
async fn poll_file_contains(
    path: &std::path::Path,
    expected_content: &[u8],
    deadline: Duration,
) -> bool {
    let end = tokio::time::Instant::now() + deadline;
    loop {
        if let Ok(data) = tokio::fs::read(path).await {
            if data == expected_content {
                return true;
            }
        }
        if tokio::time::Instant::now() >= end {
            return false;
        }
        sleep(Duration::from_millis(250)).await;
    }
}

// ── scenario ─────────────────────────────────────────────────────────────────

/// Two-instance vault sync: A creates, B joins, file flows A → B and B → A.
///
/// Marked `#[ignore]` because it requires the iroh transport layer.
#[tokio::test]
#[ignore] // requires transport
async fn test_vault_file_syncs_between_two_instances() {
    let tmp_a_data = TempDir::new().unwrap();
    let tmp_a_vault = TempDir::new().unwrap();
    let tmp_b_data = TempDir::new().unwrap();
    let tmp_b_vault = TempDir::new().unwrap();

    // ── instance A: create account + vault ───────────────────────────────────
    let net_a = build_network("A", tmp_a_data.path()).await;

    let blob_a = build_blob_store(tmp_a_data.path()).await;
    let (vault_a, invite) = Vault::create(
        &net_a,
        "sync-test-vault",
        tmp_a_vault.path().to_path_buf(),
        Arc::clone(&blob_a),
    )
    .await
    .expect("A: Vault::create failed");

    vault_a.initial_scan().await.expect("A: initial_scan failed");

    // ── A writes hello.txt ───────────────────────────────────────────────────
    let hello_content = b"hello from A";
    vault_a
        .write_file_content("hello.txt", hello_content)
        .await
        .expect("A: write_file_content failed");

    let hello_on_a = tmp_a_vault.path().join("hello.txt");
    assert!(
        hello_on_a.exists(),
        "hello.txt should exist in A's vault dir after write"
    );

    // ── instance B: join via invite ──────────────────────────────────────────
    let net_b = build_network("B", tmp_b_data.path()).await;

    let invite_str = invite.to_string();

    let blob_b = build_blob_store(tmp_b_data.path()).await;
    let vault_b = Vault::join(
        &net_b,
        &invite_str,
        tmp_b_vault.path().to_path_buf(),
        Arc::clone(&blob_b),
    )
    .await
    .expect("B: Vault::join failed");

    // CRDT convergence barrier — wait for membership to settle.
    let member_count = vault_b
        .await_members(2, Duration::from_secs(15))
        .await
        .expect("B: await_members failed");
    assert_eq!(
        member_count, 2,
        "Both A and B should be members of the vault realm"
    );

    // ── assert hello.txt materialises on B's disk ─────────────────────────
    let hello_on_b = tmp_b_vault.path().join("hello.txt");
    let arrived = poll_file_contains(&hello_on_b, hello_content, Duration::from_secs(10)).await;
    assert!(
        arrived,
        "hello.txt should appear on B's disk within 10s; vault_b files: {:?}",
        vault_b.list_files().await
    );

    // ── B edits hello.txt, assert update reaches A ────────────────────────
    let updated_content = b"edited by B";
    vault_b
        .write_file_content("hello.txt", updated_content)
        .await
        .expect("B: write updated hello.txt failed");

    let updated_on_a = poll_file_contains(&hello_on_a, updated_content, Duration::from_secs(10)).await;
    assert!(
        updated_on_a,
        "B's edit to hello.txt should propagate back to A within 10s; vault_a files: {:?}",
        vault_a.list_files().await
    );

    // Vaults stop on drop; stop the network transports explicitly.
    net_a.stop().await.ok();
    net_b.stop().await.ok();
}

/// DM realm vault sync: Love and Joy become mutual contacts via `connect()`,
/// both wire up `VaultManager::ensure_vault` on the DM realm, and files dropped
/// into either side's DM vault directory must propagate to the other.
///
/// This exercises the bug reported in the desktop app: the earlier fix only
/// covered invite-flow vaults (`Vault::create` / `Vault::join`). DM realms
/// come into existence via `IndrasNetwork::connect(peer_id)` which never ran
/// through the Vault attach/relay path properly.
#[tokio::test]
#[ignore] // requires transport
async fn test_dm_file_syncs_between_connected_peers() {
    use std::time::Instant;

    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .try_init();

    let tmp_a_data = TempDir::new().unwrap();
    let tmp_b_data = TempDir::new().unwrap();

    // Spin up Love (A) and Joy (B)
    let net_a = build_network("Love", tmp_a_data.path()).await;
    let net_b = build_network("Joy", tmp_b_data.path()).await;

    let a_id = net_a.id();
    let b_id = net_b.id();

    // Love connects to Joy — this is the DM flow under test.
    net_a.connect(b_id).await.expect("A: connect(B) failed");

    // Wait for the DM realm to appear on BOTH sides and be loadable via
    // `get_realm_by_id` (this is exactly what the polling loop in
    // home_vault.rs relies on).
    let dm_deadline = Instant::now() + Duration::from_secs(30);
    let dm_id = loop {
        let a_convs = net_a.conversation_realms();
        let b_convs = net_b.conversation_realms();
        let a_dm = a_convs
            .iter()
            .copied()
            .find(|r| net_a.dm_peer_for_realm(r) == Some(b_id));
        let b_dm = b_convs
            .iter()
            .copied()
            .find(|r| net_b.dm_peer_for_realm(r) == Some(a_id));
        if let (Some(a), Some(b)) = (a_dm, b_dm) {
            assert_eq!(a, b, "DM realm id must be deterministic and match on both sides");
            // Also require that `get_realm_by_id` returns Some — this is the
            // exact guard the polling loop uses before calling ensure_vault.
            if net_a.get_realm_by_id(&a).is_some() && net_b.get_realm_by_id(&b).is_some() {
                break a;
            }
        }
        if Instant::now() >= dm_deadline {
            panic!(
                "DM realm never materialised on both sides; a_convs={:?} b_convs={:?}",
                net_a.conversation_realms(),
                net_b.conversation_realms()
            );
        }
        sleep(Duration::from_millis(250)).await;
    };

    let realm_a = net_a.get_realm_by_id(&dm_id).expect("A: DM realm");
    let realm_b = net_b.get_realm_by_id(&dm_id).expect("B: DM realm");

    // Wire VaultManagers on both sides (mirroring what the UI polling loop does).
    let vm_a = Arc::new(VaultManager::new(tmp_a_data.path().join("vm")).await.unwrap());
    let vm_b = Arc::new(VaultManager::new(tmp_b_data.path().join("vm")).await.unwrap());

    vm_a.ensure_vault(&net_a, &realm_a).await.expect("A: ensure_vault");
    vm_b.ensure_vault(&net_b, &realm_b).await.expect("B: ensure_vault");

    let rid_bytes = *dm_id.as_bytes();
    let a_vault_path = vm_a.vault_path(&rid_bytes).await.expect("A: vault_path");
    let b_vault_path = vm_b.vault_path(&rid_bytes).await.expect("B: vault_path");

    // ── A writes FromLove.md into its DM vault directory ──────────────────
    let content_a = b"hello Joy, from Love";
    let a_file = a_vault_path.join("FromLove.md");
    tokio::fs::write(&a_file, content_a).await.unwrap();

    let b_file = b_vault_path.join("FromLove.md");
    let arrived_on_b =
        poll_file_contains(&b_file, content_a, Duration::from_secs(15)).await;
    assert!(
        arrived_on_b,
        "FromLove.md should appear on Joy's DM vault within 15s (b_vault_path={:?})",
        b_vault_path
    );

    // ── B writes back: FromJoy.md — should appear on A ────────────────────
    let content_b = b"hello Love, from Joy";
    let b_reply = b_vault_path.join("FromJoy.md");
    tokio::fs::write(&b_reply, content_b).await.unwrap();

    let a_reply = a_vault_path.join("FromJoy.md");
    let arrived_on_a =
        poll_file_contains(&a_reply, content_b, Duration::from_secs(15)).await;
    assert!(
        arrived_on_a,
        "FromJoy.md should appear on Love's DM vault within 15s (a_vault_path={:?})",
        a_vault_path
    );

    net_a.stop().await.ok();
    net_b.stop().await.ok();
}

/// Compile-time check that `vault_bridge` is publicly accessible from the crate.
#[test]
fn test_vault_bridge_module_is_public() {
    let _: fn() = || {
        let _ = std::hint::black_box(synchronicity_engine::vault_bridge::scan_vault);
    };
}

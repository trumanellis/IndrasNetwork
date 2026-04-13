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

/// Compile-time check that `vault_bridge` is publicly accessible from the crate.
#[test]
fn test_vault_bridge_module_is_public() {
    let _: fn() = || {
        let _ = std::hint::black_box(synchronicity_engine::vault_bridge::scan_vault);
    };
}

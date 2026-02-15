//! Integration tests for the shared filesystem access control system.
//!
//! Tests cover:
//! - Artifact upload to home realm via ArtifactIndex
//! - Grant access with all four modes (revocable, permanent, timed, transfer)
//! - Revoke access (skips permanent)
//! - Recall (removes revocable/timed, keeps permanent)
//! - Transfer ownership (sender gets revocable access back)
//! - Query: shared_with, artifact_index reads
//! - Persistence of artifact index across sessions

use indras_network::{AccessMode, HomeArtifactEntry, IndrasNetwork};
use tempfile::TempDir;

/// Helper: create a temp file with test data and return its path.
async fn create_test_file(dir: &std::path::Path, name: &str, data: &[u8]) -> std::path::PathBuf {
    let path = dir.join(name);
    tokio::fs::write(&path, data).await.unwrap();
    path
}

// ============================================================
// Scenario 1: Upload and Retrieve
// ============================================================

#[tokio::test]
async fn test_upload_artifact_to_index() {
    let tmp = TempDir::new().unwrap();
    let file_dir = TempDir::new().unwrap();
    let network = IndrasNetwork::new(tmp.path()).await.unwrap();
    let home = network.home_realm().await.unwrap();

    // Write a test file
    let file_path = create_test_file(file_dir.path(), "notes.txt", b"Hello, artifact!").await;

    // Upload via new filesystem API
    let artifact_id = home.upload(&file_path).await.unwrap();

    // Verify artifact ID is non-zero
    assert!(!artifact_id.bytes().iter().all(|&b| b == 0));

    // Verify it's in the artifact index
    let doc = home.artifact_index().await.unwrap();
    let index = doc.read().await;
    assert_eq!(index.active_count(), 1);

    let entries: Vec<&HomeArtifactEntry> = index.active_artifacts().collect();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "notes.txt");
    assert_eq!(entries[0].id, artifact_id);
}

#[tokio::test]
async fn test_upload_idempotent() {
    let tmp = TempDir::new().unwrap();
    let file_dir = TempDir::new().unwrap();
    let network = IndrasNetwork::new(tmp.path()).await.unwrap();
    let home = network.home_realm().await.unwrap();

    let file_path = create_test_file(file_dir.path(), "same.txt", b"duplicate test").await;

    // Upload twice — same content = same hash = idempotent
    let id1 = home.upload(&file_path).await.unwrap();
    let id2 = home.upload(&file_path).await.unwrap();

    assert_eq!(id1, id2);

    // Only one entry in the index
    let doc = home.artifact_index().await.unwrap();
    let index = doc.read().await;
    assert_eq!(index.active_count(), 1);
}

// ============================================================
// Scenario 2: Grant Access
// ============================================================

#[tokio::test]
async fn test_grant_revocable_access() {
    let tmp = TempDir::new().unwrap();
    let file_dir = TempDir::new().unwrap();
    let network = IndrasNetwork::new(tmp.path()).await.unwrap();
    let home = network.home_realm().await.unwrap();

    let file_path = create_test_file(file_dir.path(), "doc.pdf", b"PDF data").await;
    let artifact_id = home.upload(&file_path).await.unwrap();

    let grantee = [42u8; 32];
    home.grant_access(&artifact_id, grantee, AccessMode::Revocable)
        .await
        .unwrap();

    // Verify grantee can access
    let shared = home.shared_with(&grantee).await.unwrap();
    assert_eq!(shared.len(), 1);
    assert_eq!(shared[0].id, artifact_id);
}

#[tokio::test]
async fn test_grant_permanent_access() {
    let tmp = TempDir::new().unwrap();
    let file_dir = TempDir::new().unwrap();
    let network = IndrasNetwork::new(tmp.path()).await.unwrap();
    let home = network.home_realm().await.unwrap();

    let file_path = create_test_file(file_dir.path(), "photo.jpg", b"JPEG data").await;
    let artifact_id = home.upload(&file_path).await.unwrap();

    let grantee = [99u8; 32];
    home.grant_access(&artifact_id, grantee, AccessMode::Permanent)
        .await
        .unwrap();

    let shared = home.shared_with(&grantee).await.unwrap();
    assert_eq!(shared.len(), 1);

    // Verify grant mode is permanent
    let doc = home.artifact_index().await.unwrap();
    let index = doc.read().await;
    let entry = index.get(&artifact_id).unwrap();
    let grant = entry.grants.iter().find(|g| g.grantee == grantee).unwrap();
    assert!(grant.mode.allows_download());
    assert!(grant.mode.allows_reshare());
}

#[tokio::test]
async fn test_grant_timed_access() {
    let tmp = TempDir::new().unwrap();
    let file_dir = TempDir::new().unwrap();
    let network = IndrasNetwork::new(tmp.path()).await.unwrap();
    let home = network.home_realm().await.unwrap();

    let file_path = create_test_file(file_dir.path(), "temp.txt", b"temporary").await;
    let artifact_id = home.upload(&file_path).await.unwrap();

    let grantee = [55u8; 32];
    home.grant_access(
        &artifact_id,
        grantee,
        AccessMode::Timed { expires_at: 100 },
    )
    .await
    .unwrap();

    // Before expiry: accessible
    let shared = home.shared_with(&grantee).await.unwrap();
    assert_eq!(shared.len(), 1);

    // Verify it's timed
    let doc = home.artifact_index().await.unwrap();
    let index = doc.read().await;
    let entry = index.get(&artifact_id).unwrap();
    let grant = entry.grants.iter().find(|g| g.grantee == grantee).unwrap();
    assert!(grant.mode.is_expired(101));
    assert!(!grant.mode.is_expired(99));
}

// ============================================================
// Scenario 3: Revoke Access
// ============================================================

#[tokio::test]
async fn test_revoke_revocable_access() {
    let tmp = TempDir::new().unwrap();
    let file_dir = TempDir::new().unwrap();
    let network = IndrasNetwork::new(tmp.path()).await.unwrap();
    let home = network.home_realm().await.unwrap();

    let file_path = create_test_file(file_dir.path(), "revoke_me.txt", b"data").await;
    let artifact_id = home.upload(&file_path).await.unwrap();

    let grantee = [77u8; 32];
    home.grant_access(&artifact_id, grantee, AccessMode::Revocable)
        .await
        .unwrap();

    // Verify access
    assert_eq!(home.shared_with(&grantee).await.unwrap().len(), 1);

    // Revoke
    home.revoke_access(&artifact_id, &grantee).await.unwrap();

    // Verify no access
    assert_eq!(home.shared_with(&grantee).await.unwrap().len(), 0);
}

#[tokio::test]
async fn test_revoke_permanent_fails() {
    let tmp = TempDir::new().unwrap();
    let file_dir = TempDir::new().unwrap();
    let network = IndrasNetwork::new(tmp.path()).await.unwrap();
    let home = network.home_realm().await.unwrap();

    let file_path = create_test_file(file_dir.path(), "permanent.txt", b"data").await;
    let artifact_id = home.upload(&file_path).await.unwrap();

    let grantee = [88u8; 32];
    home.grant_access(&artifact_id, grantee, AccessMode::Permanent)
        .await
        .unwrap();

    // Revoke should fail for permanent
    let result = home.revoke_access(&artifact_id, &grantee).await;
    assert!(result.is_err());

    // Access should still exist
    assert_eq!(home.shared_with(&grantee).await.unwrap().len(), 1);
}

// ============================================================
// Scenario 4: Recall
// ============================================================

#[tokio::test]
async fn test_recall_removes_revocable_keeps_permanent() {
    let tmp = TempDir::new().unwrap();
    let file_dir = TempDir::new().unwrap();
    let network = IndrasNetwork::new(tmp.path()).await.unwrap();
    let home = network.home_realm().await.unwrap();

    let file_path = create_test_file(file_dir.path(), "recall_test.txt", b"data").await;
    let artifact_id = home.upload(&file_path).await.unwrap();

    let revocable_grantee = [10u8; 32];
    let permanent_grantee = [20u8; 32];

    home.grant_access(&artifact_id, revocable_grantee, AccessMode::Revocable)
        .await
        .unwrap();
    home.grant_access(&artifact_id, permanent_grantee, AccessMode::Permanent)
        .await
        .unwrap();

    // Both have access
    assert_eq!(home.shared_with(&revocable_grantee).await.unwrap().len(), 1);
    assert_eq!(home.shared_with(&permanent_grantee).await.unwrap().len(), 1);

    // Recall
    let recalled = home.recall(&artifact_id).await.unwrap();
    assert!(recalled);

    // Verify: revocable grant removed, permanent kept
    let doc = home.artifact_index().await.unwrap();
    let index = doc.read().await;
    let entry = index.get(&artifact_id).unwrap();

    let has_revocable = entry.grants.iter().any(|g| g.grantee == revocable_grantee);
    let has_permanent = entry.grants.iter().any(|g| g.grantee == permanent_grantee);
    assert!(!has_revocable, "Revocable grant should be removed by recall");
    assert!(has_permanent, "Permanent grant should survive recall");
}

// ============================================================
// Scenario 5: Transfer
// ============================================================

#[tokio::test]
async fn test_transfer_ownership() {
    let tmp = TempDir::new().unwrap();
    let file_dir = TempDir::new().unwrap();
    let network = IndrasNetwork::new(tmp.path()).await.unwrap();
    let home = network.home_realm().await.unwrap();

    let file_path = create_test_file(file_dir.path(), "transfer_me.txt", b"data").await;
    let artifact_id = home.upload(&file_path).await.unwrap();

    let recipient = [50u8; 32];

    // Transfer
    let new_entry = home.transfer(&artifact_id, recipient).await.unwrap();

    // Verify new entry is for recipient
    assert_eq!(new_entry.id, artifact_id);

    // Verify sender (us) gets revocable access in new entry
    let our_id = network.id();
    let sender_grant = new_entry.grants.iter().find(|g| g.grantee == our_id);
    assert!(sender_grant.is_some(), "Sender should get revocable access back");
    assert_eq!(sender_grant.unwrap().mode, AccessMode::Revocable);

    // Verify original is marked as transferred
    let doc = home.artifact_index().await.unwrap();
    let index = doc.read().await;
    let original = index.get(&artifact_id).unwrap();
    assert!(matches!(
        original.status,
        indras_network::ArtifactStatus::Transferred { .. }
    ));
}

#[tokio::test]
async fn test_transfer_already_transferred_fails() {
    let tmp = TempDir::new().unwrap();
    let file_dir = TempDir::new().unwrap();
    let network = IndrasNetwork::new(tmp.path()).await.unwrap();
    let home = network.home_realm().await.unwrap();

    let file_path = create_test_file(file_dir.path(), "double_transfer.txt", b"data").await;
    let artifact_id = home.upload(&file_path).await.unwrap();

    let recipient1 = [60u8; 32];
    let recipient2 = [70u8; 32];

    // First transfer succeeds
    home.transfer(&artifact_id, recipient1).await.unwrap();

    // Second transfer fails
    let result = home.transfer(&artifact_id, recipient2).await;
    assert!(result.is_err());
}

// ============================================================
// Scenario 6: Multiple Artifacts Independence
// ============================================================

#[tokio::test]
async fn test_multiple_artifacts_independent_access() {
    let tmp = TempDir::new().unwrap();
    let file_dir = TempDir::new().unwrap();
    let network = IndrasNetwork::new(tmp.path()).await.unwrap();
    let home = network.home_realm().await.unwrap();

    let path_a = create_test_file(file_dir.path(), "a.txt", b"artifact A").await;
    let path_b = create_test_file(file_dir.path(), "b.txt", b"artifact B").await;

    let id_a = home.upload(&path_a).await.unwrap();
    let _id_b = home.upload(&path_b).await.unwrap();

    let grantee = [33u8; 32];

    // Grant access only to artifact A
    home.grant_access(&id_a, grantee, AccessMode::Revocable)
        .await
        .unwrap();

    // Grantee should only see artifact A
    let shared = home.shared_with(&grantee).await.unwrap();
    assert_eq!(shared.len(), 1);
    assert_eq!(shared[0].id, id_a);

    // Recall artifact A — B should remain unaffected
    home.recall(&id_a).await.unwrap();

    let doc = home.artifact_index().await.unwrap();
    let index = doc.read().await;
    assert_eq!(index.active_count(), 1); // Only B is active
}

// ============================================================
// Scenario 7: Persistence of ArtifactIndex
// ============================================================

#[tokio::test]
async fn test_artifact_index_persists_across_sessions() {
    let tmp = TempDir::new().unwrap();
    let file_dir = TempDir::new().unwrap();
    let data_dir = tmp.path().to_path_buf();

    // Session 1: Upload and grant
    let (artifact_id, grantee) = {
        let network = IndrasNetwork::new(&data_dir).await.unwrap();
        let home = network.home_realm().await.unwrap();

        let file_path =
            create_test_file(file_dir.path(), "persistent.txt", b"persisted data").await;
        let artifact_id = home.upload(&file_path).await.unwrap();

        let grantee = [44u8; 32];
        home.grant_access(&artifact_id, grantee, AccessMode::Permanent)
            .await
            .unwrap();

        (artifact_id, grantee)
    }; // network drops, data flushed

    // Session 2: Verify persistence
    {
        let network = IndrasNetwork::new(&data_dir).await.unwrap();
        let home = network.home_realm().await.unwrap();

        // Artifact index should have the entry
        let doc = home.artifact_index().await.unwrap();
        let index = doc.read().await;
        assert_eq!(index.active_count(), 1);

        let entry = index.get(&artifact_id).unwrap();
        assert_eq!(entry.name, "persistent.txt");

        // Grant should persist
        let grant = entry.grants.iter().find(|g| g.grantee == grantee);
        assert!(grant.is_some());
        assert!(grant.unwrap().mode.allows_download());

        // shared_with should still work
        let shared = home.shared_with(&grantee).await.unwrap();
        assert_eq!(shared.len(), 1);
    }
}

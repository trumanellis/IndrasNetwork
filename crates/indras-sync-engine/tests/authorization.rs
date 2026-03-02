//! Authorization and rollback tests for sync-engine.
//!
//! Tests verify that:
//! - Only the intention creator can verify claims and complete intentions
//! - bless_claim rejects empty event_indices
//! - try_update rolls back document state on closure error

use indras_network::IndrasNetwork;
use indras_sync_engine::{HomeRealmIntentions, RealmIntentions, RealmBlessings, IntentionDocument};
use tempfile::TempDir;

/// A member ID that is NOT the network owner.
fn impostor_id() -> [u8; 32] {
    [0xFFu8; 32]
}

// ============================================================
// Auth tests via RealmIntentions on a shared realm
// (explicit creator + caller parameters)
// ============================================================

#[tokio::test]
async fn verify_claim_rejects_non_creator() {
    let tmp = TempDir::new().unwrap();
    let network = IndrasNetwork::new(tmp.path()).await.unwrap();
    let my_id = network.id();

    // Create a shared realm so we can use RealmIntentions with explicit creator
    let realm = network.create_realm("auth-test").await.unwrap();

    // Create intention with my_id as creator
    let intention_id = realm
        .create_intention("Auth test", "Testing auth", None, my_id)
        .await
        .unwrap();

    // Submit a claim so there's something to verify
    realm
        .submit_service_claim(intention_id, impostor_id(), None)
        .await
        .unwrap();

    // Impostor tries to verify -> should fail
    let result = realm.verify_service_claim(intention_id, 0, impostor_id()).await;
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("Not authorized"),
        "Expected auth error, got: {}",
        err_msg
    );
}

#[tokio::test]
async fn complete_intention_rejects_non_creator() {
    let tmp = TempDir::new().unwrap();
    let network = IndrasNetwork::new(tmp.path()).await.unwrap();
    let my_id = network.id();

    let realm = network.create_realm("auth-test-complete").await.unwrap();

    let intention_id = realm
        .create_intention("Complete auth test", "Testing", None, my_id)
        .await
        .unwrap();

    // Impostor tries to complete -> should fail
    let result = realm.complete_intention(intention_id, impostor_id()).await;
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("Not authorized"),
        "Expected auth error, got: {}",
        err_msg
    );
}

#[tokio::test]
async fn verify_claim_allows_creator() {
    let tmp = TempDir::new().unwrap();
    let network = IndrasNetwork::new(tmp.path()).await.unwrap();
    let my_id = network.id();

    let realm = network.create_realm("auth-test-allow-verify").await.unwrap();

    let intention_id = realm
        .create_intention("Creator verify", "Testing", None, my_id)
        .await
        .unwrap();

    // Submit a claim first
    realm
        .submit_service_claim(intention_id, impostor_id(), None)
        .await
        .unwrap();

    // Creator verifies -> should succeed
    let result = realm.verify_service_claim(intention_id, 0, my_id).await;
    assert!(result.is_ok(), "Creator should be able to verify: {:?}", result);
}

#[tokio::test]
async fn complete_intention_allows_creator() {
    let tmp = TempDir::new().unwrap();
    let network = IndrasNetwork::new(tmp.path()).await.unwrap();
    let my_id = network.id();

    let realm = network.create_realm("auth-test-allow-complete").await.unwrap();

    let intention_id = realm
        .create_intention("Creator complete", "Testing", None, my_id)
        .await
        .unwrap();

    // Creator completes -> should succeed
    let result = realm.complete_intention(intention_id, my_id).await;
    assert!(result.is_ok(), "Creator should be able to complete: {:?}", result);
}

// ============================================================
// bless_claim empty event_indices rejection (on shared realm)
// ============================================================

#[tokio::test]
async fn bless_claim_rejects_empty_events() {
    let tmp = TempDir::new().unwrap();
    let network = IndrasNetwork::new(tmp.path()).await.unwrap();
    let my_id = network.id();

    let realm = network.create_realm("bless-test").await.unwrap();

    let intention_id = realm
        .create_intention("Bless test", "Testing", None, my_id)
        .await
        .unwrap();

    // Bless with empty event_indices -> should fail immediately (before attention check)
    let result = realm.bless_claim(intention_id, impostor_id(), my_id, vec![]).await;
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("empty event indices"),
        "Expected empty events error, got: {}",
        err_msg
    );
}

// ============================================================
// try_update rollback on closure error (on home realm)
// ============================================================

#[tokio::test]
async fn try_update_rollback_on_error() {
    let tmp = TempDir::new().unwrap();
    let network = IndrasNetwork::new(tmp.path()).await.unwrap();
    let home = network.home_realm().await.unwrap();

    // Create an intention first
    let intention_id = home
        .create_intention("Rollback test", "Testing rollback", None)
        .await
        .unwrap();

    // Get the intentions document
    let doc = home.intentions().await.unwrap();

    // Read initial state
    let initial_count = {
        let guard = doc.read().await;
        guard.intentions.len()
    };

    // try_update with error closure: attempt to find a non-existent intention
    let fake_id = [0xABu8; 16];
    let result: indras_network::error::Result<()> = doc
        .try_update(|d: &mut IntentionDocument| {
            let intention = d
                .find_mut(&fake_id)
                .ok_or_else(|| indras_network::error::IndraError::InvalidOperation(
                    "Intention not found".into(),
                ))?;
            intention
                .complete()
                .map_err(|e| indras_network::error::IndraError::InvalidOperation(e.to_string()))
        })
        .await;
    assert!(result.is_err(), "try_update with failing closure should return Err");

    // Verify state unchanged after rollback
    let after_count = {
        let guard = doc.read().await;
        guard.intentions.len()
    };
    assert_eq!(initial_count, after_count, "State should be unchanged after rollback");

    // Verify the original intention is still intact and not completed
    let guard = doc.read().await;
    let intention = guard.find(&intention_id).unwrap();
    assert_eq!(intention.title, "Rollback test");
    assert!(
        intention.completed_at_millis.is_none(),
        "Intention should not be completed after failed try_update"
    );
}

// ============================================================
// Not-found errors via RealmIntentions
// ============================================================

#[tokio::test]
async fn verify_nonexistent_intention_returns_error() {
    let tmp = TempDir::new().unwrap();
    let network = IndrasNetwork::new(tmp.path()).await.unwrap();
    let my_id = network.id();

    let realm = network.create_realm("not-found-test").await.unwrap();

    let fake_id = [0xDEu8; 16];
    let result = realm.verify_service_claim(fake_id, 0, my_id).await;
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("not found"),
        "Expected not-found error, got: {}",
        err_msg
    );
}

#[tokio::test]
async fn complete_nonexistent_intention_returns_error() {
    let tmp = TempDir::new().unwrap();
    let network = IndrasNetwork::new(tmp.path()).await.unwrap();
    let my_id = network.id();

    let realm = network.create_realm("not-found-complete-test").await.unwrap();

    let fake_id = [0xDEu8; 16];
    let result = realm.complete_intention(fake_id, my_id).await;
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("not found"),
        "Expected not-found error, got: {}",
        err_msg
    );
}

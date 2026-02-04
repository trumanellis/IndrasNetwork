//! Integration tests for the Home Realm feature.
//!
//! Tests cover:
//! - Home realm creation and access
//! - Quest creation in home realm
//! - Image/artifact upload
//! - Markdown note creation and updates
//! - Persistence across restarts

use indras_network::{HomeArtifactMetadata, IndrasNetwork};
use indras_sync_engine::{HomeRealmNotes, HomeRealmQuests, NoteDocument};
use tempfile::TempDir;

/// Helper to generate test PNG data (minimal valid PNG).
fn generate_test_png() -> Vec<u8> {
    // Minimal valid 1x1 PNG image
    vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
        0x00, 0x00, 0x00, 0x0D, // IHDR length
        0x49, 0x48, 0x44, 0x52, // IHDR
        0x00, 0x00, 0x00, 0x01, // width = 1
        0x00, 0x00, 0x00, 0x01, // height = 1
        0x08, 0x02, // bit depth = 8, color type = 2 (RGB)
        0x00, 0x00, 0x00, // compression, filter, interlace
        0x90, 0x77, 0x53, 0xDE, // CRC
        0x00, 0x00, 0x00, 0x0C, // IDAT length
        0x49, 0x44, 0x41, 0x54, // IDAT
        0x08, 0xD7, 0x63, 0xF8, 0x0F, 0x00, 0x00, 0x01, 0x01, 0x00, 0x05, 0xFE, // compressed data
        0xD2, 0x8F, // CRC (approximate)
        0x00, 0x00, 0x00, 0x00, // IEND length
        0x49, 0x45, 0x4E, 0x44, // IEND
        0xAE, 0x42, 0x60, 0x82, // CRC
    ]
}

// ============================================================
// Scenario 1: Home Realm Quest Creation
// ============================================================

#[tokio::test]
async fn test_home_realm_exists_after_creation() {
    let tmp = TempDir::new().unwrap();
    let network = IndrasNetwork::new(tmp.path()).await.unwrap();

    // Home realm should be accessible
    let home = network.home_realm().await.unwrap();

    // Verify it has the expected deterministic ID
    let expected_id = indras_network::home_realm_id(network.id());
    assert_eq!(home.id(), expected_id);

    // Verify our member ID matches
    assert_eq!(home.member_id(), network.id());
}

#[tokio::test]
async fn test_home_realm_id_deterministic() {
    let tmp = TempDir::new().unwrap();
    let network = IndrasNetwork::new(tmp.path()).await.unwrap();

    // Get home realm twice
    let home1 = network.home_realm().await.unwrap();
    let home2 = network.home_realm().await.unwrap();

    // Should be the same realm
    assert_eq!(home1.id(), home2.id());
}

#[tokio::test]
async fn test_create_quest_in_home_realm() {
    let tmp = TempDir::new().unwrap();
    let network = IndrasNetwork::new(tmp.path()).await.unwrap();

    // Get home realm
    let home = network.home_realm().await.unwrap();

    // Create a personal quest
    let quest_id = home
        .create_quest("Personal Task", "Do something productive", None)
        .await
        .unwrap();

    // Verify quest exists
    let quests = home.quests().await.unwrap();
    let doc = quests.read().await;
    // Just our quest (welcome quest is seeded by SyncEngine, not by the SDK)
    assert_eq!(doc.quests.len(), 1);
    let quest = doc.find(&quest_id).unwrap();
    assert_eq!(quest.title, "Personal Task");
    assert_eq!(quest.description, "Do something productive");
    assert_eq!(quest.creator, network.id());
}

#[tokio::test]
async fn test_complete_quest_in_home_realm() {
    let tmp = TempDir::new().unwrap();
    let network = IndrasNetwork::new(tmp.path()).await.unwrap();
    let home = network.home_realm().await.unwrap();

    // Create and complete a quest
    let quest_id = home
        .create_quest("Task to complete", "Finish this", None)
        .await
        .unwrap();

    home.complete_quest(quest_id).await.unwrap();

    // Verify quest is complete
    let quests = home.quests().await.unwrap();
    let doc = quests.read().await;
    let quest = doc.find(&quest_id).unwrap();
    assert!(quest.is_complete());
}

// ============================================================
// Scenario 2: Image Upload to Home Realm
// ============================================================

#[tokio::test]
async fn test_upload_image_to_home_realm() {
    let tmp = TempDir::new().unwrap();
    let network = IndrasNetwork::new(tmp.path()).await.unwrap();
    let home = network.home_realm().await.unwrap();

    // Create test image data
    let image_data = generate_test_png();
    let image_size = image_data.len() as u64;

    // Upload as artifact
    let artifact_id = home
        .share_artifact(
            image_data.clone(),
            HomeArtifactMetadata {
                name: "test_image.png".to_string(),
                mime_type: Some("image/png".to_string()),
                size: image_size,
            },
        )
        .await
        .unwrap();

    // Verify artifact ID is non-zero (it's a blake3 hash)
    assert!(!artifact_id.iter().all(|&b| b == 0));

    // Retrieve artifact
    let retrieved = home.get_artifact(&artifact_id).await.unwrap();
    assert_eq!(retrieved, image_data);
}

#[tokio::test]
async fn test_create_quest_with_image() {
    let tmp = TempDir::new().unwrap();
    let network = IndrasNetwork::new(tmp.path()).await.unwrap();
    let home = network.home_realm().await.unwrap();

    // First upload an image
    let image_data = generate_test_png();
    let artifact_id = home
        .share_artifact(
            image_data,
            HomeArtifactMetadata {
                name: "task_image.png".to_string(),
                mime_type: Some("image/png".to_string()),
                size: 100,
            },
        )
        .await
        .unwrap();

    // Create quest with image reference
    let quest_id = home
        .create_quest("Task with Image", "See attached", Some(artifact_id))
        .await
        .unwrap();

    // Verify image reference
    let quests = home.quests().await.unwrap();
    let doc = quests.read().await;
    let quest = doc.find(&quest_id).unwrap();
    assert_eq!(quest.image, Some(artifact_id));
}

// ============================================================
// Scenario 3: Markdown Document (Note) Creation
// ============================================================

#[tokio::test]
async fn test_create_markdown_note() {
    let tmp = TempDir::new().unwrap();
    let network = IndrasNetwork::new(tmp.path()).await.unwrap();
    let home = network.home_realm().await.unwrap();

    // Create a markdown note
    let note_id = home
        .create_note(
            "Meeting Notes",
            "# Project Update\n\n- Item 1\n- Item 2",
            vec!["work".to_string(), "meeting".to_string()],
        )
        .await
        .unwrap();

    // Verify note exists
    let notes = home.notes().await.unwrap();
    let doc = notes.read().await;
    assert_eq!(doc.notes.len(), 1);

    let note = doc.find(&note_id).unwrap();
    assert_eq!(note.title, "Meeting Notes");
    assert!(note.content.contains("# Project Update"));
    assert_eq!(note.tags, vec!["work", "meeting"]);
    assert_eq!(note.author, network.id());
}

#[tokio::test]
async fn test_update_markdown_note() {
    let tmp = TempDir::new().unwrap();
    let network = IndrasNetwork::new(tmp.path()).await.unwrap();
    let home = network.home_realm().await.unwrap();

    // Create a note
    let note_id = home
        .create_note("Original Title", "Original content", vec![])
        .await
        .unwrap();

    // Get original timestamps
    let notes = home.notes().await.unwrap();
    let original_created = notes.read().await.find(&note_id).unwrap().created_at_millis;

    // Small delay to ensure timestamp changes
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Update note content
    home.update_note(note_id, "# Updated Content\n\nNew text")
        .await
        .unwrap();

    // Verify update
    let notes = home.notes().await.unwrap();
    let doc = notes.read().await;
    let note = doc.find(&note_id).unwrap();
    assert!(note.content.contains("Updated Content"));
    assert!(note.updated_at_millis >= original_created);
}

#[tokio::test]
async fn test_delete_note() {
    let tmp = TempDir::new().unwrap();
    let network = IndrasNetwork::new(tmp.path()).await.unwrap();
    let home = network.home_realm().await.unwrap();

    // Create a note
    let note_id = home
        .create_note("To Delete", "Content", vec![])
        .await
        .unwrap();

    // Verify it exists
    let notes = home.notes().await.unwrap();
    assert_eq!(notes.read().await.len(), 1);

    // Delete it
    let removed = home.delete_note(note_id).await.unwrap();
    assert!(removed.is_some());
    assert_eq!(removed.unwrap().title, "To Delete");

    // Verify it's gone
    let notes = home.notes().await.unwrap();
    assert_eq!(notes.read().await.len(), 0);
}

#[tokio::test]
async fn test_notes_with_tags() {
    let tmp = TempDir::new().unwrap();
    let network = IndrasNetwork::new(tmp.path()).await.unwrap();
    let home = network.home_realm().await.unwrap();

    // Create notes with different tags
    home.create_note("Work Note 1", "Content", vec!["work".to_string()])
        .await
        .unwrap();
    home.create_note("Personal Note", "Content", vec!["personal".to_string()])
        .await
        .unwrap();
    home.create_note(
        "Work Note 2",
        "Content",
        vec!["work".to_string(), "urgent".to_string()],
    )
    .await
    .unwrap();

    // Query by tag
    let notes = home.notes().await.unwrap();
    let doc = notes.read().await;

    let work_notes = doc.notes_with_tag("work");
    assert_eq!(work_notes.len(), 2);

    let personal_notes = doc.notes_with_tag("personal");
    assert_eq!(personal_notes.len(), 1);

    let urgent_notes = doc.notes_with_tag("urgent");
    assert_eq!(urgent_notes.len(), 1);
}

// ============================================================
// Scenario 4: Persistence Across Restarts
// ============================================================

#[tokio::test]
async fn test_home_realm_persistence() {
    let tmp = TempDir::new().unwrap();
    let data_dir = tmp.path().to_path_buf();

    // ===== Session 1: Create content =====
    let (member_id, quest_id, note_id, artifact_id) = {
        let network = IndrasNetwork::new(&data_dir).await.unwrap();
        let member_id = network.id();
        let home = network.home_realm().await.unwrap();

        // Create quest
        let quest_id = home
            .create_quest("Persistent Quest", "Should survive restart", None)
            .await
            .unwrap();

        // Upload image
        let image_data = generate_test_png();
        let artifact_id = home
            .share_artifact(
                image_data,
                HomeArtifactMetadata {
                    name: "persistent_image.png".to_string(),
                    mime_type: Some("image/png".to_string()),
                    size: 100,
                },
            )
            .await
            .unwrap();

        // Create note
        let note_id = home
            .create_note(
                "Persistent Note",
                "# Markdown\n\nContent here",
                vec!["important".to_string()],
            )
            .await
            .unwrap();

        // Return IDs for verification in session 2
        (member_id, quest_id, note_id, artifact_id)
    };

    // ===== Session 2: Verify persistence =====
    {
        let network = IndrasNetwork::new(&data_dir).await.unwrap();

        // Verify same identity
        assert_eq!(network.id(), member_id);

        // Verify home realm is accessible
        let home = network.home_realm().await.unwrap();

        // Verify quest persisted
        let quests = home.quests().await.unwrap();
        let doc = quests.read().await;
        // Just our quest (welcome quest is seeded by SyncEngine, not by the SDK)
        assert_eq!(doc.quests.len(), 1);
        let quest = doc.find(&quest_id).unwrap();
        assert_eq!(quest.title, "Persistent Quest");

        // Verify artifact persisted (blob store)
        let artifact_data = home.get_artifact(&artifact_id).await.unwrap();
        assert!(!artifact_data.is_empty());

        // Verify note persisted
        let notes = home.notes().await.unwrap();
        let doc = notes.read().await;
        assert_eq!(doc.notes.len(), 1);
        let note = doc.find(&note_id).unwrap();
        assert_eq!(note.title, "Persistent Note");
        assert!(note.content.contains("# Markdown"));
        assert_eq!(note.tags, vec!["important"]);
    }
}

#[tokio::test]
async fn test_home_realm_id_persists_across_sessions() {
    let tmp = TempDir::new().unwrap();
    let data_dir = tmp.path().to_path_buf();

    // Session 1: Get home realm ID
    let realm_id_session1 = {
        let network = IndrasNetwork::new(&data_dir).await.unwrap();
        let home = network.home_realm().await.unwrap();
        home.id()
    };

    // Session 2: Verify same realm ID
    {
        let network = IndrasNetwork::new(&data_dir).await.unwrap();
        let home = network.home_realm().await.unwrap();
        assert_eq!(home.id(), realm_id_session1);
    }
}

// ============================================================
// Additional unit tests for note module
// ============================================================

#[test]
fn test_note_id_uniqueness() {
    use indras_sync_engine::note::generate_note_id;

    let id1 = generate_note_id();
    let id2 = generate_note_id();
    assert_ne!(id1, id2);
}

#[test]
fn test_note_document_default() {
    let doc = NoteDocument::default();
    assert!(doc.is_empty());
    assert_eq!(doc.len(), 0);
}

#[test]
fn test_home_realm_id_different_for_different_members() {
    use indras_network::home_realm_id;

    let member1 = [1u8; 32];
    let member2 = [2u8; 32];

    let id1 = home_realm_id(member1);
    let id2 = home_realm_id(member2);

    assert_ne!(id1, id2);
}

#[test]
fn test_home_realm_id_same_for_same_member() {
    use indras_network::home_realm_id;

    let member = [42u8; 32];

    let id1 = home_realm_id(member);
    let id2 = home_realm_id(member);

    assert_eq!(id1, id2);
}

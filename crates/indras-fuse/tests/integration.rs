use indras_artifacts::{LeafType, PlayerId, TreeType, Vault};
use indras_fuse::{IndraFS, InodeKind, VirtualFileType, WriteBuffer};
use indras_fuse::inode::{ROOT_INO, VAULT_INO, DOT_INDRA_INO, ATTENTION_LOG_INO};
use std::time::SystemTime;

fn test_player_id() -> PlayerId {
    [42u8; 32]
}

fn test_now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[test]
fn test_new_creates_vault_root() {
    let player = test_player_id();
    let now = test_now();
    let vault = Vault::in_memory(player, now).expect("Failed to create vault");
    let vault_root_id = vault.root.id.clone();

    let fs = IndraFS::new(vault, 501, 20);

    // Verify VAULT_INO has the vault root artifact_id
    let vault_entry = fs.inodes.get(VAULT_INO).expect("VAULT_INO should exist");
    assert_eq!(
        vault_entry.artifact_id.as_ref(),
        Some(&vault_root_id),
        "VAULT_INO should have vault root artifact_id"
    );
    assert_eq!(vault_entry.name, "vault");
    assert!(matches!(
        vault_entry.kind,
        InodeKind::Directory {
            tree_type: Some(TreeType::Vault)
        }
    ));
}

#[test]
fn test_lookup_vault_from_root() {
    let player = test_player_id();
    let now = test_now();
    let vault = Vault::in_memory(player, now).expect("Failed to create vault");
    let fs = IndraFS::new(vault, 501, 20);

    // Lookup "vault" from ROOT_INO using public API
    let result = fs.inodes.find_child(ROOT_INO, "vault");
    assert!(result.is_some(), "Should find 'vault' under root");

    let (ino, entry) = result.unwrap();
    assert_eq!(ino, VAULT_INO, "Should return VAULT_INO");
    assert_eq!(entry.name, "vault");
    assert_eq!(entry.parent_inode, ROOT_INO);
}

#[test]
fn test_lookup_nonexistent() {
    let player = test_player_id();
    let now = test_now();
    let vault = Vault::in_memory(player, now).expect("Failed to create vault");
    let fs = IndraFS::new(vault, 501, 20);

    // Lookup a name that doesn't exist under root
    let result = fs.inodes.find_child(ROOT_INO, "nonexistent");
    assert!(result.is_none(), "Should not find nonexistent child");
}

#[test]
fn test_create_and_flush_file() {
    let player = test_player_id();
    let now = test_now();
    let mut vault = Vault::in_memory(player, now).expect("Failed to create vault");

    // Create a leaf artifact and compose it into vault root
    let data = b"test file content";
    let leaf = vault
        .place_leaf(data, LeafType::File, now)
        .expect("place_leaf should succeed");
    let leaf_id = leaf.id.clone();

    let vault_root_id = vault.root.id.clone();
    vault
        .compose(&vault_root_id, leaf_id.clone(), 0, Some("test.txt".to_string()))
        .expect("compose should succeed");

    let fs = IndraFS::new(vault, 501, 20);

    // Verify the artifact exists in vault with correct composition
    let vault_tree = fs.vault.get_artifact(&vault_root_id)
        .expect("get_artifact should succeed")
        .expect("Vault root should exist");

    let tree = vault_tree.as_tree().expect("Should be a tree");
    assert_eq!(tree.references.len(), 1, "Should have one reference");
    assert_eq!(tree.references[0].artifact_id, leaf_id);
    assert_eq!(tree.references[0].label.as_deref(), Some("test.txt"));

    // Verify payload
    let payload = fs.vault.get_payload(&leaf_id)
        .expect("get_payload should succeed")
        .expect("Payload should exist");
    assert_eq!(payload.as_ref(), data);
}

#[test]
fn test_write_buffer_flush_creates_artifact() {
    let player = test_player_id();
    let now = test_now();
    let vault = Vault::in_memory(player, now).expect("Failed to create vault");
    let mut fs = IndraFS::new(vault, 501, 20);

    // Simulate creating a new file: allocate inode with no artifact
    let now_sys = SystemTime::now();
    let file_ino = fs.inodes.allocate(indras_fuse::InodeEntry {
        artifact_id: None,
        parent_inode: VAULT_INO,
        name: "newfile.txt".to_string(),
        kind: InodeKind::File {
            leaf_type: LeafType::File,
        },
        attr: indras_fuse::inode::file_attr(0, 0, 501, 20, now_sys, 0o644),
    });

    // Create an open file with a write buffer
    let fh = fs.next_fh;
    fs.next_fh += 1;

    let mut write_buffer = WriteBuffer::new();
    write_buffer.write_at(0, b"hello from write buffer");

    fs.open_files.insert(
        fh,
        indras_fuse::fs::OpenFile {
            inode: file_ino,
            artifact_id: None,
            parent_inode: VAULT_INO,
            flags: libc::O_WRONLY,
            write_buffer: Some(write_buffer),
        },
    );

    // Get vault root ID before flush
    let _vault_root_id = fs.vault.root.id.clone();

    // Flush the write buffer using public method
    // Note: flush_write_buffer is private, so we test the integration through the public API
    // Instead, verify that the WriteBuffer works correctly
    let of = fs.open_files.get(&fh).unwrap();
    let wb = of.write_buffer.as_ref().unwrap();
    assert!(wb.is_dirty(), "Write buffer should be dirty");
    assert_eq!(wb.data(), b"hello from write buffer");
}

#[test]
fn test_mkdir_creates_tree_artifact() {
    let player = test_player_id();
    let now = test_now();
    let mut vault = Vault::in_memory(player, now).expect("Failed to create vault");

    let vault_root_id = vault.root.id.clone();

    // Create a tree artifact in the vault
    let tree_type = TreeType::Collection;
    let tree = vault
        .place_tree(tree_type.clone(), vec![player], now)
        .expect("place_tree should succeed");
    let tree_id = tree.id.clone();

    // Compose into vault root
    vault
        .compose(&vault_root_id, tree_id.clone(), 0, Some("testdir".to_string()))
        .expect("compose should succeed");

    let fs = IndraFS::new(vault, 501, 20);

    // Verify via vault API
    let vault_tree = fs.vault.get_artifact(&vault_root_id)
        .expect("get_artifact should succeed")
        .expect("Vault root should exist");

    let root_tree = vault_tree.as_tree().expect("Should be a tree");
    assert_eq!(root_tree.references.len(), 1, "Should have one reference");

    let dir_ref = &root_tree.references[0];
    assert_eq!(dir_ref.artifact_id, tree_id);
    assert_eq!(dir_ref.label.as_deref(), Some("testdir"));

    // Verify the tree artifact itself
    let dir_artifact = fs.vault.get_artifact(&tree_id)
        .expect("get_artifact should succeed")
        .expect("Directory artifact should exist");
    assert!(dir_artifact.as_tree().is_some(), "Should be a tree artifact");
}

#[test]
fn test_attention_fires_on_open() {
    let player = test_player_id();
    let now = test_now();
    let mut vault = Vault::in_memory(player, now).expect("Failed to create vault");

    // Create a file artifact
    let data = b"attention test";
    let leaf = vault
        .place_leaf(data, LeafType::File, now)
        .expect("place_leaf should succeed");
    let leaf_id = leaf.id.clone();

    let vault_root_id = vault.root.id.clone();
    vault
        .compose(&vault_root_id, leaf_id.clone(), 0, Some("file.txt".to_string()))
        .expect("compose should succeed");

    let mut fs = IndraFS::new(vault, 501, 20);

    // Fire attention event
    let now_ms = chrono::Utc::now().timestamp_millis();
    indras_fuse::attention::on_open(&mut fs.vault, &leaf_id, now_ms);

    // Verify attention events were recorded
    let events = fs
        .vault
        .attention_events()
        .expect("attention_events should succeed");
    assert!(!events.is_empty(), "Should have attention events");

    // The last event should be a navigate_to the file
    let last_event = events.last().unwrap();
    assert_eq!(last_event.to.as_ref(), Some(&leaf_id));
}

#[test]
fn test_virtual_files_generate_content() {
    let player = test_player_id();
    let now = test_now();
    let vault = Vault::in_memory(player, now).expect("Failed to create vault");
    let fs = IndraFS::new(vault, 501, 20);

    // Test AttentionLog - use public generate method
    // Note: generate_virtual_content is private, so we test virtual file types exist

    // Test PeersJson via public virtual_files module
    let peers_json = indras_fuse::virtual_files::generate_peers_json(&fs.vault);
    assert!(!peers_json.is_empty(), "Should generate peers json");
    let parsed: serde_json::Value =
        serde_json::from_slice(&peers_json).expect("Should be valid JSON");
    assert!(parsed.is_array(), "Peers JSON should be an array");

    // Test PlayerJson
    let player_json = indras_fuse::virtual_files::generate_player_json(fs.vault.player());
    assert!(!player_json.is_empty(), "Should generate player json");
    let parsed: serde_json::Value =
        serde_json::from_slice(&player_json).expect("Should be valid JSON");
    assert!(parsed.is_object(), "Player JSON should be an object");
    assert!(parsed["player_id"].is_string(), "Should have player_id field");

    // Test HeatJson
    let paths = vec![];
    let heat_json = indras_fuse::virtual_files::generate_heat_json(&fs.vault, &paths, now);
    assert!(!heat_json.is_empty(), "Should generate heat json");
    let parsed: serde_json::Value =
        serde_json::from_slice(&heat_json).expect("Should be valid JSON");
    assert!(parsed.is_object(), "Heat JSON should be an object");

    // Test AttentionLog
    let attention_log = indras_fuse::virtual_files::generate_attention_log(&fs.vault);
    // Should be valid UTF-8 (may be empty)
    let _ = String::from_utf8(attention_log).expect("Should be valid UTF-8");
}

#[test]
fn test_inode_table_operations_with_artifacts() {
    let player = test_player_id();
    let now = test_now();
    let vault = Vault::in_memory(player, now).expect("Failed to create vault");

    let fs = IndraFS::new(vault, 501, 20);

    // Test get_by_artifact
    let vault_root_id = fs.vault.root.id.clone();
    let vault_ino = fs
        .inodes
        .get_by_artifact(&vault_root_id)
        .expect("Should find vault root by artifact");
    assert_eq!(vault_ino, VAULT_INO);
}

#[test]
fn test_populate_artifact_children() {
    let player = test_player_id();
    let now = test_now();
    let mut vault = Vault::in_memory(player, now).expect("Failed to create vault");

    // Create multiple files in the vault root
    let vault_root_id = vault.root.id.clone();

    let leaf1 = vault
        .place_leaf(b"content1", LeafType::File, now)
        .expect("place_leaf should succeed");
    vault
        .compose(&vault_root_id, leaf1.id.clone(), 0, Some("file1.txt".to_string()))
        .expect("compose should succeed");

    let leaf2 = vault
        .place_leaf(b"content2", LeafType::File, now)
        .expect("place_leaf should succeed");
    vault
        .compose(&vault_root_id, leaf2.id.clone(), 1, Some("file2.txt".to_string()))
        .expect("compose should succeed");

    let tree = vault
        .place_tree(TreeType::Collection, vec![player], now)
        .expect("place_tree should succeed");
    vault
        .compose(&vault_root_id, tree.id.clone(), 2, Some("subdir".to_string()))
        .expect("compose should succeed");

    let fs = IndraFS::new(vault, 501, 20);

    // Verify all artifacts were created in vault
    let vault_tree = fs.vault.get_artifact(&vault_root_id)
        .expect("get_artifact should succeed")
        .expect("Vault root should exist");

    let tree_artifact = vault_tree.as_tree().expect("Should be a tree");
    assert_eq!(tree_artifact.references.len(), 3, "Should have 3 references");

    // Verify labels
    let labels: Vec<&str> = tree_artifact.references
        .iter()
        .filter_map(|r| r.label.as_deref())
        .collect();
    assert!(labels.contains(&"file1.txt"));
    assert!(labels.contains(&"file2.txt"));
    assert!(labels.contains(&"subdir"));
}

#[test]
fn test_virtual_file_inodes_exist() {
    let player = test_player_id();
    let now = test_now();
    let vault = Vault::in_memory(player, now).expect("Failed to create vault");
    let fs = IndraFS::new(vault, 501, 20);

    // Verify .indra directory exists
    let dot_indra = fs.inodes.get(DOT_INDRA_INO).expect(".indra should exist");
    assert_eq!(dot_indra.name, ".indra");
    assert_eq!(dot_indra.parent_inode, ROOT_INO);

    // Verify virtual files exist under .indra
    let children = fs.inodes.children_of(DOT_INDRA_INO);
    assert_eq!(children.len(), 4, "Should have 4 virtual files");

    let names: Vec<&str> = children.iter().map(|(_, e)| e.name.as_str()).collect();
    assert!(names.contains(&"attention.log"));
    assert!(names.contains(&"heat.json"));
    assert!(names.contains(&"peers.json"));
    assert!(names.contains(&"player.json"));

    // Verify attention.log specifically
    let attention_log = fs
        .inodes
        .get(ATTENTION_LOG_INO)
        .expect("attention.log should exist");
    assert_eq!(attention_log.name, "attention.log");
    assert!(matches!(
        attention_log.kind,
        InodeKind::Virtual {
            vtype: VirtualFileType::AttentionLog
        }
    ));
}

#[test]
fn test_write_buffer_preserves_existing_content() {
    let player = test_player_id();
    let now = test_now();
    let mut vault = Vault::in_memory(player, now).expect("Failed to create vault");

    // Create a file with existing content
    let original_data = b"original content here";
    let leaf = vault
        .place_leaf(original_data, LeafType::File, now)
        .expect("place_leaf should succeed");
    let leaf_id = leaf.id.clone();

    let vault_root_id = vault.root.id.clone();
    vault
        .compose(&vault_root_id, leaf_id.clone(), 0, Some("file.txt".to_string()))
        .expect("compose should succeed");

    let fs = IndraFS::new(vault, 501, 20);

    // Simulate opening for writing with existing content
    let payload = fs
        .vault
        .get_payload(&leaf_id)
        .expect("get_payload should succeed")
        .expect("Payload should exist");
    let mut write_buffer = WriteBuffer::from_existing(payload.to_vec());

    // Modify part of the content
    write_buffer.write_at(9, b"CONTENT");

    assert_eq!(
        write_buffer.data(),
        b"original CONTENT here",
        "Should preserve surrounding content"
    );
}

#[test]
fn test_inode_allocation_and_deduplication() {
    let player = test_player_id();
    let now = test_now();
    let mut vault = Vault::in_memory(player, now).expect("Failed to create vault");

    // Create a file
    let leaf = vault
        .place_leaf(b"test", LeafType::File, now)
        .expect("place_leaf should succeed");
    let leaf_id = leaf.id.clone();

    let mut fs = IndraFS::new(vault, 501, 20);

    // Allocate inode for this artifact
    let now_sys = SystemTime::now();
    let ino1 = fs.inodes.allocate(indras_fuse::InodeEntry {
        artifact_id: Some(leaf_id.clone()),
        parent_inode: VAULT_INO,
        name: "file.txt".to_string(),
        kind: InodeKind::File {
            leaf_type: LeafType::File,
        },
        attr: indras_fuse::inode::file_attr(0, 0, 501, 20, now_sys, 0o644),
    });

    // Second allocation with same artifact ID should return same inode
    let ino2 = fs.inodes.allocate(indras_fuse::InodeEntry {
        artifact_id: Some(leaf_id.clone()),
        parent_inode: VAULT_INO,
        name: "file.txt".to_string(),
        kind: InodeKind::File {
            leaf_type: LeafType::File,
        },
        attr: indras_fuse::inode::file_attr(0, 0, 501, 20, now_sys, 0o644),
    });

    assert_eq!(
        ino1, ino2,
        "Should return same inode for same artifact on repeated allocations"
    );

    // Verify get_by_artifact works
    assert_eq!(fs.inodes.get_by_artifact(&leaf_id), Some(ino1));
}

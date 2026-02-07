use indras_artifacts::*;

// Test player IDs
const NOVA: PlayerId = [1u8; 32];
const ZEPHYR: PlayerId = [2u8; 32];
const SAGE: PlayerId = [3u8; 32];

// ----------------------------------------------------------------------------
// Artifact ID Tests
// ----------------------------------------------------------------------------

#[test]
fn test_leaf_id_determinism() {
    let payload = b"hello world";
    let id1 = leaf_id(payload);
    let id2 = leaf_id(payload);
    assert_eq!(id1, id2, "Same payload should produce same Blob ID");
    assert!(id1.is_blob(), "leaf_id should produce Blob variant");
}

#[test]
fn test_leaf_id_different_payloads() {
    let id1 = leaf_id(b"hello");
    let id2 = leaf_id(b"world");
    assert_ne!(id1, id2, "Different payloads should produce different IDs");
}

#[test]
fn test_tree_id_uniqueness() {
    let id1 = generate_tree_id();
    let id2 = generate_tree_id();
    assert_ne!(id1, id2, "Each tree ID should be unique");
    assert!(id1.is_doc(), "generate_tree_id should produce Doc variant");
}

#[test]
fn test_artifact_id_discrimination() {
    let blob_id = leaf_id(b"test");
    let doc_id = generate_tree_id();

    assert!(blob_id.is_blob());
    assert!(!blob_id.is_doc());
    assert!(doc_id.is_doc());
    assert!(!doc_id.is_blob());
}

#[test]
fn test_artifact_id_debug_format() {
    let id = leaf_id(b"test");
    let debug_str = format!("{:?}", id);

    assert!(debug_str.starts_with("Blob("));
    assert!(debug_str.contains(".."));
    assert!(debug_str.ends_with(")"));
}

// ----------------------------------------------------------------------------
// InMemoryArtifactStore Tests
// ----------------------------------------------------------------------------

#[test]
fn test_artifact_store_put_get() {
    let mut store = InMemoryArtifactStore::new();
    let leaf = LeafArtifact {
        id: leaf_id(b"test"),
        size: 4,
        steward: NOVA,
        audience: vec![NOVA],
        artifact_type: LeafType::Message,
        created_at: 1000,
    };

    store.put_artifact(&Artifact::Leaf(leaf.clone())).unwrap();
    let retrieved = store.get_artifact(&leaf.id).unwrap();

    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().as_leaf().unwrap(), &leaf);
}

#[test]
fn test_artifact_store_get_nonexistent() {
    let store = InMemoryArtifactStore::new();
    let id = generate_tree_id();
    let result = store.get_artifact(&id).unwrap();
    assert!(result.is_none());
}

#[test]
fn test_artifact_store_list_by_type() {
    let mut store = InMemoryArtifactStore::new();

    let vault_tree = TreeArtifact {
        id: generate_tree_id(),
        steward: NOVA,
        audience: vec![NOVA],
        references: vec![],
        metadata: Default::default(),
        artifact_type: TreeType::Vault,
        created_at: 1000,
    };

    let story_tree = TreeArtifact {
        id: generate_tree_id(),
        steward: NOVA,
        audience: vec![NOVA],
        references: vec![],
        metadata: Default::default(),
        artifact_type: TreeType::Story,
        created_at: 2000,
    };

    store.put_artifact(&Artifact::Tree(vault_tree.clone())).unwrap();
    store.put_artifact(&Artifact::Tree(story_tree.clone())).unwrap();

    let vaults = store.list_by_type(&TreeType::Vault).unwrap();
    assert_eq!(vaults.len(), 1);
    assert_eq!(vaults[0], vault_tree.id);

    let stories = store.list_by_type(&TreeType::Story).unwrap();
    assert_eq!(stories.len(), 1);
    assert_eq!(stories[0], story_tree.id);
}

#[test]
fn test_artifact_store_list_by_steward() {
    let mut store = InMemoryArtifactStore::new();

    let nova_leaf = LeafArtifact {
        id: leaf_id(b"nova"),
        size: 4,
        steward: NOVA,
        audience: vec![NOVA],
        artifact_type: LeafType::Message,
        created_at: 1000,
    };

    let zephyr_leaf = LeafArtifact {
        id: leaf_id(b"zephyr"),
        size: 6,
        steward: ZEPHYR,
        audience: vec![ZEPHYR],
        artifact_type: LeafType::Message,
        created_at: 2000,
    };

    store.put_artifact(&Artifact::Leaf(nova_leaf.clone())).unwrap();
    store.put_artifact(&Artifact::Leaf(zephyr_leaf.clone())).unwrap();

    let nova_artifacts = store.list_by_steward(&NOVA).unwrap();
    assert_eq!(nova_artifacts.len(), 1);
    assert_eq!(nova_artifacts[0], nova_leaf.id);

    let zephyr_artifacts = store.list_by_steward(&ZEPHYR).unwrap();
    assert_eq!(zephyr_artifacts.len(), 1);
    assert_eq!(zephyr_artifacts[0], zephyr_leaf.id);
}

// ----------------------------------------------------------------------------
// InMemoryPayloadStore Tests
// ----------------------------------------------------------------------------

#[test]
fn test_payload_store_round_trip() {
    let mut store = InMemoryPayloadStore::new();
    let payload = b"hello world";

    let id = store.store_payload(payload).unwrap();
    assert!(id.is_blob());

    let retrieved = store.get_payload(&id).unwrap();
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().as_ref(), payload);
}

#[test]
fn test_payload_store_has_payload() {
    let mut store = InMemoryPayloadStore::new();
    let payload = b"test";

    let id = store.store_payload(payload).unwrap();
    assert!(store.has_payload(&id));

    let nonexistent = generate_tree_id();
    assert!(!store.has_payload(&nonexistent));
}

#[test]
fn test_payload_store_get_nonexistent() {
    let store = InMemoryPayloadStore::new();
    let id = generate_tree_id();
    let result = store.get_payload(&id).unwrap();
    assert!(result.is_none());
}

#[test]
fn test_payload_store_content_addressed() {
    let mut store = InMemoryPayloadStore::new();
    let payload = b"same content";

    let id1 = store.store_payload(payload).unwrap();
    let id2 = store.store_payload(payload).unwrap();

    assert_eq!(id1, id2, "Same content should produce same ID");
}

// ----------------------------------------------------------------------------
// InMemoryAttentionStore Tests
// ----------------------------------------------------------------------------

#[test]
fn test_attention_store_append_and_events() {
    let mut store = InMemoryAttentionStore::new();
    let artifact_id = generate_tree_id();

    let event = AttentionSwitchEvent {
        player: NOVA,
        from: None,
        to: Some(artifact_id.clone()),
        timestamp: 1000,
    };

    store.append_event(event.clone()).unwrap();
    let events = store.events(&NOVA).unwrap();

    assert_eq!(events.len(), 1);
    assert_eq!(events[0], event);
}

#[test]
fn test_attention_store_events_since() {
    let mut store = InMemoryAttentionStore::new();
    let artifact_id = generate_tree_id();

    let event1 = AttentionSwitchEvent {
        player: NOVA,
        from: None,
        to: Some(artifact_id.clone()),
        timestamp: 1000,
    };

    let event2 = AttentionSwitchEvent {
        player: NOVA,
        from: Some(artifact_id.clone()),
        to: None,
        timestamp: 2000,
    };

    store.append_event(event1).unwrap();
    store.append_event(event2.clone()).unwrap();

    let recent = store.events_since(&NOVA, 1500).unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0], event2);
}

#[test]
fn test_attention_store_integrity_consistent() {
    let mut store = InMemoryAttentionStore::new();
    let artifact_id = generate_tree_id();

    let events = vec![
        AttentionSwitchEvent {
            player: ZEPHYR,
            from: None,
            to: Some(artifact_id.clone()),
            timestamp: 1000,
        },
        AttentionSwitchEvent {
            player: ZEPHYR,
            from: Some(artifact_id),
            to: None,
            timestamp: 2000,
        },
    ];

    store.ingest_peer_log(ZEPHYR, events.clone()).unwrap();
    let result = store.check_integrity(&ZEPHYR, &events);

    assert_eq!(result, IntegrityResult::Consistent);
}

#[test]
fn test_attention_store_integrity_extended() {
    let mut store = InMemoryAttentionStore::new();
    let artifact_id = generate_tree_id();

    let initial_events = vec![
        AttentionSwitchEvent {
            player: ZEPHYR,
            from: None,
            to: Some(artifact_id.clone()),
            timestamp: 1000,
        },
    ];

    let extended_events = vec![
        AttentionSwitchEvent {
            player: ZEPHYR,
            from: None,
            to: Some(artifact_id.clone()),
            timestamp: 1000,
        },
        AttentionSwitchEvent {
            player: ZEPHYR,
            from: Some(artifact_id),
            to: None,
            timestamp: 2000,
        },
    ];

    store.ingest_peer_log(ZEPHYR, initial_events).unwrap();
    let result = store.check_integrity(&ZEPHYR, &extended_events);

    assert_eq!(result, IntegrityResult::Extended { new_events: 1 });
}

#[test]
fn test_attention_store_integrity_diverged() {
    let mut store = InMemoryAttentionStore::new();
    let artifact1 = generate_tree_id();
    let artifact2 = generate_tree_id();

    let our_events = vec![
        AttentionSwitchEvent {
            player: ZEPHYR,
            from: None,
            to: Some(artifact1),
            timestamp: 1000,
        },
    ];

    let their_events = vec![
        AttentionSwitchEvent {
            player: ZEPHYR,
            from: None,
            to: Some(artifact2),
            timestamp: 1000,
        },
    ];

    store.ingest_peer_log(ZEPHYR, our_events).unwrap();
    let result = store.check_integrity(&ZEPHYR, &their_events);

    assert_eq!(result, IntegrityResult::Diverged { first_mismatch_index: 0 });
}

#[test]
fn test_attention_store_integrity_no_prior() {
    let store = InMemoryAttentionStore::new();
    let artifact_id = generate_tree_id();

    let events = vec![
        AttentionSwitchEvent {
            player: ZEPHYR,
            from: None,
            to: Some(artifact_id),
            timestamp: 1000,
        },
    ];

    let result = store.check_integrity(&ZEPHYR, &events);
    assert_eq!(result, IntegrityResult::NoPriorReplica);
}

// ----------------------------------------------------------------------------
// Composition Tests
// ----------------------------------------------------------------------------

#[test]
fn test_add_ref_ordering_by_position() {
    let mut store = InMemoryArtifactStore::new();
    let tree_id = generate_tree_id();
    let child1 = generate_tree_id();
    let child2 = generate_tree_id();
    let child3 = generate_tree_id();

    let tree = TreeArtifact {
        id: tree_id.clone(),
        steward: NOVA,
        audience: vec![NOVA],
        references: vec![],
        metadata: Default::default(),
        artifact_type: TreeType::Story,
        created_at: 1000,
    };

    store.put_artifact(&Artifact::Tree(tree)).unwrap();

    // Add refs out of order
    store.add_ref(&tree_id, ArtifactRef {
        artifact_id: child2.clone(),
        position: 2,
        label: None,
    }).unwrap();

    store.add_ref(&tree_id, ArtifactRef {
        artifact_id: child1.clone(),
        position: 1,
        label: None,
    }).unwrap();

    store.add_ref(&tree_id, ArtifactRef {
        artifact_id: child3.clone(),
        position: 3,
        label: None,
    }).unwrap();

    let retrieved = store.get_artifact(&tree_id).unwrap().unwrap();
    let refs = retrieved.as_tree().unwrap().references.clone();

    assert_eq!(refs.len(), 3);
    assert_eq!(refs[0].artifact_id, child1);
    assert_eq!(refs[1].artifact_id, child2);
    assert_eq!(refs[2].artifact_id, child3);
}

#[test]
fn test_remove_ref() {
    let mut store = InMemoryArtifactStore::new();
    let tree_id = generate_tree_id();
    let child1 = generate_tree_id();
    let child2 = generate_tree_id();

    let tree = TreeArtifact {
        id: tree_id.clone(),
        steward: NOVA,
        audience: vec![NOVA],
        references: vec![],
        metadata: Default::default(),
        artifact_type: TreeType::Story,
        created_at: 1000,
    };

    store.put_artifact(&Artifact::Tree(tree)).unwrap();

    store.add_ref(&tree_id, ArtifactRef {
        artifact_id: child1.clone(),
        position: 0,
        label: None,
    }).unwrap();

    store.add_ref(&tree_id, ArtifactRef {
        artifact_id: child2.clone(),
        position: 1,
        label: None,
    }).unwrap();

    store.remove_ref(&tree_id, &child1).unwrap();

    let retrieved = store.get_artifact(&tree_id).unwrap().unwrap();
    let refs = retrieved.as_tree().unwrap().references.clone();

    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].artifact_id, child2);
}

// ----------------------------------------------------------------------------
// Audience & Stewardship Tests
// ----------------------------------------------------------------------------

#[test]
fn test_update_audience() {
    let mut store = InMemoryArtifactStore::new();
    let leaf_id = leaf_id(b"test");

    let leaf = LeafArtifact {
        id: leaf_id.clone(),
        size: 4,
        steward: NOVA,
        audience: vec![NOVA],
        artifact_type: LeafType::Message,
        created_at: 1000,
    };

    store.put_artifact(&Artifact::Leaf(leaf)).unwrap();

    let new_audience = vec![NOVA, ZEPHYR, SAGE];
    store.update_audience(&leaf_id, new_audience.clone()).unwrap();

    let retrieved = store.get_artifact(&leaf_id).unwrap().unwrap();
    assert_eq!(retrieved.audience(), &new_audience);
}

#[test]
fn test_transfer_stewardship() {
    let mut store = InMemoryArtifactStore::new();
    let leaf_id = leaf_id(b"test");

    let leaf = LeafArtifact {
        id: leaf_id.clone(),
        size: 4,
        steward: NOVA,
        audience: vec![NOVA],
        artifact_type: LeafType::Message,
        created_at: 1000,
    };

    store.put_artifact(&Artifact::Leaf(leaf)).unwrap();
    store.update_steward(&leaf_id, ZEPHYR).unwrap();

    let retrieved = store.get_artifact(&leaf_id).unwrap().unwrap();
    assert_eq!(retrieved.steward(), &ZEPHYR);
}

// ----------------------------------------------------------------------------
// Vault Tests
// ----------------------------------------------------------------------------

#[test]
fn test_vault_creation() {
    let vault = Vault::in_memory(NOVA, 1000).unwrap();

    assert_eq!(vault.player(), &NOVA);
    assert_eq!(vault.root.artifact_type, TreeType::Vault);
    assert_eq!(vault.root.steward, NOVA);
    assert_eq!(vault.root.audience, vec![NOVA]);
}

#[test]
fn test_vault_place_leaf() {
    let mut vault = Vault::in_memory(NOVA, 1000).unwrap();
    let payload = b"test message";

    let leaf = vault.place_leaf(payload, LeafType::Message, 1000).unwrap();

    assert_eq!(leaf.steward, NOVA);
    assert_eq!(leaf.audience, vec![NOVA]);
    assert_eq!(leaf.size, payload.len() as u64);
    assert!(leaf.id.is_blob());

    // Verify payload is stored
    assert!(vault.has_payload(&leaf.id));
    let retrieved_payload = vault.get_payload(&leaf.id).unwrap().unwrap();
    assert_eq!(retrieved_payload.as_ref(), payload);
}

#[test]
fn test_vault_place_tree() {
    let mut vault = Vault::in_memory(NOVA, 1000).unwrap();
    let audience = vec![NOVA, ZEPHYR];

    let tree = vault.place_tree(TreeType::Story, audience.clone(), 2000).unwrap();

    assert_eq!(tree.steward, NOVA);
    assert_eq!(tree.audience, audience);
    assert_eq!(tree.artifact_type, TreeType::Story);
    assert!(tree.id.is_doc());
}

#[test]
fn test_vault_compose_requires_steward() {
    let mut vault = Vault::in_memory(NOVA, 1000).unwrap();

    // Create a tree owned by ZEPHYR
    let tree = TreeArtifact {
        id: generate_tree_id(),
        steward: ZEPHYR,
        audience: vec![ZEPHYR],
        references: vec![],
        metadata: Default::default(),
        artifact_type: TreeType::Story,
        created_at: 1000,
    };

    vault.artifact_store_mut().put_artifact(&Artifact::Tree(tree.clone())).unwrap();

    let child = generate_tree_id();
    let result = vault.compose(&tree.id, child, 0, None);

    assert!(matches!(result, Err(VaultError::NotSteward)));
}

#[test]
fn test_vault_set_audience_requires_steward() {
    let mut vault = Vault::in_memory(NOVA, 1000).unwrap();

    // Create a leaf owned by ZEPHYR
    let leaf = LeafArtifact {
        id: leaf_id(b"zephyr's message"),
        size: 16,
        steward: ZEPHYR,
        audience: vec![ZEPHYR],
        artifact_type: LeafType::Message,
        created_at: 1000,
    };

    vault.artifact_store_mut().put_artifact(&Artifact::Leaf(leaf.clone())).unwrap();

    let result = vault.set_audience(&leaf.id, vec![NOVA, ZEPHYR]);
    assert!(matches!(result, Err(VaultError::NotSteward)));
}

#[test]
fn test_vault_transfer_stewardship_old_steward_loses_control() {
    let mut vault = Vault::in_memory(NOVA, 1000).unwrap();

    let leaf = vault.place_leaf(b"message", LeafType::Message, 1000).unwrap();
    let leaf_id = leaf.id.clone();

    // Nova can modify it
    vault.set_audience(&leaf_id, vec![NOVA, ZEPHYR]).unwrap();

    // Transfer to Zephyr
    vault.transfer_stewardship(&leaf_id, ZEPHYR).unwrap();

    // Nova can no longer modify it
    let result = vault.set_audience(&leaf_id, vec![NOVA]);
    assert!(matches!(result, Err(VaultError::NotSteward)));
}

// ----------------------------------------------------------------------------
// Attention & Heat Tests
// ----------------------------------------------------------------------------

#[test]
fn test_attention_navigate_to() {
    let mut vault = Vault::in_memory(NOVA, 1000).unwrap();
    let artifact_id = generate_tree_id();

    vault.navigate_to(artifact_id.clone(), 1000).unwrap();

    assert_eq!(vault.current_focus(), Some(&artifact_id));

    let events = vault.attention_events().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].player, NOVA);
    assert_eq!(events[0].from, None);
    assert_eq!(events[0].to, Some(artifact_id));
}

#[test]
fn test_attention_navigate_back() {
    let mut vault = Vault::in_memory(NOVA, 1000).unwrap();
    let artifact1 = generate_tree_id();
    let artifact2 = generate_tree_id();

    vault.navigate_to(artifact1.clone(), 1000).unwrap();
    vault.navigate_to(artifact2.clone(), 2000).unwrap();
    vault.navigate_back(artifact1.clone(), 3000).unwrap();

    assert_eq!(vault.current_focus(), Some(&artifact1));

    let events = vault.attention_events().unwrap();
    assert_eq!(events.len(), 3);
    assert_eq!(events[2].from, Some(artifact2));
    assert_eq!(events[2].to, Some(artifact1));
}

#[test]
fn test_heat_zero_for_no_peer_attention() {
    let mut vault = Vault::in_memory(NOVA, 1000).unwrap();
    let leaf = vault.place_leaf(b"test", LeafType::Message, 1000).unwrap();

    let heat = vault.heat(&leaf.id, 2000).unwrap();
    assert_eq!(heat, 0.0);
}

#[test]
fn test_heat_positive_for_peer_activity() {
    let mut vault = Vault::in_memory(NOVA, 1000).unwrap();
    let artifact_id = generate_tree_id();

    // Create artifact with NOVA and ZEPHYR in audience
    let tree = TreeArtifact {
        id: artifact_id.clone(),
        steward: NOVA,
        audience: vec![NOVA, ZEPHYR],
        references: vec![],
        metadata: Default::default(),
        artifact_type: TreeType::Story,
        created_at: 1000,
    };
    vault.artifact_store_mut().put_artifact(&Artifact::Tree(tree)).unwrap();

    // Add ZEPHYR as peer
    vault.peer(ZEPHYR, Some("Zephyr".to_string()), 1000).unwrap();

    // Ingest ZEPHYR's attention showing they viewed this artifact
    let peer_events = vec![
        AttentionSwitchEvent {
            player: ZEPHYR,
            from: None,
            to: Some(artifact_id.clone()),
            timestamp: 1000,
        },
        AttentionSwitchEvent {
            player: ZEPHYR,
            from: Some(artifact_id.clone()),
            to: None,
            timestamp: 61000, // 60 seconds dwell time
        },
    ];

    vault.ingest_peer_log(ZEPHYR, peer_events).unwrap();

    let heat = vault.heat(&artifact_id, 62000).unwrap();
    assert!(heat > 0.0, "Heat should be positive with peer attention");
}

#[test]
fn test_heat_excludes_non_audience_peers() {
    let mut vault = Vault::in_memory(NOVA, 1000).unwrap();
    let artifact_id = generate_tree_id();

    // Create artifact with only NOVA in audience (not ZEPHYR)
    let tree = TreeArtifact {
        id: artifact_id.clone(),
        steward: NOVA,
        audience: vec![NOVA],
        references: vec![],
        metadata: Default::default(),
        artifact_type: TreeType::Story,
        created_at: 1000,
    };
    vault.artifact_store_mut().put_artifact(&Artifact::Tree(tree)).unwrap();

    // Add ZEPHYR as peer
    vault.peer(ZEPHYR, Some("Zephyr".to_string()), 1000).unwrap();

    // Ingest ZEPHYR's attention even though they're not in audience
    let peer_events = vec![
        AttentionSwitchEvent {
            player: ZEPHYR,
            from: None,
            to: Some(artifact_id.clone()),
            timestamp: 1000,
        },
        AttentionSwitchEvent {
            player: ZEPHYR,
            from: Some(artifact_id.clone()),
            to: None,
            timestamp: 61000,
        },
    ];

    vault.ingest_peer_log(ZEPHYR, peer_events).unwrap();

    let heat = vault.heat(&artifact_id, 62000).unwrap();
    assert_eq!(heat, 0.0, "Heat should be 0 for non-audience peer");
}

// ----------------------------------------------------------------------------
// Peering Tests
// ----------------------------------------------------------------------------

#[test]
fn test_add_peer() {
    let mut vault = Vault::in_memory(NOVA, 1000).unwrap();

    vault.peer(ZEPHYR, Some("Zephyr".to_string()), 1000).unwrap();

    let peers = vault.peers();
    assert_eq!(peers.len(), 1);
    assert_eq!(peers[0].peer_id, ZEPHYR);
    assert_eq!(peers[0].display_name.as_deref(), Some("Zephyr"));
    assert_eq!(peers[0].since, 1000);
}

#[test]
fn test_add_peer_already_peered_error() {
    let mut vault = Vault::in_memory(NOVA, 1000).unwrap();

    vault.peer(ZEPHYR, Some("Zephyr".to_string()), 1000).unwrap();
    let result = vault.peer(ZEPHYR, Some("Zephyr 2".to_string()), 2000);

    assert!(matches!(result, Err(VaultError::AlreadyPeered)));
}

#[test]
fn test_remove_peer() {
    let mut vault = Vault::in_memory(NOVA, 1000).unwrap();

    vault.peer(ZEPHYR, Some("Zephyr".to_string()), 1000).unwrap();
    vault.unpeer(&ZEPHYR).unwrap();

    assert_eq!(vault.peers().len(), 0);
}

#[test]
fn test_remove_peer_not_peered_error() {
    let mut vault = Vault::in_memory(NOVA, 1000).unwrap();

    let result = vault.unpeer(&ZEPHYR);
    assert!(matches!(result, Err(VaultError::NotPeered)));
}

#[test]
fn test_mutual_peering_canonical_ordering() {
    let peer1 = MutualPeering::new(NOVA, ZEPHYR, 1000);
    let peer2 = MutualPeering::new(ZEPHYR, NOVA, 1000);

    assert_eq!(peer1, peer2, "Order should be canonical");
    assert!(peer1.peer_a <= peer1.peer_b, "peer_a should be <= peer_b");
}

#[test]
fn test_mutual_peering_contains() {
    let peering = MutualPeering::new(NOVA, ZEPHYR, 1000);

    assert!(peering.contains(&NOVA));
    assert!(peering.contains(&ZEPHYR));
    assert!(!peering.contains(&SAGE));
}

#[test]
fn test_mutual_peering_other() {
    let peering = MutualPeering::new(NOVA, ZEPHYR, 1000);

    assert_eq!(peering.other(&NOVA), Some(&ZEPHYR));
    assert_eq!(peering.other(&ZEPHYR), Some(&NOVA));
    assert_eq!(peering.other(&SAGE), None);
}

// ----------------------------------------------------------------------------
// Story Tests
// ----------------------------------------------------------------------------

#[test]
fn test_story_create() {
    let mut vault = Vault::in_memory(NOVA, 1000).unwrap();
    let audience = vec![NOVA, ZEPHYR];

    let story = Story::create(&mut vault, audience.clone(), 1000).unwrap();

    let artifact = vault.get_artifact(&story.id).unwrap().unwrap();
    let tree = artifact.as_tree().unwrap();

    assert_eq!(tree.artifact_type, TreeType::Story);
    assert_eq!(tree.audience, audience);
}

#[test]
fn test_story_append() {
    let mut vault = Vault::in_memory(NOVA, 1000).unwrap();
    let story = Story::create(&mut vault, vec![NOVA], 1000).unwrap();

    let leaf1 = vault.place_leaf(b"message 1", LeafType::Message, 1000).unwrap();
    let leaf2 = vault.place_leaf(b"message 2", LeafType::Message, 2000).unwrap();

    story.append(&mut vault, leaf1.id.clone(), None).unwrap();
    story.append(&mut vault, leaf2.id.clone(), None).unwrap();

    assert_eq!(story.entry_count(&vault).unwrap(), 2);
}

#[test]
fn test_story_send_message() {
    let mut vault = Vault::in_memory(NOVA, 1000).unwrap();
    let story = Story::create(&mut vault, vec![NOVA], 1000).unwrap();

    let msg_id = story.send_message(&mut vault, "hello world", 1000).unwrap();

    assert!(msg_id.is_blob());
    assert_eq!(story.entry_count(&vault).unwrap(), 1);

    let payload = vault.get_payload(&msg_id).unwrap().unwrap();
    assert_eq!(payload.as_ref(), b"hello world");
}

#[test]
fn test_story_entries_ordered_by_position() {
    let mut vault = Vault::in_memory(NOVA, 1000).unwrap();
    let story = Story::create(&mut vault, vec![NOVA], 1000).unwrap();

    let leaf1 = vault.place_leaf(b"first", LeafType::Message, 1000).unwrap();
    let leaf2 = vault.place_leaf(b"second", LeafType::Message, 2000).unwrap();
    let leaf3 = vault.place_leaf(b"third", LeafType::Message, 3000).unwrap();

    story.append(&mut vault, leaf1.id.clone(), None).unwrap();
    story.append(&mut vault, leaf2.id.clone(), None).unwrap();
    story.append(&mut vault, leaf3.id.clone(), None).unwrap();

    let entries = story.entries(&vault).unwrap();

    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].0.artifact_id, leaf1.id);
    assert_eq!(entries[1].0.artifact_id, leaf2.id);
    assert_eq!(entries[2].0.artifact_id, leaf3.id);
}

#[test]
fn test_story_branch() {
    let mut vault = Vault::in_memory(NOVA, 1000).unwrap();
    let main_story = Story::create(&mut vault, vec![NOVA], 1000).unwrap();

    main_story.send_message(&mut vault, "msg 1", 1000).unwrap();
    main_story.send_message(&mut vault, "msg 2", 2000).unwrap();

    let branch = main_story.branch(&mut vault, 1, vec![NOVA, ZEPHYR], 3000).unwrap();

    // Branch should be a separate Story
    assert_ne!(branch.id, main_story.id);

    // Branch should be referenced in main story
    let entries = main_story.entries(&vault).unwrap();
    let branch_ref = entries.iter().find(|(r, _)| r.artifact_id == branch.id);
    assert!(branch_ref.is_some());
    assert_eq!(branch_ref.unwrap().0.position, 1);
    assert_eq!(branch_ref.unwrap().0.label.as_deref(), Some("branch"));
}

// ----------------------------------------------------------------------------
// Exchange Tests
// ----------------------------------------------------------------------------

#[test]
fn test_exchange_propose() {
    let mut vault = Vault::in_memory(NOVA, 1000).unwrap();

    let my_artifact = vault.place_leaf(b"my item", LeafType::Token, 1000).unwrap();
    let their_artifact_id = generate_tree_id(); // Pretend this exists elsewhere

    let exchange = Exchange::propose(
        &mut vault,
        my_artifact.id.clone(),
        their_artifact_id.clone(),
        vec![NOVA, ZEPHYR],
        1000,
    ).unwrap();

    let artifact = vault.get_artifact(&exchange.id).unwrap().unwrap();
    let tree = artifact.as_tree().unwrap();

    assert_eq!(tree.artifact_type, TreeType::Exchange);
    assert_eq!(tree.references.len(), 3); // offered, requested, conversation
}

#[test]
fn test_exchange_conversation() {
    let mut vault = Vault::in_memory(NOVA, 1000).unwrap();

    let my_artifact = vault.place_leaf(b"my item", LeafType::Token, 1000).unwrap();
    let their_artifact_id = generate_tree_id();

    let exchange = Exchange::propose(
        &mut vault,
        my_artifact.id.clone(),
        their_artifact_id.clone(),
        vec![NOVA, ZEPHYR],
        1000,
    ).unwrap();

    let conversation = exchange.conversation(&vault).unwrap();

    // Should be a Story
    let conv_artifact = vault.get_artifact(&conversation.id).unwrap().unwrap();
    assert_eq!(conv_artifact.as_tree().unwrap().artifact_type, TreeType::Story);
}

#[test]
fn test_exchange_offered_and_requested() {
    let mut vault = Vault::in_memory(NOVA, 1000).unwrap();

    let my_artifact = vault.place_leaf(b"my item", LeafType::Token, 1000).unwrap();
    let their_artifact = vault.place_leaf(b"their item", LeafType::Token, 2000).unwrap();

    let exchange = Exchange::propose(
        &mut vault,
        my_artifact.id.clone(),
        their_artifact.id.clone(),
        vec![NOVA, ZEPHYR],
        1000,
    ).unwrap();

    let offered = exchange.offered(&vault).unwrap().unwrap();
    let requested = exchange.requested(&vault).unwrap().unwrap();

    assert_eq!(offered.id(), &my_artifact.id);
    assert_eq!(requested.id(), &their_artifact.id);
}

#[test]
fn test_exchange_accept() {
    let mut vault = Vault::in_memory(NOVA, 1000).unwrap();

    let my_artifact = vault.place_leaf(b"my item", LeafType::Token, 1000).unwrap();
    let their_artifact_id = generate_tree_id();

    let exchange = Exchange::propose(
        &mut vault,
        my_artifact.id.clone(),
        their_artifact_id.clone(),
        vec![NOVA, ZEPHYR],
        1000,
    ).unwrap();

    exchange.accept(&mut vault).unwrap();

    assert!(exchange.is_accepted_by(&vault, &NOVA).unwrap());
    assert!(!exchange.is_accepted_by(&vault, &ZEPHYR).unwrap());
}

#[test]
fn test_exchange_complete_requires_both_accept() {
    let mut vault = Vault::in_memory(NOVA, 1000).unwrap();

    let my_artifact = vault.place_leaf(b"my item", LeafType::Token, 1000).unwrap();

    // Create an artifact owned by ZEPHYR
    let their_artifact = LeafArtifact {
        id: leaf_id(b"their item"),
        size: 10,
        steward: ZEPHYR,
        audience: vec![ZEPHYR],
        artifact_type: LeafType::Token,
        created_at: 1000,
    };
    vault.artifact_store_mut().put_artifact(&Artifact::Leaf(their_artifact.clone())).unwrap();

    let exchange = Exchange::propose(
        &mut vault,
        my_artifact.id.clone(),
        their_artifact.id.clone(),
        vec![NOVA, ZEPHYR],
        1000,
    ).unwrap();

    // Only NOVA accepts
    exchange.accept(&mut vault).unwrap();

    // Complete should fail
    let result = exchange.complete(&mut vault);
    assert!(matches!(result, Err(VaultError::ExchangeNotFullyAccepted)));
}

#[test]
fn test_exchange_complete_transfers_stewardship() {
    let mut vault = Vault::in_memory(NOVA, 1000).unwrap();

    let my_artifact = vault.place_leaf(b"my item", LeafType::Token, 1000).unwrap();

    // Create an artifact owned by ZEPHYR
    let their_artifact = LeafArtifact {
        id: leaf_id(b"their item"),
        size: 10,
        steward: ZEPHYR,
        audience: vec![ZEPHYR],
        artifact_type: LeafType::Token,
        created_at: 1000,
    };
    vault.artifact_store_mut().put_artifact(&Artifact::Leaf(their_artifact.clone())).unwrap();

    let exchange = Exchange::propose(
        &mut vault,
        my_artifact.id.clone(),
        their_artifact.id.clone(),
        vec![NOVA, ZEPHYR],
        1000,
    ).unwrap();

    // Manually mark both as accepted (simulating ZEPHYR's acceptance)
    exchange.accept(&mut vault).unwrap();

    // Manually set ZEPHYR's acceptance in metadata
    let mut exch_artifact = vault.get_artifact(&exchange.id).unwrap().unwrap();
    let exch_tree = exch_artifact.as_tree_mut().unwrap();
    let key = format!("accept:{}", ZEPHYR.iter().map(|b| format!("{b:02x}")).collect::<String>());
    exch_tree.metadata.insert(key, b"true".to_vec());
    vault.artifact_store_mut().put_artifact(&exch_artifact).unwrap();

    // Now complete should work
    exchange.complete(&mut vault).unwrap();

    // Verify stewardship transferred
    let my_after = vault.get_artifact(&my_artifact.id).unwrap().unwrap();
    let their_after = vault.get_artifact(&their_artifact.id).unwrap().unwrap();

    assert_eq!(my_after.steward(), &ZEPHYR);
    assert_eq!(their_after.steward(), &NOVA);
}

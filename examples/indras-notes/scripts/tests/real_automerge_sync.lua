-- Real Automerge Sync Test
--
-- Tests REAL Automerge CRDT sync between multiple SyncableNotebook instances.
-- Uses the InterfaceDocument from indras-sync for actual CRDT replication.
--
-- Run with: indras-notes run-script scripts/tests/real_automerge_sync.lua

notes.log.info("Starting Real Automerge Sync Test")
notes.log.info("This test uses SyncableNotebook which is backed by real Automerge")

-- SCENARIO 1: Basic sync - Alice creates note, Bob receives via sync
notes.log.info("=== Scenario 1: Basic Sync ===")

-- Create Alice's notebook
local alice = notes.SyncableNotebook.new("Shared Notes", "Alice")
notes.log.info("Created Alice's notebook", { name = alice.name })

-- Alice creates a note
local alice_note = notes.Note.new("Alice's First Note", "alice")
alice:apply(notes.NoteOperation.create(alice_note))
alice:apply(notes.NoteOperation.update_content(alice_note.id, "Content written by Alice"))

notes.assert.eq(alice.note_count, 1, "Alice should have 1 note")
notes.log.info("Alice created note", { note_id = alice_note.id:sub(1, 8) })

-- Fork to Bob (initial sync - Bob gets full state)
local bob = notes.SyncableNotebook.fork(alice, "Bob")
notes.log.info("Forked notebook to Bob")

-- Verify Bob has Alice's note
notes.assert.eq(bob.note_count, 1, "Bob should have 1 note from initial fork")
local bob_view_of_alice_note = bob:find(alice_note.id:sub(1, 8))
notes.assert.not_nil(bob_view_of_alice_note, "Bob should have Alice's note")
notes.assert.eq(bob_view_of_alice_note.title, "Alice's First Note")

notes.log.info("Scenario 1 PASSED: Basic fork works correctly")

-- SCENARIO 2: Incremental sync - Alice makes more changes, Bob syncs
notes.log.info("=== Scenario 2: Incremental Sync ===")

-- Get Bob's current heads (sync checkpoint)
local bob_heads = bob:heads()
notes.log.info("Bob's heads captured", { head_count = #bob_heads })

-- Alice adds another note
local alice_note2 = notes.Note.new("Alice's Second Note", "alice")
alice:apply(notes.NoteOperation.create(alice_note2))
alice:apply(notes.NoteOperation.update_content(alice_note2.id, "More content from Alice"))

notes.assert.eq(alice.note_count, 2, "Alice should have 2 notes")
notes.assert.eq(bob.note_count, 1, "Bob should still have 1 note (no sync yet)")

notes.log.info("Alice added second note", { note_id = alice_note2.id:sub(1, 8) })

-- Generate sync message from Alice for Bob's heads
local sync_msg = alice:generate_sync(bob_heads)
notes.log.info("Generated sync message", { size = #sync_msg })

-- Bob applies the sync
local changed = bob:apply_sync(sync_msg)
notes.assert.eq(changed, true, "Sync should indicate changes were applied")

-- Verify Bob now has both notes
notes.assert.eq(bob.note_count, 2, "Bob should have 2 notes after sync")
local bob_view_of_note2 = bob:find(alice_note2.id:sub(1, 8))
notes.assert.not_nil(bob_view_of_note2, "Bob should have Alice's second note")

notes.log.info("Scenario 2 PASSED: Incremental sync works correctly")

-- SCENARIO 3: Bidirectional concurrent edits
notes.log.info("=== Scenario 3: Bidirectional Concurrent Edits ===")

-- Create fresh notebooks for this scenario
local peer_a = notes.SyncableNotebook.new("Concurrent Test", "Alice")
local peer_b = notes.SyncableNotebook.fork(peer_a, "Bob")

-- Get initial heads before concurrent edits
local a_initial_heads = peer_a:heads()
local b_initial_heads = peer_b:heads()

-- Both make concurrent edits
local a_concurrent_note = notes.Note.new("Alice's Concurrent Note", "alice")
peer_a:apply(notes.NoteOperation.create(a_concurrent_note))
notes.log.info("Alice made concurrent edit", { note_id = a_concurrent_note.id:sub(1, 8) })

local b_concurrent_note = notes.Note.new("Bob's Concurrent Note", "bob")
peer_b:apply(notes.NoteOperation.create(b_concurrent_note))
notes.log.info("Bob made concurrent edit", { note_id = b_concurrent_note.id:sub(1, 8) })

-- Before sync, each has only their own note
notes.assert.eq(peer_a.note_count, 1, "Alice should have 1 note before sync")
notes.assert.eq(peer_b.note_count, 1, "Bob should have 1 note before sync")

-- Bidirectional sync: A -> B
local sync_a_to_b = peer_a:generate_sync(b_initial_heads)
local b_changed = peer_b:apply_sync(sync_a_to_b)

-- Bidirectional sync: B -> A
local sync_b_to_a = peer_b:generate_sync(a_initial_heads)
local a_changed = peer_a:apply_sync(sync_b_to_a)

notes.log.info("Bidirectional sync completed", {
    a_changed = a_changed,
    b_changed = b_changed
})

-- CRDT Convergence: Both should have both notes
notes.assert.eq(peer_a.note_count, 2, "Alice should have 2 notes after sync")
notes.assert.eq(peer_b.note_count, 2, "Bob should have 2 notes after sync")

-- Verify cross-visibility
notes.assert.not_nil(peer_a:find(b_concurrent_note.id:sub(1, 8)), "Alice should see Bob's note")
notes.assert.not_nil(peer_b:find(a_concurrent_note.id:sub(1, 8)), "Bob should see Alice's note")

notes.log.info("Scenario 3 PASSED: Bidirectional concurrent edits converge correctly")

-- SCENARIO 4: Three-way sync (Alice, Bob, Carol)
notes.log.info("=== Scenario 4: Three-Way Sync ===")

local alice3 = notes.SyncableNotebook.new("Three-Way", "Alice")
local bob3 = notes.SyncableNotebook.fork(alice3, "Bob")
local carol3 = notes.SyncableNotebook.fork(alice3, "Carol")

-- Get initial heads
local alice3_initial = alice3:heads()
local bob3_initial = bob3:heads()
local carol3_initial = carol3:heads()

-- All three make concurrent edits
local a3_note = notes.Note.new("Alice's Note (3-way)", "alice")
alice3:apply(notes.NoteOperation.create(a3_note))

local b3_note = notes.Note.new("Bob's Note (3-way)", "bob")
bob3:apply(notes.NoteOperation.create(b3_note))

local c3_note = notes.Note.new("Carol's Note (3-way)", "carol")
carol3:apply(notes.NoteOperation.create(c3_note))

notes.log.info("All three peers made concurrent edits", {
    alice = a3_note.id:sub(1, 8),
    bob = b3_note.id:sub(1, 8),
    carol = c3_note.id:sub(1, 8)
})

-- Sync all pairs: Alice <-> Bob
local sync_a_b = alice3:generate_sync(bob3_initial)
bob3:apply_sync(sync_a_b)
local sync_b_a = bob3:generate_sync(alice3_initial)
alice3:apply_sync(sync_b_a)

-- Sync: Bob <-> Carol (Bob now has Alice's changes)
local bob3_after_alice = bob3:heads()
local sync_b_c = bob3:generate_sync(carol3_initial)
carol3:apply_sync(sync_b_c)
local sync_c_b = carol3:generate_sync(bob3_after_alice)
bob3:apply_sync(sync_c_b)

-- Final sync: Alice <-> Carol (to ensure full convergence)
local alice3_mid = alice3:heads()
local carol3_mid = carol3:heads()
local sync_a_c = alice3:generate_sync(carol3_mid)
carol3:apply_sync(sync_a_c)
local sync_c_a = carol3:generate_sync(alice3_mid)
alice3:apply_sync(sync_c_a)

notes.log.info("Three-way sync completed", {
    alice_count = alice3.note_count,
    bob_count = bob3.note_count,
    carol_count = carol3.note_count
})

-- All should have all 3 notes
notes.assert.eq(alice3.note_count, 3, "Alice should have 3 notes after 3-way sync")
notes.assert.eq(bob3.note_count, 3, "Bob should have 3 notes after 3-way sync")
notes.assert.eq(carol3.note_count, 3, "Carol should have 3 notes after 3-way sync")

-- Verify all notes visible to all peers
notes.assert.not_nil(alice3:find(b3_note.id:sub(1, 8)), "Alice should see Bob's note")
notes.assert.not_nil(alice3:find(c3_note.id:sub(1, 8)), "Alice should see Carol's note")
notes.assert.not_nil(bob3:find(a3_note.id:sub(1, 8)), "Bob should see Alice's note")
notes.assert.not_nil(bob3:find(c3_note.id:sub(1, 8)), "Bob should see Carol's note")
notes.assert.not_nil(carol3:find(a3_note.id:sub(1, 8)), "Carol should see Alice's note")
notes.assert.not_nil(carol3:find(b3_note.id:sub(1, 8)), "Carol should see Bob's note")

notes.log.info("Scenario 4 PASSED: Three-way sync converges correctly")

-- SCENARIO 5: Offline peer catches up
notes.log.info("=== Scenario 5: Offline Peer Sync ===")

local online = notes.SyncableNotebook.new("Offline Test", "Alice")
local offline = notes.SyncableNotebook.fork(online, "Bob")

-- Get offline peer's heads before going "offline"
local offline_checkpoint = offline:heads()

notes.log.info("Bob going offline", { checkpoint_heads = #offline_checkpoint })

-- Online peer makes multiple changes while other is offline
for i = 1, 5 do
    local note = notes.Note.new("Note " .. i .. " (Bob offline)", "alice")
    online:apply(notes.NoteOperation.create(note))
    online:apply(notes.NoteOperation.update_content(note.id, "Content for note " .. i))
end

notes.log.info("Alice made 5 notes while Bob was offline", {
    alice_count = online.note_count
})

-- Verify offline peer still has old state
notes.assert.eq(offline.note_count, 0, "Offline Bob should have 0 notes")

-- Bob comes online and syncs
notes.log.info("Bob coming online, syncing...")

local catchup_sync = online:generate_sync(offline_checkpoint)
local catchup_changed = offline:apply_sync(catchup_sync)

notes.assert.eq(catchup_changed, true, "Catchup sync should apply changes")
notes.assert.eq(offline.note_count, 5, "Bob should have all 5 notes after catchup")

notes.log.info("Scenario 5 PASSED: Offline peer catches up correctly", {
    bob_final_count = offline.note_count
})

-- SCENARIO 6: Content updates sync correctly
notes.log.info("=== Scenario 6: Content Update Sync ===")

local writer = notes.SyncableNotebook.new("Content Test", "Alice")
local reader = notes.SyncableNotebook.fork(writer, "Bob")

-- Create initial note
local doc_note = notes.Note.new("Collaborative Document", "alice")
writer:apply(notes.NoteOperation.create(doc_note))
writer:apply(notes.NoteOperation.update_content(doc_note.id, "Version 1"))

-- Sync to reader
local reader_heads = reader:heads()
local initial_sync = writer:generate_sync(reader_heads)
reader:apply_sync(initial_sync)

-- Verify initial content
local reader_doc = reader:find(doc_note.id:sub(1, 8))
notes.assert.eq(reader_doc.content, "Version 1", "Reader should see Version 1")

-- Writer updates content multiple times
reader_heads = reader:heads()
writer:apply(notes.NoteOperation.update_content(doc_note.id, "Version 2"))
writer:apply(notes.NoteOperation.update_content(doc_note.id, "Version 3 - Final"))

-- Sync updates
local update_sync = writer:generate_sync(reader_heads)
reader:apply_sync(update_sync)

-- Verify final content
reader_doc = reader:find(doc_note.id:sub(1, 8))
notes.assert.eq(reader_doc.content, "Version 3 - Final", "Reader should see final version")

notes.log.info("Scenario 6 PASSED: Content updates sync correctly")

-- SCENARIO 7: Delete operation sync
notes.log.info("=== Scenario 7: Delete Operation Sync ===")

local del_writer = notes.SyncableNotebook.new("Delete Test", "Alice")
local del_reader = notes.SyncableNotebook.fork(del_writer, "Bob")

-- Create a note
local to_delete = notes.Note.new("Note To Delete", "alice")
del_writer:apply(notes.NoteOperation.create(to_delete))
del_writer:apply(notes.NoteOperation.update_content(to_delete.id, "This will be deleted"))

-- Sync to reader
local reader_heads = del_reader:heads()
local create_sync = del_writer:generate_sync(reader_heads)
del_reader:apply_sync(create_sync)

-- Verify both have the note
notes.assert.eq(del_writer.note_count, 1, "Writer should have 1 note before delete")
notes.assert.eq(del_reader.note_count, 1, "Reader should have 1 note before delete")

-- Delete on writer
reader_heads = del_reader:heads()
del_writer:apply(notes.NoteOperation.delete(to_delete.id))
notes.assert.eq(del_writer.note_count, 0, "Writer should have 0 notes after delete")

-- Sync delete to reader
local delete_sync = del_writer:generate_sync(reader_heads)
del_reader:apply_sync(delete_sync)

-- Verify delete propagated
notes.assert.eq(del_reader.note_count, 0, "Reader should have 0 notes after delete sync")
notes.assert.eq(del_reader:find(to_delete.id:sub(1, 8)), nil, "Deleted note should be nil")

notes.log.info("Scenario 7 PASSED: Delete operations sync correctly")

-- Summary
notes.log.info("=" .. string.rep("=", 50))
notes.log.info("  Real Automerge Sync Test PASSED!")
notes.log.info("  All scenarios verified with REAL CRDT sync")
notes.log.info("=" .. string.rep("=", 50))

-- Multi-Instance Sync Test (Simulated)
--
-- This test demonstrates the pattern for multi-instance testing.
-- Note: Full Automerge sync requires the sync methods to be implemented
-- in the App (generate_sync, merge_sync). This is a scaffold for that.
--
-- Run with: indras-notes run-script scripts/tests/multi_instance_sync.lua

notes.log.info("Starting Multi-Instance Sync Test (Simulated)")

-- Create isolated App instances with separate storage
local alice = notes.App.new_with_temp_storage()
local bob = notes.App.new_with_temp_storage()

-- Initialize identities
alice:init("Alice")
bob:init("Bob")

notes.log.info("Created test instances", {
    alice_id = alice:peer_id(),
    bob_id = bob:peer_id()
})

-- Alice creates a notebook
notes.log.info("Alice creating notebook...")
local alice_notebook_id = alice:create_notebook("Shared Notes")
alice:open_notebook(alice_notebook_id)

notes.log.info("Alice's notebook created", {
    notebook_id = alice_notebook_id,
    creator = "alice"
})

-- Alice creates a note
local alice_note_id = alice:create_note("Alice's Note")
alice:update_note_content(alice_note_id, "Content from Alice")

notes.log.info("Alice created note", {
    note_id = alice_note_id:sub(1, 8),
    peer = "alice"
})

-- Bob creates his own notebook (simulating join - in real app would use invite)
notes.log.info("Bob creating notebook (simulating independent state)...")
local bob_notebook_id = bob:create_notebook("Shared Notes")
bob:open_notebook(bob_notebook_id)

-- Bob creates a concurrent note (before sync)
local bob_note_id = bob:create_note("Bob's Note")
bob:update_note_content(bob_note_id, "Content from Bob")

notes.log.info("Bob created note", {
    note_id = bob_note_id:sub(1, 8),
    peer = "bob"
})

-- At this point, Alice and Bob have divergent states:
-- - Alice has: "Alice's Note"
-- - Bob has: "Bob's Note"
--
-- In a real implementation with full Automerge sync, we would:
-- 1. Generate sync messages: alice:generate_sync(bob:peer_id())
-- 2. Apply sync: bob:merge_sync(sync_msg)
-- 3. Generate response: bob:generate_sync(alice:peer_id())
-- 4. Apply response: alice:merge_sync(response)
--
-- After sync, both would have both notes (CRDT convergence).

notes.log.info("Current state (before sync simulation)", {
    alice_notes = alice:note_count(),
    bob_notes = bob:note_count()
})

-- Verify individual states
notes.assert.eq(alice:note_count(), 1, "Alice should have 1 note")
notes.assert.eq(bob:note_count(), 1, "Bob should have 1 note")

-- Verify Alice's note
local alice_note = alice:find_note(alice_note_id:sub(1, 8))
notes.assert.not_nil(alice_note, "Alice should have her note")
notes.assert.eq(alice_note.title, "Alice's Note")

-- Verify Bob's note
local bob_note = bob:find_note(bob_note_id:sub(1, 8))
notes.assert.not_nil(bob_note, "Bob should have his note")
notes.assert.eq(bob_note.title, "Bob's Note")

-- SIMULATED SYNC
-- In a real implementation, this would be replaced with actual Automerge sync.
-- For now, we demonstrate the expected end state by manual operations.

notes.log.info("Simulating sync (manual state merge)...")

-- Simulate: Alice receives Bob's note
local sync_note_for_alice = notes.Note.new("Bob's Note", "bob_sync")
local sync_op_alice = notes.NoteOperation.create(sync_note_for_alice)
local alice_nb = alice:current_notebook()
alice_nb:apply(sync_op_alice)

-- Simulate: Bob receives Alice's note
local sync_note_for_bob = notes.Note.new("Alice's Note", "alice_sync")
local sync_op_bob = notes.NoteOperation.create(sync_note_for_bob)
local bob_nb = bob:current_notebook()
bob_nb:apply(sync_op_bob)

notes.log.info("Sync simulation completed", {
    alice_notes = alice:note_count(),
    bob_notes = bob:note_count()
})

-- VERIFY: Both should have 2 notes after "sync"
notes.assert.eq(alice:note_count(), 2, "Alice should have 2 notes after sync")
notes.assert.eq(bob:note_count(), 2, "Bob should have 2 notes after sync")

-- Verify log assertions (if capturing)
if notes.log_assert and notes.log_assert.count() > 0 then
    notes.log_assert.expect_sequence({
        "Alice creating notebook",
        "Alice created note",
        "Bob creating notebook",
        "Bob created note",
        "Sync simulation completed"
    })
    notes.log_assert.no_errors()
end

-- Cleanup
alice:close_notebook()
bob:close_notebook()

notes.log.info("=" .. string.rep("=", 40))
notes.log.info("  Multi-Instance Sync Test PASSED!")
notes.log.info("  (Simulated - real sync requires Automerge integration)")
notes.log.info("=" .. string.rep("=", 40))

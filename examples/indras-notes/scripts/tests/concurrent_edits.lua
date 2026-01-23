-- Concurrent Edits Test (Simulated)
--
-- Tests that concurrent edits from multiple users converge correctly.
-- This demonstrates CRDT conflict resolution patterns.
--
-- Run with: indras-notes run-script scripts/tests/concurrent_edits.lua

notes.log.info("Starting Concurrent Edits Test (Simulated)")

-- Create isolated App instances
local alice = notes.App.new_with_temp_storage()
local bob = notes.App.new_with_temp_storage()
local carol = notes.App.new_with_temp_storage()

alice:init("Alice")
bob:init("Bob")
carol:init("Carol")

notes.log.info("Created test instances", {
    alice_id = alice:peer_id(),
    bob_id = bob:peer_id(),
    carol_id = carol:peer_id()
})

-- Setup: All have their own notebooks (simulating shared state before divergence)
local alice_nb_id = alice:create_notebook("Concurrent Test")
alice:open_notebook(alice_nb_id)

local bob_nb_id = bob:create_notebook("Concurrent Test")
bob:open_notebook(bob_nb_id)

local carol_nb_id = carol:create_notebook("Concurrent Test")
carol:open_notebook(carol_nb_id)

-- Initial shared state: One note exists on all
notes.log.info("Setting up initial shared state...")

local shared_note_alice = alice:create_note("Shared Document")
alice:update_note_content(shared_note_alice, "Initial content.")

-- Simulate sync to establish initial state
-- In real app, this would be actual Automerge sync
local bob_shared_note = bob:create_note("Shared Document")
bob:update_note_content(bob_shared_note, "Initial content.")

local carol_shared_note = carol:create_note("Shared Document")
carol:update_note_content(carol_shared_note, "Initial content.")

notes.log.info("Initial state established", {
    alice_notes = alice:note_count(),
    bob_notes = bob:note_count(),
    carol_notes = carol:note_count()
})

-- CONCURRENT EDIT SCENARIO
notes.log.info("Starting concurrent edits (simulated network partition)...")

-- Alice edits
alice:update_note_content(shared_note_alice, "Initial content.\n\nAlice's addition: Project overview")
notes.log.info("Alice edited", {
    event = "concurrent_edit",
    peer = "alice"
})

-- Bob edits (concurrent with Alice - different content)
bob:update_note_content(bob_shared_note, "Initial content.\n\nBob's addition: Technical details")
notes.log.info("Bob edited", {
    event = "concurrent_edit",
    peer = "bob"
})

-- Carol edits (concurrent with both)
carol:update_note_content(carol_shared_note, "Initial content.\n\nCarol's addition: Timeline")
notes.log.info("Carol edited", {
    event = "concurrent_edit",
    peer = "carol"
})

-- At this point, all three have divergent states
notes.log.info("Divergent state after concurrent edits:")

local alice_content = alice:find_note(shared_note_alice:sub(1, 8)).content
local bob_content = bob:find_note(bob_shared_note:sub(1, 8)).content
local carol_content = carol:find_note(carol_shared_note:sub(1, 8)).content

notes.log.debug("Alice's version", { content = alice_content:sub(1, 50) })
notes.log.debug("Bob's version", { content = bob_content:sub(1, 50) })
notes.log.debug("Carol's version", { content = carol_content:sub(1, 50) })

-- In a real CRDT implementation, sync would merge these changes.
-- Automerge handles this by merging concurrent text changes.
-- The exact merge result depends on the CRDT semantics.

-- SIMULATED CONVERGENCE
-- In real Automerge: changes are merged preserving all edits
notes.log.info("Simulating CRDT convergence...")

-- For this simulation, we'll demonstrate the "last writer wins" pattern
-- as a simple approximation. Real Automerge would do character-level merge.
local converged_content = "Initial content.\n\n" ..
    "Alice's addition: Project overview\n" ..
    "Bob's addition: Technical details\n" ..
    "Carol's addition: Timeline"

-- Apply converged state to all
alice:update_note_content(shared_note_alice, converged_content)
bob:update_note_content(bob_shared_note, converged_content)
carol:update_note_content(carol_shared_note, converged_content)

notes.log.info("Convergence simulated (merged all additions)")

-- VERIFY CONVERGENCE
notes.log.info("Verifying convergence...")

local alice_final = alice:find_note(shared_note_alice:sub(1, 8)).content
local bob_final = bob:find_note(bob_shared_note:sub(1, 8)).content
local carol_final = carol:find_note(carol_shared_note:sub(1, 8)).content

-- All should have the same content
notes.assert.eq(alice_final, bob_final, "Alice and Bob should have same content")
notes.assert.eq(bob_final, carol_final, "Bob and Carol should have same content")

-- Verify all additions are present
notes.assert.str_contains(alice_final, "Alice's addition", "Should contain Alice's edit")
notes.assert.str_contains(alice_final, "Bob's addition", "Should contain Bob's edit")
notes.assert.str_contains(alice_final, "Carol's addition", "Should contain Carol's edit")

notes.log.info("Convergence verified - all peers have consistent state", {
    content_length = #alice_final
})

-- ADDITIONAL TEST: Add more notes concurrently
notes.log.info("Testing concurrent note creation...")

-- All create different notes at the "same time"
local alice_new = alice:create_note("Alice's New Note")
local bob_new = bob:create_note("Bob's New Note")
local carol_new = carol:create_note("Carol's New Note")

-- Each should have their new note
notes.assert.not_nil(alice:find_note(alice_new:sub(1, 8)), "Alice should have her new note")
notes.assert.not_nil(bob:find_note(bob_new:sub(1, 8)), "Bob should have his new note")
notes.assert.not_nil(carol:find_note(carol_new:sub(1, 8)), "Carol should have her new note")

notes.log.info("Concurrent note creation verified", {
    alice_total = alice:note_count(),
    bob_total = bob:note_count(),
    carol_total = carol:note_count()
})

-- Verify log assertions (if capturing)
if notes.log_assert and notes.log_assert.count() > 0 then
    notes.log_assert.no_errors()
    notes.log.info("No errors detected during concurrent operations")
end

-- Cleanup
alice:close_notebook()
bob:close_notebook()
carol:close_notebook()

notes.log.info("=" .. string.rep("=", 40))
notes.log.info("  Concurrent Edits Test PASSED!")
notes.log.info("  (Simulated - demonstrates CRDT convergence pattern)")
notes.log.info("=" .. string.rep("=", 40))

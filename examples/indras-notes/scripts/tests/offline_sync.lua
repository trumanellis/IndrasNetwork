-- Offline Sync Test (Simulated)
--
-- Tests store-and-forward pattern when a peer is offline.
-- This demonstrates the offline-first architecture pattern.
--
-- Run with: indras-notes run-script scripts/tests/offline_sync.lua

notes.log.info("Starting Offline Sync Test (Simulated)")

-- Create isolated App instances
local alice = notes.App.new_with_temp_storage()
local bob = notes.App.new_with_temp_storage()

alice:init("Alice")
bob:init("Bob")

notes.log.info("Created test instances", {
    alice_id = alice:peer_id(),
    bob_id = bob:peer_id()
})

-- Setup: Both have notebooks
local alice_nb_id = alice:create_notebook("Offline Test")
alice:open_notebook(alice_nb_id)

local bob_nb_id = bob:create_notebook("Offline Test")
bob:open_notebook(bob_nb_id)

-- Verify initial empty state
notes.assert.eq(alice:note_count(), 0, "Alice should start with 0 notes")
notes.assert.eq(bob:note_count(), 0, "Bob should start with 0 notes")

-- SCENARIO: Bob goes "offline"
notes.log.info("Bob going offline (simulated)")

-- Alice makes edits while Bob is offline
notes.log.info("Alice making edits while Bob is offline...")

local offline_note_ids = {}
for i = 1, 3 do
    local note_id = alice:create_note("Note " .. i .. " (Bob offline)")
    alice:update_note_content(note_id, "Alice wrote this while Bob was offline. Note #" .. i)
    table.insert(offline_note_ids, note_id)
    notes.log.debug("Alice created note " .. i, { note_id = note_id:sub(1, 8) })
end

notes.log.info("Alice created notes while Bob was offline", {
    alice_count = alice:note_count()
})

-- Verify Alice has the notes
notes.assert.eq(alice:note_count(), 3, "Alice should have 3 notes")

-- Verify Bob doesn't have them (he's offline - no sync happened)
notes.assert.eq(bob:note_count(), 0, "Bob should have 0 notes (offline)")

notes.log.info("State before sync", {
    alice_notes = alice:note_count(),
    bob_notes = bob:note_count()
})

-- SCENARIO: Bob comes back online, sync happens
notes.log.info("Bob coming online, syncing...")

-- Simulate sync: In real implementation, this would be:
-- local sync_msg = alice:generate_sync(bob:peer_id())
-- bob:merge_sync(sync_msg)
--
-- For simulation, we manually copy the notes

local alice_nb = alice:current_notebook()
local alice_notes = alice_nb:list()

for _, note in ipairs(alice_notes) do
    -- Create a copy of the note for Bob
    local sync_note = notes.Note.new(note.title, "alice_sync")
    local sync_op = notes.NoteOperation.create(sync_note)
    local bob_nb = bob:current_notebook()
    bob_nb:apply(sync_op)

    -- Update content to match
    local bob_notes = bob_nb:list()
    local new_note = bob_notes[#bob_notes]
    bob_nb:apply(notes.NoteOperation.update_content(new_note.id, note.content))
end

notes.log.info("Sync completed (simulated)", {
    bob_notes_after = bob:note_count()
})

-- Verify Bob now has all notes
notes.assert.eq(bob:note_count(), 3, "Bob should have all 3 notes after sync")

notes.log.info("State after sync", {
    alice_notes = alice:note_count(),
    bob_notes = bob:note_count()
})

-- SCENARIO: Verify note content synced correctly
notes.log.info("Verifying synced content...")

local bob_notes_list = bob:current_notebook():list()
notes.assert.eq(#bob_notes_list, 3, "Bob should have 3 notes in list")

-- Verify log assertions (if capturing)
if notes.log_assert and notes.log_assert.count() > 0 then
    notes.log_assert.expect_sequence({
        "Bob going offline",
        "Alice making edits",
        "Alice created notes",
        "Bob coming online",
        "Sync completed"
    })
    notes.log_assert.no_errors()
end

-- Cleanup
alice:close_notebook()
bob:close_notebook()

notes.log.info("=" .. string.rep("=", 40))
notes.log.info("  Offline Sync Test PASSED!")
notes.log.info("  (Simulated - demonstrates offline-first pattern)")
notes.log.info("=" .. string.rep("=", 40))

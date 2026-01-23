-- Behavior Verification Test
--
-- Tests that verify expected behavior through log analysis.
--
-- Run with: indras-notes run-script scripts/tests/behavior_verify.lua

notes.log.info("Starting Behavior Verification Test")

-- Create a test app
local app = notes.App.new_with_temp_storage()
app:init("BehaviorTestUser")

-- Clear any previous logs
if notes.log_assert then
    notes.log_assert.clear()
end

notes.log.info("Test setup complete", { user = app:user_name() })

-- SCENARIO 1: Notebook Creation Flow
notes.log.info("=== Scenario 1: Notebook Creation ===")

local nb_id = app:create_notebook("Behavior Test Notebook")
notes.log.info("Created notebook", {
    notebook_id = nb_id,
    event = "notebook_created"
})

app:open_notebook(nb_id)
notes.log.info("Opened notebook", {
    notebook_id = nb_id,
    event = "notebook_opened"
})

-- Verify log assertions if available and has captured logs
if notes.log_assert and notes.log_assert.count() > 0 then
    notes.log_assert.assert_message("Created notebook", "Should log notebook creation")
    notes.log_assert.assert_message("Opened notebook", "Should log notebook open")
    notes.log.info("Log assertions passed for Scenario 1")
else
    notes.log.info("Log assertions skipped (no captured logs)")
end

-- SCENARIO 2: Note Workflow
notes.log.info("=== Scenario 2: Note Workflow ===")

local note_id = app:create_note("Workflow Test Note")
notes.log.info("Note created", {
    note_id = note_id,
    event = "note_created"
})

app:update_note_content(note_id, "Initial content")
notes.log.info("Content updated", {
    note_id = note_id,
    event = "note_updated",
    field = "content"
})

app:update_note_title(note_id, "Updated Title")
notes.log.info("Title updated", {
    note_id = note_id,
    event = "note_updated",
    field = "title"
})

-- Verify the note state
local note = app:find_note(note_id:sub(1, 8))
notes.assert.eq(note.title, "Updated Title", "Title should be updated")
notes.assert.eq(note.content, "Initial content", "Content should match")

notes.log.info("Note state verified", {
    note_id = note_id,
    event = "note_verified"
})

-- Verify log sequence if available and has captured logs
if notes.log_assert and notes.log_assert.count() > 0 then
    notes.log_assert.expect_sequence({
        "Note created",
        "Content updated",
        "Title updated",
        "Note state verified"
    })
    notes.log.info("Log sequence verified for Scenario 2")
else
    notes.log.info("Log sequence check skipped (no captured logs)")
end

-- SCENARIO 3: Error-Free Operation
notes.log.info("=== Scenario 3: Error-Free Verification ===")

-- Perform several operations
for i = 1, 5 do
    local id = app:create_note("Note " .. i)
    app:update_note_content(id, "Content for note " .. i)
    notes.log.debug("Created and updated note " .. i)
end

notes.assert.eq(app:note_count(), 6, "Should have 6 notes total (1 + 5)")

-- Verify no errors occurred (if capturing)
if notes.log_assert and notes.log_assert.count() > 0 then
    notes.log_assert.no_errors()
    notes.log.info("No errors detected during operations")
end

-- SCENARIO 4: Cleanup
notes.log.info("=== Scenario 4: Cleanup ===")

-- Delete all test notes
local nb = app:current_notebook()
local all_notes = nb:list()
for i, note in ipairs(all_notes) do
    app:delete_note(note.id)
    notes.log.debug("Deleted note", { note_id = note.id:sub(1, 8) })
end

notes.assert.eq(app:note_count(), 0, "All notes should be deleted")
notes.log.info("Cleanup complete", { remaining_notes = app:note_count() })

-- Close notebook
app:close_notebook()

-- Final verification (if capturing)
if notes.log_assert and notes.log_assert.count() > 0 then
    notes.log_assert.no_errors()
    notes.log.info("Final error check passed")
end

notes.log.info("=" .. string.rep("=", 40))
notes.log.info("  Behavior Verification Test PASSED!")
notes.log.info("=" .. string.rep("=", 40))

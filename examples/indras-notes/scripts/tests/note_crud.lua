-- Note CRUD Operations Test
--
-- Tests basic Create, Read, Update, Delete operations on notes.
--
-- Run with: indras-notes run-script scripts/tests/note_crud.lua

notes.log.info("Starting Note CRUD Test")

-- Create a test app with temporary storage
local app = notes.App.new_with_temp_storage()
app:init("CRUDTestUser")

notes.log.info("Created test app", { user = app:user_name() })

-- CREATE NOTEBOOK
notes.log.info("Creating notebook...")
local notebook_id = app:create_notebook("Test Notebook")
notes.assert.not_nil(notebook_id, "Notebook ID should not be nil")
notes.log.info("Created notebook", { id = notebook_id })

-- Open the notebook
app:open_notebook(notebook_id)
notes.log.info("Opened notebook")

-- Verify initial state
notes.assert.eq(app:note_count(), 0, "Should start with 0 notes")

-- CREATE NOTE
notes.log.info("Testing CREATE...")
local note_id = app:create_note("My First Note")
notes.assert.not_nil(note_id, "Note ID should not be nil")
notes.log.info("Created note", { note_id = note_id })

-- Verify note count
notes.assert.eq(app:note_count(), 1, "Should have 1 note after create")

-- READ NOTE
notes.log.info("Testing READ...")
local note = app:find_note(note_id:sub(1, 8))
notes.assert.not_nil(note, "Should be able to find note by partial ID")
notes.assert.eq(note.title, "My First Note", "Title should match")
notes.assert.eq(note.content, "", "Content should be empty initially")
notes.log.info("Found note", { title = note.title, author = note.author })

-- UPDATE NOTE CONTENT
notes.log.info("Testing UPDATE content...")
app:update_note_content(note_id, "Hello, World!\n\nThis is my first note.")
note = app:find_note(note_id:sub(1, 8))
notes.assert.str_contains(note.content, "Hello, World!", "Content should contain updated text")
notes.log.info("Updated content", { content_length = #note.content })

-- UPDATE NOTE TITLE
notes.log.info("Testing UPDATE title...")
app:update_note_title(note_id, "My Updated Note")
note = app:find_note(note_id:sub(1, 8))
notes.assert.eq(note.title, "My Updated Note", "Title should be updated")
notes.log.info("Updated title", { new_title = note.title })

-- CREATE ANOTHER NOTE
notes.log.info("Creating second note...")
local note_id_2 = app:create_note("Second Note")
app:update_note_content(note_id_2, "This is the second note content.")
notes.assert.eq(app:note_count(), 2, "Should have 2 notes now")

-- List all notes
local nb = app:current_notebook()
local all_notes = nb:list()
notes.assert.eq(#all_notes, 2, "List should return 2 notes")
notes.log.info("Listed notes", { count = #all_notes })

-- DELETE NOTE
notes.log.info("Testing DELETE...")
app:delete_note(note_id)
notes.assert.eq(app:note_count(), 1, "Should have 1 note after delete")
notes.assert.nil_(app:find_note(note_id:sub(1, 8)), "Deleted note should not be found")
notes.log.info("Deleted note", { deleted_id = note_id:sub(1, 8) })

-- VERIFY REMAINING NOTE
local remaining = app:find_note(note_id_2:sub(1, 8))
notes.assert.not_nil(remaining, "Second note should still exist")
notes.assert.eq(remaining.title, "Second Note", "Remaining note should be the second one")

-- CLEANUP
app:close_notebook()
notes.log.info("Closed notebook")

notes.log.info("=" .. string.rep("=", 40))
notes.log.info("  Note CRUD Test PASSED!")
notes.log.info("=" .. string.rep("=", 40))

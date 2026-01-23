-- Shared utility functions for indras-notes Lua scripts
--
-- This module provides common helper functions used across test scripts.

local helpers = {}

-- Wait helper (simulates time passing in tests)
-- Note: In real async scenarios, you'd use proper async waits
function helpers.sleep(ms)
    -- Lua doesn't have native sleep, this is a placeholder
    -- In production, you'd integrate with tokio or similar
    local start = os.clock()
    while os.clock() - start < ms / 1000 do end
end

-- Generate a unique name with timestamp
function helpers.unique_name(prefix)
    prefix = prefix or "test"
    return prefix .. "_" .. os.time()
end

-- Pretty print a table (for debugging)
function helpers.dump(value, indent)
    indent = indent or 0
    local spaces = string.rep("  ", indent)

    if type(value) == "table" then
        local result = "{\n"
        for k, v in pairs(value) do
            result = result .. spaces .. "  " .. tostring(k) .. " = "
            result = result .. helpers.dump(v, indent + 1)
            result = result .. ",\n"
        end
        return result .. spaces .. "}"
    else
        return tostring(value)
    end
end

-- Print a section header
function helpers.section(name)
    notes.log.info("=" .. string.rep("=", 40))
    notes.log.info("  " .. name)
    notes.log.info("=" .. string.rep("=", 40))
end

-- Assert with detailed error message
function helpers.assert_eq(actual, expected, msg)
    if actual ~= expected then
        local err = msg or "Assertion failed"
        err = err .. "\n  Expected: " .. tostring(expected)
        err = err .. "\n  Actual:   " .. tostring(actual)
        error(err)
    end
end

-- Create a test app with temporary storage
function helpers.create_test_app(name)
    name = name or "TestUser"
    local app = notes.App.new_with_temp_storage()
    app:init(name)
    return app
end

-- Create a test app with a notebook already open
function helpers.create_test_app_with_notebook(user_name, notebook_name)
    user_name = user_name or "TestUser"
    notebook_name = notebook_name or "Test Notebook"

    local app = helpers.create_test_app(user_name)
    local nb_id = app:create_notebook(notebook_name)
    app:open_notebook(nb_id)

    return app, nb_id
end

-- Create multiple notes in the current notebook
function helpers.create_test_notes(app, count)
    count = count or 3
    local note_ids = {}

    for i = 1, count do
        local title = "Test Note " .. i
        local note_id = app:create_note(title)
        app:update_note_content(note_id, "Content for note " .. i)
        table.insert(note_ids, note_id)
    end

    return note_ids
end

return helpers

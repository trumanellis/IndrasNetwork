-- Navigation smoke test
-- Verifies: app starts, sidebar items exist, clicking them changes the active item.

local me = indras.my_name()
indras.log.info("Starting navigation smoke test as " .. me)

-- Wait for the app to be ready
indras.wait_for("app_ready")
indras.log.info("App is ready")

-- Check we're in workspace phase
local phase = indras.query("app_phase")
indras.assert.eq(phase, "workspace", "Should be in workspace phase")

-- Query sidebar items
local items = indras.query("sidebar_items")
indras.log.info("Found " .. #items .. " sidebar items")
indras.assert.gt(#items, 0, "Should have at least one sidebar item")

-- Click each sidebar item and verify it becomes active
for _, item in ipairs(items) do
    indras.click_sidebar(item.label)
    indras.wait(0.3)
    local active = indras.query("active_sidebar_item")
    indras.assert.eq(active, item.label, "Active item should be " .. item.label)
    indras.log.info("Verified: " .. item.label .. " (" .. item.icon .. ")")
end

indras.log.info("Navigation smoke test passed")

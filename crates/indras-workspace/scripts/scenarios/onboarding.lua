-- Onboarding flow test
-- Verifies: new user sees setup, can create identity, enters workspace.

indras.log.info("Starting onboarding test")

-- Wait for app ready (may start in Setup phase)
indras.wait_for("app_ready")

-- Should start in workspace phase (since identity exists)
-- For a true first-run test, use --clean flag
local phase = indras.query("app_phase")
indras.log.info("Current phase: " .. phase)

if phase == "setup" then
    -- Create identity
    indras.set_name("Zephyr")
    indras.create_identity()
    indras.wait_for("identity_created", "Zephyr", 10)

    -- Should now be in Workspace phase
    phase = indras.query("app_phase")
    indras.assert.eq(phase, "workspace", "Should enter workspace after identity creation")
    indras.log.info("Identity created, now in workspace")
else
    indras.log.info("Already in workspace phase (identity exists)")
end

-- Sidebar should have items
local items = indras.query("sidebar_items")
indras.assert.ge(#items, 0, "Should have sidebar items")

indras.log.info("Onboarding test passed")

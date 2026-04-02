-- live_vault_conflict_lifecycle.lua
--
-- Integration test: full conflict lifecycle for VaultSync.
--
-- Tests:
--   1. Concurrent edits within 60s window create a conflict
--   2. Conflict record has correct fields (winner_hash, loser_hash, conflict_file)
--   3. resolve_conflict() marks the conflict as resolved on both nodes
--   4. Multiple concurrent conflicts on different files are tracked independently
--   5. Identical content written by two nodes does not produce a conflict
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/live_vault_conflict_lifecycle.lua

package.path = package.path .. ";simulation/scripts/lib/?.lua"
local h = require("live_network_helpers")

print("=== Vault Conflict Lifecycle Test ===")
print()

-- ============================================================================
-- PHASE 1: Create 2 networks, connect, A creates vault, B joins
-- ============================================================================

h.section(1, "Create 2 networks and shared vault")

local nets = h.create_networks(2)
local a, b = nets[1], nets[2]
h.connect_all(nets)
print("    2 networks started and connected")

local tmp_a = os.tmpname() .. "_conflict_a"
local tmp_b = os.tmpname() .. "_conflict_b"

local vault_a, invite = indras.VaultSync.create(a, "ConflictVault", tmp_a)
print("    A created vault at " .. tmp_a)

local vault_b = indras.VaultSync.join(b, invite, tmp_b)
print("    B joined vault at " .. tmp_b)

-- ============================================================================
-- PHASE 2: A writes initial file; wait for sync to B
-- ============================================================================

h.section(2, "A writes notes/shared.md; wait for B to sync")

vault_a:write_file("notes/shared.md", "Version A")

h.assert_eventually(function()
    local fb = vault_b:list_files()
    for _, f in ipairs(fb) do
        if f.path == "notes/shared.md" then
            return true
        end
    end
    return false
end, { timeout = 15, interval = 1, msg = "B should receive notes/shared.md from A" })

print("    B has notes/shared.md")

-- ============================================================================
-- PHASE 3: Both A and B write different content concurrently (within 60s)
-- ============================================================================

h.section(3, "Concurrent edits to notes/shared.md (within 60s conflict window)")

vault_a:write_file("notes/shared.md", "Edited by A")
vault_b:write_file("notes/shared.md", "Edited by B")
print("    A wrote 'Edited by A', B wrote 'Edited by B' to notes/shared.md")

-- ============================================================================
-- PHASE 4: Verify conflict detected with correct fields
-- ============================================================================

h.section(4, "Verify conflict detected on both nodes with correct fields")

h.assert_eventually(function()
    local ca = vault_a:list_conflicts()
    for _, c in ipairs(ca) do
        if c.path == "notes/shared.md" then
            return true
        end
    end
    return false
end, { timeout = 15, interval = 1, msg = "A should detect conflict on notes/shared.md" })

local conflicts_a = vault_a:list_conflicts()
local shared_conflict = nil
for _, c in ipairs(conflicts_a) do
    if c.path == "notes/shared.md" then
        shared_conflict = c
        break
    end
end

indras.assert.true_(shared_conflict ~= nil, "conflict entry for notes/shared.md must exist")
indras.assert.true_(shared_conflict.winner_hash ~= nil and shared_conflict.winner_hash ~= "",
    "conflict must have non-empty winner_hash")
indras.assert.true_(shared_conflict.loser_hash ~= nil and shared_conflict.loser_hash ~= "",
    "conflict must have non-empty loser_hash")
indras.assert.true_(shared_conflict.conflict_file ~= nil and shared_conflict.conflict_file ~= "",
    "conflict must have a conflict_file name")
-- conflict_file should look like "notes/shared.conflict-XXXXXX.md"
indras.assert.true_(
    string.find(shared_conflict.conflict_file, "shared%.conflict") ~= nil,
    "conflict_file should contain 'shared.conflict': got " .. tostring(shared_conflict.conflict_file)
)
print("    A sees conflict: winner=" .. shared_conflict.winner_hash:sub(1, 8) ..
    "... loser=" .. shared_conflict.loser_hash:sub(1, 8) ..
    "... file=" .. shared_conflict.conflict_file)

-- B should also see the conflict after sync
h.assert_eventually(function()
    local cb = vault_b:list_conflicts()
    for _, c in ipairs(cb) do
        if c.path == "notes/shared.md" then
            return true
        end
    end
    return false
end, { timeout = 15, interval = 1, msg = "B should also see the conflict on notes/shared.md" })
print("    B also sees the conflict (synced)")

-- ============================================================================
-- PHASE 5: Resolve the conflict; verify marked resolved on both nodes
-- ============================================================================

h.section(5, "Resolve conflict on A; verify resolved on both nodes")

-- loser_hash must be the full 64-char hex string
local loser_hex = shared_conflict.loser_hash
indras.assert.eq(#loser_hex, 64, "loser_hash must be 64 hex chars, got " .. #loser_hex)

vault_a:resolve_conflict("notes/shared.md", loser_hex)
print("    A called resolve_conflict('notes/shared.md', loser_hash)")

h.assert_eventually(function()
    local ca = vault_a:list_conflicts()
    for _, c in ipairs(ca) do
        if c.path == "notes/shared.md" then
            return c.resolved == true
        end
    end
    return false
end, { timeout = 15, interval = 1, msg = "A: conflict on notes/shared.md should be marked resolved" })
print("    A: conflict marked resolved")

h.assert_eventually(function()
    local cb = vault_b:list_conflicts()
    for _, c in ipairs(cb) do
        if c.path == "notes/shared.md" then
            return c.resolved == true
        end
    end
    return false
end, { timeout = 15, interval = 1, msg = "B: conflict on notes/shared.md should be marked resolved" })
print("    B: conflict marked resolved (synced)")

-- ============================================================================
-- PHASE 6: Multiple concurrent conflicts on different files
-- ============================================================================

h.section(6, "Multiple concurrent conflicts: docs/alpha.md and docs/beta.md")

vault_a:write_file("docs/alpha.md", "A version")
vault_b:write_file("docs/alpha.md", "B version")
vault_a:write_file("docs/beta.md", "A version")
vault_b:write_file("docs/beta.md", "B version")
print("    A and B both wrote conflicting docs/alpha.md and docs/beta.md")

h.assert_eventually(function()
    local ca = vault_a:list_conflicts()
    local found_alpha, found_beta = false, false
    for _, c in ipairs(ca) do
        if c.path == "docs/alpha.md" and not c.resolved then found_alpha = true end
        if c.path == "docs/beta.md" and not c.resolved then found_beta = true end
    end
    return found_alpha and found_beta
end, { timeout = 15, interval = 1, msg = "A should detect unresolved conflicts for alpha.md and beta.md" })

local ca6 = vault_a:list_conflicts()
local unresolved = 0
for _, c in ipairs(ca6) do
    if not c.resolved then unresolved = unresolved + 1 end
end
indras.assert.ge(unresolved, 2, "at least 2 unresolved conflicts after phase 6, got " .. unresolved)
print("    A sees " .. unresolved .. " unresolved conflict(s) (>=2 expected)")

-- ============================================================================
-- PHASE 7: Identical content = no conflict
-- ============================================================================

h.section(7, "Identical content on both nodes produces no new conflict")

-- Count current conflicts before writing gamma.md
local conflicts_before = vault_a:list_conflicts()
local count_before = #conflicts_before

local identical = "identical content"
vault_a:write_file("docs/gamma.md", identical)
vault_b:write_file("docs/gamma.md", identical)
print("    A and B both wrote identical content to docs/gamma.md")

-- Wait for sync
h.assert_eventually(function()
    local fa = vault_a:list_files()
    for _, f in ipairs(fa) do
        if f.path == "docs/gamma.md" then
            local fb = vault_b:list_files()
            for _, g in ipairs(fb) do
                if g.path == "docs/gamma.md" then
                    return f.hash_hex == g.hash_hex
                end
            end
        end
    end
    return false
end, { timeout = 15, interval = 1, msg = "gamma.md should be synced with same hash on both nodes" })

-- Give conflict detection a moment to (not) fire
indras.sleep(2)

local conflicts_after = vault_a:list_conflicts()
local gamma_conflict = nil
for _, c in ipairs(conflicts_after) do
    if c.path == "docs/gamma.md" then
        gamma_conflict = c
        break
    end
end
indras.assert.false_(gamma_conflict ~= nil,
    "docs/gamma.md must NOT produce a conflict (same content = same hash)")
print("    No conflict for docs/gamma.md (same hash on both sides)")

-- ============================================================================
-- PHASE 8: Final summary
-- ============================================================================

h.section(8, "Final summary")

local final_files_a = vault_a:list_files()
local final_conflicts_a = vault_a:list_conflicts()
local resolved_count = 0
local unresolved_count = 0
for _, c in ipairs(final_conflicts_a) do
    if c.resolved then
        resolved_count = resolved_count + 1
    else
        unresolved_count = unresolved_count + 1
    end
end

print("    Total files (A): " .. #final_files_a)
print("    Total conflicts (A): " .. #final_conflicts_a ..
    " (" .. resolved_count .. " resolved, " .. unresolved_count .. " unresolved)")

-- Cleanup
vault_a:stop()
vault_b:stop()
h.stop_all(nets)

print()
print("=== All phases passed! ===")

return true

-- live_vault_sync.lua
--
-- Integration test: P2P vault sync between 3 nodes with file operations,
-- LWW merge, and conflict detection.
--
-- Exercises indras-vault-sync bindings: create/join vault, write/delete files,
-- list_files, list_conflicts, resolve_conflict.
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/live_vault_sync.lua --pretty

package.path = package.path .. ";simulation/scripts/lib/?.lua"
local h = require("live_network_helpers")

print("=== P2P Vault Sync Test ===")
print()

-- ============================================================================
-- PHASE 1: Create 3 networks and a shared vault
-- ============================================================================

h.section(1, "Create networks and vault")
local nets = h.create_networks(3)
local a, b, c = nets[1], nets[2], nets[3]
h.connect_all(nets)
print("    3 networks started and connected")

-- A creates a vault, B and C join via invite
local tmp_a = os.tmpname() .. "_vault_a"
local tmp_b = os.tmpname() .. "_vault_b"
local tmp_c = os.tmpname() .. "_vault_c"

local vault_a, invite = indras.VaultSync.create(a, "SharedVault", tmp_a)
print("    A created vault at " .. tmp_a)

local vault_b = indras.VaultSync.join(b, invite, tmp_b)
print("    B joined vault at " .. tmp_b)

local vault_c = indras.VaultSync.join(c, invite, tmp_c)
print("    C joined vault at " .. tmp_c)

-- ============================================================================
-- PHASE 2: A writes files
-- ============================================================================

h.section(2, "A writes 3 markdown files + 1 binary attachment")

vault_a:write_file("notes/hello.md", "# Hello\n\nThis is a test note.")
vault_a:write_file("notes/todo.md", "# TODO\n\n- [ ] Build vault sync\n- [ ] Test it")
vault_a:write_file("daily/2026-04-02.md", "# Daily Note\n\nWorked on vault sync.")
vault_a:write_file("attachments/diagram.png", string.rep("PNG_DATA", 100))

local files_a = vault_a:list_files()
indras.assert.eq(#files_a, 4, "A should have 4 files after writing")
print("    A has " .. #files_a .. " files in index")

-- ============================================================================
-- PHASE 3: Verify sync to B and C
-- ============================================================================

h.section(3, "Wait for CRDT sync and verify file counts")

h.assert_eventually(function()
    local fb = vault_b:list_files()
    return #fb >= 4
end, { timeout = 15, interval = 1, msg = "B should have 4 files after sync" })
print("    B synced: " .. #vault_b:list_files() .. " files")

h.assert_eventually(function()
    local fc = vault_c:list_files()
    return #fc >= 4
end, { timeout = 15, interval = 1, msg = "C should have 4 files after sync" })
print("    C synced: " .. #vault_c:list_files() .. " files")

-- ============================================================================
-- PHASE 4: B edits a file (LWW — B's later timestamp wins)
-- ============================================================================

h.section(4, "B edits hello.md (LWW)")

-- Sleep to ensure B's edit is well after A's (outside 60s conflict window)
indras.sleep(2)

vault_b:write_file("notes/hello.md", "# Hello from B\n\nB updated this note.")
print("    B overwrote notes/hello.md")

-- Wait for sync
h.assert_eventually(function()
    local fa = vault_a:list_files()
    for _, f in ipairs(fa) do
        if f.path == "notes/hello.md" then
            -- Check that A's index has B's hash
            local fb = vault_b:list_files()
            for _, bf in ipairs(fb) do
                if bf.path == "notes/hello.md" then
                    return f.hash_hex == bf.hash_hex
                end
            end
        end
    end
    return false
end, { timeout = 15, interval = 1, msg = "A should have B's version of hello.md (LWW)" })
print("    A now has B's version of hello.md")

-- ============================================================================
-- PHASE 5: Concurrent edits → conflict detection
-- ============================================================================

h.section(5, "Concurrent edit on todo.md → conflict")

-- Both A and C write to the same file with minimal delay (within 60s window)
vault_a:write_file("notes/todo.md", "# TODO (A's version)\n\n- [x] Build vault sync")
vault_c:write_file("notes/todo.md", "# TODO (C's version)\n\n- [x] Test it")
print("    A and C both edited notes/todo.md concurrently")

-- Wait for conflicts to appear
h.assert_eventually(function()
    local ca = vault_a:list_conflicts()
    return #ca > 0
end, { timeout = 15, interval = 1, msg = "A should detect conflict on todo.md" })

local conflicts_a = vault_a:list_conflicts()
print("    A sees " .. #conflicts_a .. " conflict(s)")

h.assert_eventually(function()
    local cb = vault_b:list_conflicts()
    return #cb > 0
end, { timeout = 15, interval = 1, msg = "B should also see the conflict" })
print("    B also sees the conflict (synced)")

-- ============================================================================
-- PHASE 6: File deletion syncs
-- ============================================================================

h.section(6, "A deletes a file, verify sync")

vault_a:delete_file("daily/2026-04-02.md")
print("    A deleted daily/2026-04-02.md")

h.assert_eventually(function()
    local fb = vault_b:list_files()
    for _, f in ipairs(fb) do
        if f.path == "daily/2026-04-02.md" then
            return false
        end
    end
    return true
end, { timeout = 15, interval = 1, msg = "B should not have deleted daily note" })
print("    B no longer lists the deleted file")

-- ============================================================================
-- PHASE 7: Final convergence
-- ============================================================================

h.section(7, "Final convergence check")

indras.sleep(3)

local final_a = vault_a:list_files()
local final_b = vault_b:list_files()
local final_c = vault_c:list_files()

print("    A: " .. #final_a .. " files")
print("    B: " .. #final_b .. " files")
print("    C: " .. #final_c .. " files")

indras.assert.eq(#final_a, #final_b, "A and B should have same file count")
indras.assert.eq(#final_b, #final_c, "B and C should have same file count")

print()
print("=== All phases passed! ===")

-- Cleanup
vault_a:stop()
vault_b:stop()
vault_c:stop()
h.stop_all(nets)

return true

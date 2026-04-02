-- live_vault_stress.lua
--
-- Stress test for vault sync: many files, rapid edits, renames (delete+create),
-- mixed concurrent operations, and final convergence verification.
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/live_vault_stress.lua --pretty

package.path = package.path .. ";simulation/scripts/lib/?.lua"
local h = require("live_network_helpers")

print("=== Vault Sync Stress Test ===")
print()

-- ============================================================================
-- PHASE 1: Create 3 networks (A, B, C), connect all, A creates vault, B and C join
-- ============================================================================

h.section(1, "Create networks and shared vault")

local nets = h.create_networks(3)
local a, b, c = nets[1], nets[2], nets[3]
h.connect_all(nets)
print("    3 networks started and connected")

local tmp_a = os.tmpname() .. "_stress_a"
local tmp_b = os.tmpname() .. "_stress_b"
local tmp_c = os.tmpname() .. "_stress_c"

local vault_a, invite = indras.VaultSync.create(a, "StressVault", tmp_a)
print("    A created vault at " .. tmp_a)

local vault_b = indras.VaultSync.join(b, invite, tmp_b)
print("    B joined vault at " .. tmp_b)

local vault_c = indras.VaultSync.join(c, invite, tmp_c)
print("    C joined vault at " .. tmp_c)

-- ============================================================================
-- PHASE 2: Many files — A writes 50 files across nested directories
-- ============================================================================

h.section(2, "Many files: A writes 50 files across 10 folders x 5 files")

for m = 1, 10 do
    for n = 1, 5 do
        local path = string.format("folder-%02d/note-%02d.md", m, n)
        local content = string.format("File %d in folder %d\n\nGenerated for stress test.", n, m)
        vault_a:write_file(path, content)
    end
end

local files_a = vault_a:list_files()
indras.assert.eq(#files_a, 50, "A should have 50 files after writing")
print("    A wrote " .. #files_a .. " files")

h.assert_eventually(function()
    local fb = vault_b:list_files()
    return #fb >= 50
end, { timeout = 30, interval = 1, msg = "B should have 50 files after sync" })
print("    B synced: " .. #vault_b:list_files() .. " files")

h.assert_eventually(function()
    local fc = vault_c:list_files()
    return #fc >= 50
end, { timeout = 30, interval = 1, msg = "C should have 50 files after sync" })
print("    C synced: " .. #vault_c:list_files() .. " files")

-- ============================================================================
-- PHASE 3: Rapid sequential edits — B edits same file 10 times quickly
-- ============================================================================

h.section(3, "Rapid sequential edits: B edits folder-01/note-01.md 10 times")

for i = 1, 10 do
    vault_b:write_file("folder-01/note-01.md", string.format("Edit %d by B", i))
    indras.sleep(0.1)
end
print("    B made 10 rapid edits to folder-01/note-01.md")

-- Determine B's final hash for the file
local function get_hash(vault, path)
    local files = vault:list_files()
    for _, f in ipairs(files) do
        if f.path == path and not f.deleted then
            return f.hash_hex
        end
    end
    return nil
end

local final_hash_b = get_hash(vault_b, "folder-01/note-01.md")
indras.assert.true_(final_hash_b ~= nil, "B must have folder-01/note-01.md in index")
print("    B final hash: " .. final_hash_b:sub(1, 16) .. "...")

h.assert_eventually(function()
    local ha = get_hash(vault_a, "folder-01/note-01.md")
    return ha == final_hash_b
end, { timeout = 15, interval = 1, msg = "A should converge to B's final version" })
print("    A converged to B's final version")

h.assert_eventually(function()
    local hc = get_hash(vault_c, "folder-01/note-01.md")
    return hc == final_hash_b
end, { timeout = 15, interval = 1, msg = "C should converge to B's final version" })
print("    C converged to B's final version")

-- ============================================================================
-- PHASE 4: File rename — delete old path, create new path
-- ============================================================================

h.section(4, "File rename: folder-02/note-01.md -> folder-02/renamed.md")

-- Content was set in Phase 2; reconstruct it
local rename_content = "File 1 in folder 2\n\nGenerated for stress test."

vault_a:delete_file("folder-02/note-01.md")
vault_a:write_file("folder-02/renamed.md", rename_content)
print("    A deleted folder-02/note-01.md and created folder-02/renamed.md")

-- Verify all nodes see the rename
h.assert_eventually(function()
    local fb = vault_b:list_files()
    local has_new = false
    local has_old = false
    for _, f in ipairs(fb) do
        if f.path == "folder-02/renamed.md" and not f.deleted then
            has_new = true
        end
        if f.path == "folder-02/note-01.md" and not f.deleted then
            has_old = true
        end
    end
    return has_new and not has_old
end, { timeout = 15, interval = 1, msg = "B should have renamed.md and NOT note-01.md" })
print("    B: rename synced correctly")

h.assert_eventually(function()
    local fc = vault_c:list_files()
    local has_new = false
    local has_old = false
    for _, f in ipairs(fc) do
        if f.path == "folder-02/renamed.md" and not f.deleted then
            has_new = true
        end
        if f.path == "folder-02/note-01.md" and not f.deleted then
            has_old = true
        end
    end
    return has_new and not has_old
end, { timeout = 15, interval = 1, msg = "C should have renamed.md and NOT note-01.md" })
print("    C: rename synced correctly")

-- ============================================================================
-- PHASE 5: Mixed operations from all 3 nodes simultaneously
-- ============================================================================

h.section(5, "Mixed operations: A writes 5 files, B writes 5 files, C deletes 3 files")

-- A writes 5 new files
for i = 1, 5 do
    local path = string.format("mixed/a-%02d.md", i)
    vault_a:write_file(path, string.format("Mixed file A-%d from node A", i))
end

-- B writes 5 new files
for i = 1, 5 do
    local path = string.format("mixed/b-%02d.md", i)
    vault_b:write_file(path, string.format("Mixed file B-%d from node B", i))
end

-- C deletes 3 of A's original files from folder-03
vault_c:delete_file("folder-03/note-01.md")
vault_c:delete_file("folder-03/note-02.md")
vault_c:delete_file("folder-03/note-03.md")

print("    A wrote 5 mixed/a-*.md files")
print("    B wrote 5 mixed/b-*.md files")
print("    C deleted folder-03/note-01.md through note-03.md")

-- Wait for all nodes to sync the mixed operations
-- Expected: 50 original - 1 (rename delete) - 3 (C deletes) + 1 (renamed.md) + 5 (A mixed) + 5 (B mixed) = 57
local expected_count = 57

h.assert_eventually(function()
    local fa = vault_a:list_files()
    local fb = vault_b:list_files()
    local fc = vault_c:list_files()
    return #fa >= expected_count and #fb >= expected_count and #fc >= expected_count
end, { timeout = 30, interval = 1, msg = "All nodes should have converged file count after mixed ops" })

print("    All nodes reached expected file count (" .. expected_count .. "+)")

-- ============================================================================
-- PHASE 6: Final convergence — verify all nodes have identical state
-- ============================================================================

h.section(6, "Final convergence: verify all 3 nodes have identical state")

indras.sleep(3)

-- Build sorted hash maps from each node
local function build_hash_map(vault)
    local files = vault:list_files()
    local map = {}
    for _, f in ipairs(files) do
        if not f.deleted then
            map[f.path] = f.hash_hex
        end
    end
    return map
end

local map_a = build_hash_map(vault_a)
local map_b = build_hash_map(vault_b)
local map_c = build_hash_map(vault_c)

-- Count files in each map
local function map_size(m)
    local n = 0
    for _ in pairs(m) do n = n + 1 end
    return n
end

local count_a = map_size(map_a)
local count_b = map_size(map_b)
local count_c = map_size(map_c)

print(string.format("    A: %d files, B: %d files, C: %d files", count_a, count_b, count_c))

indras.assert.eq(count_a, count_b, "A and B must have same file count")
indras.assert.eq(count_b, count_c, "B and C must have same file count")

-- For each path on A, verify B and C have same hash
local mismatches = 0
for path, hash in pairs(map_a) do
    if map_b[path] ~= hash then
        print("    MISMATCH on " .. path .. ": A=" .. hash:sub(1,8) .. " B=" .. (map_b[path] or "nil"):sub(1,8))
        mismatches = mismatches + 1
    end
    if map_c[path] ~= hash then
        print("    MISMATCH on " .. path .. ": A=" .. hash:sub(1,8) .. " C=" .. (map_c[path] or "nil"):sub(1,8))
        mismatches = mismatches + 1
    end
end

indras.assert.eq(mismatches, 0, "No hash mismatches across nodes")

-- Conflict summary
local conflicts_a = vault_a:list_conflicts()
local total_conflicts = #conflicts_a
print(string.format("    Total files: %d | Total conflicts detected: %d | Mismatches: %d",
    count_a, total_conflicts, mismatches))

print()
print("=== All phases passed! Vault sync stress test complete. ===")

-- Cleanup
vault_a:stop()
vault_b:stop()
vault_c:stop()
h.stop_all(nets)

return true

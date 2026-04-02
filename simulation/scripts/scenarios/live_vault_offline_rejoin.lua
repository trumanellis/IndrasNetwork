-- live_vault_offline_rejoin.lua
--
-- E2E test: vault offline/rejoin and node restart with persistent storage.
--
-- Tests two core paths:
--   1. Store-and-forward: node goes offline, misses edits, rejoins and syncs
--   2. Restart persistence: stop a node, restart from same data dir, verify state survives
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/live_vault_offline_rejoin.lua

package.path = package.path .. ";simulation/scripts/lib/?.lua"
local h = require("live_network_helpers")

print("=== Vault Offline Rejoin + Restart Persistence Test ===")
print()

-- ============================================================================
-- PHASE 1: Create 3 networks, connect all, A creates vault, B and C join
-- ============================================================================

h.section(1, "Create networks and shared vault")

local nets = h.create_networks(3)
local net_a, net_b, net_c = nets[1], nets[2], nets[3]
h.connect_all(nets)
print("    A, B, C started and connected")

local tmp_a = os.tmpname() .. "_vault_a"
local tmp_b = os.tmpname() .. "_vault_b"
local tmp_c = os.tmpname() .. "_vault_c"

local vault_a, invite = indras.VaultSync.create(net_a, "SharedVault", tmp_a)
print("    A created vault at " .. tmp_a)

local vault_b = indras.VaultSync.join(net_b, invite, tmp_b)
print("    B joined vault at " .. tmp_b)

local vault_c = indras.VaultSync.join(net_c, invite, tmp_c)
print("    C joined vault at " .. tmp_c)

-- ============================================================================
-- PHASE 2: A writes 3 files, verify all nodes have them
-- ============================================================================

h.section(2, "A writes 3 files, verify A/B/C all sync")

vault_a:write_file("docs/readme.md",   "# Readme\n\nInitial content from A.")
vault_a:write_file("docs/notes.md",    "# Notes\n\nFirst note from A.")
vault_a:write_file("data/config.toml", "[network]\nname = \"shared\"\n")
print("    A wrote 3 files")

h.assert_eventually(function()
    return #vault_b:list_files() >= 3
end, { timeout = 15, interval = 1, msg = "B should have 3 files after initial sync" })
print("    B synced: " .. #vault_b:list_files() .. " files")

h.assert_eventually(function()
    return #vault_c:list_files() >= 3
end, { timeout = 15, interval = 1, msg = "C should have 3 files after initial sync" })
print("    C synced: " .. #vault_c:list_files() .. " files")

-- ============================================================================
-- PHASE 3: Isolate C from the network
-- ============================================================================

h.section(3, "Isolate C (simulate offline)")

h.isolate(net_c, nets)
print("    C is now offline")

-- ============================================================================
-- PHASE 4: While C is offline, A writes 2 new files and B edits 1 existing file
-- ============================================================================

h.section(4, "A writes 2 new files; B edits notes.md while C is offline")

vault_a:write_file("docs/offline-test.md",  "# Offline Test\n\nWritten while C was away.")
vault_a:write_file("data/metrics.csv",      "ts,value\n1,42\n2,43\n")
print("    A wrote 2 new files (C missed these)")

indras.sleep(2)  -- ensure B's timestamp is after A's for LWW

vault_b:write_file("docs/notes.md", "# Notes (B edited)\n\nB updated this while C was offline.")
print("    B edited docs/notes.md (C missed this too)")

-- Verify A sees B's edit
h.assert_eventually(function()
    local fa = vault_a:list_files()
    local fb = vault_b:list_files()
    local ha, hb
    for _, f in ipairs(fa) do
        if f.path == "docs/notes.md" then ha = f.hash_hex end
    end
    for _, f in ipairs(fb) do
        if f.path == "docs/notes.md" then hb = f.hash_hex end
    end
    return ha ~= nil and ha == hb
end, { timeout = 15, interval = 1, msg = "A should have B's version of notes.md" })
print("    A/B converged on notes.md (LWW: B wins)")

-- ============================================================================
-- PHASE 5: Rejoin C, wait for sync, verify C has all 5 files
-- ============================================================================

h.section(5, "Rejoin C — verify store-and-forward delivers all 5 files")

h.rejoin(net_c, nets)
print("    C is back online")

h.assert_eventually(function()
    return #vault_c:list_files() >= 5
end, { timeout = 15, interval = 1, msg = "C should have all 5 files after rejoin" })
print("    C synced: " .. #vault_c:list_files() .. " files")

-- ============================================================================
-- PHASE 6: Verify C has B's edit of notes.md (hash matches B's version)
-- ============================================================================

h.section(6, "Verify C has B's edit of docs/notes.md")

h.assert_eventually(function()
    local fb = vault_b:list_files()
    local fc = vault_c:list_files()
    local hb, hc
    for _, f in ipairs(fb) do
        if f.path == "docs/notes.md" then hb = f.hash_hex end
    end
    for _, f in ipairs(fc) do
        if f.path == "docs/notes.md" then hc = f.hash_hex end
    end
    return hb ~= nil and hb == hc
end, { timeout = 15, interval = 1, msg = "C should have B's version of notes.md after rejoin" })
print("    C has B's version of notes.md — hash matches")

-- ============================================================================
-- PHASE 7: Stop B's network, restart from same data dir
-- ============================================================================

h.section(7, "Stop B's network and restart from same data dir")

vault_b:stop()
net_b:stop()
print("    B stopped")

-- Restart B with the same data directory
local net_b2 = indras.Network.new()
net_b2:start()
local vault_b2 = indras.VaultSync.join(net_b2, invite, tmp_b)
vault_b2:scan()  -- index files already on disk from the previous session
print("    B restarted (net_b2) with same vault path: " .. tmp_b)

-- Update nets table so rejoin works against the live set
nets[2] = net_b2

-- ============================================================================
-- PHASE 8: Reconnect restarted B, verify it still has all files
-- ============================================================================

h.section(8, "Reconnect restarted B — verify vault state survived restart")

net_b2:connect_to(nets[1])
net_b2:connect_to(nets[3])
print("    B reconnected to A and C")

h.assert_eventually(function()
    return #vault_b2:list_files() >= 5
end, { timeout = 15, interval = 1, msg = "Restarted B should have all 5 files from persistent storage" })
print("    Restarted B has " .. #vault_b2:list_files() .. " files — persistence confirmed")

-- Spot-check: B's notes.md hash matches A's
h.assert_eventually(function()
    local fa = vault_a:list_files()
    local fb2 = vault_b2:list_files()
    local ha, hb2
    for _, f in ipairs(fa) do
        if f.path == "docs/notes.md" then ha = f.hash_hex end
    end
    for _, f in ipairs(fb2) do
        if f.path == "docs/notes.md" then hb2 = f.hash_hex end
    end
    return ha ~= nil and ha == hb2
end, { timeout = 15, interval = 1, msg = "Restarted B should have same notes.md hash as A" })
print("    Restarted B notes.md hash matches A — data integrity confirmed")

-- ============================================================================
-- PHASE 9: Final convergence — all 3 nodes have identical file sets
-- ============================================================================

h.section(9, "Final convergence — A, B (restarted), C have identical file sets")

indras.sleep(3)

local function sorted_hash_map(vault)
    local files = vault:list_files()
    local map = {}
    for _, f in ipairs(files) do
        if not f.deleted then
            map[f.path] = f.hash_hex
        end
    end
    return map
end

local map_a  = sorted_hash_map(vault_a)
local map_b2 = sorted_hash_map(vault_b2)
local map_c  = sorted_hash_map(vault_c)

-- Collect all paths from A
local paths = {}
for path in pairs(map_a) do
    paths[#paths + 1] = path
end
table.sort(paths)

print("    Comparing " .. #paths .. " files across A, B (restarted), C:")

local all_match = true
for _, path in ipairs(paths) do
    local ha  = map_a[path]
    local hb2 = map_b2[path]
    local hc  = map_c[path]
    local match = (ha == hb2) and (ha == hc)
    if not match then
        all_match = false
        print("    MISMATCH: " .. path)
        print("      A:  " .. tostring(ha))
        print("      B2: " .. tostring(hb2))
        print("      C:  " .. tostring(hc))
    else
        print("    OK: " .. path)
    end
end

indras.assert.true_(all_match, "All nodes must have identical file hashes at final convergence")

-- Verify B2 and C have the same file count as A
local count_a  = 0
local count_b2 = 0
local count_c  = 0
for _ in pairs(map_a)  do count_a  = count_a  + 1 end
for _ in pairs(map_b2) do count_b2 = count_b2 + 1 end
for _ in pairs(map_c)  do count_c  = count_c  + 1 end

indras.assert.eq(count_a, count_b2, "A and restarted B must have same file count")
indras.assert.eq(count_a, count_c,  "A and C must have same file count")

print("    A: " .. count_a .. " files, B (restarted): " .. count_b2 .. " files, C: " .. count_c .. " files — all equal")

print()
print("=== All phases passed! ===")

-- Cleanup
vault_a:stop()
vault_b2:stop()
vault_c:stop()
net_b2:stop()
-- Remove restarted B from nets so stop_all doesn't double-stop it
nets[2] = net_b2  -- already stopped above; stop_all will call stop() again which should be a no-op
h.stop_all({ nets[1], nets[3] })

return true

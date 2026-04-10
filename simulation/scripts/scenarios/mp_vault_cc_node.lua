-- mp_vault_cc_node.lua
--
-- Per-node script for the concurrent conflicts & offline rejoin test.
-- Each node participates in a single shared vault. Tests concurrent writes,
-- conflict creation/resolution, and offline/rejoin catch-up sync.

package.path = package.path .. ";simulation/scripts/lib/?.lua"
local mp = require("mp_helpers")

local role      = os.getenv("ROLE")
local user_name = os.getenv("USER_NAME")
local device    = os.getenv("DEVICE")
local peers_str = os.getenv("PEERS")
local coord_dir = os.getenv("COORD_DIR")
local data_dir  = os.getenv("DATA_DIR")
local vault_dir = os.getenv("VAULT_DIR")

assert(role and user_name and device and peers_str and coord_dir
    and data_dir and vault_dir, "Missing env vars")

local peers = {}
for p in peers_str:gmatch("[^,]+") do peers[#peers + 1] = p end

local is_primary = (device == "laptop")
local function log(msg) print("[" .. role .. "] " .. msg) end

-- ── Helpers ──

--- Poll vault:list_conflicts() until a conflict for `path` appears.
--- Returns the conflict record or errors on timeout.
local function wait_for_conflict(vault, path, timeout)
    local deadline = os.time() + (timeout or 30)
    while os.time() < deadline do
        local conflicts = vault:list_conflicts()
        for _, c in ipairs(conflicts) do
            if c.path == path and not c.resolved then
                return c
            end
        end
        indras.sleep(1)
    end
    error(role .. ": timeout waiting for conflict on " .. path)
end

--- Poll vault:list_conflicts() until no unresolved conflict for `path`.
local function wait_for_no_unresolved(vault, path, timeout)
    local deadline = os.time() + (timeout or 30)
    while os.time() < deadline do
        local conflicts = vault:list_conflicts()
        local found = false
        for _, c in ipairs(conflicts) do
            if c.path == path and not c.resolved then
                found = true
                break
            end
        end
        if not found then return true end
        indras.sleep(1)
    end
    error(role .. ": timeout waiting for conflict resolution on " .. path)
end

-- ── Phase 1: Setup ──

if not is_primary then
    local backup = mp.wait_for_signal(coord_dir, user_name .. "_pq_identity", 30)
    indras.Network.import_pq_identity(data_dir, backup)
end

local net = indras.Network.new(data_dir)
net:start()
log("Started, id=" .. net:id():sub(1, 12) .. "...")

if is_primary then
    mp.write_signal(coord_dir, user_name .. "_pq_identity", net:export_pq_identity())
end

mp.write_signal(coord_dir, role .. "_addr", net:endpoint_addr_string())

for _, peer in ipairs(peers) do
    local addr = mp.wait_for_signal(coord_dir, peer .. "_addr", 45)
    net:connect_to_addr(addr)
end
log("Connected to all peers")

-- Create or join vault
local vault, invite

if role == "A_laptop" then
    vault, invite = indras.VaultSync.create(net, "ConcurrentVault", vault_dir)
    mp.write_signal(coord_dir, "invite", invite)
    log("Created vault")
else
    invite = mp.wait_for_signal(coord_dir, "invite", 30)
    vault = indras.VaultSync.join(net, invite, vault_dir)
    log("Joined vault")
end

-- Add all peers' relay addrs for blob push
for _, peer in ipairs(peers) do
    local addr = mp.wait_for_signal(coord_dir, peer .. "_addr", 30)
    vault:add_peer_relay_addr(addr)
end

-- Wait for CRDT membership to converge (4 devices total)
local actual = vault:await_members(4, 30)
if actual < 4 then
    log("WARNING: only " .. actual .. "/4 members converged")
else
    log("All " .. actual .. " members converged")
end

mp.write_signal(coord_dir, role .. "_ready", "ok")
for _, peer in ipairs(peers) do mp.wait_for_signal(coord_dir, peer .. "_ready", 60) end
log("All nodes ready")

-- ── Phase 2: Concurrent writes to different files (no conflict) ──

mp.wait_for_signal(coord_dir, "phase2", 60)

if role == "A_laptop" then
    vault:write_file("docs/from_A.md", "Written by A_laptop")
    log("Wrote docs/from_A.md")
    mp.write_signal(coord_dir, "A_laptop_wrote_phase2", "ok")
end

if role == "B_phone" then
    vault:write_file("docs/from_B.md", "Written by B_phone")
    log("Wrote docs/from_B.md")
    mp.write_signal(coord_dir, "B_phone_wrote_phase2", "ok")
end

-- All nodes wait for both writes, then verify
mp.wait_for_signal(coord_dir, "A_laptop_wrote_phase2", 30)
mp.wait_for_signal(coord_dir, "B_phone_wrote_phase2", 30)

mp.wait_for_file_content(vault_dir .. "/docs/from_A.md", "Written by A_laptop", 30)
mp.wait_for_file_content(vault_dir .. "/docs/from_B.md", "Written by B_phone", 30)
log("Both files synced")

-- Verify no conflicts
indras.sleep(2)
local conflicts = vault:list_conflicts()
local unresolved = 0
for _, c in ipairs(conflicts) do
    if not c.resolved then unresolved = unresolved + 1 end
end
assert(unresolved == 0, role .. ": unexpected conflict in phase 2, got " .. unresolved)
log("Phase 2: 0 conflicts (correct)")

mp.write_signal(coord_dir, role .. "_done_phase2", "ok")

-- ── Phase 3: Same-file concurrent edits (conflict expected) ──

mp.wait_for_signal(coord_dir, "phase3", 60)

if role == "A_laptop" then
    vault:write_file("docs/contested.md", "A's version of contested")
    log("Wrote docs/contested.md (A's version)")
    mp.write_signal(coord_dir, "A_laptop_wrote_phase3", "ok")
end

if role == "B_laptop" then
    vault:write_file("docs/contested.md", "B's version of contested")
    log("Wrote docs/contested.md (B's version)")
    mp.write_signal(coord_dir, "B_laptop_wrote_phase3", "ok")
end

-- All nodes wait for both writes to complete
mp.wait_for_signal(coord_dir, "A_laptop_wrote_phase3", 30)
mp.wait_for_signal(coord_dir, "B_laptop_wrote_phase3", 30)

-- All nodes poll for the conflict to appear
local conflict = wait_for_conflict(vault, "docs/contested.md", 30)
assert(conflict.winner_hash and conflict.winner_hash ~= "",
    role .. ": conflict must have non-empty winner_hash")
assert(conflict.loser_hash and conflict.loser_hash ~= "",
    role .. ": conflict must have non-empty loser_hash")
assert(conflict.conflict_file and conflict.conflict_file ~= "",
    role .. ": conflict must have a conflict_file")
log("Phase 3: conflict detected — winner=" .. conflict.winner_hash:sub(1, 8) ..
    "... loser=" .. conflict.loser_hash:sub(1, 8) ..
    "... file=" .. conflict.conflict_file)

-- Store loser_hash for phase 4 resolution (A_laptop will resolve)
mp.write_signal(coord_dir, role .. "_loser_hash", conflict.loser_hash)

mp.write_signal(coord_dir, role .. "_done_phase3", "ok")

-- ── Phase 4: Conflict resolution + propagation ──

mp.wait_for_signal(coord_dir, "phase4", 60)

if role == "A_laptop" then
    -- Read our own stored loser_hash
    local loser_hex = mp.wait_for_signal(coord_dir, "A_laptop_loser_hash", 5)
    assert(#loser_hex == 64, role .. ": loser_hash must be 64 hex chars, got " .. #loser_hex)

    vault:resolve_conflict("docs/contested.md", loser_hex)
    log("Resolved conflict on docs/contested.md")

    -- Verify locally
    indras.sleep(1)
    local ca = vault:list_conflicts()
    local still_has = false
    for _, c in ipairs(ca) do
        if c.path == "docs/contested.md" and not c.resolved then
            still_has = true
        end
    end
    assert(not still_has, role .. ": conflict should be resolved locally")
    log("Conflict resolved locally")

    mp.write_signal(coord_dir, "A_laptop_resolved", "ok")
end

-- All nodes wait for resolution, then verify propagation
mp.wait_for_signal(coord_dir, "A_laptop_resolved", 30)
if role ~= "A_laptop" then
    wait_for_no_unresolved(vault, "docs/contested.md", 60)
    log("Phase 4: conflict resolution propagated")
end

mp.write_signal(coord_dir, role .. "_done_phase4", "ok")

-- ── Phase 5a: B_phone goes offline, others write while it's away ──
--
-- Strategy: B_phone stops vault+net first and signals. Then A_laptop writes
-- a file. Other online nodes verify they receive it. B_phone writes locally
-- via raw io (offline). Then Phase 5b handles rejoin.
--
-- Key: A_laptop writes AFTER B_phone is confirmed offline and after a brief
-- pause to let QUIC connections to B_phone time out / be pruned.

mp.wait_for_signal(coord_dir, "phase5_offline", 60)

if role == "B_phone" then
    vault:stop()
    net:stop()
    log("Went offline (vault + net stopped)")
    mp.write_signal(coord_dir, "B_phone_offline", "ok")

    -- Write a file to the vault dir via raw io while offline
    os.execute("mkdir -p " .. vault_dir .. "/docs")
    local f = io.open(vault_dir .. "/docs/offline_edit.md", "w")
    assert(f, role .. ": failed to write offline file")
    f:write("Written while B_phone was offline")
    f:close()
    log("Wrote docs/offline_edit.md while offline (raw io)")
end

-- Online nodes: wait for B_phone to go offline, then pause for QUIC cleanup
if role ~= "B_phone" then
    mp.wait_for_signal(coord_dir, "B_phone_offline", 30)
    indras.sleep(3)
end

-- A_laptop writes while B_phone is offline
if role == "A_laptop" then
    vault:write_file("docs/while_away.md", "Written by A while B_phone was offline")
    log("Wrote docs/while_away.md (B_phone will miss this)")
    mp.write_signal(coord_dir, "A_laptop_wrote_while_offline", "ok")
end

-- A_phone and B_laptop verify they got the file from A_laptop
if role == "A_phone" or role == "B_laptop" then
    mp.wait_for_signal(coord_dir, "A_laptop_wrote_while_offline", 30)
    mp.wait_for_file_content(vault_dir .. "/docs/while_away.md",
        "Written by A while B_phone was offline", 60)
    log("Got docs/while_away.md while B_phone offline")
end

mp.write_signal(coord_dir, role .. "_done_phase5_offline", "ok")

-- ── Phase 5b: B_phone rejoins ──

mp.wait_for_signal(coord_dir, "phase5_rejoin", 60)

if role == "B_phone" then
    -- Use a fresh data dir for the restarted network to avoid lock conflicts.
    -- Import the PQ identity so it's still the same user.
    local data_dir_v2 = data_dir .. "_v2"
    os.execute("mkdir -p " .. data_dir_v2)
    local pq_backup = mp.wait_for_signal(coord_dir, user_name .. "_pq_identity", 5)
    indras.Network.import_pq_identity(data_dir_v2, pq_backup)

    net = indras.Network.new(data_dir_v2)
    net:start()
    log("Restarted network (fresh data dir), id=" .. net:id():sub(1, 12) .. "...")

    -- Publish new address
    mp.write_signal(coord_dir, "B_phone_addr_v2", net:endpoint_addr_string())

    -- Reconnect to all peers
    for _, peer in ipairs(peers) do
        local addr = mp.wait_for_signal(coord_dir, peer .. "_addr", 30)
        net:connect_to_addr(addr)
    end
    log("Reconnected to all peers")

    -- Rejoin vault at same directory (preserves existing files on disk)
    vault = indras.VaultSync.join(net, invite, vault_dir)
    vault:scan()
    log("Rejoined vault + scanned local files")

    -- Re-add peer relay addrs
    for _, peer in ipairs(peers) do
        local addr = mp.wait_for_signal(coord_dir, peer .. "_addr", 30)
        vault:add_peer_relay_addr(addr)
    end

    mp.write_signal(coord_dir, "B_phone_rejoined", "ok")

    -- Verify catch-up: should receive docs/while_away.md
    mp.wait_for_file_content(vault_dir .. "/docs/while_away.md",
        "Written by A while B_phone was offline", 60)
    log("Caught up: got docs/while_away.md")
end

-- Other nodes: connect to B_phone's new address and verify offline edit
if role ~= "B_phone" then
    local new_addr = mp.wait_for_signal(coord_dir, "B_phone_addr_v2", 60)
    net:connect_to_addr(new_addr)
    vault:add_peer_relay_addr(new_addr)
    log("Connected to B_phone's new address")

    mp.wait_for_signal(coord_dir, "B_phone_rejoined", 60)

    -- Verify we receive B_phone's offline edit (picked up by scan on rejoin)
    mp.wait_for_file_content(vault_dir .. "/docs/offline_edit.md",
        "Written while B_phone was offline", 60)
    log("Got B_phone's offline edit: docs/offline_edit.md")
end

mp.write_signal(coord_dir, role .. "_done_phase5_rejoin", "ok")

-- ── Phase 6: Final convergence verification ──

mp.wait_for_signal(coord_dir, "phase6", 60)

indras.sleep(3)

local expected_files = {
    "docs/from_A.md",
    "docs/from_B.md",
    "docs/contested.md",
    "docs/while_away.md",
    "docs/offline_edit.md",
}

local file_list = vault:list_files()
local file_set = {}
for _, f in ipairs(file_list) do
    if not f.deleted then
        file_set[f.path] = true
    end
end

local missing = {}
for _, path in ipairs(expected_files) do
    if not file_set[path] then
        missing[#missing + 1] = path
    end
end

if #missing > 0 then
    log("MISSING files: " .. table.concat(missing, ", "))
    error(role .. ": missing " .. #missing .. " file(s) at final verification")
end
log("All " .. #expected_files .. " expected files present")

-- Verify no unresolved conflicts
local final_conflicts = vault:list_conflicts()
local final_unresolved = 0
for _, c in ipairs(final_conflicts) do
    if not c.resolved then final_unresolved = final_unresolved + 1 end
end
log("Unresolved conflicts: " .. final_unresolved)
assert(final_unresolved == 0, role .. ": expected 0 unresolved conflicts, got " .. final_unresolved)

mp.write_signal(coord_dir, role .. "_done_phase6", "ok")
mp.write_signal(coord_dir, role .. "_done", "ok")

vault:stop()
net:stop()
log("SUCCESS")

-- mp_vault_combo_node.lua
--
-- Per-node script for combinatorial vault sharing test.
-- Each node participates in multiple vaults based on user membership.

package.path = package.path .. ";simulation/scripts/lib/?.lua"
local mp = require("mp_helpers")

local role      = os.getenv("ROLE")
local user_name = os.getenv("USER_NAME")
local device    = os.getenv("DEVICE")
local peers_str = os.getenv("PEERS")
local vault_defs_str = os.getenv("VAULT_DEFS")
local coord_dir = os.getenv("COORD_DIR")
local data_dir  = os.getenv("DATA_DIR")
local base_dir  = os.getenv("BASE_DIR")

assert(role and user_name and device and peers_str and vault_defs_str
    and coord_dir and data_dir and base_dir, "Missing env vars")

local peers = {}
for p in peers_str:gmatch("[^,]+") do peers[#peers + 1] = p end

local vault_defs = {}
for def in vault_defs_str:gmatch("[^;]+") do
    local name, members_str = def:match("([^:]+):(.+)")
    local members = {}
    for m in members_str:gmatch("[^,]+") do members[#members + 1] = m end
    vault_defs[#vault_defs + 1] = { name = name, members = members }
end

local function is_member(v, u)
    for _, m in ipairs(v.members) do if m == u then return true end end
    return false
end

local my_vaults = {}
for _, v in ipairs(vault_defs) do
    if is_member(v, user_name) then my_vaults[#my_vaults + 1] = v end
end

local is_primary = (device == "laptop")
local function log(msg) print("[" .. role .. "] " .. msg) end

-- ── Setup ──

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

-- ── Create/join vaults ──

local vaults = {}
for _, v in ipairs(my_vaults) do
    local vault_dir = base_dir .. "/" .. role .. "_vault_" .. v.name
    local creator = v.members[1] .. "_laptop"

    if role == creator then
        local vault_obj, invite = indras.VaultSync.create(net, v.name, vault_dir)
        mp.write_signal(coord_dir, "invite_" .. v.name, invite)
        vaults[v.name] = vault_obj
    else
        local invite = mp.wait_for_signal(coord_dir, "invite_" .. v.name, 30)
        vaults[v.name] = indras.VaultSync.join(net, invite, vault_dir)
    end

    -- Add ALL member devices' relays for blob push
    for _, peer in ipairs(peers) do
        local pu = peer:match("^(%w+)_")
        if is_member(v, pu) then
            local addr = mp.wait_for_signal(coord_dir, peer .. "_addr", 30)
            vaults[v.name]:add_peer_relay_addr(addr)
        end
    end
end
log(#my_vaults .. " vaults ready")

-- Wait for CRDT membership to converge in each vault before proceeding.
-- Each user has 2 devices (laptop + phone), so expected = #members * 2.
for _, v in ipairs(my_vaults) do
    local expected = #v.members * 2
    local actual = vaults[v.name]:await_members(expected, 30)
    if actual < expected then
        log("WARNING: " .. v.name .. " has " .. actual .. "/" .. expected .. " members after 30s")
    else
        log(v.name .. ": all " .. actual .. " members converged")
    end
end

mp.write_signal(coord_dir, role .. "_ready", "ok")
for _, peer in ipairs(peers) do mp.wait_for_signal(coord_dir, peer .. "_ready", 60) end
log("All nodes ready")

-- ── Phase 2: Write + verify one vault at a time ──

for _, v in ipairs(vault_defs) do
    local phase = "phase2_vault_" .. v.name
    mp.wait_for_signal(coord_dir, phase, 60)

    local writer = v.members[1] .. "_laptop"
    local content = "Hello from " .. v.name

    if role == writer then
        vaults[v.name]:write_file("hello.md", content)
        log("Wrote to " .. v.name)
        indras.sleep(0.5)
        mp.write_signal(coord_dir, writer .. "_wrote_" .. v.name, "ok")
    end

    if is_member(v, user_name) and role ~= writer then
        mp.wait_for_signal(coord_dir, writer .. "_wrote_" .. v.name, 30)
        local vd = base_dir .. "/" .. role .. "_vault_" .. v.name
        mp.wait_for_file_content(vd .. "/hello.md", content, 60)
        log("Got file from " .. v.name)
    end

    mp.write_signal(coord_dir, role .. "_done_" .. phase, "ok")
end
mp.write_signal(coord_dir, role .. "_done_phase2", "ok")

-- ── Phase 3: Phone edits private vault ──

mp.wait_for_signal(coord_dir, "phase3", 60)
local priv = user_name .. "_private"

if device == "phone" and vaults[priv] then
    vaults[priv]:write_file("hello.md", "Edited by " .. user_name .. "'s phone")
    log("Edited " .. priv)
    mp.write_signal(coord_dir, role .. "_edited_priv", "ok")
end

if device ~= "phone" and vaults[priv] then
    mp.wait_for_signal(coord_dir, user_name .. "_phone_edited_priv", 30)
    local vd = base_dir .. "/" .. role .. "_vault_" .. priv
    mp.wait_for_file_content(vd .. "/hello.md", "Edited by " .. user_name .. "'s phone", 45)
    log("Got private edit from phone")
end

mp.write_signal(coord_dir, role .. "_done_phase3", "ok")

-- ── Phase 4: Final verification ──

mp.wait_for_signal(coord_dir, "phase4", 60)

for _, v in ipairs(my_vaults) do
    local vd = base_dir .. "/" .. role .. "_vault_" .. v.name
    assert(mp.read_file(vd .. "/hello.md"), role .. ": missing file in " .. v.name)
end
log("All vault files present")

for _, v in ipairs(vault_defs) do
    if not is_member(v, user_name) then
        local vd = base_dir .. "/" .. role .. "_vault_" .. v.name
        assert(not io.open(vd .. "/hello.md", "r"), role .. ": leak in " .. v.name)
    end
end
log("Isolation verified")

local tc = 0
for _, v in ipairs(my_vaults) do
    for _, c in ipairs(vaults[v.name]:list_conflicts()) do
        if not c.resolved then tc = tc + 1 end
    end
end
log("Unresolved conflicts: " .. tc)

mp.write_signal(coord_dir, role .. "_done_phase4", "ok")
mp.write_signal(coord_dir, role .. "_done", "ok")
for _, v in ipairs(my_vaults) do vaults[v.name]:stop() end
net:stop()
log("SUCCESS")

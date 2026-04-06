-- mp_vault_3dev_node.lua
--
-- Per-node script for single-user 3-device vault sync.
-- All devices share the same PQ identity.

package.path = package.path .. ";simulation/scripts/lib/?.lua"
local mp = require("mp_helpers")

local role      = os.getenv("ROLE")
local peers_str = os.getenv("PEERS")
local coord_dir = os.getenv("COORD_DIR")
local data_dir  = os.getenv("DATA_DIR")
local vault_dir = os.getenv("VAULT_DIR")

local peers = {}
for p in peers_str:gmatch("[^,]+") do peers[#peers + 1] = p end

local function log(msg) print("[" .. role .. "] " .. msg) end

-- Identity
if role ~= "laptop" then
    local backup = mp.wait_for_signal(coord_dir, "pq_identity", 30)
    indras.Network.import_pq_identity(data_dir, backup)
end

local net = indras.Network.new(data_dir)
net:start()
log("Started, id=" .. net:id():sub(1, 12) .. "...")

if role == "laptop" then
    mp.write_signal(coord_dir, "pq_identity", net:export_pq_identity())
end

mp.write_signal(coord_dir, role .. "_addr", net:endpoint_addr_string())

for _, peer in ipairs(peers) do
    local addr = mp.wait_for_signal(coord_dir, peer .. "_addr", 30)
    net:connect_to_addr(addr)
end
log("Connected to all peers")

-- Vault
local vault
if role == "laptop" then
    local invite
    vault, invite = indras.VaultSync.create(net, "MyVault", vault_dir)
    mp.write_signal(coord_dir, "invite", invite)
    log("Created vault")
else
    local invite = mp.wait_for_signal(coord_dir, "invite", 30)
    vault = indras.VaultSync.join(net, invite, vault_dir)
    log("Joined vault")
end

for _, peer in ipairs(peers) do
    local addr = mp.wait_for_signal(coord_dir, peer .. "_addr", 30)
    vault:add_peer_relay_addr(addr)
end

mp.write_signal(coord_dir, role .. "_ready", "ok")
for _, peer in ipairs(peers) do mp.wait_for_signal(coord_dir, peer .. "_ready", 45) end
log("All devices ready")

-- Phase 2: Laptop writes
mp.wait_for_signal(coord_dir, "phase2", 30)
if role == "laptop" then
    vault:write_file("notes/from_laptop.md", "Written on the laptop")
    log("Wrote notes/from_laptop.md")
    mp.write_signal(coord_dir, "laptop_wrote_p2", "ok")
else
    mp.wait_for_signal(coord_dir, "laptop_wrote_p2", 30)
    mp.wait_for_file_content(vault_dir .. "/notes/from_laptop.md", "Written on the laptop", 45)
    log("Got notes/from_laptop.md")
end
mp.write_signal(coord_dir, role .. "_done_phase2", "ok")

-- Phase 3: Phone writes
mp.wait_for_signal(coord_dir, "phase3", 30)
if role == "phone" then
    vault:write_file("notes/from_phone.md", "Quick note from phone")
    log("Wrote notes/from_phone.md")
    mp.write_signal(coord_dir, "phone_wrote_p3", "ok")
else
    mp.wait_for_signal(coord_dir, "phone_wrote_p3", 30)
    mp.wait_for_file_content(vault_dir .. "/notes/from_phone.md", "Quick note from phone", 45)
    log("Got notes/from_phone.md")
end
mp.write_signal(coord_dir, role .. "_done_phase3", "ok")

-- Phase 4: Relay writes
mp.wait_for_signal(coord_dir, "phase4", 30)
if role == "relay" then
    vault:write_file("notes/from_relay.md", "Auto-synced by relay server")
    log("Wrote notes/from_relay.md")
    mp.write_signal(coord_dir, "relay_wrote_p4", "ok")
else
    mp.wait_for_signal(coord_dir, "relay_wrote_p4", 30)
    mp.wait_for_file_content(vault_dir .. "/notes/from_relay.md", "Auto-synced by relay server", 45)
    log("Got notes/from_relay.md")
end
mp.write_signal(coord_dir, role .. "_done_phase4", "ok")

-- Phase 5: Laptop edits Phone's file
mp.wait_for_signal(coord_dir, "phase5", 30)
if role == "laptop" then
    vault:write_file("notes/from_phone.md", "Edited on laptop (originally from phone)")
    log("Edited notes/from_phone.md")
    mp.write_signal(coord_dir, "laptop_wrote_p5", "ok")
else
    mp.wait_for_signal(coord_dir, "laptop_wrote_p5", 30)
    mp.wait_for_file_content(vault_dir .. "/notes/from_phone.md", "Edited on laptop (originally from phone)", 45)
    log("Got updated notes/from_phone.md")
end
mp.write_signal(coord_dir, role .. "_done_phase5", "ok")

-- Phase 6: Final verification
mp.wait_for_signal(coord_dir, "phase6", 30)
local expected = {
    ["notes/from_laptop.md"] = "Written on the laptop",
    ["notes/from_phone.md"]  = "Edited on laptop (originally from phone)",
    ["notes/from_relay.md"]  = "Auto-synced by relay server",
}
for path, content in pairs(expected) do
    local actual = mp.read_file(vault_dir .. "/" .. path)
    assert(actual == content, role .. ": " .. path .. " mismatch")
    log("Verified " .. path)
end

local conflicts = vault:list_conflicts()
local unresolved = 0
for _, c in ipairs(conflicts) do if not c.resolved then unresolved = unresolved + 1 end end
log("Unresolved conflicts: " .. unresolved)

mp.write_signal(coord_dir, role .. "_done_phase6", "ok")
mp.write_signal(coord_dir, role .. "_done", "ok")
vault:stop()
net:stop()
log("SUCCESS")

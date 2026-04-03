-- mp_vault_node.lua
--
-- Generic multi-process vault sync node. Parameterized by ROLE env var.
-- Used by mp_vault_3node.lua coordinator.
--
-- Environment variables:
--   ROLE       - node role: "love", "joy", or "peace"
--   COORD_DIR  - shared coordination directory
--   DATA_DIR   - persistent data directory for this node
--   VAULT_DIR  - vault directory path
--   PEERS      - comma-separated list of other peer roles (e.g. "joy,peace")

package.path = package.path .. ";simulation/scripts/lib/?.lua"
local mp = require("mp_helpers")

local role      = os.getenv("ROLE")
local coord_dir = os.getenv("COORD_DIR")
local data_dir  = os.getenv("DATA_DIR")
local vault_dir = os.getenv("VAULT_DIR")
local peers_str = os.getenv("PEERS")

assert(role,      "ROLE env var required")
assert(coord_dir, "COORD_DIR env var required")
assert(data_dir,  "DATA_DIR env var required")
assert(vault_dir, "VAULT_DIR env var required")
assert(peers_str, "PEERS env var required")

-- Parse peer list
local peers = {}
for p in peers_str:gmatch("[^,]+") do
    peers[#peers + 1] = p
end

local tag = "[" .. role:sub(1,1):upper() .. role:sub(2) .. "]"

local function log(msg)
    print(tag .. " " .. msg)
end

-- ============================================================================
-- Phase 1: Start network, exchange identities, connect
-- ============================================================================

log("Starting network...")
local net = indras.Network.new(data_dir)
net:start()
log("Network started, id=" .. net:id():sub(1, 16) .. "...")

-- Publish our identity code and endpoint address
mp.write_signal(coord_dir, role .. "_identity", net:identity_code())
mp.write_signal(coord_dir, role .. "_addr", net:endpoint_addr_string())
log("Published identity + endpoint address")

-- Wait for all peers' identity codes and connect
for _, peer in ipairs(peers) do
    log("Waiting for " .. peer .. "'s identity code...")
    local peer_code = mp.wait_for_signal(coord_dir, peer .. "_identity", 30)
    net:connect_by_code(peer_code)
    log("Connected to " .. peer)
end

-- ============================================================================
-- Phase 1b: Create or join vault
-- ============================================================================

local vault

if role == "love" then
    log("Creating vault...")
    local invite
    vault, invite = indras.VaultSync.create(net, "SharedVault", vault_dir)
    mp.write_signal(coord_dir, "invite", invite)
    log("Vault created, invite published")
else
    log("Waiting for vault invite...")
    local invite = mp.wait_for_signal(coord_dir, "invite", 30)
    log("Joining vault...")
    vault = indras.VaultSync.join(net, invite, vault_dir)
    log("Joined vault")
end

-- Add all peers' relays for bidirectional blob push
for _, peer in ipairs(peers) do
    local peer_addr = mp.wait_for_signal(coord_dir, peer .. "_addr", 30)
    vault:add_peer_relay_addr(peer_addr)
    log("Added " .. peer .. "'s relay")
end

-- Signal ready
mp.write_signal(coord_dir, role .. "_ready", "ok")

-- Wait for all peers to be ready
for _, peer in ipairs(peers) do
    mp.wait_for_signal(coord_dir, peer .. "_ready", 30)
end
log("All peers ready")

-- ============================================================================
-- Phase 2: Joy writes FromJoy.md → others verify
-- ============================================================================

mp.wait_for_signal(coord_dir, "phase2", 30)

if role == "joy" then
    vault:write_file("FromJoy.md", "Hello from Joy")
    log("Wrote FromJoy.md")
    mp.write_signal(coord_dir, "joy_wrote_phase2", "ok")
else
    mp.wait_for_signal(coord_dir, "joy_wrote_phase2", 30)
    mp.wait_for_file_content(vault_dir .. "/FromJoy.md", "Hello from Joy", 30)
    log("Got FromJoy.md: Hello from Joy")
end

mp.write_signal(coord_dir, role .. "_done_phase2", "ok")
for _, peer in ipairs(peers) do
    mp.wait_for_signal(coord_dir, peer .. "_done_phase2", 30)
end

-- ============================================================================
-- Phase 3: Love writes FromLove.md → others verify
-- ============================================================================

mp.wait_for_signal(coord_dir, "phase3", 30)

if role == "love" then
    vault:write_file("FromLove.md", "Hello from Love")
    log("Wrote FromLove.md")
    mp.write_signal(coord_dir, "love_wrote_phase3", "ok")
else
    mp.wait_for_signal(coord_dir, "love_wrote_phase3", 30)
    mp.wait_for_file_content(vault_dir .. "/FromLove.md", "Hello from Love", 30)
    log("Got FromLove.md: Hello from Love")
end

mp.write_signal(coord_dir, role .. "_done_phase3", "ok")
for _, peer in ipairs(peers) do
    mp.wait_for_signal(coord_dir, peer .. "_done_phase3", 30)
end

-- ============================================================================
-- Phase 4: Peace writes FromPeace.md → others verify
-- ============================================================================

mp.wait_for_signal(coord_dir, "phase4", 30)

if role == "peace" then
    vault:write_file("FromPeace.md", "Greetings from Peace")
    log("Wrote FromPeace.md")
    mp.write_signal(coord_dir, "peace_wrote_phase4", "ok")
else
    mp.wait_for_signal(coord_dir, "peace_wrote_phase4", 30)
    mp.wait_for_file_content(vault_dir .. "/FromPeace.md", "Greetings from Peace", 30)
    log("Got FromPeace.md: Greetings from Peace")
end

mp.write_signal(coord_dir, role .. "_done_phase4", "ok")
for _, peer in ipairs(peers) do
    mp.wait_for_signal(coord_dir, peer .. "_done_phase4", 30)
end

-- ============================================================================
-- Phase 5: Concurrent edits — each node edits a different file
-- ============================================================================

mp.wait_for_signal(coord_dir, "phase5", 30)

if role == "love" then
    vault:write_file("FromJoy.md", "Hello from Joy - edited by Love")
    log("Edited FromJoy.md")
elseif role == "joy" then
    vault:write_file("FromLove.md", "Hello from Love - edited by Joy")
    log("Edited FromLove.md")
elseif role == "peace" then
    vault:write_file("FromPeace.md", "Greetings from Peace - updated")
    log("Edited FromPeace.md")
end

mp.write_signal(coord_dir, role .. "_wrote_phase5", "ok")

-- Wait for all edits, then verify
for _, peer in ipairs(peers) do
    mp.wait_for_signal(coord_dir, peer .. "_wrote_phase5", 30)
end

mp.wait_for_file_content(vault_dir .. "/FromJoy.md", "Hello from Joy - edited by Love", 30)
log("Verified FromJoy.md edit")
mp.wait_for_file_content(vault_dir .. "/FromLove.md", "Hello from Love - edited by Joy", 30)
log("Verified FromLove.md edit")
mp.wait_for_file_content(vault_dir .. "/FromPeace.md", "Greetings from Peace - updated", 30)
log("Verified FromPeace.md edit")

-- ============================================================================
-- Done
-- ============================================================================

mp.write_signal(coord_dir, role .. "_done", "ok")
vault:stop()
net:stop()
log("Done - SUCCESS")

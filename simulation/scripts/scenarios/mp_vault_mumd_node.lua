-- mp_vault_mumd_node.lua
--
-- Per-node script for multi-user multi-device vault sync test.
-- 4 nodes: love_laptop, love_phone, joy_laptop, joy_phone
--
-- Environment variables:
--   ROLE       - e.g. "love_laptop", "joy_phone"
--   USER       - "love" or "joy"
--   SIBLING    - role of the same user's other device
--   PEERS      - comma-separated list of all other roles
--   COORD_DIR  - shared coordination directory
--   DATA_DIR   - persistent data directory
--   VAULT_DIR  - vault directory path

package.path = package.path .. ";simulation/scripts/lib/?.lua"
local mp = require("mp_helpers")

local role      = os.getenv("ROLE")
local user      = os.getenv("USER")
local sibling   = os.getenv("SIBLING")
local peers_str = os.getenv("PEERS")
local coord_dir = os.getenv("COORD_DIR")
local data_dir  = os.getenv("DATA_DIR")
local vault_dir = os.getenv("VAULT_DIR")

assert(role,      "ROLE required")
assert(user,      "USER required")
assert(sibling,   "SIBLING required")
assert(peers_str, "PEERS required")
assert(coord_dir, "COORD_DIR required")
assert(data_dir,  "DATA_DIR required")
assert(vault_dir, "VAULT_DIR required")

local peers = {}
for p in peers_str:gmatch("[^,]+") do peers[#peers + 1] = p end

local is_primary = (role == "love_laptop" or role == "joy_laptop")

local function log(msg) print("[" .. role .. "] " .. msg) end

-- ============================================================================
-- Phase 1: Identity setup, start network, connect, join vault
-- ============================================================================

-- Phone devices import PQ identity from their laptop sibling
if not is_primary then
    log("Waiting for PQ identity from " .. sibling .. "...")
    local backup = mp.wait_for_signal(coord_dir, user .. "_pq_identity", 30)
    indras.Network.import_pq_identity(data_dir, backup)
    log("Imported PQ identity")
end

log("Starting network...")
local net = indras.Network.new(data_dir)
net:start()
log("Network started, id=" .. net:id():sub(1, 16) .. "...")

-- Primary devices (laptops) export PQ identity for their phone sibling
if is_primary then
    local backup = net:export_pq_identity()
    mp.write_signal(coord_dir, user .. "_pq_identity", backup)
    log("Exported PQ identity for " .. sibling)
end

-- Publish endpoint address
mp.write_signal(coord_dir, role .. "_addr", net:endpoint_addr_string())
log("Published endpoint address")

-- Connect to all peers via transport
for _, peer in ipairs(peers) do
    log("Waiting for " .. peer .. "'s address...")
    local addr = mp.wait_for_signal(coord_dir, peer .. "_addr", 30)
    net:connect_to_addr(addr)
    log("Connected to " .. peer)
end

-- Create or join vault
local vault

if role == "love_laptop" then
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

-- Add all peers' relays for blob push
for _, peer in ipairs(peers) do
    local addr = mp.wait_for_signal(coord_dir, peer .. "_addr", 30)
    vault:add_peer_relay_addr(addr)
end
log("Added all peer relays")

-- Signal ready
mp.write_signal(coord_dir, role .. "_ready", "ok")
for _, peer in ipairs(peers) do
    mp.wait_for_signal(coord_dir, peer .. "_ready", 45)
end
log("All nodes ready")

-- ============================================================================
-- Phase 2: Love-Laptop writes shared/from_love.md → all 3 others see it
-- ============================================================================

mp.wait_for_signal(coord_dir, "phase2", 30)

if role == "love_laptop" then
    vault:write_file("shared/from_love.md", "Hello from Love's laptop")
    log("Wrote shared/from_love.md")
    mp.write_signal(coord_dir, "love_laptop_wrote_phase2", "ok")
else
    mp.wait_for_signal(coord_dir, "love_laptop_wrote_phase2", 30)
    mp.wait_for_file_content(vault_dir .. "/shared/from_love.md", "Hello from Love's laptop", 30)
    log("Got shared/from_love.md")
end

mp.write_signal(coord_dir, role .. "_done_phase2", "ok")

-- ============================================================================
-- Phase 3: Joy-Phone writes shared/from_joy.md → all 3 others see it
-- ============================================================================

mp.wait_for_signal(coord_dir, "phase3", 30)

if role == "joy_phone" then
    vault:write_file("shared/from_joy.md", "Hello from Joy's phone")
    log("Wrote shared/from_joy.md")
    mp.write_signal(coord_dir, "joy_phone_wrote_phase3", "ok")
else
    mp.wait_for_signal(coord_dir, "joy_phone_wrote_phase3", 30)
    mp.wait_for_file_content(vault_dir .. "/shared/from_joy.md", "Hello from Joy's phone", 30)
    log("Got shared/from_joy.md")
end

mp.write_signal(coord_dir, role .. "_done_phase3", "ok")

-- ============================================================================
-- Phase 4: Love-Phone edits Love-Laptop's file (same user, no conflict)
-- ============================================================================

mp.wait_for_signal(coord_dir, "phase4", 30)

if role == "love_phone" then
    vault:write_file("shared/from_love.md", "Updated by Love's phone")
    log("Edited shared/from_love.md")
    mp.write_signal(coord_dir, "love_phone_wrote_phase4", "ok")
else
    mp.wait_for_signal(coord_dir, "love_phone_wrote_phase4", 30)
    mp.wait_for_file_content(vault_dir .. "/shared/from_love.md", "Updated by Love's phone", 30)
    log("Got updated shared/from_love.md")
end

mp.write_signal(coord_dir, role .. "_done_phase4", "ok")

-- ============================================================================
-- Phase 5: Final verification — all devices have identical content
-- ============================================================================

mp.wait_for_signal(coord_dir, "phase5", 30)

local expected = {
    ["shared/from_love.md"] = "Updated by Love's phone",
    ["shared/from_joy.md"]  = "Hello from Joy's phone",
}

for path, content in pairs(expected) do
    local actual = mp.read_file(vault_dir .. "/" .. path)
    assert(actual == content,
        string.format("[%s] %s: expected '%s', got '%s'", role, path, content, actual or "(nil)"))
    log("Verified " .. path)
end

-- Verify no unresolved conflicts from same-user edits
local conflicts = vault:list_conflicts()
local unresolved = 0
for _, c in ipairs(conflicts) do
    if not c.resolved then unresolved = unresolved + 1 end
end
log("Unresolved conflicts: " .. unresolved)

mp.write_signal(coord_dir, role .. "_done_phase5", "ok")

-- ============================================================================
-- Done
-- ============================================================================

mp.write_signal(coord_dir, role .. "_done", "ok")
vault:stop()
net:stop()
log("Done - SUCCESS")

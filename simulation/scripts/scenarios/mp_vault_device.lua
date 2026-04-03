-- mp_vault_device.lua
--
-- Generic per-device script for same-user multi-device vault sync test.
-- Both devices share the same PQ signing identity (= same UserId)
-- but have different iroh transport keys (= different MemberId/DeviceId).
--
-- Environment variables:
--   ROLE       - "laptop" or "phone"
--   COORD_DIR  - shared coordination directory
--   DATA_DIR   - persistent data directory for this device
--   VAULT_DIR  - vault directory path

package.path = package.path .. ";simulation/scripts/lib/?.lua"
local mp = require("mp_helpers")

local role      = os.getenv("ROLE")
local coord_dir = os.getenv("COORD_DIR")
local data_dir  = os.getenv("DATA_DIR")
local vault_dir = os.getenv("VAULT_DIR")

assert(role,      "ROLE env var required")
assert(coord_dir, "COORD_DIR env var required")
assert(data_dir,  "DATA_DIR env var required")
assert(vault_dir, "VAULT_DIR env var required")

local tag = "[" .. role:sub(1,1):upper() .. role:sub(2) .. "]"
local function log(msg) print(tag .. " " .. msg) end

local peer = (role == "laptop") and "phone" or "laptop"

-- ============================================================================
-- Phase 1: Setup — import identity (phone only), start network, connect
-- ============================================================================

if role == "phone" then
    -- Phone imports only PQ signing identity (not iroh transport key).
    -- This gives Phone the same UserId but a fresh DeviceId.
    log("Waiting for PQ identity backup...")
    local backup = mp.wait_for_signal(coord_dir, "pq_identity_backup", 30)
    indras.Network.import_pq_identity(data_dir, backup)
    log("Imported PQ identity into " .. data_dir)
end

log("Starting network...")
local net = indras.Network.new(data_dir)
net:start()
log("Network started, id=" .. net:id():sub(1, 16) .. "...")

if role == "laptop" then
    -- Laptop exports PQ identity for Phone (not iroh key)
    local backup = net:export_pq_identity()
    mp.write_signal(coord_dir, "pq_identity_backup", backup)
    log("Exported PQ identity for phone")
end

-- Publish endpoint address
mp.write_signal(coord_dir, role .. "_addr", net:endpoint_addr_string())
log("Published endpoint address")

-- Wait for peer's endpoint address and connect via transport
-- (different iroh keys = different devices, CAN connect directly)
log("Waiting for " .. peer .. "'s endpoint address...")
local peer_addr_str = mp.wait_for_signal(coord_dir, peer .. "_addr", 30)
net:connect_to_addr(peer_addr_str)
log("Connected to " .. peer .. " via transport")

-- ============================================================================
-- Phase 1b: Create or join vault
-- ============================================================================

local vault

if role == "laptop" then
    log("Creating vault...")
    local invite
    vault, invite = indras.VaultSync.create(net, "MyVault", vault_dir)
    mp.write_signal(coord_dir, "invite", invite)
    log("Vault created, invite published")
else
    log("Waiting for vault invite...")
    local invite = mp.wait_for_signal(coord_dir, "invite", 30)
    log("Joining vault...")
    vault = indras.VaultSync.join(net, invite, vault_dir)
    log("Joined vault")
end

-- Add peer's relay for blob push
vault:add_peer_relay_addr(peer_addr_str)
log("Added " .. peer .. "'s relay for blob push")

-- Signal ready
mp.write_signal(coord_dir, role .. "_ready", "ok")
mp.wait_for_signal(coord_dir, peer .. "_ready", 30)
log("Both devices ready")

-- ============================================================================
-- Phase 2: Laptop writes notes/meeting.md → Phone sees it
-- ============================================================================

mp.wait_for_signal(coord_dir, "phase2", 30)

if role == "laptop" then
    vault:write_file("notes/meeting.md", "Meeting notes from laptop")
    log("Wrote notes/meeting.md")
    mp.write_signal(coord_dir, "laptop_wrote_phase2", "ok")
else
    mp.wait_for_signal(coord_dir, "laptop_wrote_phase2", 30)
    mp.wait_for_file_content(vault_dir .. "/notes/meeting.md", "Meeting notes from laptop", 30)
    log("Got notes/meeting.md: Meeting notes from laptop")
end

mp.write_signal(coord_dir, role .. "_done_phase2", "ok")
mp.wait_for_signal(coord_dir, peer .. "_done_phase2", 30)

-- ============================================================================
-- Phase 3: Phone writes notes/mobile.md → Laptop sees it
-- ============================================================================

mp.wait_for_signal(coord_dir, "phase3", 30)

if role == "phone" then
    vault:write_file("notes/mobile.md", "Quick note from phone")
    log("Wrote notes/mobile.md")
    mp.write_signal(coord_dir, "phone_wrote_phase3", "ok")
else
    mp.wait_for_signal(coord_dir, "phone_wrote_phase3", 30)
    mp.wait_for_file_content(vault_dir .. "/notes/mobile.md", "Quick note from phone", 30)
    log("Got notes/mobile.md: Quick note from phone")
end

mp.write_signal(coord_dir, role .. "_done_phase3", "ok")
mp.wait_for_signal(coord_dir, peer .. "_done_phase3", 30)

-- ============================================================================
-- Phase 4: Laptop updates notes/meeting.md → Phone sees the update
-- ============================================================================

mp.wait_for_signal(coord_dir, "phase4", 30)

if role == "laptop" then
    vault:write_file("notes/meeting.md", "Updated on laptop")
    log("Updated notes/meeting.md")
    mp.write_signal(coord_dir, "laptop_wrote_phase4", "ok")
else
    mp.wait_for_signal(coord_dir, "laptop_wrote_phase4", 30)
    mp.wait_for_file_content(vault_dir .. "/notes/meeting.md", "Updated on laptop", 30)
    log("Got updated notes/meeting.md: Updated on laptop")
end

mp.write_signal(coord_dir, role .. "_done_phase4", "ok")
mp.wait_for_signal(coord_dir, peer .. "_done_phase4", 30)

-- ============================================================================
-- Phase 5: Verify all files match on both devices
-- ============================================================================

mp.wait_for_signal(coord_dir, "phase5", 30)

local expected_files = {
    ["notes/meeting.md"] = "Updated on laptop",
    ["notes/mobile.md"]  = "Quick note from phone",
}

for path, expected in pairs(expected_files) do
    local content = mp.read_file(vault_dir .. "/" .. path)
    assert(content == expected,
        string.format("File %s mismatch: expected '%s', got '%s'", path, expected, content or "(nil)"))
    log("Verified " .. path)
end

mp.write_signal(coord_dir, role .. "_done_phase5", "ok")
mp.wait_for_signal(coord_dir, peer .. "_done_phase5", 30)

-- ============================================================================
-- Done
-- ============================================================================

mp.write_signal(coord_dir, role .. "_done", "ok")
vault:stop()
net:stop()
log("Done - SUCCESS")

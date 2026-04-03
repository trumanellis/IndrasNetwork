-- mp_vault_love.lua
--
-- Multi-process vault sync test: Love's process.
--
-- Love creates a vault, shares the invite via coordination file,
-- connects to Joy via identity code, and waits for Joy's file
-- to appear on disk with the correct content.
--
-- Environment variables:
--   COORD_DIR  - shared coordination directory
--   DATA_DIR   - persistent data directory for this node
--   VAULT_DIR  - vault directory path

package.path = package.path .. ";simulation/scripts/lib/?.lua"
local mp = require("mp_helpers")

local coord_dir = os.getenv("COORD_DIR")
local data_dir  = os.getenv("DATA_DIR")
local vault_dir = os.getenv("VAULT_DIR")

assert(coord_dir, "COORD_DIR env var required")
assert(data_dir,  "DATA_DIR env var required")
assert(vault_dir, "VAULT_DIR env var required")

print("[Love] Starting network...")
local net = indras.Network.new(data_dir)
net:start()
print("[Love] Network started, id=" .. net:id():sub(1, 16) .. "...")

-- Publish our identity code and endpoint address for Joy
local my_code = net:identity_code()
local my_addr = net:endpoint_addr_string()
mp.write_signal(coord_dir, "love_identity", my_code)
mp.write_signal(coord_dir, "love_addr", my_addr)
print("[Love] Published identity code + endpoint address")

-- Wait for Joy's identity code and connect
print("[Love] Waiting for Joy's identity code...")
local joy_code = mp.wait_for_signal(coord_dir, "joy_identity", 30)
print("[Love] Got Joy's identity code, connecting...")
net:connect_by_code(joy_code)
print("[Love] Connected to Joy")

-- Create vault and publish invite
print("[Love] Creating vault...")
local vault, invite = indras.VaultSync.create(net, "SharedVault", vault_dir)
mp.write_signal(coord_dir, "invite", invite)
print("[Love] Vault created, invite published")

-- Add Joy's relay so Love can push blobs to Joy
print("[Love] Waiting for Joy's endpoint address...")
local joy_addr = mp.wait_for_signal(coord_dir, "joy_addr", 30)
vault:add_peer_relay_addr(joy_addr)
print("[Love] Added Joy's relay for blob push")

-- Wait for Joy's file to appear on disk
print("[Love] Waiting for FromJoy.md...")
local file_path = vault_dir .. "/FromJoy.md"
mp.wait_for_file_content(file_path, "Hello Love", 30)
print("[Love] Got FromJoy.md with content: Hello Love")

-- Verify via vault index too
local files = vault:list_files()
local found = false
for _, f in ipairs(files) do
    if f.path == "FromJoy.md" then
        found = true
        break
    end
end
assert(found, "FromJoy.md should be in vault index")
print("[Love] Verified FromJoy.md in vault index")

-- ── Phase 2: Love edits FromJoy.md ──

print("[Love] Editing FromJoy.md...")
vault:write_file("FromJoy.md", "Hello Love - edited by Love")
print("[Love] Wrote updated FromJoy.md")
mp.write_signal(coord_dir, "love_edited", "ok")

-- Wait for Joy to confirm the edit
mp.wait_for_signal(coord_dir, "joy_saw_edit", 30)
print("[Love] Joy confirmed seeing the edit")

-- ── Phase 3: Love creates FromLove.md ──

print("[Love] Creating FromLove.md...")
vault:write_file("FromLove.md", "Dear Joy, with love")
print("[Love] Wrote FromLove.md")
mp.write_signal(coord_dir, "love_created_file", "ok")

-- Wait for Joy to confirm the new file
mp.wait_for_signal(coord_dir, "joy_saw_new_file", 30)
print("[Love] Joy confirmed seeing FromLove.md")

-- Signal success
mp.write_signal(coord_dir, "love_done", "ok")

-- Cleanup
vault:stop()
net:stop()
print("[Love] Done - SUCCESS")

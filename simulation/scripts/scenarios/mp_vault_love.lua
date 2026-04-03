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

-- Publish our identity code for Joy to connect to
local my_code = net:identity_code()
mp.write_signal(coord_dir, "love_identity", my_code)
print("[Love] Published identity code")

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

-- Signal success
mp.write_signal(coord_dir, "love_done", "ok")

-- Cleanup
vault:stop()
net:stop()
print("[Love] Done - SUCCESS")

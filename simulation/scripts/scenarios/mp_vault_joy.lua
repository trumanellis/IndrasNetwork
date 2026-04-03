-- mp_vault_joy.lua
--
-- Multi-process vault sync test: Joy's process.
--
-- Joy connects to Love via identity code, joins the vault via invite,
-- writes FromJoy.md with "Hello Love", and waits for confirmation.
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

print("[Joy] Starting network...")
local net = indras.Network.new(data_dir)
net:start()
print("[Joy] Network started, id=" .. net:id():sub(1, 16) .. "...")

-- Publish our identity code for Love to connect to
local my_code = net:identity_code()
mp.write_signal(coord_dir, "joy_identity", my_code)
print("[Joy] Published identity code")

-- Wait for Love's identity code and connect
print("[Joy] Waiting for Love's identity code...")
local love_code = mp.wait_for_signal(coord_dir, "love_identity", 30)
print("[Joy] Got Love's identity code, connecting...")
net:connect_by_code(love_code)
print("[Joy] Connected to Love")

-- Wait for vault invite and join
print("[Joy] Waiting for vault invite...")
local invite = mp.wait_for_signal(coord_dir, "invite", 30)
print("[Joy] Got invite, joining vault...")
local vault = indras.VaultSync.join(net, invite, vault_dir)
print("[Joy] Joined vault")

-- Write the file
print("[Joy] Writing FromJoy.md...")
vault:write_file("FromJoy.md", "Hello Love")
print("[Joy] Wrote FromJoy.md with 'Hello Love'")

-- Verify our own file is on disk
local content = mp.read_file(vault_dir .. "/FromJoy.md")
assert(content == "Hello Love", "Joy's own file should be on disk")
print("[Joy] Confirmed local copy")

-- Wait for Love to confirm receipt
print("[Joy] Waiting for Love to confirm...")
mp.wait_for_signal(coord_dir, "love_done", 30)
print("[Joy] Love confirmed receipt")

-- Signal success
mp.write_signal(coord_dir, "joy_done", "ok")

-- Cleanup
vault:stop()
net:stop()
print("[Joy] Done - SUCCESS")

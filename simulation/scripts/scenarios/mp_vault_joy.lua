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

-- Publish our identity code and endpoint address for Love
local my_code = net:identity_code()
local my_addr = net:endpoint_addr_string()
mp.write_signal(coord_dir, "joy_identity", my_code)
mp.write_signal(coord_dir, "joy_addr", my_addr)
print("[Joy] Published identity code + endpoint address")

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

-- ── Phase 2: Joy sees Love's edit to FromJoy.md ──

mp.wait_for_signal(coord_dir, "love_edited", 30)
print("[Joy] Love edited FromJoy.md, waiting for sync...")
mp.wait_for_file_content(vault_dir .. "/FromJoy.md", "Hello Love - edited by Love", 30)
print("[Joy] Got updated FromJoy.md: Hello Love - edited by Love")
mp.write_signal(coord_dir, "joy_saw_edit", "ok")

-- ── Phase 3: Joy sees Love's new file FromLove.md ──

mp.wait_for_signal(coord_dir, "love_created_file", 30)
print("[Joy] Love created FromLove.md, waiting for sync...")
mp.wait_for_file_content(vault_dir .. "/FromLove.md", "Dear Joy, with love", 30)
print("[Joy] Got FromLove.md: Dear Joy, with love")
mp.write_signal(coord_dir, "joy_saw_new_file", "ok")

-- Wait for Love to confirm all done
mp.wait_for_signal(coord_dir, "love_done", 30)
print("[Joy] Love confirmed all done")

-- Signal success
mp.write_signal(coord_dir, "joy_done", "ok")

-- Cleanup
vault:stop()
net:stop()
print("[Joy] Done - SUCCESS")

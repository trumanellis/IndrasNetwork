-- live_vault_relay_blob_sync.lua
--
-- Integration test: relay-backed blob replication between two vault peers.
--
-- Tests:
--   1. Love creates a shared vault and invites Joy
--   2. Joy writes a new markdown file (FromJoy.md) with content "Hello Love"
--   3. The file metadata AND content sync to Love's vault via relay blob replication
--   4. Love can read the actual file content from disk
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/live_vault_relay_blob_sync.lua

package.path = package.path .. ";simulation/scripts/lib/?.lua"
local h = require("live_network_helpers")

--- Read a file's content from disk.
local function read_from_disk(vault_path, rel_path)
    local full = vault_path .. "/" .. rel_path
    local f = io.open(full, "rb")
    if not f then return nil end
    local content = f:read("*a")
    f:close()
    return content
end

print("=== Vault Relay Blob Sync Test ===")
print()

-- ============================================================================
-- PHASE 1: Create Love and Joy, connect, Love creates vault, Joy joins
-- ============================================================================

h.section(1, "Love creates vault, Joy joins")

local nets = h.create_networks(2)
local love_net, joy_net = nets[1], nets[2]
h.connect_all(nets)

local tmp_love = os.tmpname() .. "_vault_love"
local tmp_joy  = os.tmpname() .. "_vault_joy"

local vault_love, invite = indras.VaultSync.create(love_net, "SharedVault", tmp_love)
print("    Love created vault at " .. tmp_love)

local vault_joy = indras.VaultSync.join(joy_net, invite, tmp_joy)
print("    Joy joined vault at " .. tmp_joy)

-- ============================================================================
-- PHASE 2: Joy writes FromJoy.md
-- ============================================================================

h.section(2, "Joy writes FromJoy.md with 'Hello Love'")

vault_joy:write_file("FromJoy.md", "Hello Love")
print("    Joy wrote FromJoy.md")

-- Verify Joy's own file is on disk
local joy_content = read_from_disk(vault_joy:vault_path(), "FromJoy.md")
indras.assert.eq(joy_content, "Hello Love", "Joy's own file should be on disk")
print("    Confirmed Joy's local copy: " .. joy_content)

-- ============================================================================
-- PHASE 3: Verify Love receives the file (metadata + content)
-- ============================================================================

h.section(3, "Verify Love receives FromJoy.md with correct content")

-- Wait for Love to see the file in the CRDT index
h.assert_eventually(function()
    local files = vault_love:list_files()
    for _, f in ipairs(files) do
        if f.path == "FromJoy.md" then return true end
    end
    return false
end, { timeout = 15, interval = 1, msg = "Love should see FromJoy.md in vault index" })
print("    Love sees FromJoy.md in vault index")

-- Wait for the file content to appear on Love's disk
h.assert_eventually(function()
    local content = read_from_disk(vault_love:vault_path(), "FromJoy.md")
    return content == "Hello Love"
end, { timeout = 15, interval = 1, msg = "Love should have FromJoy.md with content 'Hello Love' on disk" })

local love_content = read_from_disk(vault_love:vault_path(), "FromJoy.md")
print("    Love reads FromJoy.md: " .. love_content)
indras.assert.eq(love_content, "Hello Love", "Love should read Joy's message")

-- Verify hashes match between both vaults
local love_files = vault_love:list_files()
local joy_files  = vault_joy:list_files()

local function find_hash(files, path)
    for _, f in ipairs(files) do
        if f.path == path then return f.hash_hex end
    end
    return nil
end

local love_hash = find_hash(love_files, "FromJoy.md")
local joy_hash  = find_hash(joy_files,  "FromJoy.md")
indras.assert.eq(love_hash, joy_hash, "Hashes should match between Love and Joy")
print("    Hashes match: " .. love_hash:sub(1, 16) .. "...")

-- ============================================================================
-- PHASE 4: Cleanup
-- ============================================================================

h.section(4, "Cleanup")

vault_love:stop()
vault_joy:stop()
h.stop_all(nets)
print("    All stopped")

print()
print("=== All phases passed! ===")
return true

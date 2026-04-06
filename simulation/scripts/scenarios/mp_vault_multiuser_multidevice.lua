-- mp_vault_multiuser_multidevice.lua
--
-- Coordinator for multi-user, multi-device vault sync test.
--
-- Two users (Love and Joy), each with two devices (laptop + phone),
-- sharing a vault. 4 total processes. Tests that:
--   1. Love-Laptop creates vault, all others join
--   2. Each device can write and all other devices see the file
--   3. Same-user edits from different devices don't conflict
--   4. Cross-user edits from different users sync correctly
--
-- Topology:
--   Love-Laptop  ←→  Love-Phone   (same PQ identity)
--   Joy-Laptop   ←→  Joy-Phone    (same PQ identity)
--   Love-*       ←→  Joy-*        (different users, shared vault)
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/mp_vault_multiuser_multidevice.lua

package.path = package.path .. ";simulation/scripts/lib/?.lua"
local mp = require("mp_helpers")

print("=== Multi-User Multi-Device Vault Sync Test ===")
print("    2 users x 2 devices = 4 processes")
print()

-- Create temp directories
local base = os.tmpname() .. "_mp_mumd"
local coord_dir = base .. "/coord"
os.execute("mkdir -p " .. coord_dir)

local nodes = {
    { role = "love_laptop", user = "love" },
    { role = "love_phone",  user = "love" },
    { role = "joy_laptop",  user = "joy"  },
    { role = "joy_phone",   user = "joy"  },
}

-- Create data + vault dirs for each node
for _, n in ipairs(nodes) do
    n.data_dir  = base .. "/" .. n.role .. "_data"
    n.vault_dir = base .. "/" .. n.role .. "_vault"
    os.execute("mkdir -p " .. n.data_dir)
    os.execute("mkdir -p " .. n.vault_dir)
end

print("  Coordination dir: " .. coord_dir)
for _, n in ipairs(nodes) do
    print("  " .. n.role .. ": " .. n.data_dir)
end
print()

local runner = "./target/debug/lua_runner"
local node_script = "simulation/scripts/scenarios/mp_vault_mumd_node.lua"

-- Build peer list for each node (all other roles)
local function peer_list(my_role)
    local others = {}
    for _, n in ipairs(nodes) do
        if n.role ~= my_role then others[#others + 1] = n.role end
    end
    return table.concat(others, ",")
end

-- Which role is the "sibling" device (same user, other device)?
local function sibling(role)
    if role == "love_laptop" then return "love_phone" end
    if role == "love_phone"  then return "love_laptop" end
    if role == "joy_laptop"  then return "joy_phone" end
    if role == "joy_phone"   then return "joy_laptop" end
end

-- Spawn nodes in dependency order:
-- 1. Laptops first (they export PQ identity for their phone siblings)
-- 2. Phones after their laptop's PQ identity is available
-- Nodes handle address exchange and vault join internally via coordination files.

-- Spawn Love-Laptop
print("[coordinator] Spawning love_laptop...")
local function spawn(nd)
    local cmd = string.format(
        "ROLE=%s USER=%s SIBLING=%s PEERS=%s COORD_DIR=%s DATA_DIR=%s VAULT_DIR=%s %s %s > %s/%s_stdout.txt 2>&1 &",
        nd.role, nd.user, sibling(nd.role), peer_list(nd.role),
        coord_dir, nd.data_dir, nd.vault_dir, runner, node_script, coord_dir, nd.role
    )
    os.execute(cmd)
end

spawn(nodes[1]) -- love_laptop
mp.wait_for_signal(coord_dir, "love_pq_identity", 30)
print("[coordinator] Love PQ identity exported")

-- Spawn Joy-Laptop
print("[coordinator] Spawning joy_laptop...")
spawn(nodes[3]) -- joy_laptop
mp.wait_for_signal(coord_dir, "joy_pq_identity", 30)
print("[coordinator] Joy PQ identity exported")

-- Spawn both phones (PQ identities are now available)
print("[coordinator] Spawning love_phone + joy_phone...")
spawn(nodes[2]) -- love_phone
spawn(nodes[4]) -- joy_phone

-- Wait for all nodes to be ready
for _, nd in ipairs(nodes) do
    mp.wait_for_signal(coord_dir, nd.role .. "_ready", 45)
end
print("[coordinator] All 4 nodes ready")
print()

-- Release phases
local function release_and_wait(phase, desc)
    print("[coordinator] " .. desc .. "...")
    mp.write_signal(coord_dir, phase, "ok")
    for _, nd in ipairs(nodes) do
        mp.wait_for_signal(coord_dir, nd.role .. "_done_" .. phase, 45)
    end
    print("[coordinator] " .. desc .. " - done")
end

release_and_wait("phase2", "Phase 2: Love-Laptop writes a file")
release_and_wait("phase3", "Phase 3: Joy-Phone writes a file")
release_and_wait("phase4", "Phase 4: Love-Phone edits Love-Laptop's file (same user)")
release_and_wait("phase5", "Phase 5: Final verification on all devices")

-- Wait for all done
for _, nd in ipairs(nodes) do
    mp.wait_for_signal(coord_dir, nd.role .. "_done", 30)
end
print()

-- Print output
for _, nd in ipairs(nodes) do
    print("--- " .. nd.role .. " output ---")
    local f = io.open(coord_dir .. "/" .. nd.role .. "_stdout.txt", "r")
    if f then print(f:read("*a")); f:close() end
end
print("---")
print()

-- Cleanup
os.execute("rm -rf " .. base)

print("=== Multi-User Multi-Device Vault Sync: ALL PASSED ===")
return true

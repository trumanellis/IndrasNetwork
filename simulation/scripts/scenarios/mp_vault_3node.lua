-- mp_vault_3node.lua
--
-- Coordinator for 3-node multi-process vault sync test.
--
-- Spawns three separate lua_runner processes (Love, Joy, Peace) that
-- connect via identity codes, share a vault, and verify bidirectional
-- file sync across all three nodes via relay blob replication.
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/mp_vault_3node.lua

package.path = package.path .. ";simulation/scripts/lib/?.lua"
local mp = require("mp_helpers")

print("=== 3-Node Multi-Process Vault Sync Test ===")
print()

-- Create temp directories
local base = os.tmpname() .. "_mp3_vault"
local coord_dir = base .. "/coord"
local roles = { "love", "joy", "peace" }
local dirs = {}

os.execute("mkdir -p " .. coord_dir)
for _, role in ipairs(roles) do
    dirs[role] = {
        data  = base .. "/" .. role .. "_data",
        vault = base .. "/" .. role .. "_vault",
    }
    os.execute("mkdir -p " .. dirs[role].data)
    os.execute("mkdir -p " .. dirs[role].vault)
end

print("  Coordination dir: " .. coord_dir)
for _, role in ipairs(roles) do
    print("  " .. role .. " data: " .. dirs[role].data)
end
print()

-- Find the lua_runner binary
local runner = "./target/debug/lua_runner"
local node_script = "simulation/scripts/scenarios/mp_vault_node.lua"

-- Build peer lists (each node's peers = all others)
local function peer_list(role)
    local others = {}
    for _, r in ipairs(roles) do
        if r ~= role then others[#others + 1] = r end
    end
    return table.concat(others, ",")
end

-- Spawn all three processes
for _, role in ipairs(roles) do
    local env = string.format(
        "ROLE=%s COORD_DIR=%s DATA_DIR=%s VAULT_DIR=%s PEERS=%s",
        role, coord_dir, dirs[role].data, dirs[role].vault, peer_list(role)
    )
    local cmd = string.format(
        "%s %s %s > %s/%s_stdout.txt 2>&1 &",
        env, runner, node_script, coord_dir, role
    )
    print("[coordinator] Spawning " .. role .. "...")
    os.execute(cmd)
    -- Small stagger so Love starts first (creates vault before others join)
    if role == "love" then indras.sleep(1) end
end

-- Release phases one at a time
local function release_phase(name)
    print("[coordinator] Releasing " .. name .. "...")
    mp.write_signal(coord_dir, name, "ok")
end

-- Wait for all nodes to be ready
for _, role in ipairs(roles) do
    mp.wait_for_signal(coord_dir, role .. "_ready", 45)
end
print("[coordinator] All nodes ready")
print()

-- Run phases
release_phase("phase2")
for _, role in ipairs(roles) do
    mp.wait_for_signal(coord_dir, role .. "_done_phase2", 30)
end
print("[coordinator] Phase 2 complete: Joy's file synced to all")

release_phase("phase3")
for _, role in ipairs(roles) do
    mp.wait_for_signal(coord_dir, role .. "_done_phase3", 30)
end
print("[coordinator] Phase 3 complete: Love's file synced to all")

release_phase("phase4")
for _, role in ipairs(roles) do
    mp.wait_for_signal(coord_dir, role .. "_done_phase4", 30)
end
print("[coordinator] Phase 4 complete: Peace's file synced to all")

release_phase("phase5")
for _, role in ipairs(roles) do
    mp.wait_for_signal(coord_dir, role .. "_done", 45)
end
print("[coordinator] Phase 5 complete: Concurrent edits synced")
print()

-- Print subprocess output
for _, role in ipairs(roles) do
    print("--- " .. role .. " output ---")
    local f = io.open(coord_dir .. "/" .. role .. "_stdout.txt", "r")
    if f then print(f:read("*a")); f:close() end
end
print("---")
print()

-- Cleanup
os.execute("rm -rf " .. base)

print("=== 3-Node Multi-Process Vault Sync: ALL PASSED ===")
return true

-- mp_vault_3device.lua
--
-- Single-user 3-device vault sync: Laptop + Phone + Relay Server.
-- All share the same PQ identity. Coordinator spawns 3 processes.

package.path = package.path .. ";simulation/scripts/lib/?.lua"
local mp = require("mp_helpers")

print("=== Single-User 3-Device Vault Sync Test ===")
print("    1 user: Laptop + Phone + Relay Server")
print()

local base = os.tmpname() .. "_mp_3dev"
local coord_dir = base .. "/coord"
os.execute("mkdir -p " .. coord_dir)

local devices = { "laptop", "phone", "relay" }
local dirs = {}
for _, d in ipairs(devices) do
    dirs[d] = { data = base .. "/" .. d .. "_data", vault = base .. "/" .. d .. "_vault" }
    os.execute("mkdir -p " .. dirs[d].data)
    os.execute("mkdir -p " .. dirs[d].vault)
end

local runner = "./target/debug/lua_runner"
local node_script = "simulation/scripts/scenarios/mp_vault_3dev_node.lua"

local function peer_list(role)
    local others = {}
    for _, d in ipairs(devices) do
        if d ~= role then others[#others + 1] = d end
    end
    return table.concat(others, ",")
end

local function spawn(role)
    local d = dirs[role]
    os.execute(string.format(
        "ROLE=%s PEERS=%s COORD_DIR=%s DATA_DIR=%s VAULT_DIR=%s %s %s > %s/%s_stdout.txt 2>&1 &",
        role, peer_list(role), coord_dir, d.data, d.vault, runner, node_script, coord_dir, role
    ))
end

print("[coordinator] Spawning laptop...")
spawn("laptop")
mp.wait_for_signal(coord_dir, "pq_identity", 30)
print("[coordinator] PQ identity exported, spawning phone + relay...")
spawn("phone")
spawn("relay")

for _, d in ipairs(devices) do mp.wait_for_signal(coord_dir, d .. "_ready", 45) end
print("[coordinator] All 3 devices ready")
print()

local function release_and_wait(phase, desc)
    print("[coordinator] " .. desc .. "...")
    mp.write_signal(coord_dir, phase, "ok")
    for _, d in ipairs(devices) do mp.wait_for_signal(coord_dir, d .. "_done_" .. phase, 45) end
    print("[coordinator] " .. desc .. " - done")
end

release_and_wait("phase2", "Phase 2: Laptop writes a file")
release_and_wait("phase3", "Phase 3: Phone writes a file")
release_and_wait("phase4", "Phase 4: Relay writes a file")
release_and_wait("phase5", "Phase 5: Laptop edits Phone's file (same user)")
release_and_wait("phase6", "Phase 6: Final verification")

for _, d in ipairs(devices) do mp.wait_for_signal(coord_dir, d .. "_done", 30) end
print()

for _, d in ipairs(devices) do
    print("--- " .. d .. " output ---")
    local f = io.open(coord_dir .. "/" .. d .. "_stdout.txt", "r")
    if f then print(f:read("*a")); f:close() end
end
print("---")
print()

os.execute("rm -rf " .. base)
print("=== Single-User 3-Device Vault Sync: ALL PASSED ===")
return true

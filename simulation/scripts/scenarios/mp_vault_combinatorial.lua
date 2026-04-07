-- mp_vault_combinatorial.lua
--
-- KNOWN FAILING: relay QUIC connection scaling issue.
-- Each node creates too many QUIC endpoints (one per peer relay per vault).
-- Needs shared endpoint refactor in relay_sync.rs to pass.
--
-- 3 users x 2 devices = 6 processes, 7 vaults (every sharing combination).
-- Private: love, joy, peace. Pairs: love+joy, love+peace, joy+peace. All: love+joy+peace.

package.path = package.path .. ";simulation/scripts/lib/?.lua"
local mp = require("mp_helpers")

print("=== Combinatorial Vault Sharing Test ===")
print("    3 users x 2 devices = 6 processes, 7 vaults")
print()

local base = os.tmpname() .. "_mp_combo"
local coord_dir = base .. "/coord"
os.execute("mkdir -p " .. coord_dir)

local users = { "love", "joy", "peace" }
local device_types = { "laptop", "phone" }

local vaults = {
    { name = "love_private",  members = { "love" } },
    { name = "joy_private",   members = { "joy" } },
    { name = "peace_private", members = { "peace" } },
    { name = "love_joy",      members = { "love", "joy" } },
    { name = "love_peace",    members = { "love", "peace" } },
    { name = "joy_peace",     members = { "joy", "peace" } },
    { name = "all_shared",    members = { "love", "joy", "peace" } },
}

local vault_defs = {}
for _, v in ipairs(vaults) do
    vault_defs[#vault_defs + 1] = v.name .. ":" .. table.concat(v.members, ",")
end
local vault_defs_str = table.concat(vault_defs, ";")

local all_roles = {}
for _, u in ipairs(users) do
    for _, d in ipairs(device_types) do
        all_roles[#all_roles + 1] = u .. "_" .. d
    end
end

for _, role in ipairs(all_roles) do
    os.execute("mkdir -p " .. base .. "/" .. role .. "_data")
    for _, v in ipairs(vaults) do
        os.execute("mkdir -p " .. base .. "/" .. role .. "_vault_" .. v.name)
    end
end

print("  Nodes: " .. table.concat(all_roles, ", "))
print("  Vaults: " .. #vaults)
print()

local runner = "./target/debug/lua_runner"
local node_script = "simulation/scripts/scenarios/mp_vault_combo_node.lua"

local function peer_list(my_role)
    local others = {}
    for _, r in ipairs(all_roles) do
        if r ~= my_role then others[#others + 1] = r end
    end
    return table.concat(others, ",")
end

local function spawn(role, user_name, dev)
    os.execute(string.format(
        "ROLE=%s USER_NAME=%s DEVICE=%s PEERS=%s VAULT_DEFS='%s' COORD_DIR=%s DATA_DIR=%s BASE_DIR=%s %s %s > %s/%s_stdout.txt 2>&1 &",
        role, user_name, dev, peer_list(role), vault_defs_str,
        coord_dir, base .. "/" .. role .. "_data", base,
        runner, node_script, coord_dir, role
    ))
end

-- Spawn laptops first
for _, u in ipairs(users) do
    spawn(u .. "_laptop", u, "laptop")
    mp.wait_for_signal(coord_dir, u .. "_pq_identity", 30)
    print("[coordinator] " .. u .. " PQ identity exported")
end

-- Spawn phones
for _, u in ipairs(users) do spawn(u .. "_phone", u, "phone") end
print("[coordinator] All phones spawned")

for _, role in ipairs(all_roles) do mp.wait_for_signal(coord_dir, role .. "_ready", 60) end
print("[coordinator] All 6 nodes ready")
print()

-- Phase 2: write + verify one vault at a time
print("[coordinator] Phase 2: Write + verify each vault...")
for _, v in ipairs(vaults) do
    local phase = "phase2_vault_" .. v.name
    print("[coordinator]   " .. v.name .. " (" .. table.concat(v.members, "+") .. ")...")
    mp.write_signal(coord_dir, phase, "ok")
    for _, role in ipairs(all_roles) do mp.wait_for_signal(coord_dir, role .. "_done_" .. phase, 120) end
    print("[coordinator]   " .. v.name .. " - synced")
end
for _, role in ipairs(all_roles) do mp.wait_for_signal(coord_dir, role .. "_done_phase2", 60) end
print("[coordinator] Phase 2 complete")

-- Phase 3: cross-device private edits
print("[coordinator] Phase 3: Private vault cross-device edits...")
mp.write_signal(coord_dir, "phase3", "ok")
for _, role in ipairs(all_roles) do mp.wait_for_signal(coord_dir, role .. "_done_phase3", 60) end
print("[coordinator] Phase 3 complete")

-- Phase 4: final verification
print("[coordinator] Phase 4: Final verification...")
mp.write_signal(coord_dir, "phase4", "ok")
for _, role in ipairs(all_roles) do mp.wait_for_signal(coord_dir, role .. "_done_phase4", 60) end
print("[coordinator] Phase 4 complete")

for _, role in ipairs(all_roles) do mp.wait_for_signal(coord_dir, role .. "_done", 30) end
print()

for _, role in ipairs(all_roles) do
    print("--- " .. role .. " ---")
    local f = io.open(coord_dir .. "/" .. role .. "_stdout.txt", "r")
    if f then print(f:read("*a")); f:close() end
end
print("---")
print()

os.execute("rm -rf " .. base)
print("=== Combinatorial Vault Sharing: ALL PASSED ===")
return true

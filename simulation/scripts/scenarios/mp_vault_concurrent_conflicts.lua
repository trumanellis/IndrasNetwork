-- mp_vault_concurrent_conflicts.lua
--
-- Multi-process test: concurrent writes, conflict lifecycle, and offline rejoin.
--
-- 2 users (A, B) x 2 devices (laptop, phone) = 4 processes, 1 shared vault.
-- Tests what the combinatorial test does NOT: simultaneous writes, intentional
-- conflicts, conflict resolution propagation, and offline/rejoin catch-up sync.
--
-- Phases:
--   1. Setup: PQ identity, connect, create/join vault
--   2. Concurrent writes to different files (no conflict)
--   3. Same-file concurrent edits within conflict window (conflict expected)
--   4. Conflict resolution on one device, verify propagation to all
--   5. Device goes offline, local edit, rejoin + catch-up sync
--   6. Final convergence verification
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/mp_vault_concurrent_conflicts.lua

package.path = package.path .. ";simulation/scripts/lib/?.lua"
local mp = require("mp_helpers")

print("=== Concurrent Conflicts & Offline Rejoin Test ===")
print("    2 users x 2 devices = 4 processes, 1 shared vault")
print()

-- Create temp directories
local base = os.tmpname() .. "_mp_cc"
local coord_dir = base .. "/coord"
os.execute("mkdir -p " .. coord_dir)

local nodes = {
    { role = "A_laptop", user = "A", device = "laptop" },
    { role = "A_phone",  user = "A", device = "phone"  },
    { role = "B_laptop", user = "B", device = "laptop" },
    { role = "B_phone",  user = "B", device = "phone"  },
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
local node_script = "simulation/scripts/scenarios/mp_vault_cc_node.lua"

-- Build peer list for each node (all other roles)
local function peer_list(my_role)
    local others = {}
    for _, n in ipairs(nodes) do
        if n.role ~= my_role then others[#others + 1] = n.role end
    end
    return table.concat(others, ",")
end

local function spawn(nd)
    local cmd = string.format(
        "ROLE=%s USER_NAME=%s DEVICE=%s PEERS=%s COORD_DIR=%s DATA_DIR=%s VAULT_DIR=%s %s %s > %s/%s_stdout.txt 2>&1 &",
        nd.role, nd.user, nd.device, peer_list(nd.role),
        coord_dir, nd.data_dir, nd.vault_dir, runner, node_script, coord_dir, nd.role
    )
    os.execute(cmd)
end

-- Spawn laptops first (they export PQ identity for their phone siblings)
print("[coordinator] Spawning A_laptop...")
spawn(nodes[1]) -- A_laptop
mp.wait_for_signal(coord_dir, "A_pq_identity", 30)
print("[coordinator] A PQ identity exported")

print("[coordinator] Spawning B_laptop...")
spawn(nodes[3]) -- B_laptop
mp.wait_for_signal(coord_dir, "B_pq_identity", 30)
print("[coordinator] B PQ identity exported")

-- Spawn both phones (PQ identities are now available)
print("[coordinator] Spawning A_phone + B_phone...")
spawn(nodes[2]) -- A_phone
spawn(nodes[4]) -- B_phone

-- Wait for all nodes to be ready
for _, nd in ipairs(nodes) do
    mp.wait_for_signal(coord_dir, nd.role .. "_ready", 60)
end
print("[coordinator] All 4 nodes ready")
print()

-- Release phases
local function release_and_wait(phase, desc, timeout)
    timeout = timeout or 60
    print("[coordinator] " .. desc .. "...")
    mp.write_signal(coord_dir, phase, "ok")
    for _, nd in ipairs(nodes) do
        mp.wait_for_signal(coord_dir, nd.role .. "_done_" .. phase, timeout)
    end
    print("[coordinator] " .. desc .. " - done")
end

release_and_wait("phase2", "Phase 2: Concurrent writes to different files (no conflict)")
release_and_wait("phase3", "Phase 3: Same-file concurrent edits (conflict expected)")
release_and_wait("phase4", "Phase 4: Conflict resolution + propagation", 120)
release_and_wait("phase5_offline", "Phase 5a: Device offline + edits", 120)
release_and_wait("phase5_rejoin", "Phase 5b: Rejoin + catch-up sync", 120)
release_and_wait("phase6", "Phase 6: Final convergence verification", 60)

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

print("=== Concurrent Conflicts & Offline Rejoin: ALL PASSED ===")
return true

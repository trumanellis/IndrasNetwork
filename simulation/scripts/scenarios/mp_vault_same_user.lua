-- mp_vault_same_user.lua
--
-- Coordinator for same-user multi-device vault sync test.
--
-- Spawns two separate lua_runner processes (Laptop and Phone) that share
-- the same cryptographic identity, create/join a personal vault, and
-- verify bidirectional file sync between devices.
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/mp_vault_same_user.lua

package.path = package.path .. ";simulation/scripts/lib/?.lua"
local mp = require("mp_helpers")

print("=== Same-User Multi-Device Vault Sync Test ===")
print()

-- Create temp directories
local base = os.tmpname() .. "_mp_device"
local coord_dir    = base .. "/coord"
local laptop_data  = base .. "/laptop_data"
local phone_data   = base .. "/phone_data"
local laptop_vault = base .. "/laptop_vault"
local phone_vault  = base .. "/phone_vault"

os.execute("mkdir -p " .. coord_dir)
os.execute("mkdir -p " .. laptop_data)
os.execute("mkdir -p " .. phone_data)
os.execute("mkdir -p " .. laptop_vault)
os.execute("mkdir -p " .. phone_vault)

print("  Coordination dir: " .. coord_dir)
print("  Laptop: data=" .. laptop_data .. " vault=" .. laptop_vault)
print("  Phone:  data=" .. phone_data  .. " vault=" .. phone_vault)
print()

local runner = "./target/debug/lua_runner"
local device_script = "simulation/scripts/scenarios/mp_vault_device.lua"

-- Spawn Laptop first (it exports identity for Phone)
print("[coordinator] Spawning Laptop...")
local laptop_cmd = string.format(
    "ROLE=laptop COORD_DIR=%s DATA_DIR=%s VAULT_DIR=%s %s %s > %s/laptop_stdout.txt 2>&1 &",
    coord_dir, laptop_data, laptop_vault, runner, device_script, coord_dir
)
os.execute(laptop_cmd)

-- Wait for PQ identity export before spawning Phone
mp.wait_for_signal(coord_dir, "pq_identity_backup", 30)
print("[coordinator] PQ identity exported, spawning Phone...")

local phone_cmd = string.format(
    "ROLE=phone COORD_DIR=%s DATA_DIR=%s VAULT_DIR=%s %s %s > %s/phone_stdout.txt 2>&1 &",
    coord_dir, phone_data, phone_vault, runner, device_script, coord_dir
)
os.execute(phone_cmd)

-- Wait for both devices to be ready
mp.wait_for_signal(coord_dir, "laptop_ready", 30)
mp.wait_for_signal(coord_dir, "phone_ready", 30)
print("[coordinator] Both devices ready")
print()

-- Release phases
local function release_and_wait(phase_name, description)
    print("[coordinator] " .. description .. "...")
    mp.write_signal(coord_dir, phase_name, "ok")
    mp.wait_for_signal(coord_dir, "laptop_done_" .. phase_name, 30)
    mp.wait_for_signal(coord_dir, "phone_done_" .. phase_name, 30)
    print("[coordinator] " .. description .. " - done")
end

release_and_wait("phase2", "Phase 2: Laptop writes meeting notes")
release_and_wait("phase3", "Phase 3: Phone writes mobile note")
release_and_wait("phase4", "Phase 4: Laptop updates meeting notes")
release_and_wait("phase5", "Phase 5: Verify all files on both devices")

-- Wait for completion
mp.wait_for_signal(coord_dir, "laptop_done", 30)
mp.wait_for_signal(coord_dir, "phone_done", 30)
print()

-- Print subprocess output
for _, device in ipairs({"laptop", "phone"}) do
    print("--- " .. device .. " output ---")
    local f = io.open(coord_dir .. "/" .. device .. "_stdout.txt", "r")
    if f then print(f:read("*a")); f:close() end
end
print("---")
print()

-- Cleanup
os.execute("rm -rf " .. base)

print("=== Same-User Multi-Device Vault Sync: ALL PASSED ===")
return true

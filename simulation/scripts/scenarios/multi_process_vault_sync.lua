-- multi_process_vault_sync.lua
--
-- Coordinator for multi-process vault sync test.
--
-- Spawns two separate lua_runner processes (Love and Joy) that connect
-- via identity codes, create/join a shared vault, and verify that file
-- content syncs across process boundaries via relay blob replication.
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/multi_process_vault_sync.lua

print("=== Multi-Process Vault Sync Test ===")
print()

-- Create temp directories
local base = os.tmpname() .. "_mp_vault"
local coord_dir  = base .. "/coord"
local love_data  = base .. "/love_data"
local joy_data   = base .. "/joy_data"
local love_vault = base .. "/love_vault"
local joy_vault  = base .. "/joy_vault"

os.execute("mkdir -p " .. coord_dir)
os.execute("mkdir -p " .. love_data)
os.execute("mkdir -p " .. joy_data)
os.execute("mkdir -p " .. love_vault)
os.execute("mkdir -p " .. joy_vault)

print("  Coordination dir: " .. coord_dir)
print("  Love data: " .. love_data)
print("  Joy data:  " .. joy_data)
print()

-- Find the lua_runner binary
local runner = "./target/debug/lua_runner"
local f = io.open(runner, "r")
if not f then
    -- Try release build
    runner = "./target/release/lua_runner"
    f = io.open(runner, "r")
end
if f then f:close() end
assert(f or true, "lua_runner binary not found — run `cargo build -p indras-simulation` first")

-- Build the env string for each process
local love_env = string.format(
    "COORD_DIR=%s DATA_DIR=%s VAULT_DIR=%s",
    coord_dir, love_data, love_vault
)
local joy_env = string.format(
    "COORD_DIR=%s DATA_DIR=%s VAULT_DIR=%s",
    coord_dir, joy_data, joy_vault
)

local love_script = "simulation/scripts/scenarios/mp_vault_love.lua"
local joy_script  = "simulation/scripts/scenarios/mp_vault_joy.lua"

-- Spawn both processes
print("[coordinator] Spawning Love process...")
local love_cmd = string.format(
    "%s %s %s > %s/love_stdout.txt 2>&1 &",
    love_env, runner, love_script, coord_dir
)
os.execute(love_cmd)

-- Small delay so Love starts first (creates vault before Joy joins)
indras.sleep(1)

print("[coordinator] Spawning Joy process...")
local joy_cmd = string.format(
    "%s %s %s > %s/joy_stdout.txt 2>&1 &",
    joy_env, runner, joy_script, coord_dir
)
os.execute(joy_cmd)

-- Wait for both to complete (poll for done signals)
print("[coordinator] Waiting for both processes to complete...")
print()

local timeout = 60
local deadline = os.time() + timeout
local love_ok = false
local joy_ok = false

while os.time() < deadline do
    if not love_ok then
        local f = io.open(coord_dir .. "/love_done", "r")
        if f then
            love_ok = (f:read("*a") == "ok")
            f:close()
        end
    end
    if not joy_ok then
        local f = io.open(coord_dir .. "/joy_done", "r")
        if f then
            joy_ok = (f:read("*a") == "ok")
            f:close()
        end
    end
    if love_ok and joy_ok then break end
    indras.sleep(1)
end

-- Print subprocess output
print("--- Love output ---")
local lf = io.open(coord_dir .. "/love_stdout.txt", "r")
if lf then print(lf:read("*a")); lf:close() end

print("--- Joy output ---")
local jf = io.open(coord_dir .. "/joy_stdout.txt", "r")
if jf then print(jf:read("*a")); jf:close() end
print("---")
print()

-- Cleanup
os.execute("rm -rf " .. base)

-- Report results
if love_ok and joy_ok then
    print("=== Multi-Process Vault Sync: ALL PASSED ===")
    return true
else
    print("FAILED:")
    if not love_ok then print("  Love did not complete") end
    if not joy_ok  then print("  Joy did not complete") end
    error("Multi-process vault sync test failed")
end

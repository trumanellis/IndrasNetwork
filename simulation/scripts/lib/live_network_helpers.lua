-- Helper functions for IndrasNetwork live network scenarios
--
-- Utilities for scenarios using the high-level indras.Network API.

local M = {}

-- Create N network instances, start them all, return as array
-- Usage: local nets = M.create_networks(3)
function M.create_networks(count)
    local nets = {}
    for i = 1, count do
        local net = indras.Network.new()
        net:start()
        nets[i] = net
    end
    return nets
end

-- Connect all networks to each other (pairwise direct connect)
-- This uses connect_to() for in-process direct connection
function M.connect_all(nets)
    for i = 1, #nets do
        for j = i + 1, #nets do
            nets[i]:connect_to(nets[j])
        end
    end
end

-- Stop all networks
function M.stop_all(nets)
    for _, net in ipairs(nets) do
        net:stop()
    end
end

-- Wait for a condition with polling (for eventually-consistent sync)
-- predicate: function that returns true when condition is met
-- opts: { timeout = seconds (default 5), interval = seconds (default 0.1), msg = error message }
function M.wait_for(predicate, opts)
    opts = opts or {}
    local timeout = opts.timeout or 5
    local interval = opts.interval or 0.1
    local msg = opts.msg or "Condition not met within timeout"

    local elapsed = 0
    while elapsed < timeout do
        local ok, result = pcall(predicate)
        if ok and result then
            return true
        end
        indras.sleep(interval)
        elapsed = elapsed + interval
    end
    return false
end

-- Assert a condition becomes true within timeout (uses wait_for)
function M.assert_eventually(predicate, opts)
    opts = opts or {}
    local msg = opts.msg or "Assertion failed: condition not met within timeout"
    local result = M.wait_for(predicate, opts)
    if not result then
        indras.assert.fail(msg)
    end
end

-- Dump network state for debugging
function M.dump_network(net, label)
    label = label or "Network"
    print("  [" .. label .. "]")
    print("    ID: " .. net:id():sub(1, 16) .. "...")
    local name = net:display_name()
    if name then
        print("    Name: " .. name)
    end
    print("    Running: " .. tostring(net:is_running()))
    local realm_ids = net:realms()
    print("    Realms: " .. #realm_ids)
    for i, rid in ipairs(realm_ids) do
        print("      [" .. i .. "] " .. rid:sub(1, 16) .. "...")
    end
end

-- Dump realm state for debugging
function M.dump_realm(realm, label)
    label = label or "Realm"
    print("  [" .. label .. "]")
    print("    ID: " .. realm:id():sub(1, 16) .. "...")
    local name = realm:name()
    if name then
        print("    Name: " .. name)
    end
    local count = realm:member_count()
    print("    Members: " .. count)
end

-- Print a section header
function M.section(num, title)
    print("[" .. num .. "] " .. title)
end

-- Print a pass banner
function M.pass(test_name)
    print()
    print("=== " .. test_name .. " PASSED ===")
end

return M

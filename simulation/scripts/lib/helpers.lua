-- Helper functions for IndrasNetwork Lua scenarios
--
-- Common utilities used across test scenarios.

local helpers = {}

-- Get a random element from a table
function helpers.random_element(tbl)
    if #tbl == 0 then
        return nil
    end
    return tbl[math.random(#tbl)]
end

-- Get a random pair of different elements from a table
function helpers.random_pair(tbl)
    if #tbl < 2 then
        return nil, nil
    end
    local i = math.random(#tbl)
    local j = math.random(#tbl)
    while j == i do
        j = math.random(#tbl)
    end
    return tbl[i], tbl[j]
end

-- Filter a table by a predicate
function helpers.filter(tbl, predicate)
    local result = {}
    for _, v in ipairs(tbl) do
        if predicate(v) then
            table.insert(result, v)
        end
    end
    return result
end

-- Map a function over a table
function helpers.map(tbl, fn)
    local result = {}
    for i, v in ipairs(tbl) do
        result[i] = fn(v)
    end
    return result
end

-- Count elements in a table matching a predicate
function helpers.count_if(tbl, predicate)
    local count = 0
    for _, v in ipairs(tbl) do
        if predicate(v) then
            count = count + 1
        end
    end
    return count
end

-- Print a table for debugging
function helpers.dump(tbl, indent)
    indent = indent or 0
    local prefix = string.rep("  ", indent)
    for k, v in pairs(tbl) do
        if type(v) == "table" then
            print(prefix .. tostring(k) .. ":")
            helpers.dump(v, indent + 1)
        else
            print(prefix .. tostring(k) .. " = " .. tostring(v))
        end
    end
end

-- Create a context logger with automatic trace_id
function helpers.create_logger(ctx)
    local logger = {}

    function logger.trace(msg, fields)
        fields = fields or {}
        fields.trace_id = ctx.trace_id
        indras.log.trace(msg, fields)
    end

    function logger.debug(msg, fields)
        fields = fields or {}
        fields.trace_id = ctx.trace_id
        indras.log.debug(msg, fields)
    end

    function logger.info(msg, fields)
        fields = fields or {}
        fields.trace_id = ctx.trace_id
        indras.log.info(msg, fields)
    end

    function logger.warn(msg, fields)
        fields = fields or {}
        fields.trace_id = ctx.trace_id
        indras.log.warn(msg, fields)
    end

    function logger.error(msg, fields)
        fields = fields or {}
        fields.trace_id = ctx.trace_id
        indras.log.error(msg, fields)
    end

    return logger
end

-- Run simulation until a predicate is true or max_ticks reached
function helpers.run_until(sim, predicate, max_ticks)
    max_ticks = max_ticks or 1000
    for _ = 1, max_ticks do
        sim:step()
        if predicate() then
            return true
        end
    end
    return false
end

-- Assert with retry (for eventually-consistent behavior)
function helpers.assert_eventually(sim, predicate, max_ticks, msg)
    local success = helpers.run_until(sim, predicate, max_ticks)
    if not success then
        indras.assert.fail(msg or "Condition not met within " .. max_ticks .. " ticks")
    end
end

-- Wait for all messages to be delivered or timeout
function helpers.wait_for_delivery(sim, expected_count, max_ticks)
    return helpers.run_until(sim, function()
        return sim.stats.messages_delivered >= expected_count
    end, max_ticks)
end

-- Get statistics summary as a table
function helpers.stats_summary(sim)
    local stats = sim.stats
    return {
        sent = stats.messages_sent,
        delivered = stats.messages_delivered,
        dropped = stats.messages_dropped,
        delivery_rate = stats:delivery_rate(),
        average_latency = stats:average_latency(),
        average_hops = stats:average_hops()
    }
end

return helpers

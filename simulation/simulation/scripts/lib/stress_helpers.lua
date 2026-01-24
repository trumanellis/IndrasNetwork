-- Stress Helpers Library
--
-- Utility functions for stress test scenarios.
-- Provides common operations for load testing, chaos engineering, and metric validation.

local stress = {}

-- Stress test levels for configurable intensity
stress.LEVELS = {
    QUICK = {
        duration = 100,        -- ticks
        load_multiplier = 1,
        peers = 10,
        description = "Quick validation run"
    },
    MEDIUM = {
        duration = 500,        -- ticks
        load_multiplier = 5,
        peers = 50,
        description = "Moderate stress test"
    },
    FULL = {
        duration = 2000,       -- ticks
        load_multiplier = 10,
        peers = 200,
        description = "Full system stress test"
    }
}

--- Create a standardized result tracking object for stress scenarios
-- @param scenario_name string Name of the scenario being tested
-- @return table Result builder with tracking methods
function stress.result_builder(scenario_name)
    local builder = {
        scenario = scenario_name,
        start_time = os.time(),
        metrics = {},
        errors = {},
        warnings = {},
        thresholds_passed = nil,
        threshold_failures = {},
    }

    --- Record a metric value
    -- @param name string Metric name
    -- @param value any Metric value
    function builder:metric(name, value)
        self.metrics[name] = value
    end

    --- Record an error
    -- @param message string Error message
    function builder:error(message)
        table.insert(self.errors, {
            tick = nil,  -- Can be set externally
            message = message,
            timestamp = os.time()
        })
    end

    --- Record a warning
    -- @param message string Warning message
    function builder:warn(message)
        table.insert(self.warnings, {
            tick = nil,  -- Can be set externally
            message = message,
            timestamp = os.time()
        })
    end

    --- Mark threshold validation results
    -- @param passed boolean Whether thresholds passed
    -- @param failures table Optional list of threshold failures
    function builder:thresholds(passed, failures)
        self.thresholds_passed = passed
        if failures then
            self.threshold_failures = failures
        end
    end

    --- Build the final result object
    -- @return table Final result with all tracked data
    function builder:build()
        local duration = os.time() - self.start_time
        return {
            scenario = self.scenario,
            duration_seconds = duration,
            metrics = self.metrics,
            errors = self.errors,
            warnings = self.warnings,
            thresholds_passed = self.thresholds_passed,
            threshold_failures = self.threshold_failures,
            success = #self.errors == 0 and (self.thresholds_passed == nil or self.thresholds_passed)
        }
    end

    return builder
end

--- Create a latency tracker for collecting samples and computing statistics
-- @return table Tracker with add() and compute() methods
function stress.latency_tracker()
    local tracker = {
        samples = {}
    }

    --- Add a latency sample
    -- @param value number Latency value to track
    function tracker:add(value)
        table.insert(self.samples, value)
    end

    --- Compute latency statistics
    -- @return table Statistics with p50, p95, p99, min, max, avg
    function tracker:compute()
        if #self.samples == 0 then
            return {
                p50 = 0,
                p95 = 0,
                p99 = 0,
                min = 0,
                max = 0,
                avg = 0,
                count = 0
            }
        end

        -- Sort samples for percentile computation
        local sorted = {}
        for _, v in ipairs(self.samples) do
            table.insert(sorted, v)
        end
        table.sort(sorted)

        -- Calculate min/max
        local min_val = sorted[1]
        local max_val = sorted[#sorted]

        -- Calculate average
        local sum = 0
        for _, v in ipairs(sorted) do
            sum = sum + v
        end
        local avg_val = sum / #sorted

        -- Calculate percentiles
        local function percentile(p)
            local idx = math.ceil(#sorted * p / 100)
            return sorted[math.max(1, idx)]
        end

        return {
            p50 = percentile(50),
            p95 = percentile(95),
            p99 = percentile(99),
            min = min_val,
            max = max_val,
            avg = avg_val,
            count = #self.samples
        }
    end

    return tracker
end

--- Calculate throughput based on operation counts
-- @param start_tick number Starting tick
-- @param current_tick number Current tick
-- @param operation_count number Total operations completed
-- @return number Operations per tick
function stress.throughput_calculator(start_tick, current_tick, operation_count)
    local elapsed = current_tick - start_tick
    if elapsed <= 0 then
        return 0
    end
    return operation_count / elapsed
end

--- Create a chaos injection helper for fault injection testing
-- @param sim Simulation The simulation instance
-- @param config table Configuration with kill_probability and resurrect_probability
-- @return table Chaos injector with maybe_kill() and maybe_resurrect() methods
function stress.chaos_injection(sim, config)
    local chaos = {
        sim = sim,
        config = config or {},
        killed_peers = {},
        kill_count = 0,
        resurrect_count = 0
    }

    --- Maybe kill a random peer based on probability
    -- @param peers table Array of currently online peers
    -- @param probability number Optional kill probability (0.0-1.0)
    -- @return boolean true if a peer was killed
    function chaos:maybe_kill(peers, probability)
        probability = probability or self.config.kill_probability or 0.0

        if #peers == 0 or math.random() > probability then
            return false
        end

        -- Pick random peer to kill
        local victim_idx = math.random(#peers)
        local victim = peers[victim_idx]

        -- Kill the peer
        self.sim:kill_peer(victim)
        table.insert(self.killed_peers, victim)
        self.kill_count = self.kill_count + 1

        -- Remove from online peers list
        table.remove(peers, victim_idx)

        return true
    end

    --- Maybe resurrect a random killed peer based on probability
    -- @param peers table Array of currently online peers (to add resurrected peer to)
    -- @param probability number Optional resurrect probability (0.0-1.0)
    -- @return boolean true if a peer was resurrected
    function chaos:maybe_resurrect(peers, probability)
        probability = probability or self.config.resurrect_probability or 0.0

        if #self.killed_peers == 0 or math.random() > probability then
            return false
        end

        -- Pick random killed peer to resurrect
        local resurrect_idx = math.random(#self.killed_peers)
        local resurrected = self.killed_peers[resurrect_idx]

        -- Resurrect the peer
        self.sim:resurrect_peer(resurrected)
        self.resurrect_count = self.resurrect_count + 1

        -- Remove from killed list and add back to online
        table.remove(self.killed_peers, resurrect_idx)
        table.insert(peers, resurrected)

        return true
    end

    --- Get chaos statistics
    -- @return table Stats with kill_count, resurrect_count, currently_killed
    function chaos:stats()
        return {
            kill_count = self.kill_count,
            resurrect_count = self.resurrect_count,
            currently_killed = #self.killed_peers
        }
    end

    return chaos
end

--- Validate metrics against threshold configuration
-- @param metrics table Metrics to validate
-- @param thresholds table Threshold configuration
-- @return boolean passed, table failures
function stress.assert_thresholds(metrics, thresholds)
    local failures = {}

    for metric_name, threshold_config in pairs(thresholds) do
        local actual = metrics[metric_name]

        if actual == nil then
            table.insert(failures, {
                metric = metric_name,
                error = "Metric not found in results"
            })
        else
            -- Check minimum threshold
            if threshold_config.min and actual < threshold_config.min then
                table.insert(failures, {
                    metric = metric_name,
                    constraint = "min",
                    expected = threshold_config.min,
                    actual = actual
                })
            end

            -- Check maximum threshold
            if threshold_config.max and actual > threshold_config.max then
                table.insert(failures, {
                    metric = metric_name,
                    constraint = "max",
                    expected = threshold_config.max,
                    actual = actual
                })
            end
        end
    end

    return #failures == 0, failures
end

--- Pick a random peer from an array
-- @param peers table Array of peers
-- @return any Random peer, or nil if array is empty
function stress.random_peer_from(peers)
    if #peers == 0 then
        return nil
    end
    return peers[math.random(#peers)]
end

--- Format tick duration as human readable string
-- @param ticks number Number of ticks
-- @return string Formatted duration
function stress.format_duration(ticks)
    -- Assume 1 tick = 100ms (configurable)
    local tick_duration_ms = 100
    local total_ms = ticks * tick_duration_ms

    if total_ms < 1000 then
        return string.format("%dms", total_ms)
    elseif total_ms < 60000 then
        return string.format("%.2fs", total_ms / 1000)
    elseif total_ms < 3600000 then
        local minutes = math.floor(total_ms / 60000)
        local seconds = (total_ms % 60000) / 1000
        return string.format("%dm %.1fs", minutes, seconds)
    else
        local hours = math.floor(total_ms / 3600000)
        local minutes = math.floor((total_ms % 3600000) / 60000)
        return string.format("%dh %dm", hours, minutes)
    end
end

--- Create a correlation context with stress scenario tag
-- @param scenario_name string Name of the scenario
-- @return CorrelationContext
function stress.new_context(scenario_name)
    local ctx = indras.correlation.new_root()
    ctx = ctx:with_tag("scenario", scenario_name)
    ctx = ctx:with_tag("test_type", "stress")
    return ctx
end

--- Pick N random peers from an array (helper for sampling)
-- @param peers table Array of peers
-- @param count number Number of peers to pick
-- @return table Array of randomly selected peers (may be less than count if not enough peers)
function stress.random_sample(peers, count)
    if #peers <= count then
        -- Return a copy of all peers
        local result = {}
        for _, peer in ipairs(peers) do
            table.insert(result, peer)
        end
        return result
    end

    -- Shuffle and take first N
    local shuffled = {}
    for _, peer in ipairs(peers) do
        table.insert(shuffled, peer)
    end

    -- Fisher-Yates shuffle
    for i = #shuffled, 2, -1 do
        local j = math.random(i)
        shuffled[i], shuffled[j] = shuffled[j], shuffled[i]
    end

    -- Take first count elements
    local result = {}
    for i = 1, count do
        table.insert(result, shuffled[i])
    end

    return result
end

return stress

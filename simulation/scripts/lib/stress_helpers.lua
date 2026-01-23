-- Stress Test Helper Library
--
-- Shared utility functions for stress test scenarios across all IndrasNetwork modules.
-- Provides standardized configuration, metrics tracking, and chaos injection.

local stress = {}

-- ============================================================================
-- STRESS LEVELS: quick/medium/full configurations
-- ============================================================================

stress.LEVELS = {
    quick = {
        name = "quick",
        multiplier = 0.1,      -- 10% of full scale
        default_peers = 10,
        default_ops = 100,
        default_ticks = 200,
        churn_rate = 0.1,
    },
    medium = {
        name = "medium",
        multiplier = 0.5,      -- 50% of full scale
        default_peers = 20,
        default_ops = 1000,
        default_ticks = 500,
        churn_rate = 0.2,
    },
    full = {
        name = "full",
        multiplier = 1.0,      -- 100% scale (max supported)
        default_peers = 26,    -- Simulation max A-Z
        default_ops = 10000,
        default_ticks = 2000,
        churn_rate = 0.3,
    }
}

--- Get the current stress level from environment
-- @return string The stress level (quick, medium, or full)
function stress.get_level()
    return os.getenv("STRESS_LEVEL") or "medium"
end

--- Get the configuration for current stress level
-- @return table The level configuration
function stress.get_config()
    local level = stress.get_level()
    return stress.LEVELS[level] or stress.LEVELS.medium
end

--- Scale a value based on stress level
-- @param base_value number The value at full stress level
-- @return number Scaled value for current level
function stress.scale(base_value)
    local cfg = stress.get_config()
    return math.max(1, math.floor(base_value * cfg.multiplier))
end

-- ============================================================================
-- RESULT BUILDER: Standardized result tracking
-- ============================================================================

--- Create a new result builder for a scenario
-- @param scenario_name string Name of the scenario
-- @return table Result builder object
function stress.result_builder(scenario_name)
    local builder = {
        scenario = scenario_name,
        level = stress.get_level(),
        started_at = os.time(),
        metrics = {},
        assertions = {},
        passed = true,
        errors = {}
    }

    --- Add a metric to the results
    function builder:add_metric(name, value)
        self.metrics[name] = value
        return self
    end

    --- Add multiple metrics
    function builder:add_metrics(metrics_table)
        for k, v in pairs(metrics_table) do
            self.metrics[k] = v
        end
        return self
    end

    --- Record an assertion result
    function builder:record_assertion(name, passed, expected, actual)
        table.insert(self.assertions, {
            name = name,
            passed = passed,
            expected = expected,
            actual = actual
        })
        if not passed then
            self.passed = false
            table.insert(self.errors, string.format(
                "Assertion '%s' failed: expected %s, got %s",
                name, tostring(expected), tostring(actual)
            ))
        end
        return self
    end

    --- Record an error
    function builder:add_error(msg)
        table.insert(self.errors, msg)
        self.passed = false
        return self
    end

    --- Build the final result table
    function builder:build()
        self.ended_at = os.time()
        self.duration_sec = self.ended_at - self.started_at

        return {
            scenario = self.scenario,
            level = self.level,
            passed = self.passed,
            duration_sec = self.duration_sec,
            metrics = self.metrics,
            assertions = self.assertions,
            errors = self.errors
        }
    end

    return builder
end

-- ============================================================================
-- LATENCY TRACKER: Collect samples and compute percentiles
-- ============================================================================

--- Create a latency tracker for collecting timing samples
-- @return table Latency tracker object
function stress.latency_tracker()
    local tracker = {
        samples = {},
        sum = 0,
        count = 0,
        min = math.huge,
        max = 0
    }

    --- Record a latency sample
    function tracker:record(latency_us)
        table.insert(self.samples, latency_us)
        self.sum = self.sum + latency_us
        self.count = self.count + 1
        self.min = math.min(self.min, latency_us)
        self.max = math.max(self.max, latency_us)
        return self
    end

    --- Calculate average latency
    function tracker:average()
        if self.count == 0 then return 0 end
        return self.sum / self.count
    end

    --- Calculate percentile (0-100)
    function tracker:percentile(p)
        if #self.samples == 0 then return 0 end

        -- Copy and sort
        local sorted = {}
        for _, v in ipairs(self.samples) do
            table.insert(sorted, v)
        end
        table.sort(sorted)

        local idx = math.ceil(#sorted * p / 100)
        return sorted[math.max(1, idx)]
    end

    --- Get common percentiles
    function tracker:percentiles()
        return {
            p50 = self:percentile(50),
            p95 = self:percentile(95),
            p99 = self:percentile(99)
        }
    end

    --- Get full statistics
    function tracker:stats()
        return {
            count = self.count,
            sum = self.sum,
            avg = self:average(),
            min = self.count > 0 and self.min or 0,
            max = self.max,
            p50 = self:percentile(50),
            p95 = self:percentile(95),
            p99 = self:percentile(99)
        }
    end

    return tracker
end

-- ============================================================================
-- THROUGHPUT CALCULATOR: Operations per second computation
-- ============================================================================

--- Create a throughput calculator
-- @return table Throughput calculator object
function stress.throughput_calculator()
    local calc = {
        operations = 0,
        start_tick = nil,
        end_tick = nil,
        duration_ticks = 0
    }

    --- Mark the start of measurement
    function calc:start(tick)
        self.start_tick = tick
        return self
    end

    --- Record an operation
    function calc:record(count)
        self.operations = self.operations + (count or 1)
        return self
    end

    --- Mark the end of measurement
    function calc:finish(tick)
        self.end_tick = tick
        self.duration_ticks = self.end_tick - (self.start_tick or tick)
        return self
    end

    --- Calculate ops per tick
    function calc:ops_per_tick()
        if self.duration_ticks == 0 then return 0 end
        return self.operations / self.duration_ticks
    end

    --- Get stats
    function calc:stats()
        return {
            operations = self.operations,
            duration_ticks = self.duration_ticks,
            ops_per_tick = self:ops_per_tick()
        }
    end

    return calc
end

-- ============================================================================
-- CHAOS INJECTION: Random peer kills and resurrections
-- ============================================================================

--- Create a chaos injector
-- @param sim Simulation The simulation instance
-- @param config table Optional configuration
-- @return table Chaos injector object
function stress.chaos_injection(sim, config)
    config = config or {}

    local chaos = {
        sim = sim,
        kills = 0,
        resurrections = 0,
        kill_interval = config.kill_interval or 20,
        wake_rate = config.wake_rate or 0.15,
        enabled = config.enabled ~= false
    }

    --- Kill a random online peer
    function chaos:kill_peer(tick)
        if not self.enabled then return false end
        if tick % self.kill_interval ~= 0 then return false end

        local online = self.sim:online_peers()
        if #online == 0 then return false end

        local victim = online[math.random(#online)]
        self.sim:force_offline(victim)
        self.kills = self.kills + 1
        return true, victim
    end

    --- Potentially resurrect an offline peer
    function chaos:resurrect_peer()
        if not self.enabled then return false end
        if math.random() >= self.wake_rate then return false end

        local offline = self.sim:offline_peers()
        if #offline == 0 then return false end

        local zombie = offline[math.random(#offline)]
        self.sim:force_online(zombie)
        self.resurrections = self.resurrections + 1
        return true, zombie
    end

    --- Run chaos for a tick
    function chaos:tick(tick)
        local killed, victim = self:kill_peer(tick)
        local resurrected, zombie = self:resurrect_peer()
        return {
            killed = killed,
            victim = victim,
            resurrected = resurrected,
            zombie = zombie
        }
    end

    --- Get chaos statistics
    function chaos:stats()
        return {
            kills = self.kills,
            resurrections = self.resurrections,
            enabled = self.enabled
        }
    end

    return chaos
end

-- ============================================================================
-- THRESHOLD VALIDATION: Assert metrics against thresholds
-- ============================================================================

--- Assert metrics against thresholds
-- @param metrics table The metrics to validate
-- @param thresholds table The threshold configuration
-- @return boolean passed, table failures
function stress.assert_thresholds(metrics, thresholds)
    local failures = {}

    for metric_name, threshold in pairs(thresholds) do
        local actual = metrics[metric_name]
        if actual ~= nil then
            local passed = true

            if threshold.min ~= nil and actual < threshold.min then
                passed = false
                table.insert(failures, {
                    metric = metric_name,
                    type = "min",
                    expected = threshold.min,
                    actual = actual
                })
            end

            if threshold.max ~= nil and actual > threshold.max then
                passed = false
                table.insert(failures, {
                    metric = metric_name,
                    type = "max",
                    expected = threshold.max,
                    actual = actual
                })
            end
        end
    end

    return #failures == 0, failures
end

-- ============================================================================
-- RANDOM HELPERS
-- ============================================================================

--- Get a random online peer
function stress.random_online_peer(sim)
    local online = sim:online_peers()
    if #online == 0 then return nil end
    return online[math.random(#online)]
end

--- Get a random offline peer
function stress.random_offline_peer(sim)
    local offline = sim:offline_peers()
    if #offline == 0 then return nil end
    return offline[math.random(#offline)]
end

--- Get random latency with variance
function stress.random_latency(base, variance)
    return math.floor(base + (math.random() - 0.5) * 2 * variance)
end

-- ============================================================================
-- TABLE UTILITIES (Lua standard library compatible)
-- ============================================================================

--- Get table keys (standard Lua compatible, no vim dependency)
-- @param tbl table The table to get keys from
-- @return table Array of keys
function stress.table_keys(tbl)
    local keys = {}
    for k, _ in pairs(tbl) do
        table.insert(keys, k)
    end
    return keys
end

--- Count table entries
-- @param tbl table The table to count
-- @return number Number of entries
function stress.table_count(tbl)
    local count = 0
    for _ in pairs(tbl) do
        count = count + 1
    end
    return count
end

--- Calculate average of values
-- @param values table Array of numeric values
-- @return number Average value
function stress.average(values)
    if #values == 0 then return 0 end
    local sum = 0
    for _, v in ipairs(values) do
        sum = sum + v
    end
    return sum / #values
end

-- ============================================================================
-- CONTEXT HELPERS
-- ============================================================================

--- Create a correlation context for a stress scenario
-- @param scenario_name string Name of the scenario
-- @return CorrelationContext
function stress.new_context(scenario_name)
    local ctx = indras.correlation.new_root()
    ctx = ctx:with_tag("scenario", scenario_name)
    ctx = ctx:with_tag("stress_level", stress.get_level())
    return ctx
end

--- Format latency for logging (us -> human readable)
-- @param latency_us number Latency in microseconds
-- @return string Formatted string
function stress.format_latency(latency_us)
    if latency_us < 1000 then
        return string.format("%dus", latency_us)
    elseif latency_us < 1000000 then
        return string.format("%.2fms", latency_us / 1000)
    else
        return string.format("%.2fs", latency_us / 1000000)
    end
end

return stress

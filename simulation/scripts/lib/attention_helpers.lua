-- Attention Tracking Simulation Helpers
--
-- Utility functions for attention tracking simulation scenarios.
-- Uses Rust SyncEngine bindings for attention document operations.
--
-- Key Concepts:
-- - Members focus on one quest at a time to "charge it up"
-- - Attention duration accumulates from switch events
-- - Quest ranking emerges from total attention time

local attention = {}

-- ============================================================================
-- STRESS LEVELS: Attention-specific configurations
-- ============================================================================

attention.LEVELS = {
    quick = {
        name = "quick",
        members = 5,
        quests = 10,
        switches_per_member = 20,
        ticks = 200,
        focus_duration_min = 100,   -- milliseconds
        focus_duration_max = 1000,
    },
    medium = {
        name = "medium",
        members = 10,
        quests = 50,
        switches_per_member = 100,
        ticks = 500,
        focus_duration_min = 100,
        focus_duration_max = 2000,
    },
    full = {
        name = "full",
        members = 20,
        quests = 200,
        switches_per_member = 500,
        ticks = 1000,
        focus_duration_min = 100,
        focus_duration_max = 5000,
    }
}

-- Event types for JSONL logging
attention.EVENTS = {
    ATTENTION_SWITCHED = "attention_switched",
    ATTENTION_CLEARED = "attention_cleared",
    ATTENTION_CALCULATED = "attention_calculated",
    RANKING_VERIFIED = "ranking_verified",
}

-- ============================================================================
-- CONFIGURATION HELPERS
-- ============================================================================

--- Get the current stress level from environment
-- @return string The stress level (quick, medium, or full)
function attention.get_level()
    return os.getenv("STRESS_LEVEL") or "medium"
end

--- Get the attention configuration for current stress level
-- @return table The level configuration
function attention.get_config()
    local level = attention.get_level()
    return attention.LEVELS[level] or attention.LEVELS.medium
end

-- ============================================================================
-- CONTEXT AND LOGGING
-- ============================================================================

--- Create a correlation context for an attention scenario
-- @param scenario_name string Name of the scenario
-- @return CorrelationContext
function attention.new_context(scenario_name)
    local ctx = indras.correlation.new_root()
    ctx = ctx:with_tag("scenario", scenario_name)
    ctx = ctx:with_tag("subsystem", "attention")
    ctx = ctx:with_tag("stress_level", attention.get_level())
    return ctx
end

--- Create a context logger with automatic trace_id
-- @param ctx CorrelationContext The correlation context
-- @return table Logger object
function attention.create_logger(ctx)
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

    --- Log an attention event with standard fields
    function logger.event(event_type, fields)
        fields = fields or {}
        fields.trace_id = ctx.trace_id
        fields.event_type = event_type
        indras.log.info(event_type, fields)
    end

    return logger
end

-- ============================================================================
-- ATTENTION TRACKER (For simulation verification)
-- ============================================================================

--- Create an attention tracker for simulation verification
-- @return table AttentionTracker object
function attention.AttentionTracker_new()
    local tracker = {
        -- events: { { member, quest_id, timestamp } }
        events = {},
        -- current_focus: member -> quest_id
        current_focus = {},
        -- Statistics
        switches = 0,
        clears = 0,
    }

    --- Record an attention switch
    -- @param member string Member ID
    -- @param quest_id string Quest ID (nil for clear)
    -- @param timestamp number Timestamp in milliseconds
    function tracker:record_switch(member, quest_id, timestamp)
        table.insert(self.events, {
            member = member,
            quest_id = quest_id,
            timestamp = timestamp or os.time() * 1000,
        })
        self.current_focus[member] = quest_id

        if quest_id then
            self.switches = self.switches + 1
        else
            self.clears = self.clears + 1
        end
    end

    --- Get current focus for a member
    -- @param member string Member ID
    -- @return string Quest ID or nil
    function tracker:get_focus(member)
        return self.current_focus[member]
    end

    --- Calculate attention for all quests
    -- @param as_of number Timestamp to calculate attention up to
    -- @return table quest_id -> total_millis
    function tracker:calculate_attention(as_of)
        as_of = as_of or os.time() * 1000

        -- Track windows: member -> { quest_id, start_time }
        local windows = {}
        -- quest_id -> total_millis
        local attention = {}

        -- Sort events by timestamp
        local sorted = {}
        for _, e in ipairs(self.events) do
            table.insert(sorted, e)
        end
        table.sort(sorted, function(a, b) return a.timestamp < b.timestamp end)

        for _, event in ipairs(sorted) do
            if event.timestamp > as_of then
                break
            end

            -- Close any open window
            local window = windows[event.member]
            if window then
                local duration = event.timestamp - window.start_time
                attention[window.quest_id] = (attention[window.quest_id] or 0) + duration
                windows[event.member] = nil
            end

            -- Open new window if focusing
            if event.quest_id then
                windows[event.member] = {
                    quest_id = event.quest_id,
                    start_time = event.timestamp,
                }
            end
        end

        -- Close open windows at as_of
        for member, window in pairs(windows) do
            local duration = as_of - window.start_time
            attention[window.quest_id] = (attention[window.quest_id] or 0) + duration
        end

        return attention
    end

    --- Get ranked quests by attention
    -- @param as_of number Timestamp
    -- @return table Array of { quest_id, attention_millis }
    function tracker:get_ranked_quests(as_of)
        local attention = self:calculate_attention(as_of)
        local ranked = {}

        for quest_id, millis in pairs(attention) do
            table.insert(ranked, { quest_id = quest_id, attention_millis = millis })
        end

        table.sort(ranked, function(a, b)
            return a.attention_millis > b.attention_millis
        end)

        return ranked
    end

    --- Get statistics
    -- @return table Statistics table
    function tracker:stats()
        return {
            total_events = #self.events,
            switches = self.switches,
            clears = self.clears,
            active_focuses = 0,  -- count non-nil current_focus
        }
    end

    return tracker
end

-- Alias for cleaner API
attention.AttentionTracker = { new = attention.AttentionTracker_new }

-- ============================================================================
-- LATENCY MODELS
-- ============================================================================

--- Simulate attention switch latency (100-500 microseconds)
-- @return number Latency in microseconds
function attention.switch_latency()
    return 100 + math.random(400)
end

--- Simulate attention calculation latency (depends on event count)
-- @param event_count number Number of events
-- @return number Latency in microseconds
function attention.calc_latency(event_count)
    -- Base latency + O(n log n) for sorting
    return 50 + math.floor(event_count * math.log(event_count + 1) * 0.5)
end

-- ============================================================================
-- STATISTICS HELPERS
-- ============================================================================

--- Calculate percentile from array of values
-- @param values table Array of numeric values
-- @param p number Percentile (0-100)
-- @return number The percentile value
function attention.percentile(values, p)
    if #values == 0 then return 0 end

    local sorted = {}
    for _, v in ipairs(values) do
        table.insert(sorted, v)
    end
    table.sort(sorted)

    local idx = math.ceil(#sorted * p / 100)
    return sorted[math.max(1, idx)]
end

--- Calculate multiple percentiles
-- @param values table Array of numeric values
-- @return table Table with p50, p95, p99 values
function attention.percentiles(values)
    return {
        p50 = attention.percentile(values, 50),
        p95 = attention.percentile(values, 95),
        p99 = attention.percentile(values, 99),
    }
end

--- Calculate average of values
-- @param values table Array of numeric values
-- @return number Average value
function attention.average(values)
    if #values == 0 then return 0 end
    local sum = 0
    for _, v in ipairs(values) do
        sum = sum + v
    end
    return sum / #values
end

-- ============================================================================
-- RESULT BUILDER
-- ============================================================================

--- Create a result builder for attention scenarios
-- @param scenario_name string Name of the scenario
-- @return table Result builder object
function attention.result_builder(scenario_name)
    local builder = {
        scenario = scenario_name,
        level = attention.get_level(),
        started_at = os.time(),
        metrics = {},
        assertions = {},
        passed = true,
        errors = {}
    }

    function builder:add_metric(name, value)
        self.metrics[name] = value
        return self
    end

    function builder:add_metrics(metrics_table)
        for k, v in pairs(metrics_table) do
            self.metrics[k] = v
        end
        return self
    end

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

    function builder:add_error(msg)
        table.insert(self.errors, msg)
        self.passed = false
        return self
    end

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

return attention

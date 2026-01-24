-- Logging Stress Test
--
-- Stress test for indras-logging module (structured logging, correlation context propagation).
-- Tests high-volume logging with various data types, correlation chains, and context propagation.

-- Configuration levels
local CONFIG = {
    quick = {
        log_events = 1000,
        correlation_chains = 10,
        ticks = 100
    },
    medium = {
        log_events = 10000,
        correlation_chains = 100,
        ticks = 300
    },
    full = {
        log_events = 100000,
        correlation_chains = 1000,
        ticks = 1000
    }
}

-- Select config level (default to quick)
local LEVEL = os.getenv("LOG_STRESS_LEVEL") or "quick"
local config = CONFIG[LEVEL] or CONFIG.quick

-- Test parameters
local LOG_EVENTS_TARGET = config.log_events
local CORRELATION_CHAINS_TARGET = config.correlation_chains
local SIMULATION_TICKS = config.ticks
local MAX_CHILD_DEPTH = 10  -- Max depth for nested child contexts

-- Create root correlation context for the scenario
local scenario_ctx = indras.correlation.new_root()
scenario_ctx = scenario_ctx:with_tag("scenario", "logging_stress")
scenario_ctx = scenario_ctx:with_tag("level", LEVEL)

indras.log.info("Starting logging stress test", {
    trace_id = scenario_ctx.trace_id,
    level = LEVEL,
    target_log_events = LOG_EVENTS_TARGET,
    target_correlation_chains = CORRELATION_CHAINS_TARGET,
    ticks = SIMULATION_TICKS,
    max_child_depth = MAX_CHILD_DEPTH
})

-- Metrics tracking
local metrics = {
    log_events_emitted = 0,
    correlation_chains_created = 0,
    child_contexts_created = 0,
    fields_logged = 0,

    -- Track by level
    trace_logs = 0,
    debug_logs = 0,
    info_logs = 0,
    warn_logs = 0,
    error_logs = 0,

    -- Track by phase
    phase1_events = 0,
    phase2_events = 0,
    phase3_events = 0,

    -- Track data types
    string_fields = 0,
    number_fields = 0,
    boolean_fields = 0,
    table_fields = 0,

    -- Correlation tracking
    max_chain_depth = 0,
    total_chain_depth = 0
}

-- Helper to emit a log event and track metrics
local function emit_log(level, message, fields)
    if fields then
        -- Count structured fields
        for k, v in pairs(fields) do
            metrics.fields_logged = metrics.fields_logged + 1

            local vtype = type(v)
            if vtype == "string" then
                metrics.string_fields = metrics.string_fields + 1
            elseif vtype == "number" then
                metrics.number_fields = metrics.number_fields + 1
            elseif vtype == "boolean" then
                metrics.boolean_fields = metrics.boolean_fields + 1
            elseif vtype == "table" then
                metrics.table_fields = metrics.table_fields + 1
            end
        end
    end

    -- Emit log at specified level
    if level == "trace" then
        indras.log.trace(message, fields)
        metrics.trace_logs = metrics.trace_logs + 1
    elseif level == "debug" then
        indras.log.debug(message, fields)
        metrics.debug_logs = metrics.debug_logs + 1
    elseif level == "info" then
        indras.log.info(message, fields)
        metrics.info_logs = metrics.info_logs + 1
    elseif level == "warn" then
        indras.log.warn(message, fields)
        metrics.warn_logs = metrics.warn_logs + 1
    elseif level == "error" then
        indras.log.error(message, fields)
        metrics.error_logs = metrics.error_logs + 1
    else
        indras.log.info(message, fields)
        metrics.info_logs = metrics.info_logs + 1
    end

    metrics.log_events_emitted = metrics.log_events_emitted + 1
end

-- Sample data for variety in logging
local sample_strings = {
    "packet_received", "routing_decision", "state_transition",
    "cache_hit", "cache_miss", "peer_connected", "peer_disconnected",
    "message_queued", "message_sent", "message_delivered"
}

local sample_reasons = {
    "ttl_expired", "no_route", "destination_unreachable",
    "congestion", "rate_limited", "validation_failed", "timeout"
}

local sample_peers = {
    "peer-A", "peer-B", "peer-C", "peer-D", "peer-E",
    "peer-F", "peer-G", "peer-H", "peer-I", "peer-J"
}

-- PHASE 1: High-volume logging at info level
indras.log.info("Phase 1: High-volume info logging", {
    trace_id = scenario_ctx.trace_id,
    target_events = math.floor(LOG_EVENTS_TARGET * 0.4)
})

local phase1_target = math.floor(LOG_EVENTS_TARGET * 0.4)
for i = 1, phase1_target do
    local event_type = sample_strings[math.random(#sample_strings)]
    local peer_id = sample_peers[math.random(#sample_peers)]

    emit_log("info", "High volume info event", {
        trace_id = scenario_ctx.trace_id,
        event_id = i,
        event_type = event_type,
        peer_id = peer_id,
        timestamp_us = i * 1000,
        sequence = i,
        batch = math.floor(i / 100) + 1
    })

    metrics.phase1_events = metrics.phase1_events + 1
end

indras.log.info("Phase 1 complete", {
    trace_id = scenario_ctx.trace_id,
    events_emitted = metrics.phase1_events,
    total_events = metrics.log_events_emitted
})

-- PHASE 2: Mixed log levels with various data types
indras.log.info("Phase 2: Mixed log levels", {
    trace_id = scenario_ctx.trace_id,
    target_events = math.floor(LOG_EVENTS_TARGET * 0.3)
})

local phase2_target = math.floor(LOG_EVENTS_TARGET * 0.3)
local log_levels = {"trace", "debug", "info", "warn", "error"}
local level_weights = {10, 20, 40, 20, 10}  -- Percentage distribution

for i = 1, phase2_target do
    -- Select level based on weighted distribution
    local rand = math.random(100)
    local cumulative = 0
    local selected_level = "info"

    for idx, weight in ipairs(level_weights) do
        cumulative = cumulative + weight
        if rand <= cumulative then
            selected_level = log_levels[idx]
            break
        end
    end

    -- Create diverse structured fields
    local fields = {
        trace_id = scenario_ctx.trace_id,
        event_id = phase1_target + i,
        level = selected_level,
        -- Various data types
        string_field = sample_strings[math.random(#sample_strings)],
        int_field = math.random(1000),
        float_field = math.random() * 100,
        bool_field = math.random() > 0.5,
        -- Nested table
        metadata = {
            source = sample_peers[math.random(#sample_peers)],
            destination = sample_peers[math.random(#sample_peers)],
            hop_count = math.random(10)
        },
        -- Array
        tags = {"stress_test", "phase2", string.format("batch_%d", math.floor(i / 50))}
    }

    emit_log(selected_level, "Mixed level event with diverse data types", fields)
    metrics.phase2_events = metrics.phase2_events + 1
end

indras.log.info("Phase 2 complete", {
    trace_id = scenario_ctx.trace_id,
    events_emitted = metrics.phase2_events,
    total_events = metrics.log_events_emitted,
    trace_count = metrics.trace_logs,
    debug_count = metrics.debug_logs,
    info_count = metrics.info_logs,
    warn_count = metrics.warn_logs,
    error_count = metrics.error_logs
})

-- PHASE 3: Correlation chain stress (deep nesting)
indras.log.info("Phase 3: Correlation chain stress", {
    trace_id = scenario_ctx.trace_id,
    target_chains = CORRELATION_CHAINS_TARGET,
    max_depth = MAX_CHILD_DEPTH
})

-- Helper to create correlation chain and log at each level
local function create_correlation_chain(chain_id, depth)
    local root_ctx = indras.correlation.new_root()
    root_ctx = root_ctx:with_tag("chain_id", tostring(chain_id))

    metrics.correlation_chains_created = metrics.correlation_chains_created + 1

    -- Log with root context
    emit_log("info", "Correlation chain root", {
        trace_id = root_ctx.trace_id,
        span_id = root_ctx.span_id,
        chain_id = chain_id,
        depth = 0,
        hop_count = root_ctx.hop_count
    })

    -- Create child contexts
    local current_ctx = root_ctx
    for level = 1, depth do
        local child_ctx = current_ctx:child()
        metrics.child_contexts_created = metrics.child_contexts_created + 1

        -- Log with child context
        emit_log("debug", "Correlation chain child", {
            trace_id = child_ctx.trace_id,
            span_id = child_ctx.span_id,
            parent_span_id = child_ctx.parent_span_id,
            chain_id = chain_id,
            depth = level,
            hop_count = child_ctx.hop_count
        })

        current_ctx = child_ctx
    end

    -- Track max depth
    if depth > metrics.max_chain_depth then
        metrics.max_chain_depth = depth
    end
    metrics.total_chain_depth = metrics.total_chain_depth + depth

    -- Verify trace_id consistency
    indras.assert.eq(current_ctx.trace_id, root_ctx.trace_id,
        "Child context should have same trace_id as root")

    return root_ctx, current_ctx
end

-- Create correlation chains with varying depths
for chain = 1, CORRELATION_CHAINS_TARGET do
    local depth = math.random(1, MAX_CHILD_DEPTH)
    local root, leaf = create_correlation_chain(chain, depth)

    -- Final log at leaf with correlation verification
    emit_log("info", "Correlation chain complete", {
        trace_id = leaf.trace_id,
        span_id = leaf.span_id,
        chain_id = chain,
        final_depth = depth,
        final_hop_count = leaf.hop_count,
        trace_matches = (leaf.trace_id == root.trace_id)
    })

    metrics.phase3_events = metrics.phase3_events + 1

    -- Progress logging for long runs
    if chain % 100 == 0 then
        indras.log.debug("Correlation chain progress", {
            trace_id = scenario_ctx.trace_id,
            chains_completed = chain,
            total_target = CORRELATION_CHAINS_TARGET
        })
    end
end

indras.log.info("Phase 3 complete", {
    trace_id = scenario_ctx.trace_id,
    chains_created = metrics.correlation_chains_created,
    child_contexts_created = metrics.child_contexts_created,
    max_depth = metrics.max_chain_depth,
    avg_depth = metrics.total_chain_depth / metrics.correlation_chains_created
})

-- PHASE 4: Simulation ticks with periodic logging
indras.log.info("Phase 4: Simulation ticks with periodic logging", {
    trace_id = scenario_ctx.trace_id,
    ticks = SIMULATION_TICKS
})

-- Create a minimal simulation for tick progression
local mesh = indras.MeshBuilder.new(5):full_mesh()
local sim_config = indras.SimConfig.new({
    max_ticks = SIMULATION_TICKS,
    wake_probability = 0.8,
    sleep_probability = 0.1
})
local sim = indras.Simulation.new(mesh, sim_config)
sim:initialize()

local phase4_events = 0
for tick = 1, SIMULATION_TICKS do
    sim:step()

    -- Periodic logging with throughput metrics
    if tick % 50 == 0 then
        local throughput = metrics.log_events_emitted / tick

        emit_log("info", "Tick checkpoint", {
            trace_id = scenario_ctx.trace_id,
            tick = tick,
            total_events = metrics.log_events_emitted,
            throughput_per_tick = throughput,
            phase = "phase4",
            online_peers = #sim:online_peers()
        })

        phase4_events = phase4_events + 1
    end

    -- Occasional correlation chain during simulation
    if tick % 100 == 0 then
        local ctx = indras.correlation.new_root()
        ctx = ctx:with_tag("tick", tostring(tick))

        emit_log("debug", "Tick correlation event", {
            trace_id = ctx.trace_id,
            span_id = ctx.span_id,
            tick = tick
        })

        phase4_events = phase4_events + 1
    end
end

indras.log.info("Phase 4 complete", {
    trace_id = scenario_ctx.trace_id,
    ticks = SIMULATION_TICKS,
    events_emitted = phase4_events
})

-- Calculate final metrics
local avg_chain_depth = 0
if metrics.correlation_chains_created > 0 then
    avg_chain_depth = metrics.total_chain_depth / metrics.correlation_chains_created
end

local avg_fields_per_event = 0
if metrics.log_events_emitted > 0 then
    avg_fields_per_event = metrics.fields_logged / metrics.log_events_emitted
end

local log_throughput = 0
if SIMULATION_TICKS > 0 then
    log_throughput = metrics.log_events_emitted / SIMULATION_TICKS
end

-- Final report
indras.log.info("Logging stress test complete", {
    trace_id = scenario_ctx.trace_id,
    level = LEVEL,
    -- Event metrics
    total_log_events = metrics.log_events_emitted,
    target_log_events = LOG_EVENTS_TARGET,
    -- Correlation metrics
    correlation_chains = metrics.correlation_chains_created,
    child_contexts = metrics.child_contexts_created,
    max_chain_depth = metrics.max_chain_depth,
    avg_chain_depth = avg_chain_depth,
    -- Level distribution
    trace_logs = metrics.trace_logs,
    debug_logs = metrics.debug_logs,
    info_logs = metrics.info_logs,
    warn_logs = metrics.warn_logs,
    error_logs = metrics.error_logs,
    -- Field metrics
    total_fields = metrics.fields_logged,
    avg_fields_per_event = avg_fields_per_event,
    string_fields = metrics.string_fields,
    number_fields = metrics.number_fields,
    boolean_fields = metrics.boolean_fields,
    table_fields = metrics.table_fields,
    -- Throughput
    log_throughput_per_tick = log_throughput,
    -- Phase breakdown
    phase1_events = metrics.phase1_events,
    phase2_events = metrics.phase2_events,
    phase3_events = metrics.phase3_events
})

-- Assertions to verify test goals
indras.assert.ge(metrics.log_events_emitted, LOG_EVENTS_TARGET * 0.9,
    "Should emit at least 90% of target log events")

indras.assert.ge(metrics.correlation_chains_created, CORRELATION_CHAINS_TARGET,
    "Should create all target correlation chains")

indras.assert.gt(metrics.child_contexts_created, 0,
    "Should create child contexts")

indras.assert.ge(metrics.max_chain_depth, 1,
    "Should have correlation chains with depth >= 1")

indras.assert.gt(metrics.fields_logged, 0,
    "Should log structured fields")

-- Verify log level distribution (should have used all levels)
indras.assert.gt(metrics.trace_logs, 0, "Should have trace logs")
indras.assert.gt(metrics.debug_logs, 0, "Should have debug logs")
indras.assert.gt(metrics.info_logs, 0, "Should have info logs")
indras.assert.gt(metrics.warn_logs, 0, "Should have warn logs")
indras.assert.gt(metrics.error_logs, 0, "Should have error logs")

-- Verify data type variety
indras.assert.gt(metrics.string_fields, 0, "Should log string fields")
indras.assert.gt(metrics.number_fields, 0, "Should log number fields")
indras.assert.gt(metrics.boolean_fields, 0, "Should log boolean fields")
indras.assert.gt(metrics.table_fields, 0, "Should log table fields")

indras.log.info("All assertions passed", {
    trace_id = scenario_ctx.trace_id
})

-- Return metrics for external analysis
return {
    level = LEVEL,
    log_events_emitted = metrics.log_events_emitted,
    correlation_chains_created = metrics.correlation_chains_created,
    child_contexts_created = metrics.child_contexts_created,
    log_throughput_per_tick = log_throughput,
    fields_logged = metrics.fields_logged,
    avg_fields_per_event = avg_fields_per_event,
    max_chain_depth = metrics.max_chain_depth,
    avg_chain_depth = avg_chain_depth,
    -- Level breakdown
    trace_logs = metrics.trace_logs,
    debug_logs = metrics.debug_logs,
    info_logs = metrics.info_logs,
    warn_logs = metrics.warn_logs,
    error_logs = metrics.error_logs,
    -- Data type breakdown
    string_fields = metrics.string_fields,
    number_fields = metrics.number_fields,
    boolean_fields = metrics.boolean_fields,
    table_fields = metrics.table_fields
}

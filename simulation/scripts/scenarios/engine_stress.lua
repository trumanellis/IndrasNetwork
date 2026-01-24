-- Engine Stress Test
--
-- Stress tests the simulation engine itself: tick rate, large topologies,
-- event log memory, and Lua binding overhead.
-- Tests raw simulation performance and scalability limits.

-- Configuration levels
local CONFIG = {
    quick = {
        peers = 20,
        ticks = 100,
        events_per_tick = 10,
    },
    medium = {
        peers = 20,
        ticks = 1000,
        events_per_tick = 50,
    },
    full = {
        peers = 26,
        ticks = 10000,
        events_per_tick = 200,
    }
}

-- Select configuration (default to quick)
local config_level = os.getenv("STRESS_LEVEL") or "quick"
local cfg = CONFIG[config_level] or CONFIG.quick

-- Create correlation context
local ctx = indras.correlation.new_root()
ctx = ctx:with_tag("scenario", "engine_stress")
ctx = ctx:with_tag("level", config_level)

indras.log.info("Starting engine stress test", {
    trace_id = ctx.trace_id,
    config_level = config_level,
    peers = cfg.peers,
    ticks = cfg.ticks,
    events_per_tick = cfg.events_per_tick
})

-- Create large mesh topology
indras.log.debug("Creating large mesh topology", {
    trace_id = ctx.trace_id,
    peers = cfg.peers
})

local mesh = indras.MeshBuilder.new(cfg.peers):random(0.3)

indras.log.info("Mesh topology created", {
    trace_id = ctx.trace_id,
    peers = mesh:peer_count(),
    edges = mesh:edge_count(),
    avg_degree = mesh:edge_count() / mesh:peer_count()
})

-- Create simulation with high wake probability for maximum activity
local sim_config = indras.SimConfig.new({
    wake_probability = 0.8,
    sleep_probability = 0.05,
    initial_online_probability = 0.95,
    max_ticks = cfg.ticks,
    trace_routing = true
})

local sim = indras.Simulation.new(mesh, sim_config)
sim:initialize()

local all_peers = mesh:peers()

-- Tracking metrics
local total_ticks = 0
local events_generated = 0
local lua_api_calls = 0
local mesh_operations = 0

-- Helper functions
local function random_online_peer()
    local online = sim:online_peers()
    lua_api_calls = lua_api_calls + 1
    if #online == 0 then return nil end
    return online[math.random(#online)]
end

local function random_peer()
    return all_peers[math.random(#all_peers)]
end

local function generate_events(count)
    local generated = 0
    for _ = 1, count do
        local sender = random_online_peer()
        local receiver = random_online_peer()

        if sender and receiver and sender ~= receiver then
            -- Send message (generates event)
            sim:send_message(sender, receiver, "stress_msg")
            lua_api_calls = lua_api_calls + 1
            generated = generated + 1

            -- PQ signature operation (generates event)
            local sign_latency = 200 + math.random(-50, 50)
            sim:record_pq_signature(sender, sign_latency, 256)
            lua_api_calls = lua_api_calls + 1
            generated = generated + 1

            -- PQ verification operation (generates event)
            local verify_latency = 150 + math.random(-30, 30)
            sim:record_pq_verification(receiver, sender, verify_latency, true)
            lua_api_calls = lua_api_calls + 1
            generated = generated + 1
        end
    end
    return generated
end

local function perform_mesh_operations()
    -- Query mesh structure (test Lua binding overhead)
    local ops = 0

    -- Peer lookups
    for _ = 1, 10 do
        local peer = random_peer()
        local _ = mesh:neighbors(peer)
        ops = ops + 1
        lua_api_calls = lua_api_calls + 1
    end

    -- Online/offline queries
    local _ = sim:online_peers()
    ops = ops + 1
    lua_api_calls = lua_api_calls + 1

    local _ = sim:offline_peers()
    ops = ops + 1
    lua_api_calls = lua_api_calls + 1

    -- Peer count queries
    local _ = mesh:peer_count()
    ops = ops + 1
    lua_api_calls = lua_api_calls + 1

    local _ = mesh:edge_count()
    ops = ops + 1
    lua_api_calls = lua_api_calls + 1

    return ops
end

-- Phase 1: Warmup (establish baseline tick rate)
indras.log.info("Phase 1: Warmup", {
    trace_id = ctx.trace_id,
    warmup_ticks = math.floor(cfg.ticks * 0.1)
})

local warmup_ticks = math.floor(cfg.ticks * 0.1)
local warmup_start = os.clock()

for tick = 1, warmup_ticks do
    -- Light event generation during warmup
    local generated = generate_events(math.floor(cfg.events_per_tick * 0.2))
    events_generated = events_generated + generated

    -- Advance simulation
    sim:step()
    lua_api_calls = lua_api_calls + 1
    total_ticks = total_ticks + 1

    -- Mesh operations
    mesh_operations = mesh_operations + perform_mesh_operations()
end

local warmup_elapsed = os.clock() - warmup_start
local warmup_tps = warmup_ticks / warmup_elapsed

indras.log.info("Phase 1 complete", {
    trace_id = ctx.trace_id,
    ticks = warmup_ticks,
    elapsed_sec = warmup_elapsed,
    ticks_per_second = warmup_tps,
    events_generated = events_generated
})

-- Phase 2: Stress (maximum event generation)
indras.log.info("Phase 2: Stress", {
    trace_id = ctx.trace_id,
    stress_ticks = math.floor(cfg.ticks * 0.6),
    events_per_tick = cfg.events_per_tick
})

local stress_ticks = math.floor(cfg.ticks * 0.6)
local stress_start = os.clock()
local stress_start_events = events_generated

for tick = 1, stress_ticks do
    -- Maximum event generation
    local generated = generate_events(cfg.events_per_tick)
    events_generated = events_generated + generated

    -- Advance simulation
    sim:step()
    lua_api_calls = lua_api_calls + 1
    total_ticks = total_ticks + 1

    -- Frequent mesh operations
    mesh_operations = mesh_operations + perform_mesh_operations()

    -- Progress logging (every 10%)
    if tick % math.max(1, math.floor(stress_ticks / 10)) == 0 then
        local elapsed = os.clock() - stress_start
        local current_tps = tick / elapsed
        local stats = sim.stats

        -- Check event log size
        local event_log = sim:event_log()
        lua_api_calls = lua_api_calls + 1

        indras.log.info("Stress progress", {
            trace_id = ctx.trace_id,
            tick = tick,
            total_tick = total_ticks,
            progress_pct = math.floor(tick / stress_ticks * 100),
            ticks_per_second = current_tps,
            events_generated = events_generated,
            event_log_size = #event_log,
            online_peers = #sim:online_peers(),
            messages_delivered = stats.messages_delivered
        })
    end
end

local stress_elapsed = os.clock() - stress_start
local stress_tps = stress_ticks / stress_elapsed
local stress_events = events_generated - stress_start_events

indras.log.info("Phase 2 complete", {
    trace_id = ctx.trace_id,
    ticks = stress_ticks,
    elapsed_sec = stress_elapsed,
    ticks_per_second = stress_tps,
    events_generated = stress_events,
    events_per_second = stress_events / stress_elapsed
})

-- Phase 3: Measurement (clean tick rate measurement)
indras.log.info("Phase 3: Measurement", {
    trace_id = ctx.trace_id,
    measurement_ticks = math.floor(cfg.ticks * 0.3)
})

local measurement_ticks = math.floor(cfg.ticks * 0.3)
local measurement_start = os.clock()
local measurement_start_events = events_generated

for tick = 1, measurement_ticks do
    -- Moderate event generation for clean measurement
    local generated = generate_events(math.floor(cfg.events_per_tick * 0.5))
    events_generated = events_generated + generated

    -- Advance simulation
    sim:step()
    lua_api_calls = lua_api_calls + 1
    total_ticks = total_ticks + 1

    -- Mesh operations
    mesh_operations = mesh_operations + perform_mesh_operations()
end

local measurement_elapsed = os.clock() - measurement_start
local measurement_tps = measurement_ticks / measurement_elapsed
local measurement_events = events_generated - measurement_start_events

indras.log.info("Phase 3 complete", {
    trace_id = ctx.trace_id,
    ticks = measurement_ticks,
    elapsed_sec = measurement_elapsed,
    ticks_per_second = measurement_tps,
    events_generated = measurement_events
})

-- Final statistics
local total_elapsed = os.clock() - warmup_start
local overall_tps = total_ticks / total_elapsed
local stats = sim.stats
local event_log = sim:event_log()
lua_api_calls = lua_api_calls + 1

-- Memory/performance metrics
local event_log_size = #event_log
local events_per_tick_actual = events_generated / total_ticks
local lua_api_calls_per_tick = lua_api_calls / total_ticks
local mesh_ops_per_tick = mesh_operations / total_ticks

indras.log.info("Engine stress test completed", {
    trace_id = ctx.trace_id,
    config_level = config_level,
    -- Test parameters
    total_peers = cfg.peers,
    target_ticks = cfg.ticks,
    target_events_per_tick = cfg.events_per_tick,
    -- Execution metrics
    total_ticks = total_ticks,
    total_elapsed_sec = total_elapsed,
    ticks_per_second = overall_tps,
    -- Phase metrics
    warmup_tps = warmup_tps,
    stress_tps = stress_tps,
    measurement_tps = measurement_tps,
    -- Event metrics
    events_generated = events_generated,
    events_per_tick_actual = events_per_tick_actual,
    events_per_second = events_generated / total_elapsed,
    event_log_size = event_log_size,
    -- Lua binding metrics
    lua_api_calls = lua_api_calls,
    lua_api_calls_per_tick = lua_api_calls_per_tick,
    mesh_operations = mesh_operations,
    mesh_ops_per_tick = mesh_ops_per_tick,
    -- Simulation stats
    messages_sent = stats.messages_sent,
    messages_delivered = stats.messages_delivered,
    delivery_rate = stats:delivery_rate(),
    signatures_created = stats.pq_signatures_created,
    signatures_verified = stats.pq_signatures_verified,
    avg_signature_latency_us = stats:avg_signature_latency_us(),
    avg_verification_latency_us = stats:avg_verification_latency_us()
})

-- Assertions
indras.assert.eq(total_ticks, cfg.ticks, "Should have run all ticks")
indras.assert.gt(events_generated, 0, "Should have generated events")
indras.assert.gt(overall_tps, 0, "Should have positive tick rate")
indras.assert.gt(lua_api_calls, 0, "Should have made Lua API calls")
indras.assert.gt(mesh_operations, 0, "Should have performed mesh operations")

-- Performance expectations based on config level
if config_level == "quick" then
    indras.assert.gt(overall_tps, 10, "Quick mode should achieve > 10 TPS")
elseif config_level == "medium" then
    indras.assert.gt(overall_tps, 5, "Medium mode should achieve > 5 TPS")
elseif config_level == "full" then
    indras.assert.gt(overall_tps, 1, "Full mode should achieve > 1 TPS")
end

indras.log.info("Engine stress test passed", {
    trace_id = ctx.trace_id,
    ticks_per_second = overall_tps,
    events_generated = events_generated,
    lua_api_calls = lua_api_calls,
    event_log_size = event_log_size
})

-- Return metrics
return {
    -- Configuration
    config_level = config_level,
    peers = cfg.peers,
    target_ticks = cfg.ticks,
    target_events_per_tick = cfg.events_per_tick,
    -- Performance metrics
    total_ticks = total_ticks,
    ticks_per_second = overall_tps,
    total_elapsed_sec = total_elapsed,
    -- Phase breakdown
    warmup_tps = warmup_tps,
    stress_tps = stress_tps,
    measurement_tps = measurement_tps,
    -- Event metrics
    events_generated = events_generated,
    events_per_tick_actual = events_per_tick_actual,
    events_per_second = events_generated / total_elapsed,
    event_log_size = event_log_size,
    -- Lua binding overhead
    lua_api_calls = lua_api_calls,
    lua_api_calls_per_tick = lua_api_calls_per_tick,
    mesh_operations = mesh_operations,
    mesh_ops_per_tick = mesh_ops_per_tick,
    -- Network metrics
    delivery_rate = stats:delivery_rate(),
    avg_latency = stats:average_latency(),
    avg_hops = stats:average_hops()
}

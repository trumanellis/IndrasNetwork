-- Core Stress Test
--
-- Stress tests the indras-core module (foundation traits, PeerId, packets, events, priorities).
-- Tests core type operations: PeerId creation/comparison, message/packet operations,
-- event generation, and priority handling.

local ctx = indras.correlation.new_root()
ctx = ctx:with_tag("scenario", "core_stress")

-- Configuration levels
local CONFIG = {
    quick = {
        peer_ops = 500,
        packet_ops = 500,
        event_ops = 1000,
        ticks = 100
    },
    medium = {
        peer_ops = 5000,
        packet_ops = 5000,
        event_ops = 10000,
        ticks = 300
    },
    full = {
        peer_ops = 50000,
        packet_ops = 50000,
        event_ops = 100000,
        ticks = 1000
    }
}

-- Default to quick if no environment variable set
local level = os.getenv("STRESS_LEVEL") or "quick"
local config = CONFIG[level] or CONFIG.quick

indras.log.info("Starting core stress test", {
    trace_id = ctx.trace_id,
    level = level,
    peer_ops = config.peer_ops,
    packet_ops = config.packet_ops,
    event_ops = config.event_ops,
    ticks = config.ticks
})

-- Tracking metrics
local metrics = {
    peer_id_ops = 0,
    packet_ops = 0,
    event_count = 0,
    priority_ops = 0,
    error_count = 0,
    phase_times = {}
}

-- Priority levels to test
local PRIORITIES = {"low", "normal", "high", "critical"}

-- Helper: random priority
local function random_priority()
    return PRIORITIES[math.random(#PRIORITIES)]
end

-- Helper: generate random payload of specified size
local function random_payload(size)
    local chars = {}
    for i = 1, size do
        chars[i] = string.char(math.random(32, 126))
    end
    return table.concat(chars)
end

-- ============================================================
-- Phase 1: PeerId Operations
-- ============================================================
indras.log.info("Phase 1: PeerId operations", {
    trace_id = ctx.trace_id,
    target_ops = config.peer_ops
})

local phase1_start = os.clock()

-- Create a pool of PeerIds
local peer_pool = {}
for i = 1, 26 do
    local char = string.char(64 + i) -- A-Z
    local peer = indras.PeerId.new(char)
    table.insert(peer_pool, peer)
end

-- Test PeerId creation
for i = 1, math.floor(config.peer_ops / 3) do
    local char_code = 65 + (i % 26) -- A-Z
    local char = string.char(char_code)
    local peer = indras.PeerId.new(char)
    metrics.peer_id_ops = metrics.peer_id_ops + 1

    -- Verify creation
    if tostring(peer) ~= char then
        metrics.error_count = metrics.error_count + 1
        indras.log.error("PeerId creation failed", {
            trace_id = ctx.trace_id,
            expected = char,
            got = tostring(peer)
        })
    end
end

-- Test PeerId.range_to()
for i = 1, math.floor(config.peer_ops / 3) do
    local end_char = string.char(65 + (i % 26))
    local range = indras.PeerId.range_to(end_char)
    metrics.peer_id_ops = metrics.peer_id_ops + 1

    -- Verify range size
    local expected_size = (string.byte(end_char) - 64)
    if #range ~= expected_size then
        metrics.error_count = metrics.error_count + 1
        indras.log.error("PeerId range_to failed", {
            trace_id = ctx.trace_id,
            end_char = end_char,
            expected_size = expected_size,
            got = #range
        })
    end
end

-- Test PeerId comparisons
for i = 1, math.floor(config.peer_ops / 3) do
    local peer1 = peer_pool[math.random(#peer_pool)]
    local peer2 = peer_pool[math.random(#peer_pool)]

    -- Test equality
    local eq = (peer1 == peer2)
    -- Test ordering
    local lt = (peer1 < peer2)
    local le = (peer1 <= peer2)

    metrics.peer_id_ops = metrics.peer_id_ops + 3

    -- Verify consistency
    if peer1 == peer2 and (peer1 < peer2 or peer2 < peer1) then
        metrics.error_count = metrics.error_count + 1
        indras.log.error("PeerId comparison inconsistency", {
            trace_id = ctx.trace_id,
            peer1 = tostring(peer1),
            peer2 = tostring(peer2)
        })
    end
end

local phase1_time = os.clock() - phase1_start
metrics.phase_times.phase1 = phase1_time

indras.log.info("Phase 1 completed", {
    trace_id = ctx.trace_id,
    peer_id_ops = metrics.peer_id_ops,
    errors = metrics.error_count,
    duration_sec = phase1_time
})

-- ============================================================
-- Phase 2: Packet/Message Operations
-- ============================================================
indras.log.info("Phase 2: Packet/message operations", {
    trace_id = ctx.trace_id,
    target_ops = config.packet_ops
})

local phase2_start = os.clock()

-- Create a small mesh for message operations
local MESH_SIZE = math.min(10, #peer_pool)
local mesh = indras.MeshBuilder.new(MESH_SIZE):random(0.5)

local sim_config = indras.SimConfig.new({
    wake_probability = 0.2,
    sleep_probability = 0.1,
    initial_online_probability = 0.8,
    max_ticks = config.ticks,
    trace_routing = false -- Disable for performance
})

local sim = indras.Simulation.new(mesh, sim_config)
sim:initialize()

local mesh_peers = mesh:peers()

-- Payload sizes to test (bytes)
local PAYLOAD_SIZES = {10, 50, 100, 500, 1000, 5000}

-- Generate packet operations
for i = 1, config.packet_ops do
    local sender = mesh_peers[math.random(#mesh_peers)]
    local receiver = mesh_peers[math.random(#mesh_peers)]

    if sender ~= receiver then
        -- Random payload size
        local payload_size = PAYLOAD_SIZES[math.random(#PAYLOAD_SIZES)]
        local payload = random_payload(payload_size)

        -- Send message (this creates a packet internally)
        sim:send_message(sender, receiver, payload)
        metrics.packet_ops = metrics.packet_ops + 1

        -- Assign priority (conceptually - Lua bindings track this)
        local priority = random_priority()
        metrics.priority_ops = metrics.priority_ops + 1
    end

    -- Progress logging
    if i % 1000 == 0 then
        indras.log.debug("Packet ops progress", {
            trace_id = ctx.trace_id,
            completed = i,
            target = config.packet_ops
        })
    end
end

local phase2_time = os.clock() - phase2_start
metrics.phase_times.phase2 = phase2_time

indras.log.info("Phase 2 completed", {
    trace_id = ctx.trace_id,
    packet_ops = metrics.packet_ops,
    priority_ops = metrics.priority_ops,
    errors = metrics.error_count,
    duration_sec = phase2_time
})

-- ============================================================
-- Phase 3: Event Throughput
-- ============================================================
indras.log.info("Phase 3: Event throughput", {
    trace_id = ctx.trace_id,
    target_events = config.event_ops,
    ticks = config.ticks
})

local phase3_start = os.clock()

-- High-rate message sending to generate events
local messages_per_tick = math.ceil(config.event_ops / config.ticks)

for tick = 1, config.ticks do
    -- Generate high volume of messages to create events
    for _ = 1, messages_per_tick do
        local sender = mesh_peers[math.random(#mesh_peers)]
        local receiver = mesh_peers[math.random(#mesh_peers)]

        if sender ~= receiver then
            -- Various payload sizes
            local payload_size = PAYLOAD_SIZES[math.random(#PAYLOAD_SIZES)]
            local payload = random_payload(payload_size)

            sim:send_message(sender, receiver, payload)
            metrics.event_count = metrics.event_count + 1
        end
    end

    -- Step simulation (generates events: Send, Relay, Delivered, etc.)
    sim:step()

    -- Progress logging
    if tick % 100 == 0 then
        local stats = sim.stats
        indras.log.debug("Event generation progress", {
            trace_id = ctx.trace_id,
            tick = tick,
            events_generated = metrics.event_count,
            messages_sent = stats.messages_sent,
            messages_delivered = stats.messages_delivered,
            online_peers = #sim:online_peers()
        })
    end
end

local phase3_time = os.clock() - phase3_start
metrics.phase_times.phase3 = phase3_time

indras.log.info("Phase 3 completed", {
    trace_id = ctx.trace_id,
    event_count = metrics.event_count,
    errors = metrics.error_count,
    duration_sec = phase3_time
})

-- ============================================================
-- Priority Testing
-- ============================================================
indras.log.info("Testing priority levels", {
    trace_id = ctx.trace_id
})

-- Create Priority instances and verify
local priority_checks = {
    low = indras.Priority.low(),
    normal = indras.Priority.normal(),
    high = indras.Priority.high(),
    critical = indras.Priority.critical()
}

for name, priority in pairs(priority_checks) do
    if tostring(priority) ~= name then
        metrics.error_count = metrics.error_count + 1
        indras.log.error("Priority creation failed", {
            trace_id = ctx.trace_id,
            expected = name,
            got = tostring(priority)
        })
    end
    metrics.priority_ops = metrics.priority_ops + 1
end

-- ============================================================
-- Final Statistics
-- ============================================================
local total_time = phase1_time + phase2_time + phase3_time
local total_ops = metrics.peer_id_ops + metrics.packet_ops + metrics.event_count

local final_stats = sim.stats

-- Calculate throughput
local ops_per_tick = 0
if config.ticks > 0 then
    ops_per_tick = metrics.event_count / config.ticks
end

indras.log.info("Core stress test completed", {
    trace_id = ctx.trace_id,
    level = level,
    -- Operation counts
    peer_id_ops = metrics.peer_id_ops,
    packet_ops = metrics.packet_ops,
    event_count = metrics.event_count,
    priority_ops = metrics.priority_ops,
    total_ops = total_ops,
    error_count = metrics.error_count,
    -- Timing
    phase1_time_sec = phase1_time,
    phase2_time_sec = phase2_time,
    phase3_time_sec = phase3_time,
    total_time_sec = total_time,
    -- Throughput
    ops_per_tick = ops_per_tick,
    ops_per_second = total_ops / total_time,
    -- Simulation stats
    messages_sent = final_stats.messages_sent,
    messages_delivered = final_stats.messages_delivered,
    messages_dropped = final_stats.messages_dropped,
    delivery_rate = final_stats:delivery_rate(),
    avg_latency = final_stats:average_latency(),
    avg_hops = final_stats:average_hops()
})

-- ============================================================
-- Assertions
-- ============================================================
indras.assert.eq(metrics.error_count, 0, "Should have zero errors in core operations")
indras.assert.gt(metrics.peer_id_ops, 0, "Should have completed PeerId operations")
indras.assert.gt(metrics.packet_ops, 0, "Should have completed packet operations")
indras.assert.gt(metrics.event_count, 0, "Should have generated events")
indras.assert.gt(metrics.priority_ops, 0, "Should have tested priority operations")

-- Verify operation counts meet minimum thresholds
local peer_threshold = config.peer_ops * 0.85 -- Allow 15% variance due to randomness
local packet_threshold = config.packet_ops * 0.85
local event_threshold = config.event_ops * 0.85

indras.assert.ge(metrics.peer_id_ops, peer_threshold,
    string.format("PeerId ops should meet threshold (expected >= %d, got %d)", peer_threshold, metrics.peer_id_ops))
indras.assert.ge(metrics.packet_ops, packet_threshold,
    string.format("Packet ops should meet threshold (expected >= %d, got %d)", packet_threshold, metrics.packet_ops))
indras.assert.ge(metrics.event_count, event_threshold,
    string.format("Event ops should meet threshold (expected >= %d, got %d)", event_threshold, metrics.event_count))

-- Verify all priorities were tested
indras.assert.ge(metrics.priority_ops, 4, "Should have tested all priority levels")

-- Verify simulation delivered some messages
indras.assert.gt(final_stats.messages_delivered, 0, "Should have delivered some messages")
indras.assert.gt(final_stats:delivery_rate(), 0.0, "Delivery rate should be > 0")

indras.log.info("Core stress test passed all assertions", {
    trace_id = ctx.trace_id,
    total_ops = total_ops,
    ops_per_second = total_ops / total_time,
    zero_errors = true
})

-- Return metrics table
return {
    level = level,
    peer_id_ops = metrics.peer_id_ops,
    packet_ops = metrics.packet_ops,
    event_count = metrics.event_count,
    priority_ops = metrics.priority_ops,
    error_count = metrics.error_count,
    ops_per_tick = ops_per_tick,
    ops_per_second = total_ops / total_time,
    total_time_sec = total_time,
    delivery_rate = final_stats:delivery_rate(),
    phase_times = metrics.phase_times
}

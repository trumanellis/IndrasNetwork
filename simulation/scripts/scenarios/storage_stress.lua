-- Storage Stress Test
--
-- Stress tests the indras-storage module via high-volume event generation,
-- pending delivery tracking, and message expiration (simulated quota eviction).
--
-- Since the simulation abstracts storage, we simulate storage behavior through:
-- - High-volume message sending (generates event log entries)
-- - Tracking pending messages (messages sent but not delivered)
-- - Message expiration (simulates quota eviction)

local ctx = indras.correlation.new_root()
ctx = ctx:with_tag("scenario", "storage_stress")

-- Configuration levels
local CONFIG = {
    quick = {
        peer_count = 10,
        target_events = 1000,
        max_ticks = 200,
        burst_multiplier = 5,
    },
    medium = {
        peer_count = 20,
        target_events = 10000,
        max_ticks = 500,
        burst_multiplier = 10,
    },
    full = {
        peer_count = 26,
        target_events = 100000,
        max_ticks = 2000,
        burst_multiplier = 20,
    }
}

-- Select test level (default: quick)
local test_level = os.getenv("STRESS_LEVEL") or "quick"
local config = CONFIG[test_level] or CONFIG.quick

indras.log.info("Starting storage stress test", {
    trace_id = ctx.trace_id,
    level = test_level,
    peer_count = config.peer_count,
    target_events = config.target_events,
    max_ticks = config.max_ticks,
    burst_multiplier = config.burst_multiplier
})

-- Create mesh topology (sparse random for realistic distribution)
local mesh = indras.MeshBuilder.new(config.peer_count):random(0.3)

indras.log.debug("Created storage stress mesh", {
    trace_id = ctx.trace_id,
    peers = mesh:peer_count(),
    edges = mesh:edge_count()
})

-- Create simulation with moderate network dynamics
local sim_config = indras.SimConfig.new({
    wake_probability = 0.1,
    sleep_probability = 0.05,
    initial_online_probability = 0.8,
    max_ticks = config.max_ticks,
    trace_routing = false  -- Reduce overhead for high-volume testing
})

local sim = indras.Simulation.new(mesh, sim_config)
sim:initialize()

local all_peers = mesh:peers()

-- Storage stress tracking
local metrics = {
    event_count = 0,
    phase_events = {0, 0, 0},
    pending_queue_sizes = {},  -- Track pending queue size over time
    pending_queue_max = 0,
    eviction_count = 0,
    tick_event_counts = {},  -- Events per tick for rate calculation
}

-- Pending message tracking (simulates pending delivery queue)
local pending_messages = {}
local message_ttl = 50  -- Message expires after 50 ticks (simulates quota eviction)
local next_msg_id = 1

-- Helper functions
local function random_online_peer()
    local online = sim:online_peers()
    if #online == 0 then return nil end
    return online[math.random(#online)]
end

local function get_random_pair()
    local sender = random_online_peer()
    local receiver = random_online_peer()
    if sender and receiver and sender ~= receiver then
        return sender, receiver
    end
    return nil, nil
end

-- Track pending messages (simulates storage pending delivery tracking)
local function send_tracked_message(sender, receiver, tick)
    local msg_id = next_msg_id
    next_msg_id = next_msg_id + 1

    sim:send_message(sender, receiver, string.format("storage_msg_%d", msg_id))

    -- Track as pending
    pending_messages[msg_id] = {
        sender = sender,
        receiver = receiver,
        created_tick = tick,
        expires_tick = tick + message_ttl
    }

    metrics.event_count = metrics.event_count + 1
    return msg_id
end

-- Check for message delivery and expiration
local function update_pending_queue(tick)
    local delivered_count = 0
    local expired_count = 0

    for msg_id, msg in pairs(pending_messages) do
        -- Simulate delivery check (in real storage, this would be checking delivery status)
        -- For simulation, assume ~80% delivery rate per message age
        local age = tick - msg.created_tick
        if age > 2 and math.random() < 0.2 then
            -- Message delivered
            pending_messages[msg_id] = nil
            delivered_count = delivered_count + 1
        elseif tick >= msg.expires_tick then
            -- Message expired (quota eviction)
            pending_messages[msg_id] = nil
            expired_count = expired_count + 1
            metrics.eviction_count = metrics.eviction_count + 1
        end
    end

    return delivered_count, expired_count
end

-- Get current pending queue size
local function get_pending_size()
    local count = 0
    for _ in pairs(pending_messages) do
        count = count + 1
    end
    return count
end

-- Generate steady load of events
local function steady_event_generation(msgs_per_tick)
    local sent = 0
    for _ = 1, msgs_per_tick do
        local sender, receiver = get_random_pair()
        if sender and receiver then
            send_tracked_message(sender, receiver, sim.tick)
            sent = sent + 1
        end
    end
    return sent
end

-- Calculate events per tick based on target
local base_rate = math.ceil(config.target_events / (config.max_ticks * 0.8))  -- 80% time for steady phase

-- Phase definitions
local phase1_end = math.floor(config.max_ticks * 0.5)    -- 50% steady
local phase2_end = math.floor(config.max_ticks * 0.75)   -- 25% burst
local phase3_end = config.max_ticks                      -- 25% drain

indras.log.info("Running storage stress simulation", {
    trace_id = ctx.trace_id,
    base_rate = base_rate,
    phase1_end = phase1_end,
    phase2_end = phase2_end,
    phase3_end = phase3_end
})

-- Main simulation loop
for tick = 1, config.max_ticks do
    local phase = 1
    local sent = 0

    if tick <= phase1_end then
        -- Phase 1: Steady event generation
        phase = 1
        sent = steady_event_generation(base_rate)
    elseif tick <= phase2_end then
        -- Phase 2: Burst mode (high rate)
        phase = 2
        sent = steady_event_generation(base_rate * config.burst_multiplier)
    else
        -- Phase 3: Drain (reduced rate, let pending queue drain)
        phase = 3
        sent = steady_event_generation(math.max(1, math.floor(base_rate * 0.2)))
    end

    metrics.phase_events[phase] = metrics.phase_events[phase] + sent
    metrics.tick_event_counts[tick] = sent

    -- Update pending message tracking
    local delivered, expired = update_pending_queue(tick)
    local pending_size = get_pending_size()
    table.insert(metrics.pending_queue_sizes, pending_size)

    if pending_size > metrics.pending_queue_max then
        metrics.pending_queue_max = pending_size
    end

    -- Advance simulation
    sim:step()

    -- Progress logging
    if tick % 100 == 0 or tick == phase1_end or tick == phase2_end then
        local stats = sim.stats
        indras.log.info("Storage stress progress", {
            trace_id = ctx.trace_id,
            tick = tick,
            phase = phase,
            online_count = #sim:online_peers(),
            events_generated = metrics.event_count,
            pending_queue_size = pending_size,
            pending_queue_max = metrics.pending_queue_max,
            evictions = metrics.eviction_count,
            messages_sent = stats.messages_sent,
            messages_delivered = stats.messages_delivered,
            messages_dropped = stats.messages_dropped
        })
    end
end

-- Final statistics
local stats = sim.stats

-- Calculate derived metrics
local function calculate_append_rate()
    local total = 0
    for _, count in ipairs(metrics.tick_event_counts) do
        total = total + count
    end
    return total / #metrics.tick_event_counts
end

local function calculate_delivery_throughput()
    if config.max_ticks == 0 then return 0 end
    return stats.messages_delivered / config.max_ticks
end

local avg_append_rate = calculate_append_rate()
local delivery_throughput = calculate_delivery_throughput()

-- Calculate average pending queue size
local avg_pending = 0
if #metrics.pending_queue_sizes > 0 then
    local sum = 0
    for _, size in ipairs(metrics.pending_queue_sizes) do
        sum = sum + size
    end
    avg_pending = sum / #metrics.pending_queue_sizes
end

indras.log.info("Storage stress test completed", {
    trace_id = ctx.trace_id,
    final_tick = sim.tick,
    -- Storage metrics
    total_events = metrics.event_count,
    phase1_events = metrics.phase_events[1],
    phase2_events = metrics.phase_events[2],
    phase3_events = metrics.phase_events[3],
    append_rate = avg_append_rate,
    pending_queue_max = metrics.pending_queue_max,
    avg_pending_queue = avg_pending,
    eviction_count = metrics.eviction_count,
    eviction_rate = metrics.event_count > 0 and (metrics.eviction_count / metrics.event_count) or 0,
    delivery_throughput = delivery_throughput,
    -- Network metrics
    messages_sent = stats.messages_sent,
    messages_delivered = stats.messages_delivered,
    messages_dropped = stats.messages_dropped,
    delivery_rate = stats:delivery_rate(),
    average_latency = stats:average_latency(),
    average_hops = stats:average_hops()
})

-- Assertions
indras.assert.gt(metrics.event_count, 0, "Should have generated events")
indras.assert.ge(metrics.event_count, config.target_events * 0.9,
    "Should approach target event count (within 10%)")

-- Verify burst phase generated more than steady phase
indras.assert.gt(metrics.phase_events[2], metrics.phase_events[1],
    "Burst phase should generate more events than steady phase")

-- Verify pending queue was utilized
indras.assert.gt(metrics.pending_queue_max, 0, "Should have pending messages")

-- Verify some evictions occurred (storage quota behavior)
if config.target_events > 1000 then
    indras.assert.gt(metrics.eviction_count, 0, "Should have some message evictions")
end

-- Network delivery rate should be reasonable
indras.assert.gt(stats:delivery_rate(), 0.0, "Should deliver some messages")

indras.log.info("Storage stress test passed", {
    trace_id = ctx.trace_id,
    event_count = metrics.event_count,
    append_rate = avg_append_rate,
    pending_queue_max = metrics.pending_queue_max,
    eviction_rate = metrics.event_count > 0 and (metrics.eviction_count / metrics.event_count) or 0,
    delivery_rate = stats:delivery_rate()
})

-- Return metrics for external analysis
return {
    level = test_level,
    -- Storage metrics
    event_count = metrics.event_count,
    append_rate = avg_append_rate,
    pending_queue_max = metrics.pending_queue_max,
    avg_pending_queue = avg_pending,
    eviction_count = metrics.eviction_count,
    eviction_rate = metrics.event_count > 0 and (metrics.eviction_count / metrics.event_count) or 0,
    delivery_throughput = delivery_throughput,
    -- Phase breakdown
    phase1_events = metrics.phase_events[1],
    phase2_events = metrics.phase_events[2],
    phase3_events = metrics.phase_events[3],
    -- Network metrics
    delivery_rate = stats:delivery_rate(),
    messages_delivered = stats.messages_delivered,
    messages_dropped = stats.messages_dropped,
    average_latency = stats:average_latency(),
    average_hops = stats:average_hops()
}

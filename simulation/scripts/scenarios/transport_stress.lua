-- Transport Stress Test
--
-- Stress tests the indras-transport module by simulating connection establishment,
-- peer discovery, and connection churn. Since the simulation doesn't directly expose
-- the transport layer, we simulate transport behavior through network topology changes
-- and peer state transitions.
--
-- Simulates:
-- - Rapid peer online/offline transitions (QUIC handshakes)
-- - Message delivery during connection churn
-- - Peer discovery via mesh neighbor queries
--
-- This scenario validates transport resilience under high connection churn.

local ctx = indras.correlation.new_root()
ctx = ctx:with_tag("scenario", "transport_stress")

-- Configuration levels
local CONFIG = {
    quick = {
        peer_count = 10,
        connection_cycles = 100,
        max_ticks = 200,
        churn_rate = 0.2,
        message_rate = 3
    },
    medium = {
        peer_count = 20,
        connection_cycles = 500,
        max_ticks = 500,
        churn_rate = 0.3,
        message_rate = 5
    },
    full = {
        peer_count = 26,
        connection_cycles = 2000,
        max_ticks = 1000,
        churn_rate = 0.4,
        message_rate = 10
    }
}

-- Select configuration level (default: medium)
local level = os.getenv("STRESS_LEVEL") or "medium"
local config = CONFIG[level] or CONFIG.medium

indras.log.info("Starting transport stress test", {
    trace_id = ctx.trace_id,
    level = level,
    peer_count = config.peer_count,
    connection_cycles = config.connection_cycles,
    max_ticks = config.max_ticks,
    churn_rate = config.churn_rate,
    message_rate = config.message_rate
})

-- Create random mesh topology (simulates peer discovery)
local mesh = indras.MeshBuilder.new(config.peer_count):random(0.3)

indras.log.debug("Created mesh topology", {
    trace_id = ctx.trace_id,
    peers = mesh:peer_count(),
    edges = mesh:edge_count()
})

-- Create simulation
local sim_config = indras.SimConfig.new({
    wake_probability = 0.05,
    sleep_probability = 0.05,
    initial_online_probability = 0.8,
    max_ticks = config.max_ticks,
    trace_routing = false  -- Disabled for performance under high load
})

local sim = indras.Simulation.new(mesh, sim_config)
sim:initialize()

local all_peers = mesh:peers()

-- Transport simulation tracking
local connection_cycles = 0
local successful_deliveries_during_churn = 0
local discovery_queries = 0
local online_durations = {}
local connection_failures = 0
local phase_transitions = 0

-- Peer state tracking for connection cycle detection
local peer_states = {}
for _, peer in ipairs(all_peers) do
    peer_states[tostring(peer)] = {
        online = sim:is_online(peer),
        online_since = 0,
        cycle_count = 0
    }
end

-- Helper functions
local function random_online_peer()
    local online = sim:online_peers()
    if #online == 0 then return nil end
    return online[math.random(#online)]
end

local function random_offline_peer()
    local offline = sim:offline_peers()
    if #offline == 0 then return nil end
    return offline[math.random(#offline)]
end

local function simulate_discovery()
    -- Simulate peer discovery by querying mesh neighbors
    local peer = random_online_peer()
    if peer then
        local neighbors = mesh:neighbors(peer)
        discovery_queries = discovery_queries + 1
        return #neighbors
    end
    return 0
end

local function track_connection_state(tick)
    -- Track online/offline transitions and durations
    for _, peer in ipairs(all_peers) do
        local peer_id = tostring(peer)
        local state = peer_states[peer_id]
        local currently_online = sim:is_online(peer)

        if currently_online ~= state.online then
            -- State transition detected
            if state.online then
                -- Was online, now offline (disconnect)
                local duration = tick - state.online_since
                table.insert(online_durations, duration)
            else
                -- Was offline, now online (connect)
                state.cycle_count = state.cycle_count + 1
                connection_cycles = connection_cycles + 1
                state.online_since = tick
            end
            state.online = currently_online
        end
    end
end

local function simulate_connection_churn(tick, churn_intensity)
    -- Simulate transport-layer connection churn
    -- Higher churn_intensity = more aggressive connect/disconnect

    -- Random disconnections (simulates connection failures, timeouts)
    if math.random() < churn_intensity then
        local victim = random_online_peer()
        if victim then
            sim:force_offline(victim)
            connection_failures = connection_failures + 1
        end
    end

    -- Random reconnections (simulates connection establishment)
    if math.random() < churn_intensity * 1.5 then
        local peer = random_offline_peer()
        if peer then
            sim:force_online(peer)
        end
    end
end

local function send_messages_during_churn()
    -- Test message delivery while connections are churning
    for _ = 1, config.message_rate do
        local sender = random_online_peer()
        local receiver = random_online_peer()

        if sender and receiver and sender ~= receiver then
            -- Track if message is sent during high churn
            local message_id = string.format("transport-%d", sim.tick)
            sim:send_message(sender, receiver, message_id)
        end
    end
end

-- Phase 1: Stable connections, baseline messaging
indras.log.info("Phase 1: Stable baseline", {
    trace_id = ctx.trace_id,
    phase = 1,
    description = "Establishing stable connections and baseline message delivery"
})
phase_transitions = phase_transitions + 1

local phase1_end = math.floor(config.max_ticks * 0.3)
for tick = 1, phase1_end do
    -- Minimal churn, focus on discovery and baseline delivery
    if tick % 10 == 0 then
        simulate_discovery()
    end

    send_messages_during_churn()
    track_connection_state(tick)

    sim:step()

    if tick % 50 == 0 then
        indras.log.debug("Phase 1 progress", {
            trace_id = ctx.trace_id,
            tick = tick,
            online_count = #sim:online_peers(),
            connection_cycles = connection_cycles,
            discovery_queries = discovery_queries
        })
    end
end

local phase1_stats = sim.stats
indras.log.info("Phase 1 complete", {
    trace_id = ctx.trace_id,
    phase = 1,
    ticks = phase1_end,
    connection_cycles = connection_cycles,
    messages_delivered = phase1_stats.messages_delivered,
    delivery_rate = phase1_stats:delivery_rate()
})

-- Phase 2: High churn, rapid connect/disconnect
indras.log.info("Phase 2: High churn stress", {
    trace_id = ctx.trace_id,
    phase = 2,
    description = "Simulating aggressive connection churn and transport stress"
})
phase_transitions = phase_transitions + 1

local phase2_start = phase1_end + 1
local phase2_end = math.floor(config.max_ticks * 0.7)
local baseline_delivered = phase1_stats.messages_delivered

for tick = phase2_start, phase2_end do
    -- Aggressive churn
    simulate_connection_churn(tick, config.churn_rate)

    -- Continue discovery attempts
    if tick % 5 == 0 then
        simulate_discovery()
    end

    -- Send messages during churn to test delivery resilience
    send_messages_during_churn()
    track_connection_state(tick)

    sim:step()

    if tick % 100 == 0 then
        local current_stats = sim.stats
        local churn_deliveries = current_stats.messages_delivered - baseline_delivered
        indras.log.debug("Phase 2 progress", {
            trace_id = ctx.trace_id,
            tick = tick,
            online_count = #sim:online_peers(),
            connection_cycles = connection_cycles,
            connection_failures = connection_failures,
            deliveries_during_churn = churn_deliveries
        })
    end
end

local phase2_stats = sim.stats
successful_deliveries_during_churn = phase2_stats.messages_delivered - baseline_delivered

indras.log.info("Phase 2 complete", {
    trace_id = ctx.trace_id,
    phase = 2,
    ticks = phase2_end - phase2_start + 1,
    connection_cycles = connection_cycles,
    connection_failures = connection_failures,
    deliveries_during_churn = successful_deliveries_during_churn,
    delivery_rate = phase2_stats:delivery_rate()
})

-- Phase 3: Recovery and stabilization
indras.log.info("Phase 3: Recovery", {
    trace_id = ctx.trace_id,
    phase = 3,
    description = "Network stabilization and recovery verification"
})
phase_transitions = phase_transitions + 1

local phase3_start = phase2_end + 1

-- Bring all peers online for recovery
for _, peer in ipairs(all_peers) do
    if not sim:is_online(peer) then
        sim:force_online(peer)
    end
end

for tick = phase3_start, config.max_ticks do
    -- Minimal churn, allow stabilization
    simulate_connection_churn(tick, config.churn_rate * 0.2)

    -- Continue discovery
    if tick % 8 == 0 then
        simulate_discovery()
    end

    send_messages_during_churn()
    track_connection_state(tick)

    sim:step()

    if tick % 50 == 0 then
        indras.log.debug("Phase 3 progress", {
            trace_id = ctx.trace_id,
            tick = tick,
            online_count = #sim:online_peers(),
            connection_cycles = connection_cycles
        })
    end
end

-- Calculate metrics
local final_stats = sim.stats

local function average(values)
    if #values == 0 then return 0 end
    local sum = 0
    for _, v in ipairs(values) do
        sum = sum + v
    end
    return sum / #values
end

local avg_online_duration = average(online_durations)
local connection_failure_rate = 0
if connection_cycles > 0 then
    connection_failure_rate = connection_failures / connection_cycles
end

-- Final results
indras.log.info("Transport stress test completed", {
    trace_id = ctx.trace_id,
    final_tick = sim.tick,
    level = level,
    -- Connection metrics
    connection_cycles = connection_cycles,
    target_cycles = config.connection_cycles,
    connection_failures = connection_failures,
    connection_failure_rate = connection_failure_rate,
    avg_online_duration_ticks = math.floor(avg_online_duration),
    -- Discovery metrics
    discovery_queries = discovery_queries,
    -- Delivery metrics
    total_messages_sent = final_stats.messages_sent,
    total_messages_delivered = final_stats.messages_delivered,
    deliveries_during_churn = successful_deliveries_during_churn,
    messages_dropped = final_stats.messages_dropped,
    delivery_rate = final_stats:delivery_rate(),
    -- Network metrics
    average_latency = final_stats:average_latency(),
    average_hops = final_stats:average_hops(),
    -- Phase tracking
    phase_transitions = phase_transitions
})

-- Assertions
indras.assert.gt(connection_cycles, 0, "Should have connection cycles")
indras.assert.gt(discovery_queries, 0, "Should have performed discovery queries")
indras.assert.gt(final_stats.messages_delivered, 0, "Should have delivered messages")

-- Connection cycles should meet or approach target (allowing for some variance)
local cycle_achievement_rate = connection_cycles / config.connection_cycles
indras.assert.gt(cycle_achievement_rate, 0.5,
    string.format("Should achieve at least 50%% of target connection cycles (got %.1f%%)",
        cycle_achievement_rate * 100))

-- Delivery rate should remain reasonable even under churn
indras.assert.gt(final_stats:delivery_rate(), 0.3,
    "Should maintain >30% delivery rate under transport stress")

-- Should have successful deliveries during high churn phase
indras.assert.gt(successful_deliveries_during_churn, 0,
    "Should deliver messages even during high connection churn")

indras.log.info("Transport stress test passed", {
    trace_id = ctx.trace_id,
    connection_cycles = connection_cycles,
    cycle_achievement_rate = cycle_achievement_rate,
    delivery_rate = final_stats:delivery_rate(),
    deliveries_during_churn = successful_deliveries_during_churn
})

return {
    level = level,
    connection_cycles = connection_cycles,
    connection_failures = connection_failures,
    connection_failure_rate = connection_failure_rate,
    avg_online_duration_ticks = math.floor(avg_online_duration),
    discovery_queries = discovery_queries,
    successful_deliveries_during_churn = successful_deliveries_during_churn,
    delivery_rate = final_stats:delivery_rate(),
    messages_delivered = final_stats.messages_delivered,
    messages_dropped = final_stats.messages_dropped,
    average_latency = final_stats:average_latency(),
    average_hops = final_stats:average_hops(),
    phase_transitions = phase_transitions
}

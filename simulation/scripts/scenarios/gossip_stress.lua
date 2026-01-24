-- Gossip Stress Test
--
-- Stress tests indras-gossip module with topic-based pub/sub and message dissemination.
-- Simulates broadcast-style messaging where one sender's message reaches all connected peers.

local ctx = indras.correlation.new_root()
ctx = ctx:with_tag("scenario", "gossip_stress")

-- Configuration levels
local CONFIG = {
    quick = {
        peer_count = 10,
        topic_count = 2,
        broadcast_count = 50,
        max_ticks = 150
    },
    medium = {
        peer_count = 20,
        topic_count = 10,
        broadcast_count = 200,
        max_ticks = 400
    },
    full = {
        peer_count = 26,
        topic_count = 50,
        broadcast_count = 1000,
        max_ticks = 1000
    }
}

-- Select config level (default: quick)
local level = os.getenv("STRESS_LEVEL") or "quick"
local config = CONFIG[level] or CONFIG.quick

indras.log.info("Starting gossip stress test", {
    trace_id = ctx.trace_id,
    level = level,
    peer_count = config.peer_count,
    topic_count = config.topic_count,
    broadcast_count = config.broadcast_count,
    max_ticks = config.max_ticks
})

-- PQ signature latency parameters (microseconds)
local SIGN_LATENCY_BASE = 200
local SIGN_LATENCY_VARIANCE = 80
local VERIFY_LATENCY_BASE = 150
local VERIFY_LATENCY_VARIANCE = 50

-- Helper: random latency with variance
local function random_latency(base, variance)
    return math.floor(base + (math.random() - 0.5) * 2 * variance)
end

-- Create mesh topology with good connectivity for gossip
local mesh = indras.MeshBuilder.new(config.peer_count):random(0.5)

indras.log.debug("Created gossip mesh", {
    trace_id = ctx.trace_id,
    peers = mesh:peer_count(),
    edges = mesh:edge_count(),
    avg_neighbors = mesh:edge_count() * 2 / mesh:peer_count()
})

-- Create simulation
local sim_config = indras.SimConfig.new({
    wake_probability = 0.1,
    sleep_probability = 0.05,
    initial_online_probability = 0.9,
    max_ticks = config.max_ticks,
    trace_routing = true
})

local sim = indras.Simulation.new(mesh, sim_config)
sim:initialize()

-- Get all peers
local all_peers = mesh:peers()

-- Topic management
local topics = {}
for i = 1, config.topic_count do
    topics[i] = string.format("topic-%d", i)
end

-- Gossip tracking structures
local broadcast_id_counter = 0
local broadcast_tracking = {}  -- [broadcast_id] = { sender, topic, sent_tick, receivers = {}, signature_size }
local peer_message_counts = {}  -- [peer_id] = count
local topic_message_counts = {}  -- [topic] = count
local duplicate_count = 0
local total_messages_received = 0

-- Initialize per-peer tracking
for _, peer in ipairs(all_peers) do
    peer_message_counts[tostring(peer)] = 0
end

-- Helper: get random online peer
local function random_online_peer()
    local online = sim:online_peers()
    if #online == 0 then return nil end
    return online[math.random(#online)]
end

-- Helper: broadcast message to all neighbors
local function broadcast_message(sender, topic, broadcast_id)
    local neighbors = mesh:neighbors(sender)
    local signature_size = 256  -- ML-DSA-65 signature size (approx)

    -- Sign the broadcast message (PQ signature)
    local sign_latency = random_latency(SIGN_LATENCY_BASE, SIGN_LATENCY_VARIANCE)
    sim:record_pq_signature(sender, sign_latency, signature_size)

    -- Initialize tracking for this broadcast
    broadcast_tracking[broadcast_id] = {
        sender = tostring(sender),
        topic = topic,
        sent_tick = sim.tick,
        receivers = {},
        signature_size = signature_size
    }

    -- Send to all neighbors
    local sent_count = 0
    for _, neighbor in ipairs(neighbors) do
        if sim:is_online(neighbor) then
            sim:send_message(sender, neighbor, "gossip:" .. topic .. ":" .. broadcast_id)
            sent_count = sent_count + 1
        end
    end

    return sent_count
end

-- Helper: process received gossip message (with relay)
local function process_gossip_message(receiver, msg_type, broadcast_id)
    local tracking = broadcast_tracking[broadcast_id]
    if not tracking then
        return  -- Unknown broadcast
    end

    local receiver_id = tostring(receiver)

    -- Check if already received (duplicate detection)
    if tracking.receivers[receiver_id] then
        duplicate_count = duplicate_count + 1
        return
    end

    -- Mark as received
    tracking.receivers[receiver_id] = sim.tick
    total_messages_received = total_messages_received + 1
    peer_message_counts[receiver_id] = peer_message_counts[receiver_id] + 1

    -- Count by topic
    topic_message_counts[tracking.topic] = (topic_message_counts[tracking.topic] or 0) + 1

    -- Verify signature (PQ verification)
    local verify_latency = random_latency(VERIFY_LATENCY_BASE, VERIFY_LATENCY_VARIANCE)
    -- In real gossip, we verify the original sender's signature
    local sender_peer = all_peers[1]  -- Placeholder, would be tracked properly
    sim:record_pq_verification(receiver, sender_peer, verify_latency, true)

    -- Relay to neighbors (gossip propagation)
    local neighbors = mesh:neighbors(receiver)
    for _, neighbor in ipairs(neighbors) do
        if sim:is_online(neighbor) and not tracking.receivers[tostring(neighbor)] then
            sim:send_message(receiver, neighbor, msg_type)
        end
    end
end

-- Message handler hook
sim:set_message_handler(function(sender, receiver, msg_type)
    -- Parse gossip messages
    local gossip_prefix = "gossip:"
    if msg_type:sub(1, #gossip_prefix) == gossip_prefix then
        local parts = {}
        for part in msg_type:gmatch("[^:]+") do
            table.insert(parts, part)
        end

        if #parts == 3 then
            local topic = parts[2]
            local broadcast_id = tonumber(parts[3])
            if broadcast_id then
                process_gossip_message(receiver, msg_type, broadcast_id)
            end
        end
    end
end)

-- Phase tracking
local broadcasts_sent = 0
local phase_1_broadcasts = math.floor(config.broadcast_count * 0.3)  -- 30%
local phase_2_broadcasts = math.floor(config.broadcast_count * 0.5)  -- 50%
local phase_3_broadcasts = config.broadcast_count - phase_1_broadcasts - phase_2_broadcasts  -- 20%

local current_phase = 1
local phase_broadcasts_done = 0

indras.log.info("Phase breakdown", {
    trace_id = ctx.trace_id,
    phase_1 = phase_1_broadcasts,
    phase_2 = phase_2_broadcasts,
    phase_3 = phase_3_broadcasts
})

-- Run simulation with three phases
indras.log.info("Starting gossip simulation", {
    trace_id = ctx.trace_id,
    ticks = config.max_ticks
})

for tick = 1, config.max_ticks do
    -- Determine current phase
    if broadcasts_sent >= phase_1_broadcasts and current_phase == 1 then
        current_phase = 2
        phase_broadcasts_done = 0
        indras.log.info("Entering Phase 2: Multiple topics, high rate", {
            trace_id = ctx.trace_id,
            tick = tick
        })
    elseif broadcasts_sent >= (phase_1_broadcasts + phase_2_broadcasts) and current_phase == 2 then
        current_phase = 3
        phase_broadcasts_done = 0
        indras.log.info("Entering Phase 3: Burst broadcasts", {
            trace_id = ctx.trace_id,
            tick = tick
        })
    end

    -- Phase 1: Single topic, low rate (1 broadcast every 2 ticks)
    if current_phase == 1 and tick % 2 == 0 and broadcasts_sent < phase_1_broadcasts then
        local sender = random_online_peer()
        if sender then
            broadcast_id_counter = broadcast_id_counter + 1
            local topic = topics[1]  -- Single topic
            broadcast_message(sender, topic, broadcast_id_counter)
            broadcasts_sent = broadcasts_sent + 1
            phase_broadcasts_done = phase_broadcasts_done + 1
        end

    -- Phase 2: Multiple topics, high rate (2 broadcasts per tick)
    elseif current_phase == 2 and broadcasts_sent < (phase_1_broadcasts + phase_2_broadcasts) then
        for _ = 1, 2 do
            if broadcasts_sent >= (phase_1_broadcasts + phase_2_broadcasts) then
                break
            end

            local sender = random_online_peer()
            if sender then
                broadcast_id_counter = broadcast_id_counter + 1
                local topic = topics[math.random(#topics)]  -- Random topic
                broadcast_message(sender, topic, broadcast_id_counter)
                broadcasts_sent = broadcasts_sent + 1
                phase_broadcasts_done = phase_broadcasts_done + 1
            end
        end

    -- Phase 3: Burst broadcasts (5 broadcasts per tick)
    elseif current_phase == 3 and broadcasts_sent < config.broadcast_count then
        for _ = 1, 5 do
            if broadcasts_sent >= config.broadcast_count then
                break
            end

            local sender = random_online_peer()
            if sender then
                broadcast_id_counter = broadcast_id_counter + 1
                local topic = topics[math.random(#topics)]  -- Random topic
                broadcast_message(sender, topic, broadcast_id_counter)
                broadcasts_sent = broadcasts_sent + 1
                phase_broadcasts_done = phase_broadcasts_done + 1
            end
        end
    end

    -- Advance simulation
    sim:step()

    -- Progress logging
    if tick % 100 == 0 or (tick % 50 == 0 and config.peer_count <= 50) then
        local stats = sim.stats
        indras.log.info("Gossip progress checkpoint", {
            trace_id = ctx.trace_id,
            tick = tick,
            phase = current_phase,
            broadcasts_sent = broadcasts_sent,
            messages_received = total_messages_received,
            duplicates = duplicate_count,
            signatures_created = stats.pq_signatures_created,
            signatures_verified = stats.pq_signatures_verified,
            messages_delivered = stats.messages_delivered,
            online_peers = #sim:online_peers()
        })
    end
end

-- Calculate final metrics
local stats = sim.stats

-- Dissemination metrics
local dissemination_latencies = {}
local fully_propagated_count = 0
local total_expected_receivers = 0

for broadcast_id, tracking in pairs(broadcast_tracking) do
    local receiver_count = 0
    local max_latency = 0

    for receiver_id, received_tick in pairs(tracking.receivers) do
        receiver_count = receiver_count + 1
        local latency = received_tick - tracking.sent_tick
        table.insert(dissemination_latencies, latency)
        if latency > max_latency then
            max_latency = latency
        end
    end

    -- Expected: all online peers except sender should receive
    local expected = #sim:online_peers() - 1
    total_expected_receivers = total_expected_receivers + math.max(1, expected)

    -- Count as fully propagated if reached >90% of expected
    if expected > 0 and receiver_count >= expected * 0.9 then
        fully_propagated_count = fully_propagated_count + 1
    end
end

-- Calculate average dissemination latency
local dissemination_latency_avg = 0
if #dissemination_latencies > 0 then
    local sum = 0
    for _, latency in ipairs(dissemination_latencies) do
        sum = sum + latency
    end
    dissemination_latency_avg = sum / #dissemination_latencies
end

-- Calculate duplication rate
local duplication_rate = 0
if total_messages_received > 0 then
    duplication_rate = duplicate_count / (total_messages_received + duplicate_count)
end

-- Calculate delivery rate
local delivery_rate = 0
if total_expected_receivers > 0 then
    delivery_rate = total_messages_received / total_expected_receivers
end

indras.log.info("Gossip stress test completed", {
    trace_id = ctx.trace_id,
    final_tick = sim.tick,
    -- Broadcast metrics
    broadcasts_sent = broadcasts_sent,
    messages_received = total_messages_received,
    duplicates = duplicate_count,
    -- Dissemination metrics
    dissemination_latency_avg = dissemination_latency_avg,
    duplication_rate = duplication_rate,
    delivery_rate = delivery_rate,
    fully_propagated_count = fully_propagated_count,
    fully_propagated_rate = fully_propagated_count / math.max(1, broadcasts_sent),
    -- PQ signature metrics
    signatures_created = stats.pq_signatures_created,
    signatures_verified = stats.pq_signatures_verified,
    signature_failures = stats.pq_signature_failures,
    avg_sign_latency_us = stats:avg_signature_latency_us(),
    avg_verify_latency_us = stats:avg_verification_latency_us(),
    -- Network metrics
    messages_sent = stats.messages_sent,
    messages_delivered = stats.messages_delivered,
    messages_dropped = stats.messages_dropped,
    network_delivery_rate = stats:delivery_rate(),
    average_latency = stats:average_latency(),
    average_hops = stats:average_hops()
})

-- Assertions
indras.assert.eq(broadcasts_sent, config.broadcast_count, "Should send all configured broadcasts")
indras.assert.gt(total_messages_received, 0, "Should receive gossip messages")
indras.assert.gt(stats.pq_signatures_created, 0, "Should create signatures for broadcasts")
indras.assert.gt(stats.pq_signatures_verified, 0, "Should verify signatures on receipt")

-- Delivery rate should be high (>70% for well-connected mesh)
indras.assert.gt(delivery_rate, 0.7, "Should have good delivery rate in gossip network")

-- Dissemination latency should be reasonable (< 20 ticks on average for well-connected mesh)
if config.peer_count <= 50 then
    indras.assert.lt(dissemination_latency_avg, 20, "Dissemination latency should be low for small networks")
end

indras.log.info("Gossip stress test passed", {
    trace_id = ctx.trace_id,
    broadcasts_sent = broadcasts_sent,
    delivery_rate = delivery_rate,
    dissemination_latency_avg = dissemination_latency_avg,
    duplication_rate = duplication_rate
})

-- Return metrics table
return {
    broadcasts_sent = broadcasts_sent,
    messages_received = total_messages_received,
    dissemination_latency_avg = dissemination_latency_avg,
    duplication_rate = duplication_rate,
    delivery_rate = delivery_rate,
    fully_propagated_rate = fully_propagated_count / math.max(1, broadcasts_sent),
    -- Additional metrics
    signatures_per_broadcast = stats.pq_signatures_created / math.max(1, broadcasts_sent),
    avg_sign_latency_us = stats:avg_signature_latency_us(),
    avg_verify_latency_us = stats:avg_verification_latency_us(),
    network_delivery_rate = stats:delivery_rate(),
    average_hops = stats:average_hops()
}

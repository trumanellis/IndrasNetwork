-- Routing Stress Test
--
-- Stress tests the indras-routing module: store-and-forward routing, mutual peer tracking, back-propagation.
-- Tests routing resilience under network churn with chaos injection.

local ctx = indras.correlation.new_root()
ctx = ctx:with_tag("scenario", "routing_stress")

-- Configuration levels
local CONFIG = {
    quick = {
        peers = 10,
        messages = 100,
        ticks = 200,
        churn_rate = 0.1
    },
    medium = {
        peers = 20,
        messages = 1000,
        ticks = 500,
        churn_rate = 0.2
    },
    full = {
        peers = 26,
        messages = 10000,
        ticks = 2000,
        churn_rate = 0.3
    }
}

-- Select configuration level (default: medium)
local level = os.getenv("STRESS_LEVEL") or "medium"
local cfg = CONFIG[level]

if not cfg then
    error("Invalid configuration level: " .. level .. ". Valid levels: quick, medium, full")
end

-- Test parameters
local PEER_COUNT = cfg.peers
local TOTAL_MESSAGES = cfg.messages
local SIMULATION_TICKS = cfg.ticks
local CHURN_RATE = cfg.churn_rate
local EDGE_PROBABILITY = 0.4
local KILL_INTERVAL = 20           -- ticks between random peer kills
local RANDOM_WAKE_RATE = 0.15      -- 15% chance to wake a dead peer per tick
local DRAIN_PHASE_TICKS = 100      -- ticks to drain messages after main phase

-- Latency parameters (microseconds) for signed messages
local SIGN_LATENCY_BASE = 200
local SIGN_LATENCY_VARIANCE = 100
local VERIFY_LATENCY_BASE = 150
local VERIFY_LATENCY_VARIANCE = 50

indras.log.info("Starting routing stress test", {
    trace_id = ctx.trace_id,
    level = level,
    peers = PEER_COUNT,
    total_messages = TOTAL_MESSAGES,
    duration = SIMULATION_TICKS,
    churn_rate = CHURN_RATE,
    edge_probability = EDGE_PROBABILITY
})

-- Create random mesh topology
local mesh = indras.MeshBuilder.new(PEER_COUNT):random(EDGE_PROBABILITY)

indras.log.debug("Created random mesh", {
    trace_id = ctx.trace_id,
    peers = mesh:peer_count(),
    edges = mesh:edge_count()
})

-- Create simulation
local config = indras.SimConfig.new({
    wake_probability = 0.15,
    sleep_probability = 0.1,
    initial_online_probability = 0.7,
    max_ticks = SIMULATION_TICKS + DRAIN_PHASE_TICKS,
    trace_routing = true
})

local sim = indras.Simulation.new(mesh, config)
sim:initialize()

-- Get all peers
local all_peers = mesh:peers()

-- Helper functions
local function random_latency(base, variance)
    return math.floor(base + (math.random() - 0.5) * 2 * variance)
end

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

local function random_peer()
    return all_peers[math.random(#all_peers)]
end

-- Chaos tracking
local kills = 0
local resurrections = 0
local messages_sent_count = 0

-- Chaos functions
local function chaos_kill(tick)
    if tick % KILL_INTERVAL == 0 then
        local victim = random_online_peer()
        if victim then
            sim:force_offline(victim)
            kills = kills + 1
            indras.log.warn("Chaos: killed peer", {
                trace_id = ctx.trace_id,
                peer = tostring(victim),
                tick = tick,
                total_kills = kills
            })
        end
    end
end

local function chaos_resurrect()
    if math.random() < RANDOM_WAKE_RATE then
        local zombie = random_offline_peer()
        if zombie then
            sim:force_online(zombie)
            resurrections = resurrections + 1
            indras.log.debug("Chaos: resurrected peer", {
                trace_id = ctx.trace_id,
                peer = tostring(zombie),
                resurrections = resurrections
            })
        end
    end
end

-- Message generation
local function generate_messages()
    -- Burst messages during main phase
    local messages_per_tick = math.ceil(TOTAL_MESSAGES / SIMULATION_TICKS)

    for _ = 1, messages_per_tick do
        if messages_sent_count >= TOTAL_MESSAGES then
            break
        end

        local sender = random_online_peer()
        local receiver = random_peer()  -- Can be online or offline (for routing)

        if sender and receiver and sender ~= receiver then
            -- Send network message (routing will handle store-and-forward)
            sim:send_message(sender, receiver, "routing_stress")
            messages_sent_count = messages_sent_count + 1

            -- For 30% of messages, record PQ signature operations
            if math.random() < 0.3 then
                local sign_latency = random_latency(SIGN_LATENCY_BASE, SIGN_LATENCY_VARIANCE)
                sim:record_pq_signature(sender, sign_latency, 256)

                -- If receiver is online, record verification
                if sim:is_online(receiver) then
                    local verify_latency = random_latency(VERIFY_LATENCY_BASE, VERIFY_LATENCY_VARIANCE)
                    sim:record_pq_verification(receiver, sender, verify_latency, true)
                end
            end
        end
    end
end

-- Phase 1: Message burst with churn
indras.log.info("Phase 1: Message burst with network churn", {
    trace_id = ctx.trace_id,
    ticks = SIMULATION_TICKS,
    target_messages = TOTAL_MESSAGES
})

for tick = 1, SIMULATION_TICKS do
    -- Apply chaos
    chaos_kill(tick)
    chaos_resurrect()

    -- Generate messages
    generate_messages()

    -- Advance simulation
    sim:step()

    -- Progress logging
    if tick % 50 == 0 then
        local stats = sim.stats
        indras.log.info("Phase 1 progress checkpoint", {
            trace_id = ctx.trace_id,
            tick = tick,
            online_count = #sim:online_peers(),
            kills = kills,
            resurrections = resurrections,
            messages_sent = messages_sent_count,
            messages_delivered = stats.messages_delivered,
            messages_dropped = stats.messages_dropped,
            delivery_rate = stats:delivery_rate()
        })
    end
end

-- Phase 2: Drain phase (wake all peers)
indras.log.info("Phase 2: Drain phase - waking all peers", {
    trace_id = ctx.trace_id,
    ticks = DRAIN_PHASE_TICKS
})

-- Wake all offline peers
local offline = sim:offline_peers()
for _, peer in ipairs(offline) do
    sim:force_online(peer)
end

indras.log.debug("All peers awakened", {
    trace_id = ctx.trace_id,
    online_count = #sim:online_peers()
})

-- Continue simulation to allow message delivery
for tick = 1, DRAIN_PHASE_TICKS do
    sim:step()

    if tick % 50 == 0 then
        local stats = sim.stats
        indras.log.info("Phase 2 progress checkpoint", {
            trace_id = ctx.trace_id,
            drain_tick = tick,
            messages_delivered = stats.messages_delivered,
            messages_dropped = stats.messages_dropped,
            delivery_rate = stats:delivery_rate()
        })
    end
end

-- Final statistics
local stats = sim.stats

-- Calculate routing-specific metrics
local delivery_rate = stats:delivery_rate()
local avg_latency = stats:average_latency()
local avg_hops = stats:average_hops()

-- Calculate backpropagation success rate (approximated by successful multi-hop deliveries)
local backprop_success_rate = 0
if stats.relayed_deliveries > 0 then
    backprop_success_rate = stats.relayed_deliveries / (stats.direct_deliveries + stats.relayed_deliveries)
end

-- Calculate direct vs relayed ratio
local direct_ratio = 0
local relayed_ratio = 0
if stats.messages_delivered > 0 then
    direct_ratio = stats.direct_deliveries / stats.messages_delivered
    relayed_ratio = stats.relayed_deliveries / stats.messages_delivered
end

indras.log.info("Routing stress test completed", {
    trace_id = ctx.trace_id,
    final_tick = sim.tick,
    -- Chaos metrics
    total_kills = kills,
    total_resurrections = resurrections,
    -- Routing metrics
    messages_sent = messages_sent_count,
    messages_delivered = stats.messages_delivered,
    messages_dropped = stats.messages_dropped,
    direct_deliveries = stats.direct_deliveries,
    relayed_deliveries = stats.relayed_deliveries,
    delivery_rate = delivery_rate,
    avg_latency = avg_latency,
    avg_hops = avg_hops,
    backprop_success_rate = backprop_success_rate,
    direct_ratio = direct_ratio,
    relayed_ratio = relayed_ratio,
    -- PQ signature metrics
    signatures_created = stats.pq_signatures_created,
    signatures_verified = stats.pq_signatures_verified,
    signature_failures = stats.pq_signature_failures,
    avg_sign_latency_us = stats:avg_signature_latency_us(),
    avg_verify_latency_us = stats:avg_verification_latency_us()
})

-- Assertions against thresholds (adjusted for chaos)
local min_delivery_rate = 0.5  -- 50% under chaos is acceptable
if level == "quick" then
    min_delivery_rate = 0.6  -- Higher threshold for quick runs
elseif level == "full" then
    min_delivery_rate = 0.4  -- More lenient for full runs with high churn
end

indras.assert.gt(stats.messages_sent, 0, "Should have sent messages")
indras.assert.gt(stats.messages_delivered, 0, "Should have delivered messages")
indras.assert.ge(delivery_rate, min_delivery_rate,
    string.format("Delivery rate should be at least %.0f%% under chaos", min_delivery_rate * 100))

-- Routing should use both direct and relayed paths
if stats.messages_delivered > 10 then
    indras.assert.gt(stats.direct_deliveries, 0, "Should have some direct deliveries")
    indras.assert.gt(relayed_ratio, 0, "Should have some routed deliveries")
end

-- Average hops should be reasonable (not too many hops)
if avg_hops > 0 then
    indras.assert.lt(avg_hops, 10, "Average hops should be reasonable (< 10)")
end

-- Backpropagation should work for most routed messages
if stats.relayed_deliveries > 0 then
    indras.assert.ge(backprop_success_rate, 0.3,
        "Backpropagation should succeed for at least 30% of routed messages")
end

indras.log.info("Routing stress test passed", {
    trace_id = ctx.trace_id,
    level = level,
    delivery_rate = delivery_rate,
    backprop_success_rate = backprop_success_rate,
    direct_ratio = direct_ratio,
    relayed_ratio = relayed_ratio
})

return {
    -- Configuration
    level = level,
    -- Chaos stats
    kills = kills,
    resurrections = resurrections,
    -- Routing stats
    delivery_rate = delivery_rate,
    avg_latency = avg_latency,
    avg_hops = avg_hops,
    backprop_success_rate = backprop_success_rate,
    direct_ratio = direct_ratio,
    relayed_ratio = relayed_ratio,
    -- Totals
    total_messages = messages_sent_count,
    messages_delivered = stats.messages_delivered,
    messages_dropped = stats.messages_dropped,
    direct_deliveries = stats.direct_deliveries,
    relayed_deliveries = stats.relayed_deliveries,
    -- PQ stats
    total_signatures = stats.pq_signatures_created,
    signature_failure_rate = stats:signature_failure_rate()
}

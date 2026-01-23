-- PQ Chaos Monkey Test
--
-- Stress test with random failures, peer disconnections, and PQ operations.
-- Tests resilience of signature verification and KEM under chaotic conditions.

local ctx = indras.correlation.new_root()
ctx = ctx:with_tag("scenario", "pq_chaos_monkey")

-- Configuration
local PEER_COUNT = 20
local MESSAGE_RATE = 5           -- msgs per tick
local SIMULATION_TICKS = 200
local KILL_INTERVAL = 15         -- ticks between random peer kills
local SIGNATURE_CORRUPTION_RATE = 0.02  -- 2% bad signatures
local KEM_FAILURE_RATE = 0.01    -- 1% KEM failures
local RANDOM_WAKE_RATE = 0.1     -- 10% chance to wake a dead peer

-- Latency parameters (microseconds)
local SIGN_LATENCY_BASE = 200
local SIGN_LATENCY_VARIANCE = 100
local VERIFY_LATENCY_BASE = 150
local VERIFY_LATENCY_VARIANCE = 50
local KEM_ENCAP_BASE = 75
local KEM_ENCAP_VARIANCE = 25
local KEM_DECAP_BASE = 75
local KEM_DECAP_VARIANCE = 25

indras.log.info("Starting PQ chaos monkey test", {
    trace_id = ctx.trace_id,
    peers = PEER_COUNT,
    message_rate = MESSAGE_RATE,
    duration = SIMULATION_TICKS,
    kill_interval = KILL_INTERVAL,
    signature_corruption_rate = SIGNATURE_CORRUPTION_RATE,
    kem_failure_rate = KEM_FAILURE_RATE
})

-- Create random mesh topology
local mesh = indras.MeshBuilder.new(PEER_COUNT):random(0.4)

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
    max_ticks = SIMULATION_TICKS,
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
local corrupted_signatures = 0
local kem_failures_injected = 0

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

-- Message and PQ operation generation
local function generate_pq_operations()
    for _ = 1, MESSAGE_RATE do
        local sender = random_online_peer()
        local receiver = random_online_peer()

        if sender and receiver and sender ~= receiver then
            -- Simulate signed message
            local sign_latency = random_latency(SIGN_LATENCY_BASE, SIGN_LATENCY_VARIANCE)
            sim:record_pq_signature(sender, sign_latency, 256)

            -- Verification with potential corruption
            local verify_latency = random_latency(VERIFY_LATENCY_BASE, VERIFY_LATENCY_VARIANCE)
            local corrupt = math.random() < SIGNATURE_CORRUPTION_RATE
            if corrupt then
                corrupted_signatures = corrupted_signatures + 1
            end
            sim:record_pq_verification(receiver, sender, verify_latency, not corrupt)

            -- Also send network message
            sim:send_message(sender, receiver, "chaos_msg")
        end
    end

    -- Occasional KEM operations (e.g., key rotation, new member join)
    if math.random() < 0.2 then  -- 20% chance per tick
        local initiator = random_online_peer()
        local target = random_online_peer()

        if initiator and target and initiator ~= target then
            local encap_latency = random_latency(KEM_ENCAP_BASE, KEM_ENCAP_VARIANCE)
            sim:record_kem_encapsulation(initiator, target, encap_latency)

            local decap_latency = random_latency(KEM_DECAP_BASE, KEM_DECAP_VARIANCE)
            local kem_fail = math.random() < KEM_FAILURE_RATE
            if kem_fail then
                kem_failures_injected = kem_failures_injected + 1
            end
            sim:record_kem_decapsulation(target, initiator, decap_latency, not kem_fail)

            -- Simulated invite flow occasionally
            if math.random() < 0.3 then
                local interface_id = string.format("chaos-interface-%d", sim.tick)
                sim:record_invite_created(initiator, target, interface_id)
                if not kem_fail then
                    sim:record_invite_accepted(target, interface_id)
                else
                    sim:record_invite_failed(target, interface_id, "KEM failure during chaos")
                end
            end
        end
    end
end

-- Run simulation with chaos
indras.log.info("Running chaos simulation", {
    trace_id = ctx.trace_id,
    ticks = SIMULATION_TICKS
})

for tick = 1, SIMULATION_TICKS do
    -- Apply chaos
    chaos_kill(tick)
    chaos_resurrect()

    -- Generate PQ operations
    generate_pq_operations()

    -- Advance simulation
    sim:step()

    -- Progress logging
    if tick % 50 == 0 then
        local stats = sim.stats
        indras.log.info("Chaos progress checkpoint", {
            trace_id = ctx.trace_id,
            tick = tick,
            online_count = #sim:online_peers(),
            kills = kills,
            resurrections = resurrections,
            signatures_created = stats.pq_signatures_created,
            signatures_verified = stats.pq_signatures_verified,
            signature_failures = stats.pq_signature_failures,
            messages_delivered = stats.messages_delivered,
            messages_dropped = stats.messages_dropped
        })
    end
end

-- Final statistics
local stats = sim.stats

-- Calculate derived metrics
local pq_signature_delivery_rate = 0
if stats.pq_signatures_created > 0 then
    pq_signature_delivery_rate = (stats.pq_signatures_verified + stats.pq_signature_failures) / stats.pq_signatures_created
end

indras.log.info("Chaos test completed", {
    trace_id = ctx.trace_id,
    final_tick = sim.tick,
    -- Chaos metrics
    total_kills = kills,
    total_resurrections = resurrections,
    corrupted_signatures = corrupted_signatures,
    kem_failures_injected = kem_failures_injected,
    -- PQ signature metrics
    signatures_created = stats.pq_signatures_created,
    signatures_verified = stats.pq_signatures_verified,
    signature_failures = stats.pq_signature_failures,
    signature_failure_rate = stats:signature_failure_rate(),
    avg_sign_latency_us = stats:avg_signature_latency_us(),
    avg_verify_latency_us = stats:avg_verification_latency_us(),
    -- KEM metrics
    kem_encapsulations = stats.pq_kem_encapsulations,
    kem_decapsulations = stats.pq_kem_decapsulations,
    kem_failures = stats.pq_kem_failures,
    kem_failure_rate = stats:kem_failure_rate(),
    avg_encap_latency_us = stats:avg_kem_encap_latency_us(),
    avg_decap_latency_us = stats:avg_kem_decap_latency_us(),
    -- Invite metrics
    invites_created = stats.invites_created,
    invites_accepted = stats.invites_accepted,
    invites_failed = stats.invites_failed,
    invite_success_rate = stats:invite_success_rate(),
    -- Network metrics
    messages_sent = stats.messages_sent,
    messages_delivered = stats.messages_delivered,
    messages_dropped = stats.messages_dropped,
    delivery_rate = stats:delivery_rate(),
    average_latency = stats:average_latency(),
    average_hops = stats:average_hops()
})

-- Assertions
-- Allow for chaos-induced failures but ensure system doesn't completely break
indras.assert.gt(stats.pq_signatures_created, 0, "Should have created signatures")
indras.assert.gt(stats.pq_signatures_verified, 0, "Should have verified some signatures")

-- Signature failure rate should be close to corruption rate (within tolerance)
local expected_sig_failure_rate = SIGNATURE_CORRUPTION_RATE
local actual_sig_failure_rate = stats:signature_failure_rate()
indras.assert.lt(math.abs(actual_sig_failure_rate - expected_sig_failure_rate), 0.05,
    "Signature failure rate should be close to corruption rate")

-- KEM should have some successful operations
if stats.pq_kem_encapsulations > 0 then
    indras.assert.gt(stats.pq_kem_decapsulations, 0, "Should have some successful decapsulations")
end

-- Network should have some delivery despite chaos
indras.assert.gt(stats:delivery_rate(), 0.0, "Should deliver at least some messages")

indras.log.info("PQ chaos monkey test passed", {
    trace_id = ctx.trace_id,
    signature_failure_rate = actual_sig_failure_rate,
    kem_failure_rate = stats:kem_failure_rate(),
    message_delivery_rate = stats:delivery_rate()
})

return {
    -- Chaos stats
    kills = kills,
    resurrections = resurrections,
    corrupted_signatures = corrupted_signatures,
    kem_failures_injected = kem_failures_injected,
    -- PQ stats
    signature_failure_rate = actual_sig_failure_rate,
    kem_failure_rate = stats:kem_failure_rate(),
    invite_success_rate = stats:invite_success_rate(),
    -- Network stats
    delivery_rate = stats:delivery_rate(),
    -- Totals
    total_signatures = stats.pq_signatures_created,
    total_kem_ops = stats.pq_kem_encapsulations
}

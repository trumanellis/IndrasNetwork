-- PQ Signature Throughput Test
--
-- Tests high-volume message signing and verification to find throughput ceiling.
-- Ramps up message rate until performance degrades.

local ctx = indras.correlation.new_root()
ctx = ctx:with_tag("scenario", "pq_signature_throughput")

-- Configuration
local PEER_COUNT = 10
local BASE_MSGS_PER_TICK = 10
local RAMP_INTERVAL = 30         -- ticks between rate increases
local RAMP_INCREASE = 10         -- additional msgs per tick after each ramp
local MAX_RATE = 100             -- maximum msgs per tick
local TEST_DURATION = 200        -- total simulation ticks

-- Latency parameters (microseconds)
local SIGN_LATENCY_BASE = 200
local SIGN_LATENCY_VARIANCE = 50
local VERIFY_LATENCY_BASE = 150
local VERIFY_LATENCY_VARIANCE = 30

indras.log.info("Starting PQ signature throughput test", {
    trace_id = ctx.trace_id,
    peers = PEER_COUNT,
    base_rate = BASE_MSGS_PER_TICK,
    ramp_interval = RAMP_INTERVAL,
    max_rate = MAX_RATE,
    duration = TEST_DURATION
})

-- Create mesh
local mesh = indras.MeshBuilder.new(PEER_COUNT):full_mesh()
local config = indras.SimConfig.manual()
config.max_ticks = TEST_DURATION
local sim = indras.Simulation.new(mesh, config)
sim:initialize()

-- Force all peers online
local peers = mesh:peers()
for _, peer in ipairs(peers) do
    sim:force_online(peer)
end

-- Helper functions
local function random_latency(base, variance)
    return math.floor(base + (math.random() - 0.5) * 2 * variance)
end

local function random_peer()
    return peers[math.random(#peers)]
end

-- Track metrics per phase
local phase_metrics = {}
local current_rate = BASE_MSGS_PER_TICK
local current_phase = 1
local phase_start_tick = 0
local phase_latencies = {}

-- Run simulation
indras.log.info("Running throughput ramp test", {
    trace_id = ctx.trace_id
})

for tick = 1, TEST_DURATION do
    -- Check for rate increase
    if tick > 1 and (tick - 1) % RAMP_INTERVAL == 0 and current_rate < MAX_RATE then
        -- Save phase metrics
        local avg_latency = 0
        if #phase_latencies > 0 then
            local sum = 0
            for _, l in ipairs(phase_latencies) do sum = sum + l end
            avg_latency = sum / #phase_latencies
        end

        table.insert(phase_metrics, {
            phase = current_phase,
            rate = current_rate,
            ticks = tick - phase_start_tick,
            operations = #phase_latencies,
            avg_latency_us = math.floor(avg_latency),
            throughput = math.floor(#phase_latencies / (tick - phase_start_tick))
        })

        indras.log.info("Phase completed", {
            trace_id = ctx.trace_id,
            phase = current_phase,
            rate = current_rate,
            operations = #phase_latencies,
            avg_latency_us = math.floor(avg_latency)
        })

        -- Ramp up
        current_rate = math.min(current_rate + RAMP_INCREASE, MAX_RATE)
        current_phase = current_phase + 1
        phase_start_tick = tick
        phase_latencies = {}

        indras.log.info("Rate increased", {
            trace_id = ctx.trace_id,
            new_rate = current_rate,
            phase = current_phase
        })
    end

    -- Generate messages at current rate
    for i = 1, current_rate do
        local sender = random_peer()
        local verifier = random_peer()
        while verifier == sender do
            verifier = random_peer()
        end

        -- Sign
        local sign_latency = random_latency(SIGN_LATENCY_BASE, SIGN_LATENCY_VARIANCE)
        sim:record_pq_signature(sender, sign_latency, 256)
        table.insert(phase_latencies, sign_latency)

        -- Verify
        local verify_latency = random_latency(VERIFY_LATENCY_BASE, VERIFY_LATENCY_VARIANCE)
        local success = math.random() > 0.001
        sim:record_pq_verification(verifier, sender, verify_latency, success)
    end

    sim:step()

    -- Progress logging
    if tick % 50 == 0 then
        indras.log.debug("Progress checkpoint", {
            trace_id = ctx.trace_id,
            tick = tick,
            current_rate = current_rate,
            total_signatures = sim.stats.pq_signatures_created
        })
    end
end

-- Save final phase metrics
if #phase_latencies > 0 then
    local sum = 0
    for _, l in ipairs(phase_latencies) do sum = sum + l end
    local avg_latency = sum / #phase_latencies

    table.insert(phase_metrics, {
        phase = current_phase,
        rate = current_rate,
        ticks = TEST_DURATION - phase_start_tick,
        operations = #phase_latencies,
        avg_latency_us = math.floor(avg_latency),
        throughput = math.floor(#phase_latencies / (TEST_DURATION - phase_start_tick))
    })
end

-- Calculate overall stats
local stats = sim.stats
local total_ops = stats.pq_signatures_created
local total_time_us = stats.pq_signature_create_time_us
local avg_latency = stats:avg_signature_latency_us()

-- Find peak throughput phase
local peak_phase = phase_metrics[1]
for _, pm in ipairs(phase_metrics) do
    if pm.throughput > peak_phase.throughput then
        peak_phase = pm
    end
end

indras.log.info("Throughput test completed", {
    trace_id = ctx.trace_id,
    total_signatures = total_ops,
    total_verifications = stats.pq_signatures_verified,
    verification_failures = stats.pq_signature_failures,
    failure_rate = stats:signature_failure_rate(),
    avg_sign_latency_us = avg_latency,
    avg_verify_latency_us = stats:avg_verification_latency_us(),
    phases_completed = #phase_metrics
})

-- Log each phase
for _, pm in ipairs(phase_metrics) do
    indras.log.info("Phase result", {
        trace_id = ctx.trace_id,
        phase = pm.phase,
        target_rate = pm.rate,
        actual_throughput = pm.throughput,
        avg_latency_us = pm.avg_latency_us,
        operations = pm.operations
    })
end

indras.log.info("Peak throughput identified", {
    trace_id = ctx.trace_id,
    peak_phase = peak_phase.phase,
    peak_rate = peak_phase.rate,
    peak_throughput = peak_phase.throughput,
    peak_latency_us = peak_phase.avg_latency_us
})

-- Assertions
indras.assert.gt(total_ops, 0, "Should have created signatures")
indras.assert.lt(stats:signature_failure_rate(), 0.01, "Failure rate should be under 1%")
indras.assert.gt(peak_phase.throughput, 0, "Should have measurable throughput")

indras.log.info("PQ signature throughput test passed", {
    trace_id = ctx.trace_id
})

return {
    phases = phase_metrics,
    peak = peak_phase,
    total_signatures = total_ops,
    avg_latency_us = avg_latency,
    failure_rate = stats:signature_failure_rate()
}

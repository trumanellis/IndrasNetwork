-- PQ Large Interface Sync Test
--
-- Tests sync cycle performance with many members.
-- Measures signatures per cycle, cycle time, and bandwidth.

local ctx = indras.correlation.new_root()
ctx = ctx:with_tag("scenario", "pq_large_interface_sync")

-- Configuration
local MEMBER_COUNTS = {5, 10, 15, 20, 26}  -- Max 26 peers (A-Z)
local SYNC_CYCLES = 5
local PENDING_EVENTS_PER_MEMBER = 3
local INTERFACE_ID = "large-interface-sync"

-- Latency parameters (microseconds)
local SIGN_LATENCY_BASE = 200
local SIGN_LATENCY_VARIANCE = 50
local VERIFY_LATENCY_BASE = 150
local VERIFY_LATENCY_VARIANCE = 30

indras.log.info("Starting PQ large interface sync test", {
    trace_id = ctx.trace_id,
    member_counts = table.concat(MEMBER_COUNTS, ","),
    sync_cycles = SYNC_CYCLES,
    events_per_member = PENDING_EVENTS_PER_MEMBER
})

-- Helper functions
local function random_latency(base, variance)
    return math.floor(base + (math.random() - 0.5) * 2 * variance)
end

-- Track metrics for each interface size
local size_metrics = {}

for _, member_count in ipairs(MEMBER_COUNTS) do
    local size_ctx = ctx:with_tag("members", tostring(member_count))

    indras.log.info("Testing interface size", {
        trace_id = size_ctx.trace_id,
        member_count = member_count
    })

    -- Create mesh with this many members
    local mesh = indras.MeshBuilder.new(member_count):full_mesh()
    local config = indras.SimConfig.manual()
    local sim = indras.Simulation.new(mesh, config)
    sim:initialize()

    -- Force all members online
    local members = mesh:peers()
    for _, member in ipairs(members) do
        sim:force_online(member)
    end

    local cycle_metrics = {}

    for cycle = 1, SYNC_CYCLES do
        local cycle_ctx = size_ctx:with_tag("cycle", tostring(cycle))
        local cycle_start_tick = sim.tick
        local cycle_signatures_before = sim.stats.pq_signatures_created
        local cycle_verifications_before = sim.stats.pq_signatures_verified

        indras.log.debug("Starting sync cycle", {
            trace_id = cycle_ctx.trace_id,
            cycle = cycle,
            members = member_count
        })

        -- Simulate sync: each member signs events for all other members
        -- In reality, a sync request is signed and sent to each peer
        local cycle_latencies = {}

        for _, sender in ipairs(members) do
            -- Generate pending events for this member
            for event_num = 1, PENDING_EVENTS_PER_MEMBER do
                -- Sign the event message
                local sign_latency = random_latency(SIGN_LATENCY_BASE, SIGN_LATENCY_VARIANCE)
                local msg_size = 256 + (event_num * 64)  -- Variable message sizes
                sim:record_pq_signature(sender, sign_latency, msg_size)
                table.insert(cycle_latencies, sign_latency)

                -- Each other member verifies
                for _, receiver in ipairs(members) do
                    if receiver ~= sender then
                        local verify_latency = random_latency(VERIFY_LATENCY_BASE, VERIFY_LATENCY_VARIANCE)
                        local success = math.random() > 0.0005  -- 0.05% failure rate
                        sim:record_pq_verification(receiver, sender, verify_latency, success)
                    end
                end
            end
        end

        -- Advance simulation for this cycle
        for _ = 1, 5 do
            sim:step()
        end

        -- Calculate cycle metrics
        local cycle_signatures = sim.stats.pq_signatures_created - cycle_signatures_before
        local cycle_verifications = sim.stats.pq_signatures_verified - cycle_verifications_before
        local cycle_duration = sim.tick - cycle_start_tick

        local avg_latency = 0
        if #cycle_latencies > 0 then
            local sum = 0
            for _, l in ipairs(cycle_latencies) do sum = sum + l end
            avg_latency = sum / #cycle_latencies
        end

        -- Estimate bandwidth (signature ~2400 bytes for ML-DSA-65)
        local sig_size_bytes = 2400
        local bandwidth_bytes = cycle_signatures * sig_size_bytes

        local cm = {
            cycle = cycle,
            signatures = cycle_signatures,
            verifications = cycle_verifications,
            duration_ticks = cycle_duration,
            avg_sign_latency_us = math.floor(avg_latency),
            bandwidth_bytes = bandwidth_bytes,
            signatures_per_tick = math.floor(cycle_signatures / cycle_duration)
        }
        table.insert(cycle_metrics, cm)

        indras.log.debug("Sync cycle completed", {
            trace_id = cycle_ctx.trace_id,
            cycle = cycle,
            signatures = cm.signatures,
            verifications = cm.verifications,
            bandwidth_kb = math.floor(cm.bandwidth_bytes / 1024)
        })
    end

    -- Calculate average metrics for this interface size
    local total_sigs = 0
    local total_verifs = 0
    local total_latency = 0
    local total_bandwidth = 0
    for _, cm in ipairs(cycle_metrics) do
        total_sigs = total_sigs + cm.signatures
        total_verifs = total_verifs + cm.verifications
        total_latency = total_latency + cm.avg_sign_latency_us
        total_bandwidth = total_bandwidth + cm.bandwidth_bytes
    end

    local size_metric = {
        member_count = member_count,
        total_cycles = SYNC_CYCLES,
        avg_signatures_per_cycle = math.floor(total_sigs / SYNC_CYCLES),
        avg_verifications_per_cycle = math.floor(total_verifs / SYNC_CYCLES),
        avg_sign_latency_us = math.floor(total_latency / SYNC_CYCLES),
        avg_bandwidth_per_cycle_kb = math.floor(total_bandwidth / SYNC_CYCLES / 1024),
        total_bandwidth_kb = math.floor(total_bandwidth / 1024),
        signatures_per_member_per_cycle = math.floor(total_sigs / SYNC_CYCLES / member_count),
        verifications_per_member_per_cycle = math.floor(total_verifs / SYNC_CYCLES / member_count)
    }
    table.insert(size_metrics, size_metric)

    indras.log.info("Interface size test completed", {
        trace_id = size_ctx.trace_id,
        member_count = member_count,
        avg_sigs_per_cycle = size_metric.avg_signatures_per_cycle,
        avg_verifs_per_cycle = size_metric.avg_verifications_per_cycle,
        avg_bandwidth_kb = size_metric.avg_bandwidth_per_cycle_kb,
        total_signatures = sim.stats.pq_signatures_created,
        signature_failures = sim.stats.pq_signature_failures,
        failure_rate = sim.stats:signature_failure_rate()
    })
end

-- Log scaling analysis
indras.log.info("Scaling analysis", {
    trace_id = ctx.trace_id
})

for _, sm in ipairs(size_metrics) do
    indras.log.info("Interface size metrics", {
        trace_id = ctx.trace_id,
        members = sm.member_count,
        sigs_per_cycle = sm.avg_signatures_per_cycle,
        verifs_per_cycle = sm.avg_verifications_per_cycle,
        bandwidth_kb_per_cycle = sm.avg_bandwidth_per_cycle_kb,
        avg_latency_us = sm.avg_sign_latency_us
    })
end

-- Calculate scaling factors
if #size_metrics >= 2 then
    local first = size_metrics[1]
    local last = size_metrics[#size_metrics]

    local member_scale = last.member_count / first.member_count
    local sig_scale = last.avg_signatures_per_cycle / first.avg_signatures_per_cycle
    local bandwidth_scale = last.avg_bandwidth_per_cycle_kb / first.avg_bandwidth_per_cycle_kb

    indras.log.info("Scaling factors", {
        trace_id = ctx.trace_id,
        member_scale = string.format("%.1fx", member_scale),
        signature_scale = string.format("%.1fx", sig_scale),
        bandwidth_scale = string.format("%.1fx", bandwidth_scale),
        efficiency = string.format("%.2f", sig_scale / member_scale)
    })
end

-- Assertions
for _, sm in ipairs(size_metrics) do
    indras.assert.gt(sm.avg_signatures_per_cycle, 0, "Should have signatures per cycle")
    indras.assert.gt(sm.avg_verifications_per_cycle, 0, "Should have verifications per cycle")
end

-- Verify signatures scale with members
local first = size_metrics[1]
local last = size_metrics[#size_metrics]
indras.assert.gt(last.avg_signatures_per_cycle, first.avg_signatures_per_cycle,
    "Signatures should increase with more members")

indras.log.info("PQ large interface sync test passed", {
    trace_id = ctx.trace_id,
    sizes_tested = #MEMBER_COUNTS
})

return {
    size_metrics = size_metrics,
    member_counts = MEMBER_COUNTS,
    sync_cycles = SYNC_CYCLES
}

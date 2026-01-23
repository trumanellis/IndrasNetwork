-- PQ Concurrent Joins Test
--
-- Tests many nodes joining an interface simultaneously, stressing
-- KEM encapsulation/decapsulation operations.

local ctx = indras.correlation.new_root()
ctx = ctx:with_tag("scenario", "pq_concurrent_joins")

-- Configuration
local INITIAL_MEMBERS = 1        -- Start with creator only
local JOIN_WAVES = {5, 5, 7, 7}  -- Nodes per wave (limited to 26 total peers A-Z)
local WAVE_INTERVAL = 20         -- ticks between waves
local INTERFACE_ID = "test-interface-001"

-- KEM latency parameters (microseconds)
local KEM_ENCAP_BASE = 75
local KEM_ENCAP_VARIANCE = 25
local KEM_DECAP_BASE = 75
local KEM_DECAP_VARIANCE = 25

-- Calculate total peers needed
local total_joiners = 0
for _, wave_size in ipairs(JOIN_WAVES) do
    total_joiners = total_joiners + wave_size
end
local PEER_COUNT = INITIAL_MEMBERS + total_joiners

indras.log.info("Starting PQ concurrent joins test", {
    trace_id = ctx.trace_id,
    total_peers = PEER_COUNT,
    initial_members = INITIAL_MEMBERS,
    join_waves = #JOIN_WAVES,
    wave_sizes = table.concat(JOIN_WAVES, ",")
})

-- Create mesh (fully connected for simplicity)
local mesh = indras.MeshBuilder.new(PEER_COUNT):full_mesh()
local config = indras.SimConfig.manual()
local sim = indras.Simulation.new(mesh, config)
sim:initialize()

-- Get all peers
local peers = mesh:peers()

-- Force initial members online
local members = {}
for i = 1, INITIAL_MEMBERS do
    sim:force_online(peers[i])
    table.insert(members, peers[i])
end

-- Track pending joiners
local joiner_index = INITIAL_MEMBERS + 1

-- Helper functions
local function random_latency(base, variance)
    return math.floor(base + (math.random() - 0.5) * 2 * variance)
end

-- Wave tracking
local wave_metrics = {}

-- Process join waves
for wave_num, wave_size in ipairs(JOIN_WAVES) do
    local wave_ctx = ctx:with_tag("wave", tostring(wave_num))
    local wave_start_tick = sim.tick

    indras.log.info("Starting join wave", {
        trace_id = wave_ctx.trace_id,
        wave = wave_num,
        wave_size = wave_size,
        current_members = #members
    })

    local wave_encap_latencies = {}
    local wave_decap_latencies = {}
    local wave_successes = 0
    local wave_failures = 0

    -- Process all joiners in this wave
    local wave_joiners = {}
    for i = 1, wave_size do
        if joiner_index <= #peers then
            local joiner = peers[joiner_index]
            sim:force_online(joiner)
            table.insert(wave_joiners, joiner)
            joiner_index = joiner_index + 1
        end
    end

    -- Simulate join process for each joiner
    -- In a real system: existing member encapsulates key for joiner, joiner decapsulates
    for _, joiner in ipairs(wave_joiners) do
        -- Pick a random existing member to create the invite
        local inviter = members[math.random(#members)]

        -- Record invite creation
        sim:record_invite_created(inviter, joiner, INTERFACE_ID)

        -- Inviter encapsulates interface key for joiner
        local encap_latency = random_latency(KEM_ENCAP_BASE, KEM_ENCAP_VARIANCE)
        sim:record_kem_encapsulation(inviter, joiner, encap_latency)
        table.insert(wave_encap_latencies, encap_latency)

        -- Joiner decapsulates to get interface key
        local decap_latency = random_latency(KEM_DECAP_BASE, KEM_DECAP_VARIANCE)
        local success = math.random() > 0.005  -- 0.5% failure rate
        sim:record_kem_decapsulation(joiner, inviter, decap_latency, success)
        table.insert(wave_decap_latencies, decap_latency)

        if success then
            sim:record_invite_accepted(joiner, INTERFACE_ID)
            table.insert(members, joiner)
            wave_successes = wave_successes + 1
        else
            sim:record_invite_failed(joiner, INTERFACE_ID, "KEM decapsulation failed")
            wave_failures = wave_failures + 1
        end

        -- Log individual join
        indras.log.trace("Join processed", {
            trace_id = wave_ctx.trace_id,
            joiner = tostring(joiner),
            inviter = tostring(inviter),
            success = success,
            encap_latency_us = encap_latency,
            decap_latency_us = decap_latency
        })
    end

    -- Run some ticks to simulate network propagation
    for _ = 1, WAVE_INTERVAL do
        sim:step()
    end

    -- Calculate wave metrics
    local avg_encap = 0
    local avg_decap = 0
    if #wave_encap_latencies > 0 then
        local sum = 0
        for _, l in ipairs(wave_encap_latencies) do sum = sum + l end
        avg_encap = sum / #wave_encap_latencies
    end
    if #wave_decap_latencies > 0 then
        local sum = 0
        for _, l in ipairs(wave_decap_latencies) do sum = sum + l end
        avg_decap = sum / #wave_decap_latencies
    end

    local wave_metric = {
        wave = wave_num,
        wave_size = wave_size,
        successes = wave_successes,
        failures = wave_failures,
        success_rate = wave_successes / (wave_successes + wave_failures),
        avg_encap_latency_us = math.floor(avg_encap),
        avg_decap_latency_us = math.floor(avg_decap),
        total_join_time_ticks = sim.tick - wave_start_tick,
        members_after = #members
    }
    table.insert(wave_metrics, wave_metric)

    indras.log.info("Wave completed", {
        trace_id = wave_ctx.trace_id,
        wave = wave_num,
        successes = wave_successes,
        failures = wave_failures,
        success_rate = wave_metric.success_rate,
        avg_encap_latency_us = wave_metric.avg_encap_latency_us,
        avg_decap_latency_us = wave_metric.avg_decap_latency_us,
        members_now = #members
    })
end

-- Final statistics
local stats = sim.stats
indras.log.info("Concurrent joins test completed", {
    trace_id = ctx.trace_id,
    final_tick = sim.tick,
    final_members = #members,
    total_kem_encapsulations = stats.pq_kem_encapsulations,
    total_kem_decapsulations = stats.pq_kem_decapsulations,
    kem_failures = stats.pq_kem_failures,
    kem_failure_rate = stats:kem_failure_rate(),
    invites_created = stats.invites_created,
    invites_accepted = stats.invites_accepted,
    invites_failed = stats.invites_failed,
    invite_success_rate = stats:invite_success_rate(),
    avg_encap_latency_us = stats:avg_kem_encap_latency_us(),
    avg_decap_latency_us = stats:avg_kem_decap_latency_us()
})

-- Log wave summary
for _, wm in ipairs(wave_metrics) do
    indras.log.info("Wave summary", {
        trace_id = ctx.trace_id,
        wave = wm.wave,
        wave_size = wm.wave_size,
        success_rate = wm.success_rate,
        avg_join_latency_us = wm.avg_encap_latency_us + wm.avg_decap_latency_us
    })
end

-- Assertions
indras.assert.gt(#members, INITIAL_MEMBERS, "Should have added members")
indras.assert.gt(stats:invite_success_rate(), 0.95, "Invite success rate should be > 95%")
indras.assert.lt(stats:kem_failure_rate(), 0.02, "KEM failure rate should be < 2%")

-- Verify scaling behavior
local first_wave = wave_metrics[1]
local last_wave = wave_metrics[#wave_metrics]
indras.assert.gt(last_wave.wave_size, first_wave.wave_size, "Waves should increase in size")

indras.log.info("PQ concurrent joins test passed", {
    trace_id = ctx.trace_id,
    final_member_count = #members,
    total_waves = #JOIN_WAVES
})

return {
    waves = wave_metrics,
    final_members = #members,
    total_encapsulations = stats.pq_kem_encapsulations,
    total_decapsulations = stats.pq_kem_decapsulations,
    invite_success_rate = stats:invite_success_rate(),
    kem_failure_rate = stats:kem_failure_rate()
}

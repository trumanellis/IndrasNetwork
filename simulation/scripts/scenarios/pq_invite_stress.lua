-- PQ Invite Stress Test
--
-- Tests invite creation and acceptance at scale with failure injection.
-- Validates PQ invite flow under various error conditions.

local ctx = indras.correlation.new_root()
ctx = ctx:with_tag("scenario", "pq_invite_stress")

-- Configuration
local BASE_MEMBERS = 3
local INVITES_PER_PHASE = {5, 7, 8}  -- Total: 20 + 3 = 23 < 26
local PHASE_DURATION = 40        -- ticks per phase
local FAILURE_INJECTION_RATE = 0.05  -- 5% injected failures
local CORRUPTION_RATE = 0.02     -- 2% corrupted ciphertext

-- KEM latency parameters (microseconds)
local KEM_ENCAP_BASE = 75
local KEM_ENCAP_VARIANCE = 25
local KEM_DECAP_BASE = 75
local KEM_DECAP_VARIANCE = 25

-- Calculate total peers needed (max 26 for A-Z)
local total_invitees = 0
for _, count in ipairs(INVITES_PER_PHASE) do
    total_invitees = total_invitees + count
end
local PEER_COUNT = math.min(BASE_MEMBERS + total_invitees + 3, 26)  -- Capped at 26

indras.log.info("Starting PQ invite stress test", {
    trace_id = ctx.trace_id,
    base_members = BASE_MEMBERS,
    phases = #INVITES_PER_PHASE,
    invites_per_phase = table.concat(INVITES_PER_PHASE, ","),
    failure_injection_rate = FAILURE_INJECTION_RATE,
    corruption_rate = CORRUPTION_RATE
})

-- Create mesh
local mesh = indras.MeshBuilder.new(PEER_COUNT):full_mesh()
local config = indras.SimConfig.manual()
local sim = indras.Simulation.new(mesh, config)
sim:initialize()

-- Get all peers and setup initial members
local all_peers = mesh:peers()
local members = {}
local available_peers = {}

-- Setup base members
for i = 1, BASE_MEMBERS do
    local peer = all_peers[i]
    sim:force_online(peer)
    table.insert(members, peer)
end

-- Pool of available invitees
for i = BASE_MEMBERS + 1, #all_peers do
    table.insert(available_peers, all_peers[i])
end

-- Helper functions
local function random_latency(base, variance)
    return math.floor(base + (math.random() - 0.5) * 2 * variance)
end

local function random_member()
    return members[math.random(#members)]
end

local function take_available_peer()
    if #available_peers == 0 then return nil end
    local idx = math.random(#available_peers)
    local peer = available_peers[idx]
    table.remove(available_peers, idx)
    return peer
end

-- Track metrics
local phase_metrics = {}
local all_invite_latencies = {}

-- Process each phase
for phase_num, invites_target in ipairs(INVITES_PER_PHASE) do
    local phase_ctx = ctx:with_tag("phase", tostring(phase_num))
    local phase_start_tick = sim.tick

    indras.log.info("Starting invite phase", {
        trace_id = phase_ctx.trace_id,
        phase = phase_num,
        target_invites = invites_target,
        current_members = #members,
        available_peers = #available_peers
    })

    local phase_successes = 0
    local phase_failures = 0
    local phase_injected_failures = 0
    local phase_corruptions = 0
    local phase_latencies = {}

    for i = 1, invites_target do
        local invitee = take_available_peer()
        if not invitee then
            indras.log.warn("No more available peers", {
                trace_id = phase_ctx.trace_id,
                phase = phase_num,
                invite_num = i
            })
            break
        end

        sim:force_online(invitee)
        local inviter = random_member()
        local interface_id = string.format("interface-phase%d-%d", phase_num, i)

        -- Decide failure type
        local inject_failure = math.random() < FAILURE_INJECTION_RATE
        local inject_corruption = not inject_failure and math.random() < CORRUPTION_RATE

        -- Record invite creation
        sim:record_invite_created(inviter, invitee, interface_id)

        -- Inviter encapsulates key
        local encap_latency = random_latency(KEM_ENCAP_BASE, KEM_ENCAP_VARIANCE)
        sim:record_kem_encapsulation(inviter, invitee, encap_latency)
        table.insert(phase_latencies, encap_latency)

        -- Invitee decapsulates
        local decap_latency = random_latency(KEM_DECAP_BASE, KEM_DECAP_VARIANCE)
        local success = true
        local failure_reason = nil

        if inject_failure then
            success = false
            failure_reason = "Injected test failure"
            phase_injected_failures = phase_injected_failures + 1
        elseif inject_corruption then
            success = false
            failure_reason = "Corrupted ciphertext"
            phase_corruptions = phase_corruptions + 1
        end

        sim:record_kem_decapsulation(invitee, inviter, decap_latency, success)

        if success then
            sim:record_invite_accepted(invitee, interface_id)
            table.insert(members, invitee)
            phase_successes = phase_successes + 1
        else
            sim:record_invite_failed(invitee, interface_id, failure_reason)
            phase_failures = phase_failures + 1
            -- Put peer back in pool for potential retry
            table.insert(available_peers, invitee)
        end

        -- Log individual invite
        indras.log.trace("Invite processed", {
            trace_id = phase_ctx.trace_id,
            invite_num = i,
            inviter = tostring(inviter),
            invitee = tostring(invitee),
            success = success,
            failure_reason = failure_reason,
            total_latency_us = encap_latency + decap_latency
        })

        -- Advance simulation occasionally
        if i % 10 == 0 then
            sim:step()
        end
    end

    -- Run remaining ticks for this phase
    local ticks_remaining = PHASE_DURATION - (sim.tick - phase_start_tick)
    for _ = 1, math.max(1, ticks_remaining) do
        sim:step()
    end

    -- Calculate phase metrics
    local avg_latency = 0
    if #phase_latencies > 0 then
        local sum = 0
        for _, l in ipairs(phase_latencies) do sum = sum + l end
        avg_latency = sum / #phase_latencies

        for _, l in ipairs(phase_latencies) do
            table.insert(all_invite_latencies, l)
        end
    end

    local pm = {
        phase = phase_num,
        target = invites_target,
        attempted = phase_successes + phase_failures,
        successes = phase_successes,
        failures = phase_failures,
        injected_failures = phase_injected_failures,
        corruptions = phase_corruptions,
        success_rate = phase_successes / (phase_successes + phase_failures),
        avg_encap_latency_us = math.floor(avg_latency),
        members_after = #members
    }
    table.insert(phase_metrics, pm)

    indras.log.info("Invite phase completed", {
        trace_id = phase_ctx.trace_id,
        phase = phase_num,
        attempted = pm.attempted,
        successes = pm.successes,
        failures = pm.failures,
        injected_failures = pm.injected_failures,
        corruptions = pm.corruptions,
        success_rate = pm.success_rate,
        avg_latency_us = pm.avg_encap_latency_us,
        members_now = #members
    })
end

-- Calculate percentiles for latencies
local function percentile(values, p)
    if #values == 0 then return 0 end
    local sorted = {}
    for _, v in ipairs(values) do table.insert(sorted, v) end
    table.sort(sorted)
    local idx = math.ceil(#sorted * p / 100)
    return sorted[math.max(1, idx)]
end

-- Final statistics
local stats = sim.stats
indras.log.info("Invite stress test completed", {
    trace_id = ctx.trace_id,
    final_tick = sim.tick,
    final_members = #members,
    total_invites_created = stats.invites_created,
    total_invites_accepted = stats.invites_accepted,
    total_invites_failed = stats.invites_failed,
    invite_success_rate = stats:invite_success_rate(),
    total_kem_encapsulations = stats.pq_kem_encapsulations,
    total_kem_decapsulations = stats.pq_kem_decapsulations,
    kem_failures = stats.pq_kem_failures,
    kem_failure_rate = stats:kem_failure_rate(),
    avg_encap_latency_us = stats:avg_kem_encap_latency_us(),
    avg_decap_latency_us = stats:avg_kem_decap_latency_us()
})

-- Latency percentiles
if #all_invite_latencies > 0 then
    indras.log.info("KEM latency percentiles", {
        trace_id = ctx.trace_id,
        p50_us = percentile(all_invite_latencies, 50),
        p95_us = percentile(all_invite_latencies, 95),
        p99_us = percentile(all_invite_latencies, 99)
    })
end

-- Log phase summary
for _, pm in ipairs(phase_metrics) do
    indras.log.info("Phase summary", {
        trace_id = ctx.trace_id,
        phase = pm.phase,
        target = pm.target,
        success_rate = pm.success_rate,
        injected_failures = pm.injected_failures,
        corruptions = pm.corruptions
    })
end

-- Assertions
indras.assert.gt(stats.invites_created, 0, "Should have created invites")
indras.assert.gt(stats.invites_accepted, 0, "Should have accepted invites")

-- Account for injected failures in success rate expectation
local expected_min_success = 1.0 - FAILURE_INJECTION_RATE - CORRUPTION_RATE - 0.05  -- 5% margin
indras.assert.gt(stats:invite_success_rate(), expected_min_success,
    "Success rate should be above expected minimum with failure injection")

-- KEM latency should be reasonable
indras.assert.lt(percentile(all_invite_latencies, 99), 200,
    "KEM p99 latency should be under 200us")

indras.log.info("PQ invite stress test passed", {
    trace_id = ctx.trace_id,
    final_members = #members
})

return {
    phases = phase_metrics,
    final_members = #members,
    total_invites = stats.invites_created,
    success_rate = stats:invite_success_rate(),
    kem_failure_rate = stats:kem_failure_rate(),
    latency_p99_us = percentile(all_invite_latencies, 99)
}

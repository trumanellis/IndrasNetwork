-- Crypto Stress Test
--
-- Stress tests the indras-crypto module (ML-DSA-65 signatures, ML-KEM-768 KEM, key distribution).
-- Tests signature creation/verification, KEM encapsulation/decapsulation, and invite flows under high load.

-- Fix package path for helper libraries
package.path = package.path .. ";scripts/lib/?.lua;simulation/scripts/lib/?.lua"

local pq_helpers = require("pq_helpers")

local ctx = pq_helpers.new_context("crypto_stress")

-- Configuration levels
local CONFIG = {
    quick = {
        peers = 5,
        signature_ops = 500,
        kem_ops = 200,
        ticks = 100,
    },
    medium = {
        peers = 20,
        signature_ops = 5000,
        kem_ops = 2000,
        ticks = 300,
    },
    full = {
        peers = 100,
        signature_ops = 50000,
        kem_ops = 20000,
        ticks = 1000,
    },
}

-- Select configuration (default to quick)
local level = os.getenv("STRESS_LEVEL") or "quick"
local config = CONFIG[level]
if not config then
    error(string.format("Invalid stress level: %s (must be quick, medium, or full)", level))
end

-- Failure rates
local SIGNATURE_FAILURE_RATE = 0.001  -- 0.1%
local KEM_FAILURE_RATE = 0.0005       -- 0.05%

indras.log.info("Starting crypto stress test", {
    trace_id = ctx.trace_id,
    level = level,
    peers = config.peers,
    signature_ops = config.signature_ops,
    kem_ops = config.kem_ops,
    ticks = config.ticks,
    signature_failure_rate = SIGNATURE_FAILURE_RATE,
    kem_failure_rate = KEM_FAILURE_RATE
})

-- Create full mesh topology for maximum crypto operations
local mesh = indras.MeshBuilder.new(config.peers):full_mesh()

indras.log.debug("Created full mesh topology", {
    trace_id = ctx.trace_id,
    peers = mesh:peer_count(),
    edges = mesh:edge_count()
})

-- Create simulation
local sim_config = indras.SimConfig.new({
    wake_probability = 0.0,     -- Keep all peers online for stress test
    sleep_probability = 0.0,
    initial_online_probability = 1.0,
    max_ticks = config.ticks,
    trace_routing = false       -- Disable for performance
})

local sim = indras.Simulation.new(mesh, sim_config)
sim:initialize()

-- Get all peers (all will be online)
local all_peers = mesh:peers()

-- Latency tracking arrays
local signature_latencies = {}
local verification_latencies = {}
local encap_latencies = {}
local decap_latencies = {}

-- Operation counters
local total_signature_ops = 0
local total_verification_ops = 0
local total_kem_encap_ops = 0
local total_kem_decap_ops = 0
local total_invites_created = 0
local total_invites_accepted = 0
local total_invites_failed = 0

-- Helper to select random peer
local function random_peer()
    return all_peers[math.random(#all_peers)]
end

-- Helper to select two different peers
local function random_peer_pair()
    local p1 = random_peer()
    local p2 = random_peer()
    while p1 == p2 do
        p2 = random_peer()
    end
    return p1, p2
end

-- Phase 1: High-volume signature creation/verification
indras.log.info("Phase 1: Signature stress test", {
    trace_id = ctx.trace_id,
    target_operations = config.signature_ops
})

local phase1_ticks = math.ceil(config.ticks * 0.4)
local sigs_per_tick = math.ceil(config.signature_ops / phase1_ticks)

for tick = 1, phase1_ticks do
    for _ = 1, sigs_per_tick do
        local sender, receiver = random_peer_pair()

        -- Create signature
        local sign_latency = pq_helpers.sign_latency()
        sim:record_pq_signature(sender, sign_latency, 256)
        table.insert(signature_latencies, sign_latency)
        total_signature_ops = total_signature_ops + 1

        -- Verify signature
        local verify_latency = pq_helpers.verify_latency()
        local success = math.random() > SIGNATURE_FAILURE_RATE
        sim:record_pq_verification(receiver, sender, verify_latency, success)
        table.insert(verification_latencies, verify_latency)
        total_verification_ops = total_verification_ops + 1

        -- Send network message with signature
        sim:send_message(sender, receiver, "signed_msg")
    end

    sim:step()

    -- Progress logging
    if tick % 20 == 0 then
        indras.log.info("Phase 1 progress", {
            trace_id = ctx.trace_id,
            tick = tick,
            total_ticks = phase1_ticks,
            signatures_created = total_signature_ops,
            verifications = total_verification_ops,
            avg_sign_latency_us = pq_helpers.average(signature_latencies),
            avg_verify_latency_us = pq_helpers.average(verification_latencies)
        })
    end
end

indras.log.info("Phase 1 completed", {
    trace_id = ctx.trace_id,
    signatures_created = total_signature_ops,
    verifications = total_verification_ops,
    signature_failures = sim.stats.pq_signature_failures,
    p50_sign_latency_us = pq_helpers.percentile(signature_latencies, 50),
    p95_sign_latency_us = pq_helpers.percentile(signature_latencies, 95),
    p99_sign_latency_us = pq_helpers.percentile(signature_latencies, 99)
})

-- Phase 2: Concurrent KEM encapsulation/decapsulation
indras.log.info("Phase 2: KEM stress test", {
    trace_id = ctx.trace_id,
    target_operations = config.kem_ops
})

local phase2_ticks = math.ceil(config.ticks * 0.4)
local kems_per_tick = math.ceil(config.kem_ops / phase2_ticks)

for tick = 1, phase2_ticks do
    for _ = 1, kems_per_tick do
        local initiator, target = random_peer_pair()

        -- Encapsulation
        local encap_latency = pq_helpers.encap_latency()
        sim:record_kem_encapsulation(initiator, target, encap_latency)
        table.insert(encap_latencies, encap_latency)
        total_kem_encap_ops = total_kem_encap_ops + 1

        -- Decapsulation
        local decap_latency = pq_helpers.decap_latency()
        local success = math.random() > KEM_FAILURE_RATE
        sim:record_kem_decapsulation(target, initiator, decap_latency, success)
        table.insert(decap_latencies, decap_latency)
        total_kem_decap_ops = total_kem_decap_ops + 1
    end

    sim:step()

    -- Progress logging
    if tick % 20 == 0 then
        indras.log.info("Phase 2 progress", {
            trace_id = ctx.trace_id,
            tick = tick,
            total_ticks = phase2_ticks,
            encapsulations = total_kem_encap_ops,
            decapsulations = total_kem_decap_ops,
            avg_encap_latency_us = pq_helpers.average(encap_latencies),
            avg_decap_latency_us = pq_helpers.average(decap_latencies)
        })
    end
end

indras.log.info("Phase 2 completed", {
    trace_id = ctx.trace_id,
    encapsulations = total_kem_encap_ops,
    decapsulations = total_kem_decap_ops,
    kem_failures = sim.stats.pq_kem_failures,
    p50_encap_latency_us = pq_helpers.percentile(encap_latencies, 50),
    p95_encap_latency_us = pq_helpers.percentile(encap_latencies, 95),
    p99_encap_latency_us = pq_helpers.percentile(encap_latencies, 99)
})

-- Phase 3: Key distribution (invite creation/acceptance)
indras.log.info("Phase 3: Key distribution stress test", {
    trace_id = ctx.trace_id,
    interface_count = config.peers
})

local phase3_ticks = config.ticks - phase1_ticks - phase2_ticks

for tick = 1, phase3_ticks do
    -- Create interfaces with invites
    local creator = random_peer()
    local interface_id = string.format("stress-interface-%d-%s", tick, tostring(creator))

    -- Each peer creates invites for a subset of other peers
    local invite_count = math.min(5, config.peers - 1)
    for i = 1, invite_count do
        local member = all_peers[(math.random(#all_peers - 1) % (#all_peers - 1)) + 1]
        if member ~= creator then
            -- Create invite
            sim:record_invite_created(creator, member, interface_id)
            total_invites_created = total_invites_created + 1

            -- KEM operations for invite
            local encap_latency = pq_helpers.encap_latency()
            sim:record_kem_encapsulation(creator, member, encap_latency)
            table.insert(encap_latencies, encap_latency)

            local decap_latency = pq_helpers.decap_latency()
            local success = math.random() > KEM_FAILURE_RATE
            sim:record_kem_decapsulation(member, creator, decap_latency, success)
            table.insert(decap_latencies, decap_latency)

            -- Accept or fail invite based on KEM result
            if success then
                sim:record_invite_accepted(member, interface_id)
                total_invites_accepted = total_invites_accepted + 1
            else
                sim:record_invite_failed(member, interface_id, "KEM decapsulation failed")
                total_invites_failed = total_invites_failed + 1
            end
        end
    end

    sim:step()

    -- Progress logging
    if tick % 20 == 0 then
        indras.log.info("Phase 3 progress", {
            trace_id = ctx.trace_id,
            tick = tick,
            total_ticks = phase3_ticks,
            invites_created = total_invites_created,
            invites_accepted = total_invites_accepted,
            invites_failed = total_invites_failed
        })
    end
end

indras.log.info("Phase 3 completed", {
    trace_id = ctx.trace_id,
    invites_created = total_invites_created,
    invites_accepted = total_invites_accepted,
    invites_failed = total_invites_failed,
    invite_success_rate = total_invites_created > 0 and (total_invites_accepted / total_invites_created) or 0
})

-- Calculate final statistics and percentiles
local stats = sim.stats

local sig_percentiles = pq_helpers.percentiles(signature_latencies)
local verify_percentiles = pq_helpers.percentiles(verification_latencies)
local encap_percentiles = pq_helpers.percentiles(encap_latencies)
local decap_percentiles = pq_helpers.percentiles(decap_latencies)

indras.log.info("Crypto stress test completed", {
    trace_id = ctx.trace_id,
    level = level,
    final_tick = sim.tick,
    -- Signature metrics
    signatures_created = stats.pq_signatures_created,
    signatures_verified = stats.pq_signatures_verified,
    signature_failures = stats.pq_signature_failures,
    signature_failure_rate = stats:signature_failure_rate(),
    signature_latency_p50_us = sig_percentiles.p50,
    signature_latency_p95_us = sig_percentiles.p95,
    signature_latency_p99_us = sig_percentiles.p99,
    verification_latency_p50_us = verify_percentiles.p50,
    verification_latency_p95_us = verify_percentiles.p95,
    verification_latency_p99_us = verify_percentiles.p99,
    -- KEM metrics
    kem_encapsulations = stats.pq_kem_encapsulations,
    kem_decapsulations = stats.pq_kem_decapsulations,
    kem_failures = stats.pq_kem_failures,
    kem_failure_rate = stats:kem_failure_rate(),
    encap_latency_p50_us = encap_percentiles.p50,
    encap_latency_p95_us = encap_percentiles.p95,
    encap_latency_p99_us = encap_percentiles.p99,
    decap_latency_p50_us = decap_percentiles.p50,
    decap_latency_p95_us = decap_percentiles.p95,
    decap_latency_p99_us = decap_percentiles.p99,
    -- Invite metrics
    invites_created = stats.invites_created,
    invites_accepted = stats.invites_accepted,
    invites_failed = stats.invites_failed,
    invite_success_rate = stats:invite_success_rate(),
    -- Network metrics
    messages_sent = stats.messages_sent,
    messages_delivered = stats.messages_delivered,
    delivery_rate = stats:delivery_rate()
})

-- Assertions
indras.assert.eq(stats.pq_signatures_created, total_signature_ops, "All signatures should be created")
indras.assert.eq(stats.pq_signatures_verified + stats.pq_signature_failures, total_verification_ops,
    "All verifications should complete (success or failure)")

-- Signature failure rate should be close to expected rate
local actual_sig_failure_rate = stats:signature_failure_rate()
indras.assert.lt(math.abs(actual_sig_failure_rate - SIGNATURE_FAILURE_RATE), 0.002,
    "Signature failure rate should be close to " .. SIGNATURE_FAILURE_RATE)

-- P99 latency thresholds (allow for variance)
indras.assert.lt(sig_percentiles.p99, 500, "P99 signature latency should be under 500us")
indras.assert.lt(verify_percentiles.p99, 300, "P99 verification latency should be under 300us")
indras.assert.lt(encap_percentiles.p99, 150, "P99 encapsulation latency should be under 150us")
indras.assert.lt(decap_percentiles.p99, 150, "P99 decapsulation latency should be under 150us")

-- KEM operations should mostly succeed
indras.assert.gt(stats.pq_kem_decapsulations, 0, "Should have successful KEM decapsulations")
local actual_kem_failure_rate = stats:kem_failure_rate()
indras.assert.lt(math.abs(actual_kem_failure_rate - KEM_FAILURE_RATE), 0.001,
    "KEM failure rate should be close to " .. KEM_FAILURE_RATE)

-- Invite success rate should be high (close to 1 - KEM_FAILURE_RATE)
local expected_invite_success_rate = 1.0 - KEM_FAILURE_RATE
indras.assert.gt(stats:invite_success_rate(), expected_invite_success_rate - 0.01,
    "Invite success rate should be close to " .. expected_invite_success_rate)

indras.log.info("Crypto stress test passed all assertions", {
    trace_id = ctx.trace_id,
    level = level
})

-- Return comprehensive metrics
return {
    level = level,
    peers = config.peers,
    -- Signature metrics
    total_signatures = stats.pq_signatures_created,
    signature_failure_rate = actual_sig_failure_rate,
    signature_latency = {
        p50 = sig_percentiles.p50,
        p95 = sig_percentiles.p95,
        p99 = sig_percentiles.p99,
        avg = pq_helpers.average(signature_latencies),
    },
    verification_latency = {
        p50 = verify_percentiles.p50,
        p95 = verify_percentiles.p95,
        p99 = verify_percentiles.p99,
        avg = pq_helpers.average(verification_latencies),
    },
    -- KEM metrics
    total_kem_ops = stats.pq_kem_encapsulations,
    kem_failure_rate = actual_kem_failure_rate,
    encap_latency = {
        p50 = encap_percentiles.p50,
        p95 = encap_percentiles.p95,
        p99 = encap_percentiles.p99,
        avg = pq_helpers.average(encap_latencies),
    },
    decap_latency = {
        p50 = decap_percentiles.p50,
        p95 = decap_percentiles.p95,
        p99 = decap_percentiles.p99,
        avg = pq_helpers.average(decap_latencies),
    },
    -- Invite metrics
    total_invites = stats.invites_created,
    invite_success_rate = stats:invite_success_rate(),
    -- Network metrics
    delivery_rate = stats:delivery_rate(),
}

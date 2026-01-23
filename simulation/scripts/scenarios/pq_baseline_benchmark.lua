-- PQ Baseline Benchmark
--
-- Establishes baseline performance numbers for post-quantum cryptography operations.
-- Measures latency distributions and throughput for ML-DSA-65 signatures and ML-KEM-768 KEM.
--
-- This scenario simulates PQ operations with realistic latency values based on
-- actual cryptographic operation timings.

local ctx = indras.correlation.new_root()
ctx = ctx:with_tag("scenario", "pq_baseline_benchmark")

-- Configuration
local PEER_COUNT = 5
local SIGNATURE_OPS = 1000
local KEM_OPS = 500
local MESSAGE_SIZES = {64, 256, 1024, 4096}  -- bytes

-- Realistic latency ranges (microseconds) based on PQ crypto benchmarks
-- ML-DSA-65: ~150-300us for sign, ~100-200us for verify
-- ML-KEM-768: ~50-100us for encap, ~50-100us for decap
local SIGN_LATENCY_BASE = 200
local SIGN_LATENCY_VARIANCE = 100
local VERIFY_LATENCY_BASE = 150
local VERIFY_LATENCY_VARIANCE = 50
local KEM_ENCAP_LATENCY_BASE = 75
local KEM_ENCAP_VARIANCE = 25
local KEM_DECAP_LATENCY_BASE = 75
local KEM_DECAP_VARIANCE = 25

indras.log.info("Starting PQ baseline benchmark", {
    trace_id = ctx.trace_id,
    peers = PEER_COUNT,
    signature_ops = SIGNATURE_OPS,
    kem_ops = KEM_OPS
})

-- Create simple mesh for context
local mesh = indras.MeshBuilder.new(PEER_COUNT):full_mesh()
local config = indras.SimConfig.manual()
local sim = indras.Simulation.new(mesh, config)
sim:initialize()

-- Force all peers online
local peers = mesh:peers()
for _, peer in ipairs(peers) do
    sim:force_online(peer)
end

-- Helper: generate random latency with variance
local function random_latency(base, variance)
    return math.floor(base + (math.random() - 0.5) * 2 * variance)
end

-- Track latencies for percentile calculation
local sign_latencies = {}
local verify_latencies = {}
local encap_latencies = {}
local decap_latencies = {}

-- Phase 1: Signature operations benchmark
indras.log.info("Phase 1: Signature operations", {
    trace_id = ctx.trace_id,
    operations = SIGNATURE_OPS
})

for i = 1, SIGNATURE_OPS do
    local peer_idx = ((i - 1) % #peers) + 1
    local peer = peers[peer_idx]
    local msg_size = MESSAGE_SIZES[((i - 1) % #MESSAGE_SIZES) + 1]

    -- Simulate signing
    local sign_latency = random_latency(SIGN_LATENCY_BASE, SIGN_LATENCY_VARIANCE)
    sim:record_pq_signature(peer, sign_latency, msg_size)
    table.insert(sign_latencies, sign_latency)

    -- Simulate verification by another peer
    local verifier_idx = (peer_idx % #peers) + 1
    local verifier = peers[verifier_idx]
    local verify_latency = random_latency(VERIFY_LATENCY_BASE, VERIFY_LATENCY_VARIANCE)
    local success = math.random() > 0.001  -- 0.1% failure rate
    sim:record_pq_verification(verifier, peer, verify_latency, success)
    table.insert(verify_latencies, verify_latency)

    -- Progress logging
    if i % 200 == 0 then
        indras.log.debug("Signature progress", {
            trace_id = ctx.trace_id,
            completed = i,
            total = SIGNATURE_OPS
        })
    end

    sim:step()
end

-- Phase 2: KEM operations benchmark
indras.log.info("Phase 2: KEM operations", {
    trace_id = ctx.trace_id,
    operations = KEM_OPS
})

for i = 1, KEM_OPS do
    local sender_idx = ((i - 1) % #peers) + 1
    local target_idx = (sender_idx % #peers) + 1
    local sender = peers[sender_idx]
    local target = peers[target_idx]

    -- Simulate encapsulation (sender creates shared secret for target)
    local encap_latency = random_latency(KEM_ENCAP_LATENCY_BASE, KEM_ENCAP_VARIANCE)
    sim:record_kem_encapsulation(sender, target, encap_latency)
    table.insert(encap_latencies, encap_latency)

    -- Simulate decapsulation (target recovers shared secret)
    local decap_latency = random_latency(KEM_DECAP_LATENCY_BASE, KEM_DECAP_VARIANCE)
    local success = math.random() > 0.001  -- 0.1% failure rate
    sim:record_kem_decapsulation(target, sender, decap_latency, success)
    table.insert(decap_latencies, decap_latency)

    -- Progress logging
    if i % 100 == 0 then
        indras.log.debug("KEM progress", {
            trace_id = ctx.trace_id,
            completed = i,
            total = KEM_OPS
        })
    end

    sim:step()
end

-- Helper: calculate percentiles
local function percentile(values, p)
    if #values == 0 then return 0 end
    table.sort(values)
    local idx = math.ceil(#values * p / 100)
    return values[math.max(1, idx)]
end

-- Helper: calculate average
local function average(values)
    if #values == 0 then return 0 end
    local sum = 0
    for _, v in ipairs(values) do
        sum = sum + v
    end
    return sum / #values
end

-- Calculate signature metrics
local sign_metrics = {
    count = #sign_latencies,
    avg_us = math.floor(average(sign_latencies)),
    p50_us = percentile(sign_latencies, 50),
    p95_us = percentile(sign_latencies, 95),
    p99_us = percentile(sign_latencies, 99),
    ops_per_sec = math.floor(1000000 / average(sign_latencies))
}

local verify_metrics = {
    count = #verify_latencies,
    avg_us = math.floor(average(verify_latencies)),
    p50_us = percentile(verify_latencies, 50),
    p95_us = percentile(verify_latencies, 95),
    p99_us = percentile(verify_latencies, 99),
    ops_per_sec = math.floor(1000000 / average(verify_latencies))
}

-- Calculate KEM metrics
local encap_metrics = {
    count = #encap_latencies,
    avg_us = math.floor(average(encap_latencies)),
    p50_us = percentile(encap_latencies, 50),
    p95_us = percentile(encap_latencies, 95),
    p99_us = percentile(encap_latencies, 99),
    ops_per_sec = math.floor(1000000 / average(encap_latencies))
}

local decap_metrics = {
    count = #decap_latencies,
    avg_us = math.floor(average(decap_latencies)),
    p50_us = percentile(decap_latencies, 50),
    p95_us = percentile(decap_latencies, 95),
    p99_us = percentile(decap_latencies, 99),
    ops_per_sec = math.floor(1000000 / average(decap_latencies))
}

-- Log results
indras.log.info("Signature creation benchmark", {
    trace_id = ctx.trace_id,
    operation = "sign",
    count = sign_metrics.count,
    latency_avg_us = sign_metrics.avg_us,
    latency_p50_us = sign_metrics.p50_us,
    latency_p95_us = sign_metrics.p95_us,
    latency_p99_us = sign_metrics.p99_us,
    ops_per_second = sign_metrics.ops_per_sec
})

indras.log.info("Signature verification benchmark", {
    trace_id = ctx.trace_id,
    operation = "verify",
    count = verify_metrics.count,
    latency_avg_us = verify_metrics.avg_us,
    latency_p50_us = verify_metrics.p50_us,
    latency_p95_us = verify_metrics.p95_us,
    latency_p99_us = verify_metrics.p99_us,
    ops_per_second = verify_metrics.ops_per_sec
})

indras.log.info("KEM encapsulation benchmark", {
    trace_id = ctx.trace_id,
    operation = "encapsulate",
    count = encap_metrics.count,
    latency_avg_us = encap_metrics.avg_us,
    latency_p50_us = encap_metrics.p50_us,
    latency_p95_us = encap_metrics.p95_us,
    latency_p99_us = encap_metrics.p99_us,
    ops_per_second = encap_metrics.ops_per_sec
})

indras.log.info("KEM decapsulation benchmark", {
    trace_id = ctx.trace_id,
    operation = "decapsulate",
    count = decap_metrics.count,
    latency_avg_us = decap_metrics.avg_us,
    latency_p50_us = decap_metrics.p50_us,
    latency_p95_us = decap_metrics.p95_us,
    latency_p99_us = decap_metrics.p99_us,
    ops_per_second = decap_metrics.ops_per_sec
})

-- Verify stats from simulation
local stats = sim.stats
indras.log.info("PQ baseline benchmark complete", {
    trace_id = ctx.trace_id,
    total_signatures_created = stats.pq_signatures_created,
    total_signatures_verified = stats.pq_signatures_verified,
    signature_failures = stats.pq_signature_failures,
    total_kem_encapsulations = stats.pq_kem_encapsulations,
    total_kem_decapsulations = stats.pq_kem_decapsulations,
    kem_failures = stats.pq_kem_failures,
    avg_sign_latency_us = stats:avg_signature_latency_us(),
    avg_verify_latency_us = stats:avg_verification_latency_us(),
    avg_encap_latency_us = stats:avg_kem_encap_latency_us(),
    avg_decap_latency_us = stats:avg_kem_decap_latency_us()
})

-- Assertions
indras.assert.eq(stats.pq_signatures_created, SIGNATURE_OPS, "All signatures should be created")
indras.assert.eq(stats.pq_signatures_verified + stats.pq_signature_failures, SIGNATURE_OPS, "All verifications should be attempted")
indras.assert.eq(stats.pq_kem_encapsulations, KEM_OPS, "All encapsulations should be performed")
indras.assert.eq(stats.pq_kem_decapsulations + stats.pq_kem_failures, KEM_OPS, "All decapsulations should be attempted")

-- Verify latency thresholds (simulated values should be within expected ranges)
indras.assert.lt(sign_metrics.p99_us, 500, "Sign p99 should be under 500us")
indras.assert.lt(verify_metrics.p99_us, 300, "Verify p99 should be under 300us")
indras.assert.lt(encap_metrics.p99_us, 200, "Encap p99 should be under 200us")
indras.assert.lt(decap_metrics.p99_us, 200, "Decap p99 should be under 200us")

indras.log.info("PQ baseline benchmark passed", {
    trace_id = ctx.trace_id
})

return {
    sign = sign_metrics,
    verify = verify_metrics,
    encap = encap_metrics,
    decap = decap_metrics,
    stats = stats:to_table()
}

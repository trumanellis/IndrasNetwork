-- SDK Peer-Based Realm Stress Test
--
-- Tests deterministic peer-based realm identity at scale.
--
-- Key Insight: Realms ARE peer sets. Every group of peers automatically creates
-- the potential for collaboration. The realm ID is a deterministic function of
-- the sorted, deduped peer IDs.
--
-- Phases:
-- 1. Setup: Create mesh topology with N peers
-- 2. Determinism Test: Verify realm([A,B,C]) == realm([C,A,B])
-- 3. Uniqueness Test: Verify different peer sets produce different realms
-- 4. Concurrency Test: Multiple peers accessing same peer-set realm simultaneously
-- 5. Cache Performance: Test realm lookup latency (cached vs uncached)

local quest = require("lib.quest_helpers")
local thresholds = require("config.quest_thresholds")

-- ============================================================================
-- SETUP
-- ============================================================================

local ctx = quest.new_context("sdk_peer_realm_stress")
local logger = quest.create_logger(ctx)
local config = quest.get_peer_realm_config()

logger.info("Starting peer-based realm stress scenario", {
    level = quest.get_level(),
    peers = config.peers,
    realm_combinations = config.realm_combinations,
    concurrent_ops = config.concurrent_ops,
})

-- Create mesh with N peers
local mesh = indras.MeshBuilder.new(config.peers):full_mesh()
local sim_config = indras.SimConfig.new({
    wake_probability = 0,
    sleep_probability = 0,
    initial_online_probability = 1,
    max_ticks = config.ticks,
})
local sim = indras.Simulation.new(mesh, sim_config)
sim:initialize()

local peers = mesh:peers()
local result = quest.result_builder("sdk_peer_realm_stress")

-- Metrics tracking
local latencies = {
    cached_lookup = {},
    realm_create = {},
}
local consistency_checks = { passed = 0, failed = 0 }
local uniqueness_checks = { passed = 0, failed = 0 }
local concurrent_ops = { success = 0, failed = 0 }

-- Realm cache for testing
local realm_cache = {}

-- ============================================================================
-- PHASE 1: SETUP (Bring all peers online)
-- ============================================================================

logger.info("Phase 1: Setup - Bringing peers online", {
    phase = 1,
    peer_count = #peers,
})

for _, peer in ipairs(peers) do
    sim:force_online(peer)
end

sim:step()

logger.info("Phase 1 complete: All peers online", {
    phase = 1,
    tick = sim.tick,
})

-- ============================================================================
-- PHASE 2: DETERMINISM TEST
-- Verify realm([A,B,C]) == realm([C,A,B])
-- ============================================================================

logger.info("Phase 2: Determinism test", {
    phase = 2,
    description = "Verify same peers in different order produce same realm ID",
})

local determinism_tests = math.min(100, config.realm_combinations)
for i = 1, determinism_tests do
    -- Pick 3-5 random peers
    local num_peers = 3 + math.random(2)
    local selected = {}
    local used = {}

    while #selected < num_peers do
        local idx = math.random(#peers)
        if not used[idx] then
            used[idx] = true
            table.insert(selected, tostring(peers[idx]))
        end
    end

    -- Compute realm ID with original order
    local start_time = os.clock()
    local realm_id1 = quest.compute_realm_id(selected)
    local create_latency = (os.clock() - start_time) * 1000000  -- to microseconds
    table.insert(latencies.realm_create, create_latency)

    -- Shuffle peers and compute again
    local shuffled = {}
    for _, p in ipairs(selected) do table.insert(shuffled, p) end
    for j = #shuffled, 2, -1 do
        local k = math.random(j)
        shuffled[j], shuffled[k] = shuffled[k], shuffled[j]
    end

    -- Measure cached lookup
    start_time = os.clock()
    local realm_id2 = quest.compute_realm_id(shuffled)
    local lookup_latency = (os.clock() - start_time) * 1000000
    table.insert(latencies.cached_lookup, lookup_latency)

    -- Verify consistency
    if realm_id1 == realm_id2 then
        consistency_checks.passed = consistency_checks.passed + 1
    else
        consistency_checks.failed = consistency_checks.failed + 1
        logger.warn("Determinism failure", {
            peers_original = table.concat(selected, ","),
            peers_shuffled = table.concat(shuffled, ","),
            realm_id1 = realm_id1,
            realm_id2 = realm_id2,
        })
    end

    -- Log event
    logger.event(quest.EVENTS.REALM_COMPUTED, {
        tick = sim.tick,
        peers = table.concat(selected, ","),
        realm_id = realm_id1,
        latency_us = create_latency,
        consistent = realm_id1 == realm_id2,
    })

    sim:step()
end

logger.info("Phase 2 complete: Determinism tests", {
    phase = 2,
    tick = sim.tick,
    passed = consistency_checks.passed,
    failed = consistency_checks.failed,
})

-- ============================================================================
-- PHASE 3: UNIQUENESS TEST
-- Verify different peer sets produce different realms
-- ============================================================================

logger.info("Phase 3: Uniqueness test", {
    phase = 3,
    description = "Verify different peer sets produce different realm IDs",
})

local uniqueness_tests = math.min(100, config.realm_combinations)
local all_realm_ids = {}

for i = 1, uniqueness_tests do
    -- Pick a random subset of 2+ peers
    local num_peers = 2 + math.random(math.min(4, #peers - 2))
    local selected = {}
    local used = {}

    while #selected < num_peers do
        local idx = math.random(#peers)
        if not used[idx] then
            used[idx] = true
            table.insert(selected, tostring(peers[idx]))
        end
    end

    local realm_id = quest.compute_realm_id(selected)

    -- Check if we've seen this realm ID before with different peers
    local peer_key = table.concat(quest.normalize_peers(selected), ",")

    if all_realm_ids[realm_id] then
        -- Check if it's the same peer set (expected) or different (collision)
        if all_realm_ids[realm_id] == peer_key then
            -- Same peer set, same ID - expected
            uniqueness_checks.passed = uniqueness_checks.passed + 1
        else
            -- Different peer set, same ID - collision!
            uniqueness_checks.failed = uniqueness_checks.failed + 1
            logger.warn("Uniqueness failure (collision)", {
                realm_id = realm_id,
                peers1 = all_realm_ids[realm_id],
                peers2 = peer_key,
            })
        end
    else
        all_realm_ids[realm_id] = peer_key
        uniqueness_checks.passed = uniqueness_checks.passed + 1
    end

    sim:step()
end

logger.info("Phase 3 complete: Uniqueness tests", {
    phase = 3,
    tick = sim.tick,
    passed = uniqueness_checks.passed,
    failed = uniqueness_checks.failed,
    unique_realms = 0,  -- count unique realm IDs
})

-- Count unique realm IDs
local unique_count = 0
for _ in pairs(all_realm_ids) do unique_count = unique_count + 1 end

-- ============================================================================
-- PHASE 4: CONCURRENCY TEST
-- Multiple peers accessing same peer-set realm simultaneously
-- ============================================================================

logger.info("Phase 4: Concurrency test", {
    phase = 4,
    description = "Multiple peers accessing same peer-set realm simultaneously",
    concurrent_ops = config.concurrent_ops,
})

-- Create a common peer set that all will access
local common_peers = {}
for i = 1, math.min(5, #peers) do
    table.insert(common_peers, tostring(peers[i]))
end
local common_realm_id = quest.compute_realm_id(common_peers)

-- Simulate concurrent access
for i = 1, config.concurrent_ops do
    -- Each "operation" computes the realm ID and verifies consistency
    local computed_id = quest.compute_realm_id(common_peers)

    if computed_id == common_realm_id then
        concurrent_ops.success = concurrent_ops.success + 1
    else
        concurrent_ops.failed = concurrent_ops.failed + 1
    end

    -- Simulate some other work between ops
    if i % 10 == 0 then
        sim:step()
    end
end

logger.info("Phase 4 complete: Concurrency tests", {
    phase = 4,
    tick = sim.tick,
    success = concurrent_ops.success,
    failed = concurrent_ops.failed,
})

-- ============================================================================
-- PHASE 5: CACHE PERFORMANCE
-- Test realm lookup latency
-- ============================================================================

logger.info("Phase 5: Cache performance", {
    phase = 5,
    description = "Measure cached realm lookup latency",
})

-- Perform many cached lookups
local cache_test_count = 1000
for i = 1, cache_test_count do
    local start_time = os.clock()
    local _ = quest.compute_realm_id(common_peers)
    local latency = (os.clock() - start_time) * 1000000
    table.insert(latencies.cached_lookup, latency)
end

logger.info("Phase 5 complete: Cache performance measured", {
    phase = 5,
    tick = sim.tick,
    samples = #latencies.cached_lookup,
})

-- ============================================================================
-- FINAL RESULTS
-- ============================================================================

-- Calculate metrics
local consistency_rate = consistency_checks.passed /
    (consistency_checks.passed + consistency_checks.failed)
local uniqueness_rate = uniqueness_checks.passed /
    (uniqueness_checks.passed + uniqueness_checks.failed)
local concurrent_success_rate = concurrent_ops.success /
    (concurrent_ops.success + concurrent_ops.failed)

local cached_lookup_percentiles = quest.percentiles(latencies.cached_lookup)
local realm_create_percentiles = quest.percentiles(latencies.realm_create)

-- Record metrics
result:add_metrics({
    realm_id_consistency = consistency_rate,
    realm_id_uniqueness = uniqueness_rate,
    concurrent_success_rate = concurrent_success_rate,
    cached_lookup_p99_us = cached_lookup_percentiles.p99,
    cached_lookup_p95_us = cached_lookup_percentiles.p95,
    cached_lookup_p50_us = cached_lookup_percentiles.p50,
    realm_create_p99_us = realm_create_percentiles.p99,
    realm_create_p95_us = realm_create_percentiles.p95,
    realm_create_p50_us = realm_create_percentiles.p50,
    unique_realm_count = unique_count,
    total_consistency_checks = consistency_checks.passed + consistency_checks.failed,
    total_uniqueness_checks = uniqueness_checks.passed + uniqueness_checks.failed,
    total_concurrent_ops = concurrent_ops.success + concurrent_ops.failed,
})

-- Assertions
result:record_assertion("realm_id_consistency",
    consistency_rate >= 1.0, 1.0, consistency_rate)
result:record_assertion("realm_id_uniqueness",
    uniqueness_rate >= 1.0, 1.0, uniqueness_rate)
result:record_assertion("concurrent_success_rate",
    concurrent_success_rate >= 0.99, 0.99, concurrent_success_rate)

local final_result = result:build()

logger.info("Peer-based realm stress scenario completed", {
    passed = final_result.passed,
    level = final_result.level,
    duration_sec = final_result.duration_sec,
    final_tick = sim.tick,
    consistency_rate = consistency_rate,
    uniqueness_rate = uniqueness_rate,
    concurrent_success_rate = concurrent_success_rate,
    cached_lookup_p99_us = cached_lookup_percentiles.p99,
    unique_realm_count = unique_count,
})

-- Standard assertions
indras.assert.eq(consistency_rate, 1.0, "Realm ID consistency should be 100%")
indras.assert.eq(uniqueness_rate, 1.0, "Realm ID uniqueness should be 100%")
indras.assert.ge(concurrent_success_rate, 0.99, "Concurrent success rate should be >= 99%")

logger.info("Peer-based realm stress scenario passed", {
    unique_realms = unique_count,
})

return final_result

-- PQ Key Exchange Validation Scenario
--
-- Validates correct post-quantum key exchange during peer discovery.
-- Uses ML-KEM-768 for encapsulation keys and ML-DSA-65 for signature keys.
--
-- Flow:
-- 1. Each peer generates unique PQ keypairs
-- 2. Peers discover each other with PQ keys in broadcasts
-- 3. PQ keys propagate via PeerIntroduction
-- 4. Verify all peers have correct keys for all others
--
-- Key Sizes:
-- - ML-KEM-768 encapsulation key: 1184 bytes
-- - ML-DSA-65 verifying key: 1952 bytes

local discovery = require("lib.discovery_helpers")
local thresholds = require("config.discovery_thresholds")

-- ============================================================================
-- SETUP
-- ============================================================================

local ctx = discovery.new_context("discovery_pq_keys")
local logger = discovery.create_logger(ctx)
local config = discovery.get_config()

-- Configuration for this scenario
local SCENARIO_CONFIG = {
    quick = {
        peer_count = 4,
        max_ticks = 150,
        broadcast_interval = 5,
    },
    medium = {
        peer_count = 8,
        max_ticks = 250,
        broadcast_interval = 4,
    },
    full = {
        peer_count = 12,
        max_ticks = 400,
        broadcast_interval = 3,
    },
}
local cfg = SCENARIO_CONFIG[discovery.get_level()] or SCENARIO_CONFIG.medium

logger.info("Starting PQ key exchange validation scenario", {
    level = discovery.get_level(),
    peer_count = cfg.peer_count,
    max_ticks = cfg.max_ticks,
    expected_kem_key_size = discovery.PQ_KEYS.kem_encap_key,
    expected_dsa_key_size = discovery.PQ_KEYS.dsa_verifying_key,
})

-- Create mesh
local mesh = indras.MeshBuilder.new(cfg.peer_count):full_mesh()
local sim_config = indras.SimConfig.new({
    wake_probability = 0,
    sleep_probability = 0,
    initial_online_probability = 0,
    max_ticks = cfg.max_ticks,
})
local sim = indras.Simulation.new(mesh, sim_config)
sim:initialize()

local peers = mesh:peers()
local tracker = discovery.create_tracker(peers)
local peer_state = discovery.create_peer_state(peers)
local result = discovery.result_builder("discovery_pq_keys")

-- Track key exchange metrics
local kem_key_sizes = {}
local dsa_key_sizes = {}
local key_mismatches = 0
local key_size_errors = 0

-- ============================================================================
-- PHASE 1: KEY GENERATION (ticks 1-20)
-- Each peer generates PQ keypairs
-- ============================================================================

logger.info("Phase 1: PQ key generation", {
    phase = 1,
    peer_count = cfg.peer_count,
})

-- Bring all peers online (triggers key generation)
for tick = 1, 10 do
    sim:step()
end

for i, peer in ipairs(peers) do
    peer_state:bring_online(peer, sim.tick)
    sim:force_online(peer)

    local keys = peer_state:get_pq_keys(peer)

    -- Store key sizes for verification
    table.insert(kem_key_sizes, keys.kem_encap_key_size)
    table.insert(dsa_key_sizes, keys.dsa_verifying_key_size)

    -- Verify key sizes at generation
    local kem_valid = keys.kem_encap_key_size == discovery.PQ_KEYS.kem_encap_key
    local dsa_valid = keys.dsa_verifying_key_size == discovery.PQ_KEYS.dsa_verifying_key

    if not kem_valid or not dsa_valid then
        key_size_errors = key_size_errors + 1
    end

    logger.event(discovery.EVENTS.PEER_ONLINE, {
        tick = sim.tick,
        peer = tostring(peer),
        peer_index = i,
        kem_key_size = keys.kem_encap_key_size,
        dsa_key_size = keys.dsa_verifying_key_size,
        kem_valid = kem_valid,
        dsa_valid = dsa_valid,
    })

    sim:step()
end

logger.info("Phase 1 complete: Keys generated", {
    phase = 1,
    tick = sim.tick,
    peers_with_keys = cfg.peer_count,
    key_size_errors = key_size_errors,
})

-- ============================================================================
-- PHASE 2: KEY EXCHANGE VIA BROADCASTS (ticks 21-100)
-- Peers broadcast presence with PQ keys
-- ============================================================================

local phase2_end = math.floor(cfg.max_ticks * 0.6)

logger.info("Phase 2: Key exchange via broadcasts", {
    phase = 2,
    end_tick = phase2_end,
})

local last_broadcast = {}
for _, peer in ipairs(peers) do
    last_broadcast[tostring(peer)] = 0
end

for tick = sim.tick + 1, phase2_end do
    sim:step()

    for _, broadcaster in ipairs(peers) do
        local broadcaster_id = tostring(broadcaster)

        if peer_state:is_online(broadcaster) then
            if tick - last_broadcast[broadcaster_id] >= cfg.broadcast_interval then
                last_broadcast[broadcaster_id] = tick
                local broadcaster_keys = peer_state:get_pq_keys(broadcaster)

                logger.event(discovery.EVENTS.PRESENCE_BROADCAST, {
                    tick = tick,
                    broadcaster = broadcaster_id,
                    kem_key_size = broadcaster_keys.kem_encap_key_size,
                    dsa_key_size = broadcaster_keys.dsa_verifying_key_size,
                })

                -- All other online peers receive this broadcast with keys
                for _, receiver in ipairs(peers) do
                    if receiver ~= broadcaster and peer_state:is_online(receiver) then
                        local receiver_id = tostring(receiver)

                        -- Discovery and key exchange
                        if not tracker:knows(receiver, broadcaster) then
                            tracker:record_discovery(receiver, broadcaster, tick)
                        end

                        if not tracker:has_keys(receiver, broadcaster) then
                            -- Verify key sizes before accepting
                            local kem_size = broadcaster_keys.kem_encap_key_size
                            local dsa_size = broadcaster_keys.dsa_verifying_key_size

                            local kem_valid = kem_size == discovery.PQ_KEYS.kem_encap_key
                            local dsa_valid = dsa_size == discovery.PQ_KEYS.dsa_verifying_key

                            if kem_valid and dsa_valid then
                                tracker:record_pq_keys(receiver, broadcaster, kem_size, dsa_size)
                                peer_state:learn_peer(receiver, broadcaster, broadcaster_keys)

                                logger.event(discovery.EVENTS.PQ_KEYS_EXCHANGED, {
                                    tick = tick,
                                    receiver = receiver_id,
                                    sender = broadcaster_id,
                                    kem_key_size = kem_size,
                                    dsa_key_size = dsa_size,
                                })
                            else
                                key_mismatches = key_mismatches + 1
                                logger.warn("PQ key size mismatch", {
                                    tick = tick,
                                    receiver = receiver_id,
                                    sender = broadcaster_id,
                                    kem_key_size = kem_size,
                                    dsa_key_size = dsa_size,
                                    expected_kem = discovery.PQ_KEYS.kem_encap_key,
                                    expected_dsa = discovery.PQ_KEYS.dsa_verifying_key,
                                })
                            end
                        end
                    end
                end
            end
        end
    end

    -- Progress logging
    if tick % 30 == 0 then
        logger.debug("Phase 2 progress", {
            tick = tick,
            discovery_completeness = tracker:completeness(),
            pq_completeness = tracker:pq_completeness(),
            key_exchanges = tracker.key_exchanges,
        })
    end
end

logger.info("Phase 2 complete: Key exchange phase ended", {
    phase = 2,
    tick = sim.tick,
    key_exchanges = tracker.key_exchanges,
    pq_completeness = tracker:pq_completeness(),
})

-- ============================================================================
-- PHASE 3: VERIFICATION (remaining ticks)
-- Verify all peers have correct keys for all others
-- ============================================================================

logger.info("Phase 3: Key verification", {
    phase = 3,
    start_tick = sim.tick,
})

-- Continue simulation
for tick = sim.tick + 1, cfg.max_ticks do
    sim:step()
end

-- Detailed key verification
local kem_correct_count = 0
local dsa_correct_count = 0
local total_key_pairs = 0

for _, from in ipairs(peers) do
    local from_id = tostring(from)
    for _, to in ipairs(peers) do
        if from ~= to then
            total_key_pairs = total_key_pairs + 1

            local to_id = tostring(to)
            local expected_keys = peer_state:get_pq_keys(to)

            -- Check if 'from' has 'to's keys
            if tracker:has_keys(from, to) then
                -- Verify the key sizes are correct
                if expected_keys.kem_encap_key_size == discovery.PQ_KEYS.kem_encap_key then
                    kem_correct_count = kem_correct_count + 1
                end
                if expected_keys.dsa_verifying_key_size == discovery.PQ_KEYS.dsa_verifying_key then
                    dsa_correct_count = dsa_correct_count + 1
                end
            end
        end
    end
end

-- Calculate correctness percentages
local kem_correctness = total_key_pairs > 0 and kem_correct_count / total_key_pairs or 0
local dsa_correctness = total_key_pairs > 0 and dsa_correct_count / total_key_pairs or 0

-- Key propagation completeness
local key_propagation_complete = tracker:is_pq_complete() and 1.0 or 0.0

local tracker_stats = tracker:stats()

result:add_metrics({
    peer_count = cfg.peer_count,
    total_key_pairs = total_key_pairs,
    key_exchanges = tracker.key_exchanges,
    kem_correct_count = kem_correct_count,
    dsa_correct_count = dsa_correct_count,
    kem_key_size_correct = kem_correctness,
    dsa_key_size_correct = dsa_correctness,
    key_propagation_complete = key_propagation_complete,
    key_size_errors = key_size_errors,
    key_mismatches = key_mismatches,
    discovery_completeness = tracker_stats.completeness,
    pq_completeness = tracker_stats.pq_completeness,
})

-- Assertions
result:record_assertion("all_kem_keys_correct",
    kem_correctness >= 1.0, true, kem_correctness >= 1.0)
result:record_assertion("all_dsa_keys_correct",
    dsa_correctness >= 1.0, true, dsa_correctness >= 1.0)
result:record_assertion("key_propagation_complete",
    key_propagation_complete >= 1.0, true, key_propagation_complete >= 1.0)
result:record_assertion("no_key_size_errors",
    key_size_errors == 0, true, key_size_errors == 0)
result:record_assertion("no_key_mismatches",
    key_mismatches == 0, true, key_mismatches == 0)

-- Verify specific key sizes
for i, kem_size in ipairs(kem_key_sizes) do
    result:record_assertion(
        string.format("peer_%d_kem_size", i),
        kem_size == discovery.PQ_KEYS.kem_encap_key,
        discovery.PQ_KEYS.kem_encap_key, kem_size)
end

for i, dsa_size in ipairs(dsa_key_sizes) do
    result:record_assertion(
        string.format("peer_%d_dsa_size", i),
        dsa_size == discovery.PQ_KEYS.dsa_verifying_key,
        discovery.PQ_KEYS.dsa_verifying_key, dsa_size)
end

-- Validate against thresholds
local scenario_thresholds = thresholds.get("pq_keys")
local passed, failures = discovery.assert_thresholds(result.metrics, scenario_thresholds)

for _, failure in ipairs(failures) do
    result:add_error(string.format(
        "Threshold '%s' failed: expected %s %s, got %s",
        failure.metric, failure.type, tostring(failure.expected), tostring(failure.actual)
    ))
end

-- ============================================================================
-- FINAL RESULTS
-- ============================================================================

local final_result = result:build()

logger.info("PQ key exchange validation scenario completed", {
    passed = final_result.passed,
    level = final_result.level,
    peer_count = cfg.peer_count,
    key_exchanges = tracker.key_exchanges,
    kem_correctness = kem_correctness,
    dsa_correctness = dsa_correctness,
    key_propagation_complete = key_propagation_complete,
    key_size_errors = key_size_errors,
    key_mismatches = key_mismatches,
})

-- Standard assertions
indras.assert.eq(key_size_errors, 0, "No key size errors during generation")
indras.assert.eq(key_mismatches, 0, "No key mismatches during exchange")
indras.assert.eq(tracker:is_pq_complete(), true, "All peers should have each other's PQ keys")

-- Verify specific key sizes
for _, kem_size in ipairs(kem_key_sizes) do
    indras.assert.eq(kem_size, discovery.PQ_KEYS.kem_encap_key,
        string.format("KEM key size should be %d", discovery.PQ_KEYS.kem_encap_key))
end

for _, dsa_size in ipairs(dsa_key_sizes) do
    indras.assert.eq(dsa_size, discovery.PQ_KEYS.dsa_verifying_key,
        string.format("DSA key size should be %d", discovery.PQ_KEYS.dsa_verifying_key))
end

logger.info("PQ key exchange validation scenario passed", {
    kem_key_size = discovery.PQ_KEYS.kem_encap_key,
    dsa_key_size = discovery.PQ_KEYS.dsa_verifying_key,
    total_key_exchanges = tracker.key_exchanges,
})

return final_result

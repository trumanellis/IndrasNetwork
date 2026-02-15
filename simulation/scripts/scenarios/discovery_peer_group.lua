-- Multi-Peer Group Discovery Scenario
--
-- Tests discovery among 3+ peers, where multiple overlapping realms emerge.
--
-- Flow:
-- 1. A comes online
-- 2. B discovers A -> Realm {A, B} now exists
-- 3. C discovers A -> Realm {A, C} now exists
-- 4. C discovers B -> Realm {B, C} now exists
-- 5. All three know each other -> Realm {A, B, C} also exists
-- 6. Four distinct realms emerge from three peers
--
-- Key insight: N peers can form 2^N - N - 1 distinct realms (all subsets of size >= 2)

local discovery = require("lib.discovery_helpers")
local thresholds = require("config.discovery_thresholds")

-- ============================================================================
-- SETUP
-- ============================================================================

local ctx = discovery.new_context("discovery_peer_group")
local logger = discovery.create_logger(ctx)
local config = discovery.get_config()

-- Configuration for this scenario
local SCENARIO_CONFIG = {
    quick = { peer_count = 3, max_ticks = 150, broadcast_interval = 5 },
    medium = { peer_count = 5, max_ticks = 300, broadcast_interval = 4 },
    full = { peer_count = 8, max_ticks = 500, broadcast_interval = 3 },
}
local cfg = SCENARIO_CONFIG[discovery.get_level()] or SCENARIO_CONFIG.medium

logger.info("Starting multi-peer group discovery scenario", {
    level = discovery.get_level(),
    peer_count = cfg.peer_count,
    max_ticks = cfg.max_ticks,
    expected_realms = discovery.count_possible_realms(cfg.peer_count),
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
local result = discovery.result_builder("discovery_peer_group")

-- Track discovered realms
local discovered_realms = {}
local realm_discovery_ticks = {}

-- ============================================================================
-- PHASE 1: STAGGERED ONLINE (10% of ticks)
-- Peers come online one by one
-- ============================================================================

local phase1_end = math.floor(cfg.max_ticks * 0.1)

logger.info("Phase 1: Staggered peer online", {
    phase = 1,
    end_tick = phase1_end,
    peer_count = cfg.peer_count,
})

local online_interval = math.floor(phase1_end / cfg.peer_count)

for i, peer in ipairs(peers) do
    local target_tick = (i - 1) * online_interval + 1

    -- Run until target tick
    while sim.tick < target_tick do
        sim:step()
    end

    -- Bring peer online
    peer_state:bring_online(peer, sim.tick)
    sim:force_online(peer)

    logger.event(discovery.EVENTS.PEER_ONLINE, {
        tick = sim.tick,
        peer = tostring(peer),
        peer_index = i,
        kem_key_size = peer_state:get_pq_keys(peer).kem_encap_key_size,
        dsa_key_size = peer_state:get_pq_keys(peer).dsa_verifying_key_size,
    })
end

logger.info("Phase 1 complete: All peers online", {
    phase = 1,
    tick = sim.tick,
    online_count = #peer_state:online_peers(),
})

-- ============================================================================
-- PHASE 2: DISCOVERY BROADCASTS (60% of ticks)
-- Each peer broadcasts presence and discovers others
-- ============================================================================

local phase2_end = math.floor(cfg.max_ticks * 0.7)

logger.info("Phase 2: Discovery broadcasts", {
    phase = 2,
    start_tick = sim.tick,
    end_tick = phase2_end,
})

-- Track last broadcast tick for each peer
local last_broadcast = {}
for _, peer in ipairs(peers) do
    last_broadcast[tostring(peer)] = 0
end

for tick = sim.tick + 1, phase2_end do
    sim:step()

    -- Each online peer broadcasts at intervals
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

                -- All other online peers receive this broadcast
                for _, receiver in ipairs(peers) do
                    if receiver ~= broadcaster and peer_state:is_online(receiver) then
                        local receiver_id = tostring(receiver)

                        -- Check if this is a new discovery
                        if not tracker:knows(receiver, broadcaster) then
                            -- Receiver discovers broadcaster
                            tracker:record_discovery(receiver, broadcaster, tick)
                            tracker:record_pq_keys(receiver, broadcaster,
                                broadcaster_keys.kem_encap_key_size,
                                broadcaster_keys.dsa_verifying_key_size)
                            peer_state:learn_peer(receiver, broadcaster, broadcaster_keys)

                            logger.event(discovery.EVENTS.PEER_DISCOVERED, {
                                tick = tick,
                                discoverer = receiver_id,
                                discovered = broadcaster_id,
                                group_size = #tracker:known_peers(receiver) + 1,
                                pq_kem_key_size = broadcaster_keys.kem_encap_key_size,
                                pq_dsa_key_size = broadcaster_keys.dsa_verifying_key_size,
                            })

                            -- Check for new realm possibilities
                            local known = tracker:known_peers(receiver)
                            for _, realm_id in ipairs(discovery.realms_for_peer(receiver, known)) do
                                if not discovered_realms[realm_id] then
                                    discovered_realms[realm_id] = true
                                    realm_discovery_ticks[realm_id] = tick

                                    logger.event(discovery.EVENTS.REALM_AVAILABLE, {
                                        tick = tick,
                                        realm_id = realm_id,
                                        triggered_by = receiver_id,
                                    })
                                end
                            end
                        end
                    end
                end
            end
        end
    end

    -- Check for convergence
    if tracker:is_complete() and not result.metrics.convergence_tick then
        result:add_metric("convergence_tick", tick)
        logger.event(discovery.EVENTS.CONVERGENCE_ACHIEVED, {
            tick = tick,
            total_discoveries = tracker.discoveries,
            total_realms = 0,  -- Will count later
        })
    end

    -- Progress logging
    if tick % 50 == 0 then
        logger.debug("Phase 2 progress", {
            tick = tick,
            completeness = tracker:completeness(),
            discoveries = tracker.discoveries,
            realms_discovered = 0,  -- Placeholder
        })
    end
end

logger.info("Phase 2 complete: Discovery phase ended", {
    phase = 2,
    tick = sim.tick,
    completeness = tracker:completeness(),
    total_discoveries = tracker.discoveries,
})

-- ============================================================================
-- PHASE 3: VERIFICATION (30% of ticks)
-- Verify all peers know each other and count realms
-- ============================================================================

local phase3_end = cfg.max_ticks

logger.info("Phase 3: Verification", {
    phase = 3,
    start_tick = sim.tick,
    end_tick = phase3_end,
})

-- Count realms
local realm_count = 0
for _ in pairs(discovered_realms) do
    realm_count = realm_count + 1
end

-- Expected realms for full discovery
local expected_realms = discovery.count_possible_realms(cfg.peer_count)

-- Build discovery matrix for logging
local discovery_matrix = {}
for _, from in ipairs(peers) do
    local from_id = tostring(from)
    discovery_matrix[from_id] = {}
    for _, to in ipairs(peers) do
        if from ~= to then
            local to_id = tostring(to)
            discovery_matrix[from_id][to_id] = {
                discovered = tracker:knows(from, to),
                has_pq_keys = tracker:has_keys(from, to),
            }
        end
    end
end

-- Calculate metrics
local tracker_stats = tracker:stats()
local latencies = tracker:get_latencies()
local latency_stats = discovery.percentiles(latencies)

result:add_metrics({
    peer_count = cfg.peer_count,
    total_discoveries = tracker.discoveries,
    discovery_completeness = tracker_stats.completeness,
    pq_completeness = tracker_stats.pq_completeness,
    realms_formed = realm_count,
    expected_realms = expected_realms,
    avg_discovery_latency_ticks = discovery.average(latencies),
    discovery_latency_p50 = latency_stats.p50,
    discovery_latency_p95 = latency_stats.p95,
    discovery_latency_p99 = latency_stats.p99,
})

-- Assertions
result:record_assertion("discovery_complete",
    tracker:is_complete(), true, tracker:is_complete())
result:record_assertion("pq_keys_complete",
    tracker:is_pq_complete(), true, tracker:is_pq_complete())
result:record_assertion("realms_formed",
    realm_count >= 1, true, realm_count >= 1)

-- Verify all peer pairs
local all_pairs_discovered = true
local all_pairs_have_keys = true
for _, from in ipairs(peers) do
    for _, to in ipairs(peers) do
        if from ~= to then
            if not tracker:knows(from, to) then
                all_pairs_discovered = false
            end
            if not tracker:has_keys(from, to) then
                all_pairs_have_keys = false
            end
        end
    end
end

result:record_assertion("all_pairs_discovered",
    all_pairs_discovered, true, all_pairs_discovered)
result:record_assertion("all_pairs_have_keys",
    all_pairs_have_keys, true, all_pairs_have_keys)

-- ============================================================================
-- FINAL RESULTS
-- ============================================================================

local final_result = result:build()

logger.info("Multi-peer group discovery scenario completed", {
    passed = final_result.passed,
    level = final_result.level,
    peer_count = cfg.peer_count,
    total_discoveries = tracker.discoveries,
    discovery_completeness = tracker_stats.completeness,
    pq_completeness = tracker_stats.pq_completeness,
    realms_formed = realm_count,
    expected_realms = expected_realms,
    avg_discovery_latency = discovery.average(latencies),
    convergence_tick = result.metrics.convergence_tick,
})

-- Standard assertions
indras.assert.eq(tracker:is_complete(), true, "All peers should discover each other")
indras.assert.eq(tracker:is_pq_complete(), true, "All peers should have each other's PQ keys")
indras.assert.gt(realm_count, 0, "At least one realm should be formed")

logger.info("Multi-peer group discovery scenario passed", {
    realms_formed = realm_count,
    total_discoveries = tracker.discoveries,
})

return final_result

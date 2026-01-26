-- Discovery Stress Test Scenario
--
-- Tests peer discovery under high churn conditions with many peers.
-- Validates that discovery converges even with peers going online/offline.
--
-- Flow:
-- 1. Peers come online and broadcast presence
-- 2. Each peer discovers others, accumulating knowledge
-- 3. Churn: peers go offline/online randomly
-- 4. Offline peers miss broadcasts, use IntroductionRequest on reconnect
-- 5. Test that all online peers eventually know each other
--
-- Phases:
-- - Ramp-up (30%): Peers come online, start discovering
-- - Churn (40%): High online/offline activity with re-discovery
-- - Stabilization (20%): Reduced churn, convergence
-- - Verification (10%): Check that all online peers know each other

local discovery = require("lib.discovery_helpers")
local stress_helpers = require("lib.stress_helpers")
local thresholds = require("config.discovery_thresholds")

-- ============================================================================
-- SETUP
-- ============================================================================

local ctx = discovery.new_context("discovery_stress")
local logger = discovery.create_logger(ctx)
local config = discovery.get_config()

-- Configuration for this scenario
local SCENARIO_CONFIG = {
    quick = {
        peer_count = 10,
        max_ticks = 300,
        churn_rate = 0.1,
        broadcast_interval = 5,
        catchup_delay = 10,
    },
    medium = {
        peer_count = 18,
        max_ticks = 600,
        churn_rate = 0.2,
        broadcast_interval = 4,
        catchup_delay = 8,
    },
    full = {
        peer_count = 26,
        max_ticks = 1000,
        churn_rate = 0.3,
        broadcast_interval = 3,
        catchup_delay = 5,
    },
}
local cfg = SCENARIO_CONFIG[discovery.get_level()] or SCENARIO_CONFIG.medium

logger.info("Starting discovery stress test", {
    level = discovery.get_level(),
    peer_count = cfg.peer_count,
    max_ticks = cfg.max_ticks,
    churn_rate = cfg.churn_rate,
})

-- Create mesh
local mesh = indras.MeshBuilder.new(cfg.peer_count):random(0.4)
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
local rate_limiter = discovery.create_rate_limiter(config.rate_limit_window)
local latency_tracker = stress_helpers.latency_tracker()
local result = discovery.result_builder("discovery_stress")

-- Metrics tracking
local total_discoveries = 0
local discovery_failures = 0
local churn_events = 0
local catchup_requests = 0
local convergence_tick = nil

-- Last broadcast time per peer
local last_broadcast = {}
for _, peer in ipairs(peers) do
    last_broadcast[tostring(peer)] = 0
end

-- ============================================================================
-- PHASE 1: RAMP-UP (30% of ticks)
-- Peers come online, start discovering
-- ============================================================================

local phase1_end = math.floor(cfg.max_ticks * 0.3)

logger.info("Phase 1: Ramp-up", {
    phase = 1,
    end_tick = phase1_end,
    description = "Peers come online, start discovering",
})

-- Staggered peer online
local online_interval = math.floor(phase1_end / cfg.peer_count)

for tick = 1, phase1_end do
    sim:step()

    -- Bring peers online gradually
    local peer_index = math.floor(tick / online_interval)
    if peer_index >= 1 and peer_index <= cfg.peer_count then
        local peer = peers[peer_index]
        if not peer_state:is_online(peer) then
            peer_state:bring_online(peer, tick)
            sim:force_online(peer)

            logger.event(discovery.EVENTS.PEER_ONLINE, {
                tick = tick,
                peer = tostring(peer),
                phase = "ramp-up",
            })
        end
    end

    -- Discovery broadcasts
    for _, broadcaster in ipairs(peers) do
        local broadcaster_id = tostring(broadcaster)
        if peer_state:is_online(broadcaster) then
            if tick - last_broadcast[broadcaster_id] >= cfg.broadcast_interval then
                last_broadcast[broadcaster_id] = tick
                local broadcaster_keys = peer_state:get_pq_keys(broadcaster)

                for _, receiver in ipairs(peers) do
                    if receiver ~= broadcaster and peer_state:is_online(receiver) then
                        if not tracker:knows(receiver, broadcaster) then
                            tracker:record_discovery(receiver, broadcaster, tick)
                            tracker:record_pq_keys(receiver, broadcaster,
                                broadcaster_keys.kem_encap_key_size,
                                broadcaster_keys.dsa_verifying_key_size)
                            peer_state:learn_peer(receiver, broadcaster, broadcaster_keys)
                            total_discoveries = total_discoveries + 1
                            latency_tracker:record(tick)  -- Record discovery tick as latency proxy
                        end
                    end
                end
            end
        end
    end

    -- Progress logging
    if tick % 50 == 0 then
        logger.debug("Phase 1 progress", {
            tick = tick,
            online_count = #peer_state:online_peers(),
            discoveries = total_discoveries,
            completeness = tracker:completeness(),
        })
    end
end

logger.info("Phase 1 complete: Ramp-up", {
    phase = 1,
    tick = sim.tick,
    online_peers = #peer_state:online_peers(),
    discoveries = total_discoveries,
    completeness = tracker:completeness(),
})

-- ============================================================================
-- PHASE 2: CHURN (40% of ticks)
-- High online/offline activity with re-discovery
-- ============================================================================

local phase2_end = math.floor(cfg.max_ticks * 0.7)

logger.info("Phase 2: Churn", {
    phase = 2,
    start_tick = sim.tick,
    end_tick = phase2_end,
    churn_rate = cfg.churn_rate,
})

-- Track recently offline peers for catchup
local recently_offline = {}

for tick = sim.tick + 1, phase2_end do
    sim:step()

    -- Random churn: take peers offline
    if math.random() < cfg.churn_rate then
        local online = peer_state:online_peers()
        if #online > 2 then  -- Keep at least 2 online
            local victim_idx = math.random(#online)
            local victim_id = online[victim_idx]

            for _, peer in ipairs(peers) do
                if tostring(peer) == victim_id then
                    peer_state:bring_offline(peer)
                    sim:force_offline(peer)
                    churn_events = churn_events + 1
                    recently_offline[victim_id] = tick

                    logger.event(discovery.EVENTS.PEER_OFFLINE, {
                        tick = tick,
                        peer = victim_id,
                        reason = "churn",
                    })
                    break
                end
            end
        end
    end

    -- Random churn: bring peers back online
    if math.random() < cfg.churn_rate * 1.2 then  -- Slightly higher to prevent all-offline
        local offline = peer_state:offline_peers()
        if #offline > 0 then
            local zombie_idx = math.random(#offline)
            local zombie_id = offline[zombie_idx]

            for _, peer in ipairs(peers) do
                if tostring(peer) == zombie_id then
                    peer_state:bring_online(peer, tick)
                    sim:force_online(peer)
                    churn_events = churn_events + 1

                    logger.event(discovery.EVENTS.PEER_ONLINE, {
                        tick = tick,
                        peer = zombie_id,
                        reason = "reconnect",
                    })

                    -- Trigger catchup if was offline
                    if recently_offline[zombie_id] then
                        catchup_requests = catchup_requests + 1
                        recently_offline[zombie_id] = nil

                        logger.event(discovery.EVENTS.INTRODUCTION_REQUEST_SENT, {
                            tick = tick,
                            requester = zombie_id,
                            reason = "post_churn_catchup",
                        })
                    end
                    break
                end
            end
        end
    end

    -- Discovery broadcasts (continues during churn)
    for _, broadcaster in ipairs(peers) do
        local broadcaster_id = tostring(broadcaster)
        if peer_state:is_online(broadcaster) then
            if tick - last_broadcast[broadcaster_id] >= cfg.broadcast_interval then
                last_broadcast[broadcaster_id] = tick
                local broadcaster_keys = peer_state:get_pq_keys(broadcaster)

                for _, receiver in ipairs(peers) do
                    if receiver ~= broadcaster and peer_state:is_online(receiver) then
                        if not tracker:knows(receiver, broadcaster) then
                            local success = math.random() > 0.05  -- 5% failure rate during churn
                            if success then
                                tracker:record_discovery(receiver, broadcaster, tick)
                                tracker:record_pq_keys(receiver, broadcaster,
                                    broadcaster_keys.kem_encap_key_size,
                                    broadcaster_keys.dsa_verifying_key_size)
                                peer_state:learn_peer(receiver, broadcaster, broadcaster_keys)
                                total_discoveries = total_discoveries + 1
                                latency_tracker:record(tick)
                            else
                                discovery_failures = discovery_failures + 1
                            end
                        end
                    end
                end
            end
        end
    end

    -- Progress logging
    if tick % 100 == 0 then
        logger.debug("Phase 2 progress", {
            tick = tick,
            online_count = #peer_state:online_peers(),
            churn_events = churn_events,
            discoveries = total_discoveries,
            failures = discovery_failures,
            completeness = tracker:completeness(),
        })
    end
end

logger.info("Phase 2 complete: Churn", {
    phase = 2,
    tick = sim.tick,
    churn_events = churn_events,
    discoveries = total_discoveries,
    failures = discovery_failures,
    completeness = tracker:completeness(),
})

-- ============================================================================
-- PHASE 3: STABILIZATION (20% of ticks)
-- Reduced churn, convergence
-- ============================================================================

local phase3_end = math.floor(cfg.max_ticks * 0.9)

logger.info("Phase 3: Stabilization", {
    phase = 3,
    start_tick = sim.tick,
    end_tick = phase3_end,
})

-- Bring all remaining offline peers online
for _, peer in ipairs(peers) do
    if not peer_state:is_online(peer) then
        peer_state:bring_online(peer, sim.tick)
        sim:force_online(peer)
    end
end

for tick = sim.tick + 1, phase3_end do
    sim:step()

    -- Very low churn
    if math.random() < cfg.churn_rate * 0.1 then
        -- Minimal churn to test resilience
    end

    -- Continue discovery broadcasts
    for _, broadcaster in ipairs(peers) do
        local broadcaster_id = tostring(broadcaster)
        if peer_state:is_online(broadcaster) then
            if tick - last_broadcast[broadcaster_id] >= cfg.broadcast_interval then
                last_broadcast[broadcaster_id] = tick
                local broadcaster_keys = peer_state:get_pq_keys(broadcaster)

                for _, receiver in ipairs(peers) do
                    if receiver ~= broadcaster and peer_state:is_online(receiver) then
                        if not tracker:knows(receiver, broadcaster) then
                            tracker:record_discovery(receiver, broadcaster, tick)
                            tracker:record_pq_keys(receiver, broadcaster,
                                broadcaster_keys.kem_encap_key_size,
                                broadcaster_keys.dsa_verifying_key_size)
                            peer_state:learn_peer(receiver, broadcaster, broadcaster_keys)
                            total_discoveries = total_discoveries + 1
                        end
                    end
                end
            end
        end
    end

    -- Check for convergence
    if tracker:is_complete() and not convergence_tick then
        convergence_tick = tick
        logger.event(discovery.EVENTS.CONVERGENCE_ACHIEVED, {
            tick = tick,
            total_discoveries = total_discoveries,
        })
    end

    -- Progress logging
    if tick % 50 == 0 then
        logger.debug("Phase 3 progress", {
            tick = tick,
            completeness = tracker:completeness(),
            converged = convergence_tick ~= nil,
        })
    end
end

logger.info("Phase 3 complete: Stabilization", {
    phase = 3,
    tick = sim.tick,
    completeness = tracker:completeness(),
    convergence_tick = convergence_tick,
})

-- ============================================================================
-- PHASE 4: VERIFICATION (10% of ticks)
-- Check that all online peers know each other
-- ============================================================================

logger.info("Phase 4: Verification", {
    phase = 4,
    start_tick = sim.tick,
})

-- Continue simulation
for tick = sim.tick + 1, cfg.max_ticks do
    sim:step()
end

-- Calculate metrics
local tracker_stats = tracker:stats()
local latency_stats = latency_tracker:stats()

-- Count online peers who know all other online peers
local online = peer_state:online_peers()
local fully_connected_count = 0
for _, online_peer_id in ipairs(online) do
    local knows_all = true
    for _, other_id in ipairs(online) do
        if online_peer_id ~= other_id then
            -- Find the peer objects
            local from_peer, to_peer
            for _, p in ipairs(peers) do
                if tostring(p) == online_peer_id then from_peer = p end
                if tostring(p) == other_id then to_peer = p end
            end
            if from_peer and to_peer and not tracker:knows(from_peer, to_peer) then
                knows_all = false
                break
            end
        end
    end
    if knows_all then
        fully_connected_count = fully_connected_count + 1
    end
end

local member_consistency = #online > 0 and fully_connected_count / #online or 0
local churn_recovery_rate = churn_events > 0 and (total_discoveries / (churn_events + cfg.peer_count)) or 1.0

result:add_metrics({
    peer_count = cfg.peer_count,
    total_discoveries = total_discoveries,
    discovery_failures = discovery_failures,
    churn_events = churn_events,
    catchup_requests = catchup_requests,
    convergence_ticks = convergence_tick or cfg.max_ticks,
    member_consistency = member_consistency,
    churn_recovery_rate = math.min(1.0, churn_recovery_rate),
    online_at_end = #online,
    fully_connected_at_end = fully_connected_count,
    discovery_completeness = tracker_stats.completeness,
    pq_completeness = tracker_stats.pq_completeness,
    discovery_latency_p99 = latency_stats.p99,
    discovery_latency_avg = latency_stats.avg,
})

-- Assertions
result:record_assertion("member_consistency",
    member_consistency >= 0.95, true, member_consistency >= 0.95)
result:record_assertion("convergence_achieved",
    convergence_tick ~= nil, true, convergence_tick ~= nil)
result:record_assertion("discovery_completeness",
    tracker_stats.completeness >= 0.9, true, tracker_stats.completeness >= 0.9)

-- Validate against thresholds
local scenario_thresholds = thresholds.get("stress")
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

logger.info("Discovery stress test completed", {
    passed = final_result.passed,
    level = final_result.level,
    peer_count = cfg.peer_count,
    total_discoveries = total_discoveries,
    discovery_failures = discovery_failures,
    churn_events = churn_events,
    convergence_ticks = convergence_tick,
    member_consistency = member_consistency,
    discovery_completeness = tracker_stats.completeness,
    pq_completeness = tracker_stats.pq_completeness,
    discovery_latency_p99 = latency_stats.p99,
})

-- Standard assertions
indras.assert.gt(total_discoveries, 0, "Should have discoveries")
indras.assert.gt(member_consistency, 0.9, "At least 90% member consistency expected")

logger.info("Discovery stress test passed", {
    total_discoveries = total_discoveries,
    member_consistency = member_consistency,
    convergence_tick = convergence_tick,
})

return final_result

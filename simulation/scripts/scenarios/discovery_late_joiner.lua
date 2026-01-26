-- Late Joiner Discovery Scenario
--
-- Tests the IntroductionRequest mechanism for peers who join after
-- missing initial discovery broadcasts.
--
-- Flow:
-- 1. Alice, Bob, Carol all discover each other (realms {A,B}, {A,C}, {B,C}, {A,B,C} exist)
-- 2. Dave comes online later, misses their broadcasts
-- 3. Dave sends IntroductionRequest
-- 4. Existing peers respond with IntroductionResponse
-- 5. Dave now knows Alice, Bob, Carol
-- 6. New realms available: {D,A}, {D,B}, {D,C}, {D,A,B}, {D,A,C}, {D,B,C}, {D,A,B,C}
--
-- This scenario validates the catch-up discovery mechanism.

local discovery = require("lib.discovery_helpers")
local thresholds = require("config.discovery_thresholds")

-- ============================================================================
-- SETUP
-- ============================================================================

local ctx = discovery.new_context("discovery_late_joiner")
local logger = discovery.create_logger(ctx)
local config = discovery.get_config()

-- Configuration for this scenario
local SCENARIO_CONFIG = {
    quick = {
        initial_peers = 3,
        late_joiners = 1,
        max_ticks = 200,
        initial_discovery_end = 60,
        late_arrival_tick = 80,
        catchup_window = 40,
    },
    medium = {
        initial_peers = 5,
        late_joiners = 2,
        max_ticks = 400,
        initial_discovery_end = 100,
        late_arrival_tick = 150,
        catchup_window = 60,
    },
    full = {
        initial_peers = 8,
        late_joiners = 3,
        max_ticks = 600,
        initial_discovery_end = 150,
        late_arrival_tick = 200,
        catchup_window = 80,
    },
}
local cfg = SCENARIO_CONFIG[discovery.get_level()] or SCENARIO_CONFIG.medium

local total_peers = cfg.initial_peers + cfg.late_joiners

logger.info("Starting late joiner discovery scenario", {
    level = discovery.get_level(),
    initial_peers = cfg.initial_peers,
    late_joiners = cfg.late_joiners,
    total_peers = total_peers,
    max_ticks = cfg.max_ticks,
})

-- Create mesh with all peers
local mesh = indras.MeshBuilder.new(total_peers):full_mesh()
local sim_config = indras.SimConfig.new({
    wake_probability = 0,
    sleep_probability = 0,
    initial_online_probability = 0,
    max_ticks = cfg.max_ticks,
})
local sim = indras.Simulation.new(mesh, sim_config)
sim:initialize()

local all_peers = mesh:peers()
local initial_peers = {}
local late_joiners = {}

for i, peer in ipairs(all_peers) do
    if i <= cfg.initial_peers then
        table.insert(initial_peers, peer)
    else
        table.insert(late_joiners, peer)
    end
end

local tracker = discovery.create_tracker(all_peers)
local peer_state = discovery.create_peer_state(all_peers)
local rate_limiter = discovery.create_rate_limiter(config.rate_limit_window)
local result = discovery.result_builder("discovery_late_joiner")

-- Track metrics
local catchup_requests_sent = 0
local catchup_responses_received = 0
local late_joiner_discoveries = 0

-- ============================================================================
-- PHASE 1: INITIAL DISCOVERY (ticks 1-60)
-- Initial peers discover each other
-- ============================================================================

logger.info("Phase 1: Initial peer discovery", {
    phase = 1,
    initial_peers = cfg.initial_peers,
    end_tick = cfg.initial_discovery_end,
})

-- Bring initial peers online
for tick = 1, 5 do
    sim:step()
end

for _, peer in ipairs(initial_peers) do
    peer_state:bring_online(peer, sim.tick)
    sim:force_online(peer)

    logger.event(discovery.EVENTS.PEER_ONLINE, {
        tick = sim.tick,
        peer = tostring(peer),
        role = "initial",
    })
    sim:step()
end

-- Initial peers broadcast and discover each other
local broadcast_interval = 5
local last_broadcast = {}
for _, peer in ipairs(initial_peers) do
    last_broadcast[tostring(peer)] = 0
end

for tick = sim.tick + 1, cfg.initial_discovery_end do
    sim:step()

    for _, broadcaster in ipairs(initial_peers) do
        local broadcaster_id = tostring(broadcaster)

        if tick - last_broadcast[broadcaster_id] >= broadcast_interval then
            last_broadcast[broadcaster_id] = tick
            local broadcaster_keys = peer_state:get_pq_keys(broadcaster)

            logger.event(discovery.EVENTS.PRESENCE_BROADCAST, {
                tick = tick,
                broadcaster = broadcaster_id,
                phase = "initial",
            })

            for _, receiver in ipairs(initial_peers) do
                if receiver ~= broadcaster and peer_state:is_online(receiver) then
                    if not tracker:knows(receiver, broadcaster) then
                        tracker:record_discovery(receiver, broadcaster, tick)
                        tracker:record_pq_keys(receiver, broadcaster,
                            broadcaster_keys.kem_encap_key_size,
                            broadcaster_keys.dsa_verifying_key_size)
                        peer_state:learn_peer(receiver, broadcaster, broadcaster_keys)

                        logger.event(discovery.EVENTS.PEER_DISCOVERED, {
                            tick = tick,
                            discoverer = tostring(receiver),
                            discovered = broadcaster_id,
                            phase = "initial",
                        })
                    end
                end
            end
        end
    end
end

-- Verify initial discovery complete
local initial_complete = true
for _, from in ipairs(initial_peers) do
    for _, to in ipairs(initial_peers) do
        if from ~= to and not tracker:knows(from, to) then
            initial_complete = false
            break
        end
    end
end

logger.info("Phase 1 complete: Initial discovery", {
    phase = 1,
    tick = sim.tick,
    initial_complete = initial_complete,
    discoveries = tracker.discoveries,
})

-- Count initial realms
local initial_realms = discovery.count_possible_realms(cfg.initial_peers)
result:add_metric("initial_realms", initial_realms)

-- ============================================================================
-- PHASE 2: LATE ARRIVAL (ticks 61-100)
-- Late joiners come online
-- ============================================================================

logger.info("Phase 2: Late joiner arrival", {
    phase = 2,
    late_joiners = cfg.late_joiners,
    arrival_tick = cfg.late_arrival_tick,
})

-- Run until late arrival tick
for tick = sim.tick + 1, cfg.late_arrival_tick do
    sim:step()
end

-- Bring late joiners online
for i, late_peer in ipairs(late_joiners) do
    peer_state:bring_online(late_peer, sim.tick)
    sim:force_online(late_peer)

    logger.event(discovery.EVENTS.PEER_ONLINE, {
        tick = sim.tick,
        peer = tostring(late_peer),
        role = "late_joiner",
        joiner_index = i,
    })

    sim:step()
end

logger.info("Phase 2 complete: Late joiners online", {
    phase = 2,
    tick = sim.tick,
    late_joiners_online = #late_joiners,
})

-- ============================================================================
-- PHASE 3: CATCH-UP REQUESTS (ticks 101-160)
-- Late joiners send IntroductionRequests and receive responses
-- ============================================================================

local catchup_end = cfg.late_arrival_tick + cfg.catchup_window

logger.info("Phase 3: Catch-up discovery", {
    phase = 3,
    start_tick = sim.tick,
    end_tick = catchup_end,
})

-- Each late joiner sends introduction request
for _, late_peer in ipairs(late_joiners) do
    local late_peer_id = tostring(late_peer)

    -- Send IntroductionRequest
    catchup_requests_sent = catchup_requests_sent + 1

    logger.event(discovery.EVENTS.INTRODUCTION_REQUEST_SENT, {
        tick = sim.tick,
        requester = late_peer_id,
    })

    -- Initial peers respond with IntroductionResponse
    for _, initial_peer in ipairs(initial_peers) do
        local initial_peer_id = tostring(initial_peer)

        if peer_state:is_online(initial_peer) then
            -- Rate limit check
            if rate_limiter:record_response(initial_peer, late_peer, sim.tick) then
                local initial_keys = peer_state:get_pq_keys(initial_peer)

                logger.event(discovery.EVENTS.INTRODUCTION_RESPONSE_SENT, {
                    tick = sim.tick,
                    responder = initial_peer_id,
                    requester = late_peer_id,
                })

                -- Late joiner discovers initial peer
                if not tracker:knows(late_peer, initial_peer) then
                    tracker:record_discovery(late_peer, initial_peer, sim.tick)
                    tracker:record_pq_keys(late_peer, initial_peer,
                        initial_keys.kem_encap_key_size,
                        initial_keys.dsa_verifying_key_size)
                    peer_state:learn_peer(late_peer, initial_peer, initial_keys)
                    late_joiner_discoveries = late_joiner_discoveries + 1
                    catchup_responses_received = catchup_responses_received + 1

                    logger.event(discovery.EVENTS.PEER_DISCOVERED, {
                        tick = sim.tick,
                        discoverer = late_peer_id,
                        discovered = initial_peer_id,
                        mechanism = "introduction_response",
                    })
                end

                -- Initial peer also learns about late joiner
                local late_keys = peer_state:get_pq_keys(late_peer)
                if not tracker:knows(initial_peer, late_peer) then
                    tracker:record_discovery(initial_peer, late_peer, sim.tick)
                    tracker:record_pq_keys(initial_peer, late_peer,
                        late_keys.kem_encap_key_size,
                        late_keys.dsa_verifying_key_size)
                    peer_state:learn_peer(initial_peer, late_peer, late_keys)

                    logger.event(discovery.EVENTS.PEER_DISCOVERED, {
                        tick = sim.tick,
                        discoverer = initial_peer_id,
                        discovered = late_peer_id,
                        mechanism = "introduction_request",
                    })
                end
            else
                logger.event(discovery.EVENTS.INTRODUCTION_RESPONSE_RATE_LIMITED, {
                    tick = sim.tick,
                    responder = initial_peer_id,
                    requester = late_peer_id,
                })
            end
        end
    end

    sim:step()
end

-- Continue running to allow discovery to complete
for tick = sim.tick + 1, catchup_end do
    sim:step()

    -- Late joiners discover each other through broadcasts
    for _, late_peer in ipairs(late_joiners) do
        if peer_state:is_online(late_peer) then
            local late_keys = peer_state:get_pq_keys(late_peer)

            for _, other_late in ipairs(late_joiners) do
                if late_peer ~= other_late and peer_state:is_online(other_late) then
                    if not tracker:knows(other_late, late_peer) then
                        tracker:record_discovery(other_late, late_peer, tick)
                        tracker:record_pq_keys(other_late, late_peer,
                            late_keys.kem_encap_key_size,
                            late_keys.dsa_verifying_key_size)
                        peer_state:learn_peer(other_late, late_peer, late_keys)

                        logger.event(discovery.EVENTS.PEER_DISCOVERED, {
                            tick = tick,
                            discoverer = tostring(other_late),
                            discovered = tostring(late_peer),
                            mechanism = "broadcast",
                        })
                    end
                end
            end
        end
    end
end

logger.info("Phase 3 complete: Catch-up discovery", {
    phase = 3,
    tick = sim.tick,
    catchup_requests_sent = catchup_requests_sent,
    catchup_responses_received = catchup_responses_received,
    late_joiner_discoveries = late_joiner_discoveries,
})

-- ============================================================================
-- PHASE 4: VERIFICATION (remaining ticks)
-- Verify all peers know each other
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

-- Count new realms available to late joiners
local new_realms_for_late_joiners = 0
for _, late_peer in ipairs(late_joiners) do
    local known = tracker:known_peers(late_peer)
    new_realms_for_late_joiners = new_realms_for_late_joiners + #discovery.realms_for_peer(late_peer, known)
end

-- Calculate total possible realms with all peers
local total_possible_realms = discovery.count_possible_realms(total_peers)
local new_realms_from_late_joiners = total_possible_realms - initial_realms

-- Calculate catchup latency (from arrival to full discovery)
local catchup_latency = catchup_end - cfg.late_arrival_tick

-- Rate limiter stats
local limiter_stats = rate_limiter:stats()

result:add_metrics({
    initial_peers = cfg.initial_peers,
    late_joiners = cfg.late_joiners,
    total_peers = total_peers,
    catchup_requests_sent = catchup_requests_sent,
    catchup_responses_received = catchup_responses_received,
    catchup_latency_ticks = catchup_latency,
    late_joiner_discoveries = late_joiner_discoveries,
    new_realms_available = new_realms_from_late_joiners,
    initial_realms = initial_realms,
    total_possible_realms = total_possible_realms,
    discovery_completeness = tracker_stats.completeness,
    pq_completeness = tracker_stats.pq_completeness,
    rate_limit_allowed = limiter_stats.allowed,
    rate_limit_limited = limiter_stats.limited,
})

-- Success rate calculation
local catchup_success_rate = 0
if catchup_requests_sent > 0 then
    catchup_success_rate = catchup_responses_received / (catchup_requests_sent * cfg.initial_peers)
end
result:add_metric("catchup_success_rate", catchup_success_rate)

-- Assertions
result:record_assertion("discovery_complete",
    tracker:is_complete(), true, tracker:is_complete())
result:record_assertion("pq_keys_complete",
    tracker:is_pq_complete(), true, tracker:is_pq_complete())
result:record_assertion("late_joiners_discovered_initial",
    late_joiner_discoveries >= cfg.late_joiners * cfg.initial_peers * 0.9,
    true, late_joiner_discoveries >= cfg.late_joiners * cfg.initial_peers * 0.9)
result:record_assertion("new_realms_available",
    new_realms_from_late_joiners > 0, true, new_realms_from_late_joiners > 0)

-- ============================================================================
-- FINAL RESULTS
-- ============================================================================

local final_result = result:build()

logger.info("Late joiner discovery scenario completed", {
    passed = final_result.passed,
    level = final_result.level,
    initial_peers = cfg.initial_peers,
    late_joiners = cfg.late_joiners,
    catchup_requests_sent = catchup_requests_sent,
    catchup_responses_received = catchup_responses_received,
    catchup_success_rate = catchup_success_rate,
    catchup_latency_ticks = catchup_latency,
    new_realms_available = new_realms_from_late_joiners,
    discovery_completeness = tracker_stats.completeness,
    pq_completeness = tracker_stats.pq_completeness,
})

-- Standard assertions
indras.assert.gt(catchup_responses_received, 0, "Should receive catch-up responses")
indras.assert.gt(new_realms_from_late_joiners, 0, "Late joiners should create new realm possibilities")

logger.info("Late joiner discovery scenario passed", {
    catchup_responses = catchup_responses_received,
    new_realms = new_realms_from_late_joiners,
})

return final_result

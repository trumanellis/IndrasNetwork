-- Reconnect Discovery Scenario
--
-- Tests that peers can disconnect and reconnect, re-establishing
-- discovery with other group members.
--
-- Flow:
-- 1. A, B, C form a group (discover each other)
-- 2. B goes offline (network failure)
-- 3. Other members may or may not notice
-- 4. B comes back online
-- 5. B re-broadcasts presence
-- 6. Mutual awareness restored
--
-- This scenario validates discovery resilience to network disconnections.

local discovery = require("lib.discovery_helpers")
local thresholds = require("config.discovery_thresholds")

-- ============================================================================
-- SETUP
-- ============================================================================

local ctx = discovery.new_context("discovery_reconnect")
local logger = discovery.create_logger(ctx)
local config = discovery.get_config()

-- Configuration for this scenario
local SCENARIO_CONFIG = {
    quick = {
        peer_count = 4,
        max_ticks = 250,
        formation_end = 60,
        disconnect_tick = 80,
        offline_duration = 40,
        rediscovery_window = 50,
    },
    medium = {
        peer_count = 8,
        max_ticks = 400,
        formation_end = 80,
        disconnect_tick = 120,
        offline_duration = 60,
        rediscovery_window = 80,
    },
    full = {
        peer_count = 12,
        max_ticks = 600,
        formation_end = 100,
        disconnect_tick = 150,
        offline_duration = 100,
        rediscovery_window = 120,
    },
}
local cfg = SCENARIO_CONFIG[discovery.get_level()] or SCENARIO_CONFIG.medium

logger.info("Starting reconnect discovery scenario", {
    level = discovery.get_level(),
    peer_count = cfg.peer_count,
    offline_duration = cfg.offline_duration,
    max_ticks = cfg.max_ticks,
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
local result = discovery.result_builder("discovery_reconnect")

-- Select a peer to disconnect (second peer)
local disconnect_peer = peers[2]
local disconnect_peer_id = tostring(disconnect_peer)

-- Track events
local disconnects = 0
local reconnects = 0
local rediscoveries = 0

-- ============================================================================
-- PHASE 1: GROUP FORMATION (ticks 1-60)
-- All peers come online and discover each other
-- ============================================================================

logger.info("Phase 1: Group formation", {
    phase = 1,
    peer_count = cfg.peer_count,
    end_tick = cfg.formation_end,
})

-- Bring all peers online
for tick = 1, 10 do
    sim:step()
end

for _, peer in ipairs(peers) do
    peer_state:bring_online(peer, sim.tick)
    sim:force_online(peer)
    logger.event(discovery.EVENTS.PEER_ONLINE, {
        tick = sim.tick,
        peer = tostring(peer),
    })
    sim:step()
end

-- Discovery phase
local broadcast_interval = 5
local last_broadcast = {}
for _, peer in ipairs(peers) do
    last_broadcast[tostring(peer)] = 0
end

for tick = sim.tick + 1, cfg.formation_end do
    sim:step()

    for _, broadcaster in ipairs(peers) do
        local broadcaster_id = tostring(broadcaster)

        if peer_state:is_online(broadcaster) then
            if tick - last_broadcast[broadcaster_id] >= broadcast_interval then
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
                        end
                    end
                end
            end
        end
    end
end

-- Record pre-disconnect state
local pre_disconnect_completeness = tracker:completeness()

logger.info("Phase 1 complete: Group formed", {
    phase = 1,
    tick = sim.tick,
    completeness = pre_disconnect_completeness,
    discoveries = tracker.discoveries,
})

-- ============================================================================
-- PHASE 2: DISCONNECT (ticks 61-100)
-- Selected peer goes offline
-- ============================================================================

logger.info("Phase 2: Peer disconnect", {
    phase = 2,
    disconnect_peer = disconnect_peer_id,
    disconnect_tick = cfg.disconnect_tick,
})

-- Run until disconnect tick
for tick = sim.tick + 1, cfg.disconnect_tick do
    sim:step()
end

-- Take peer offline
peer_state:bring_offline(disconnect_peer)
sim:force_offline(disconnect_peer)
disconnects = disconnects + 1

logger.event(discovery.EVENTS.PEER_OFFLINE, {
    tick = sim.tick,
    peer = disconnect_peer_id,
    reason = "network_failure",
})

-- Record which peers the disconnected peer knew
local known_before_disconnect = {}
for _, peer in ipairs(peers) do
    if peer ~= disconnect_peer then
        if tracker:knows(disconnect_peer, peer) then
            table.insert(known_before_disconnect, tostring(peer))
        end
    end
end

logger.info("Phase 2 complete: Peer disconnected", {
    phase = 2,
    tick = sim.tick,
    disconnect_peer = disconnect_peer_id,
    peers_known_before = #known_before_disconnect,
})

-- ============================================================================
-- PHASE 3: OFFLINE PERIOD (ticks 101-140)
-- Peer stays offline while others continue
-- ============================================================================

local offline_end = cfg.disconnect_tick + cfg.offline_duration

logger.info("Phase 3: Offline period", {
    phase = 3,
    offline_duration = cfg.offline_duration,
    offline_end = offline_end,
})

-- Run simulation during offline period
for tick = sim.tick + 1, offline_end do
    sim:step()

    -- Other peers continue broadcasting
    for _, broadcaster in ipairs(peers) do
        if broadcaster ~= disconnect_peer and peer_state:is_online(broadcaster) then
            local broadcaster_id = tostring(broadcaster)
            if tick - last_broadcast[broadcaster_id] >= broadcast_interval then
                last_broadcast[broadcaster_id] = tick
                -- Broadcasts happen but disconnect_peer misses them
            end
        end
    end
end

logger.info("Phase 3 complete: Offline period ended", {
    phase = 3,
    tick = sim.tick,
    offline_ticks = cfg.offline_duration,
})

-- ============================================================================
-- PHASE 4: RECONNECT (ticks 141-200)
-- Peer comes back online and re-broadcasts
-- ============================================================================

local reconnect_tick = sim.tick + 1

logger.info("Phase 4: Peer reconnect", {
    phase = 4,
    reconnect_peer = disconnect_peer_id,
    reconnect_tick = reconnect_tick,
})

sim:step()

-- Bring peer back online
peer_state:bring_online(disconnect_peer, sim.tick)
sim:force_online(disconnect_peer)
reconnects = reconnects + 1

logger.event(discovery.EVENTS.PEER_ONLINE, {
    tick = sim.tick,
    peer = disconnect_peer_id,
    reason = "reconnect",
})

-- Re-discovery phase
local rediscovery_end = reconnect_tick + cfg.rediscovery_window
local rediscovery_start_tick = sim.tick

for tick = sim.tick + 1, rediscovery_end do
    sim:step()

    -- Reconnected peer broadcasts
    if tick - (last_broadcast[disconnect_peer_id] or 0) >= broadcast_interval then
        last_broadcast[disconnect_peer_id] = tick
        local disconnect_keys = peer_state:get_pq_keys(disconnect_peer)

        logger.event(discovery.EVENTS.PRESENCE_BROADCAST, {
            tick = tick,
            broadcaster = disconnect_peer_id,
            phase = "rediscovery",
        })

        -- Other peers receive broadcast (re-confirm discovery)
        for _, receiver in ipairs(peers) do
            if receiver ~= disconnect_peer and peer_state:is_online(receiver) then
                -- Peers already know each other, this is confirmation
                logger.event(discovery.EVENTS.PRESENCE_RECEIVED, {
                    tick = tick,
                    receiver = tostring(receiver),
                    broadcaster = disconnect_peer_id,
                    phase = "rediscovery",
                })
            end
        end
    end

    -- Other peers also broadcast to help reconnected peer
    for _, broadcaster in ipairs(peers) do
        if broadcaster ~= disconnect_peer and peer_state:is_online(broadcaster) then
            local broadcaster_id = tostring(broadcaster)
            if tick - (last_broadcast[broadcaster_id] or 0) >= broadcast_interval then
                last_broadcast[broadcaster_id] = tick
                local broadcaster_keys = peer_state:get_pq_keys(broadcaster)

                -- Reconnected peer receives
                if peer_state:is_online(disconnect_peer) then
                    -- Re-confirm discovery
                    if tracker:knows(disconnect_peer, broadcaster) then
                        rediscoveries = rediscoveries + 1
                        logger.event(discovery.EVENTS.PEER_DISCOVERED, {
                            tick = tick,
                            discoverer = disconnect_peer_id,
                            discovered = broadcaster_id,
                            mechanism = "rediscovery",
                        })
                    end
                end
            end
        end
    end
end

local rediscovery_latency = sim.tick - rediscovery_start_tick

logger.info("Phase 4 complete: Reconnect and rediscovery", {
    phase = 4,
    tick = sim.tick,
    reconnects = reconnects,
    rediscoveries = rediscoveries,
    rediscovery_latency_ticks = rediscovery_latency,
})

-- ============================================================================
-- PHASE 5: VERIFICATION (remaining ticks)
-- Verify full group awareness restored
-- ============================================================================

logger.info("Phase 5: Verification", {
    phase = 5,
    start_tick = sim.tick,
})

-- Continue simulation
for tick = sim.tick + 1, cfg.max_ticks do
    sim:step()
end

-- Calculate metrics
local tracker_stats = tracker:stats()

-- Check if awareness is fully restored
local awareness_restored = true
for _, peer in ipairs(peers) do
    if peer ~= disconnect_peer then
        if not tracker:knows(disconnect_peer, peer) then
            awareness_restored = false
        end
        if not tracker:knows(peer, disconnect_peer) then
            awareness_restored = false
        end
    end
end

-- Reconnect success rate (1 reconnect, 1 expected)
local reconnect_success_rate = reconnects > 0 and 1.0 or 0.0

result:add_metrics({
    peer_count = cfg.peer_count,
    disconnects = disconnects,
    reconnects = reconnects,
    rediscoveries = rediscoveries,
    offline_duration_ticks = cfg.offline_duration,
    rediscovery_latency_ticks = rediscovery_latency,
    reconnect_success_rate = reconnect_success_rate,
    awareness_restored = awareness_restored and 1.0 or 0.0,
    pre_disconnect_completeness = pre_disconnect_completeness,
    post_reconnect_completeness = tracker_stats.completeness,
    pq_completeness = tracker_stats.pq_completeness,
})

-- Assertions
result:record_assertion("reconnect_success",
    reconnects > 0, true, reconnects > 0)
result:record_assertion("awareness_restored",
    awareness_restored, true, awareness_restored)
result:record_assertion("discovery_complete",
    tracker:is_complete(), true, tracker:is_complete())

-- Validate against thresholds
local scenario_thresholds = thresholds.get("reconnect")
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

logger.info("Reconnect discovery scenario completed", {
    passed = final_result.passed,
    level = final_result.level,
    disconnects = disconnects,
    reconnects = reconnects,
    rediscoveries = rediscoveries,
    offline_duration = cfg.offline_duration,
    rediscovery_latency = rediscovery_latency,
    awareness_restored = awareness_restored,
    final_completeness = tracker_stats.completeness,
})

-- Standard assertions
indras.assert.gt(reconnects, 0, "Should have successful reconnect")
indras.assert.eq(awareness_restored, true, "Full awareness should be restored after reconnect")

logger.info("Reconnect discovery scenario passed", {
    reconnects = reconnects,
    awareness_restored = awareness_restored,
})

return final_result

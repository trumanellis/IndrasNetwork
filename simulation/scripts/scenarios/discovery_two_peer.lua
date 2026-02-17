-- Two-Peer Discovery Scenario
--
-- Tests the fundamental discovery flow: two peers discover each other,
-- birthing a realm between them.
--
-- Flow:
-- 1. A and B both come online
-- 2. A broadcasts presence with her PQ keys
-- 3. B receives A's broadcast, learns about her
-- 4. B broadcasts presence with his PQ keys
-- 5. A receives B's broadcast, learns about him
-- 6. Realm {A, B} now implicitly exists
--
-- This scenario validates the core mutual discovery mechanism with PQ keys.

local discovery = require("lib.discovery_helpers")
local thresholds = require("config.discovery_thresholds")

-- ============================================================================
-- SETUP
-- ============================================================================

local ctx = discovery.new_context("discovery_two_peer")
local logger = discovery.create_logger(ctx)
local config = discovery.get_config()

logger.info("Starting two-peer discovery scenario", {
    level = discovery.get_level(),
    ticks = config.ticks,
})

-- Configuration for this scenario
local SCENARIO_CONFIG = {
    quick = { online_tick = 5, broadcast_tick = 10, verify_start = 30, max_ticks = 100 },
    medium = { online_tick = 3, broadcast_tick = 8, verify_start = 25, max_ticks = 80 },
    full = { online_tick = 2, broadcast_tick = 5, verify_start = 20, max_ticks = 60 },
}
local cfg = SCENARIO_CONFIG[discovery.get_level()] or SCENARIO_CONFIG.medium

-- Create mesh with just 2 peers (A and B)
local mesh = indras.MeshBuilder.new(2):full_mesh()
local sim_config = indras.SimConfig.new({
    wake_probability = 0,
    sleep_probability = 0,
    initial_online_probability = 0,  -- Start offline
    max_ticks = cfg.max_ticks,
})
local sim = indras.Simulation.new(mesh, sim_config)
sim:initialize()

local peers = mesh:peers()
local a = peers[1]
local b = peers[2]

-- Create discovery tracker and peer state
local tracker = discovery.create_tracker(peers)
local peer_state = discovery.create_peer_state(peers)
local result = discovery.result_builder("discovery_two_peer")

-- Phase tracking
local phases = {
    online_complete = false,
    broadcast_complete = false,
    discovery_complete = false,
    verify_complete = false,
}

-- Discovery events
local a_broadcast_tick = nil
local b_broadcast_tick = nil
local a_discovered_b_tick = nil
local b_discovered_a_tick = nil

-- ============================================================================
-- PHASE 1: ONLINE (ticks 1-10)
-- Both peers come online and generate PQ keys
-- ============================================================================

logger.info("Phase 1: Bringing peers online", {
    phase = 1,
    description = "Peers come online and generate PQ keys",
})

for tick = 1, cfg.online_tick do
    sim:step()
end

-- Bring A online
peer_state:bring_online(a, sim.tick)
sim:force_online(a)
logger.event(discovery.EVENTS.PEER_ONLINE, {
    tick = sim.tick,
    peer = tostring(a),
    kem_key_size = peer_state:get_pq_keys(a).kem_encap_key_size,
    dsa_key_size = peer_state:get_pq_keys(a).dsa_verifying_key_size,
})

sim:step()

-- Bring B online
peer_state:bring_online(b, sim.tick)
sim:force_online(b)
logger.event(discovery.EVENTS.PEER_ONLINE, {
    tick = sim.tick,
    peer = tostring(b),
    kem_key_size = peer_state:get_pq_keys(b).kem_encap_key_size,
    dsa_key_size = peer_state:get_pq_keys(b).dsa_verifying_key_size,
})

phases.online_complete = true
logger.info("Phase 1 complete: Both peers online", {
    phase = 1,
    tick = sim.tick,
    a_online = peer_state:is_online(a),
    b_online = peer_state:is_online(b),
})

-- ============================================================================
-- PHASE 2: BROADCAST (ticks 11-30)
-- Each peer broadcasts InterfaceJoin with PQ keys
-- ============================================================================

logger.info("Phase 2: Presence broadcast", {
    phase = 2,
    description = "Peers broadcast presence with PQ keys",
})

-- Continue to broadcast phase
for tick = sim.tick + 1, cfg.broadcast_tick do
    sim:step()
end

-- A broadcasts presence
a_broadcast_tick = sim.tick
local a_keys = peer_state:get_pq_keys(a)
logger.event(discovery.EVENTS.PRESENCE_BROADCAST, {
    tick = sim.tick,
    broadcaster = tostring(a),
    msg_type = discovery.MSG_TYPES.INTERFACE_JOIN,
    kem_key_size = a_keys.kem_encap_key_size,
    dsa_key_size = a_keys.dsa_verifying_key_size,
})

sim:step()

-- B receives A's broadcast and discovers her
peer_state:learn_peer(b, a, a_keys)
tracker:record_discovery(b, a, sim.tick)
tracker:record_pq_keys(b, a, a_keys.kem_encap_key_size, a_keys.dsa_verifying_key_size)
b_discovered_a_tick = sim.tick

logger.event(discovery.EVENTS.PRESENCE_RECEIVED, {
    tick = sim.tick,
    receiver = tostring(b),
    broadcaster = tostring(a),
})

logger.event(discovery.EVENTS.PEER_DISCOVERED, {
    tick = sim.tick,
    discoverer = tostring(b),
    discovered = tostring(a),
    pq_kem_key_size = a_keys.kem_encap_key_size,
    pq_dsa_key_size = a_keys.dsa_verifying_key_size,
})

sim:step()

-- B broadcasts presence
b_broadcast_tick = sim.tick
local b_keys = peer_state:get_pq_keys(b)
logger.event(discovery.EVENTS.PRESENCE_BROADCAST, {
    tick = sim.tick,
    broadcaster = tostring(b),
    msg_type = discovery.MSG_TYPES.INTERFACE_JOIN,
    kem_key_size = b_keys.kem_encap_key_size,
    dsa_key_size = b_keys.dsa_verifying_key_size,
})

sim:step()

-- A receives B's broadcast and discovers him
peer_state:learn_peer(a, b, b_keys)
tracker:record_discovery(a, b, sim.tick)
tracker:record_pq_keys(a, b, b_keys.kem_encap_key_size, b_keys.dsa_verifying_key_size)
a_discovered_b_tick = sim.tick

logger.event(discovery.EVENTS.PRESENCE_RECEIVED, {
    tick = sim.tick,
    receiver = tostring(a),
    broadcaster = tostring(b),
})

logger.event(discovery.EVENTS.PEER_DISCOVERED, {
    tick = sim.tick,
    discoverer = tostring(a),
    discovered = tostring(b),
    pq_kem_key_size = b_keys.kem_encap_key_size,
    pq_dsa_key_size = b_keys.dsa_verifying_key_size,
})

phases.broadcast_complete = true
logger.info("Phase 2 complete: Broadcasts exchanged", {
    phase = 2,
    tick = sim.tick,
    a_broadcast_tick = a_broadcast_tick,
    b_broadcast_tick = b_broadcast_tick,
})

-- ============================================================================
-- PHASE 3: DISCOVERY (ticks 31-50)
-- Mutual discovery completes, realm emerges
-- ============================================================================

logger.info("Phase 3: Discovery verification", {
    phase = 3,
    description = "Verify mutual discovery and realm emergence",
})

-- Check mutual discovery
local a_knows_b = tracker:knows(a, b)
local b_knows_a = tracker:knows(b, a)
local a_has_b_keys = tracker:has_keys(a, b)
local b_has_a_keys = tracker:has_keys(b, a)

phases.discovery_complete = a_knows_b and b_knows_a

-- Calculate realm ID for the pair
local realm_id = discovery.realm_id({a, b})

logger.event(discovery.EVENTS.REALM_AVAILABLE, {
    tick = sim.tick,
    realm_id = realm_id,
    members = {tostring(a), tostring(b)},
    member_count = 2,
})

logger.info("Phase 3 complete: Discovery state", {
    phase = 3,
    tick = sim.tick,
    a_knows_b = a_knows_b,
    b_knows_a = b_knows_a,
    a_has_b_keys = a_has_b_keys,
    b_has_a_keys = b_has_a_keys,
    realm_id = realm_id,
})

-- ============================================================================
-- PHASE 4: VERIFY (ticks 51-100)
-- Assert bidirectional awareness with PQ keys
-- ============================================================================

logger.info("Phase 4: Verification", {
    phase = 4,
    description = "Assert bidirectional awareness with PQ keys",
})

-- Continue simulation to verification phase
for tick = sim.tick + 1, cfg.verify_start do
    sim:step()
end

-- Calculate metrics
local tracker_stats = tracker:stats()
local discovery_latency = math.max(
    a_discovered_b_tick - a_broadcast_tick,
    b_discovered_a_tick - b_broadcast_tick
)

-- Record metrics
result:add_metrics({
    a_knows_b = a_knows_b and 1 or 0,
    b_knows_a = b_knows_a and 1 or 0,
    a_has_b_keys = a_has_b_keys and 1 or 0,
    b_has_a_keys = b_has_a_keys and 1 or 0,
    mutual_discovery_ticks = discovery_latency,
    pq_key_exchange_complete = (a_has_b_keys and b_has_a_keys) and 1.0 or 0.0,
    discovery_completeness = tracker_stats.completeness,
    pq_completeness = tracker_stats.pq_completeness,
})

-- Assertions
result:record_assertion("a_knows_b", a_knows_b, true, a_knows_b)
result:record_assertion("b_knows_a", b_knows_a, true, b_knows_a)
result:record_assertion("a_has_b_pq_keys", a_has_b_keys, true, a_has_b_keys)
result:record_assertion("b_has_a_pq_keys", b_has_a_keys, true, b_has_a_keys)

-- Validate PQ key sizes
local a_kem_size = peer_state:get_pq_keys(a).kem_encap_key_size
local a_dsa_size = peer_state:get_pq_keys(a).dsa_verifying_key_size
local b_kem_size = peer_state:get_pq_keys(b).kem_encap_key_size
local b_dsa_size = peer_state:get_pq_keys(b).dsa_verifying_key_size

result:record_assertion("a_kem_key_size",
    a_kem_size == discovery.PQ_KEYS.kem_encap_key,
    discovery.PQ_KEYS.kem_encap_key, a_kem_size)
result:record_assertion("b_kem_key_size",
    b_kem_size == discovery.PQ_KEYS.kem_encap_key,
    discovery.PQ_KEYS.kem_encap_key, b_kem_size)
result:record_assertion("a_dsa_key_size",
    a_dsa_size == discovery.PQ_KEYS.dsa_verifying_key,
    discovery.PQ_KEYS.dsa_verifying_key, a_dsa_size)
result:record_assertion("b_dsa_key_size",
    b_dsa_size == discovery.PQ_KEYS.dsa_verifying_key,
    discovery.PQ_KEYS.dsa_verifying_key, b_dsa_size)

phases.verify_complete = true

-- ============================================================================
-- FINAL RESULTS
-- ============================================================================

local final_result = result:build()

logger.info("Two-peer discovery scenario completed", {
    passed = final_result.passed,
    level = final_result.level,
    duration_sec = final_result.duration_sec,
    final_tick = sim.tick,
    a_knows_b = a_knows_b,
    b_knows_a = b_knows_a,
    a_has_b_keys = a_has_b_keys,
    b_has_a_keys = b_has_a_keys,
    discovery_latency_ticks = discovery_latency,
    realm_id = realm_id,
})

-- Standard assertions
indras.assert.eq(a_knows_b, true, "A should know B")
indras.assert.eq(b_knows_a, true, "B should know A")
indras.assert.eq(a_has_b_keys, true, "A should have B's PQ keys")
indras.assert.eq(b_has_a_keys, true, "B should have A's PQ keys")
indras.assert.eq(a_kem_size, discovery.PQ_KEYS.kem_encap_key, "A KEM key size should be 1184")
indras.assert.eq(b_kem_size, discovery.PQ_KEYS.kem_encap_key, "B KEM key size should be 1184")
indras.assert.eq(a_dsa_size, discovery.PQ_KEYS.dsa_verifying_key, "A DSA key size should be 1952")
indras.assert.eq(b_dsa_size, discovery.PQ_KEYS.dsa_verifying_key, "B DSA key size should be 1952")

logger.info("Two-peer discovery scenario passed", {
    realm_id = realm_id,
    discovery_latency_ticks = discovery_latency,
})

return final_result

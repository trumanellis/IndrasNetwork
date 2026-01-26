-- Two-Peer Discovery Scenario
--
-- Tests the fundamental discovery flow: two peers discover each other,
-- birthing a realm between them.
--
-- Flow:
-- 1. Alice and Bob both come online
-- 2. Alice broadcasts presence with her PQ keys
-- 3. Bob receives Alice's broadcast, learns about her
-- 4. Bob broadcasts presence with his PQ keys
-- 5. Alice receives Bob's broadcast, learns about him
-- 6. Realm {Alice, Bob} now implicitly exists
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

-- Create mesh with just 2 peers (Alice and Bob)
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
local alice = peers[1]
local bob = peers[2]

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
local alice_broadcast_tick = nil
local bob_broadcast_tick = nil
local alice_discovered_bob_tick = nil
local bob_discovered_alice_tick = nil

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

-- Bring Alice online
peer_state:bring_online(alice, sim.tick)
sim:force_online(alice)
logger.event(discovery.EVENTS.PEER_ONLINE, {
    tick = sim.tick,
    peer = tostring(alice),
    kem_key_size = peer_state:get_pq_keys(alice).kem_encap_key_size,
    dsa_key_size = peer_state:get_pq_keys(alice).dsa_verifying_key_size,
})

sim:step()

-- Bring Bob online
peer_state:bring_online(bob, sim.tick)
sim:force_online(bob)
logger.event(discovery.EVENTS.PEER_ONLINE, {
    tick = sim.tick,
    peer = tostring(bob),
    kem_key_size = peer_state:get_pq_keys(bob).kem_encap_key_size,
    dsa_key_size = peer_state:get_pq_keys(bob).dsa_verifying_key_size,
})

phases.online_complete = true
logger.info("Phase 1 complete: Both peers online", {
    phase = 1,
    tick = sim.tick,
    alice_online = peer_state:is_online(alice),
    bob_online = peer_state:is_online(bob),
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

-- Alice broadcasts presence
alice_broadcast_tick = sim.tick
local alice_keys = peer_state:get_pq_keys(alice)
logger.event(discovery.EVENTS.PRESENCE_BROADCAST, {
    tick = sim.tick,
    broadcaster = tostring(alice),
    msg_type = discovery.MSG_TYPES.INTERFACE_JOIN,
    kem_key_size = alice_keys.kem_encap_key_size,
    dsa_key_size = alice_keys.dsa_verifying_key_size,
})

sim:step()

-- Bob receives Alice's broadcast and discovers her
peer_state:learn_peer(bob, alice, alice_keys)
tracker:record_discovery(bob, alice, sim.tick)
tracker:record_pq_keys(bob, alice, alice_keys.kem_encap_key_size, alice_keys.dsa_verifying_key_size)
bob_discovered_alice_tick = sim.tick

logger.event(discovery.EVENTS.PRESENCE_RECEIVED, {
    tick = sim.tick,
    receiver = tostring(bob),
    broadcaster = tostring(alice),
})

logger.event(discovery.EVENTS.PEER_DISCOVERED, {
    tick = sim.tick,
    discoverer = tostring(bob),
    discovered = tostring(alice),
    pq_kem_key_size = alice_keys.kem_encap_key_size,
    pq_dsa_key_size = alice_keys.dsa_verifying_key_size,
})

sim:step()

-- Bob broadcasts presence
bob_broadcast_tick = sim.tick
local bob_keys = peer_state:get_pq_keys(bob)
logger.event(discovery.EVENTS.PRESENCE_BROADCAST, {
    tick = sim.tick,
    broadcaster = tostring(bob),
    msg_type = discovery.MSG_TYPES.INTERFACE_JOIN,
    kem_key_size = bob_keys.kem_encap_key_size,
    dsa_key_size = bob_keys.dsa_verifying_key_size,
})

sim:step()

-- Alice receives Bob's broadcast and discovers him
peer_state:learn_peer(alice, bob, bob_keys)
tracker:record_discovery(alice, bob, sim.tick)
tracker:record_pq_keys(alice, bob, bob_keys.kem_encap_key_size, bob_keys.dsa_verifying_key_size)
alice_discovered_bob_tick = sim.tick

logger.event(discovery.EVENTS.PRESENCE_RECEIVED, {
    tick = sim.tick,
    receiver = tostring(alice),
    broadcaster = tostring(bob),
})

logger.event(discovery.EVENTS.PEER_DISCOVERED, {
    tick = sim.tick,
    discoverer = tostring(alice),
    discovered = tostring(bob),
    pq_kem_key_size = bob_keys.kem_encap_key_size,
    pq_dsa_key_size = bob_keys.dsa_verifying_key_size,
})

phases.broadcast_complete = true
logger.info("Phase 2 complete: Broadcasts exchanged", {
    phase = 2,
    tick = sim.tick,
    alice_broadcast_tick = alice_broadcast_tick,
    bob_broadcast_tick = bob_broadcast_tick,
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
local alice_knows_bob = tracker:knows(alice, bob)
local bob_knows_alice = tracker:knows(bob, alice)
local alice_has_bob_keys = tracker:has_keys(alice, bob)
local bob_has_alice_keys = tracker:has_keys(bob, alice)

phases.discovery_complete = alice_knows_bob and bob_knows_alice

-- Calculate realm ID for the pair
local realm_id = discovery.realm_id({alice, bob})

logger.event(discovery.EVENTS.REALM_AVAILABLE, {
    tick = sim.tick,
    realm_id = realm_id,
    members = {tostring(alice), tostring(bob)},
    member_count = 2,
})

logger.info("Phase 3 complete: Discovery state", {
    phase = 3,
    tick = sim.tick,
    alice_knows_bob = alice_knows_bob,
    bob_knows_alice = bob_knows_alice,
    alice_has_bob_keys = alice_has_bob_keys,
    bob_has_alice_keys = bob_has_alice_keys,
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
    alice_discovered_bob_tick - alice_broadcast_tick,
    bob_discovered_alice_tick - bob_broadcast_tick
)

-- Record metrics
result:add_metrics({
    alice_knows_bob = alice_knows_bob and 1 or 0,
    bob_knows_alice = bob_knows_alice and 1 or 0,
    alice_has_bob_keys = alice_has_bob_keys and 1 or 0,
    bob_has_alice_keys = bob_has_alice_keys and 1 or 0,
    mutual_discovery_ticks = discovery_latency,
    pq_key_exchange_complete = (alice_has_bob_keys and bob_has_alice_keys) and 1.0 or 0.0,
    discovery_completeness = tracker_stats.completeness,
    pq_completeness = tracker_stats.pq_completeness,
})

-- Assertions
result:record_assertion("alice_knows_bob", alice_knows_bob, true, alice_knows_bob)
result:record_assertion("bob_knows_alice", bob_knows_alice, true, bob_knows_alice)
result:record_assertion("alice_has_bob_pq_keys", alice_has_bob_keys, true, alice_has_bob_keys)
result:record_assertion("bob_has_alice_pq_keys", bob_has_alice_keys, true, bob_has_alice_keys)

-- Validate PQ key sizes
local alice_kem_size = peer_state:get_pq_keys(alice).kem_encap_key_size
local alice_dsa_size = peer_state:get_pq_keys(alice).dsa_verifying_key_size
local bob_kem_size = peer_state:get_pq_keys(bob).kem_encap_key_size
local bob_dsa_size = peer_state:get_pq_keys(bob).dsa_verifying_key_size

result:record_assertion("alice_kem_key_size",
    alice_kem_size == discovery.PQ_KEYS.kem_encap_key,
    discovery.PQ_KEYS.kem_encap_key, alice_kem_size)
result:record_assertion("bob_kem_key_size",
    bob_kem_size == discovery.PQ_KEYS.kem_encap_key,
    discovery.PQ_KEYS.kem_encap_key, bob_kem_size)
result:record_assertion("alice_dsa_key_size",
    alice_dsa_size == discovery.PQ_KEYS.dsa_verifying_key,
    discovery.PQ_KEYS.dsa_verifying_key, alice_dsa_size)
result:record_assertion("bob_dsa_key_size",
    bob_dsa_size == discovery.PQ_KEYS.dsa_verifying_key,
    discovery.PQ_KEYS.dsa_verifying_key, bob_dsa_size)

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
    alice_knows_bob = alice_knows_bob,
    bob_knows_alice = bob_knows_alice,
    alice_has_bob_keys = alice_has_bob_keys,
    bob_has_alice_keys = bob_has_alice_keys,
    discovery_latency_ticks = discovery_latency,
    realm_id = realm_id,
})

-- Standard assertions
indras.assert.eq(alice_knows_bob, true, "Alice should know Bob")
indras.assert.eq(bob_knows_alice, true, "Bob should know Alice")
indras.assert.eq(alice_has_bob_keys, true, "Alice should have Bob's PQ keys")
indras.assert.eq(bob_has_alice_keys, true, "Bob should have Alice's PQ keys")
indras.assert.eq(alice_kem_size, discovery.PQ_KEYS.kem_encap_key, "Alice KEM key size should be 1184")
indras.assert.eq(bob_kem_size, discovery.PQ_KEYS.kem_encap_key, "Bob KEM key size should be 1184")
indras.assert.eq(alice_dsa_size, discovery.PQ_KEYS.dsa_verifying_key, "Alice DSA key size should be 1952")
indras.assert.eq(bob_dsa_size, discovery.PQ_KEYS.dsa_verifying_key, "Bob DSA key size should be 1952")

logger.info("Two-peer discovery scenario passed", {
    realm_id = realm_id,
    discovery_latency_ticks = discovery_latency,
})

return final_result

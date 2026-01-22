-- Hub Failure Scenario (Star Topology)
-- Tests behavior when the central hub node fails mid-delivery
--
-- Topology: Star with H as hub
--       A
--       |
--   B - H - D
--       |
--       C
--
-- Scenario:
-- 1. All nodes online, verify hub-mediated delivery
-- 2. Start sending message A -> D (must go through H)
-- 3. Hub H fails mid-operation
-- 4. Hub H recovers
-- 5. Verify message is eventually delivered

local ctx = indras.correlation.new_root()

indras.log.info("Starting hub failure scenario", {
    trace_id = ctx.trace_id,
    scenario = "hub_failure",
    description = "Tests star topology resilience when hub fails"
})

-- Create star topology with H as the central hub
local mesh = indras.Mesh.from_edges({
    {'A', 'H'},
    {'B', 'H'},
    {'C', 'H'},
    {'D', 'H'}
})

indras.log.info("Created star topology", {
    trace_id = ctx.trace_id,
    peers = mesh:peer_count(),
    edges = mesh:edge_count(),
    topology = "Star with H as hub"
})

local a = indras.PeerId.new('A')
local b = indras.PeerId.new('B')
local c_peer = indras.PeerId.new('C')
local d = indras.PeerId.new('D')
local h = indras.PeerId.new('H')

-- Verify star structure - no direct connections between spokes
indras.assert.true_(not mesh:are_connected(a, b), "A should not connect directly to B")
indras.assert.true_(not mesh:are_connected(a, d), "A should not connect directly to D")
indras.assert.true_(not mesh:are_connected(b, c_peer), "B should not connect directly to C")

-- All spokes connect through hub
indras.assert.true_(mesh:are_connected(a, h), "A should connect to hub H")
indras.assert.true_(mesh:are_connected(d, h), "D should connect to hub H")

local config = indras.SimConfig.manual()
config.trace_routing = true
local sim = indras.Simulation.new(mesh, config)

-- ============================================
-- Phase 1: Verify hub-mediated delivery works
-- ============================================
indras.log.info("Phase 1: Testing hub-mediated delivery", { trace_id = ctx.trace_id })

-- All nodes online
sim:force_online(a)
sim:force_online(b)
sim:force_online(c_peer)
sim:force_online(d)
sim:force_online(h)

sim:send_message(b, c_peer, "Hub test B->C")
sim:run_ticks(5)

indras.assert.eq(sim.stats.messages_delivered, 1, "B->C message should be delivered via hub")
indras.log.info("Hub-mediated delivery verified", {
    trace_id = ctx.trace_id,
    delivered = sim.stats.messages_delivered
})

-- ============================================
-- Phase 2: Send message then fail hub
-- ============================================
indras.log.info("Phase 2: Sending A->D then failing hub", { trace_id = ctx.trace_id })

sim:send_message(a, d, "Message before hub failure")

-- Run 1 tick to let message reach hub
sim:run_ticks(1)

-- Now fail the hub!
sim:force_offline(h)
indras.assert.true_(not sim:is_online(h), "Hub H should be offline")

indras.log.info("Hub failed!", {
    trace_id = ctx.trace_id,
    tick = sim.tick,
    online_peers = #sim:online_peers()
})

-- Try to run more - delivery should be blocked
local pre_fail_delivered = sim.stats.messages_delivered
sim:run_ticks(10)

indras.log.info("After hub failure", {
    trace_id = ctx.trace_id,
    tick = sim.tick,
    delivered = sim.stats.messages_delivered,
    state = sim:state_summary()
})

-- ============================================
-- Phase 3: Try sending more messages (should queue)
-- ============================================
indras.log.info("Phase 3: Sending more messages while hub down", { trace_id = ctx.trace_id })

sim:send_message(c_peer, a, "Message during hub outage")
sim:run_ticks(5)

indras.log.info("Messages queued during hub outage", {
    trace_id = ctx.trace_id,
    state = sim:state_summary()
})

-- ============================================
-- Phase 4: Recover hub
-- ============================================
indras.log.info("Phase 4: Recovering hub", { trace_id = ctx.trace_id })

sim:force_online(h)
indras.assert.true_(sim:is_online(h), "Hub H should be back online")

-- ============================================
-- Phase 5: Verify delivery resumes
-- ============================================
sim:run_ticks(15)

indras.log.info("After hub recovery", {
    trace_id = ctx.trace_id,
    tick = sim.tick,
    delivered = sim.stats.messages_delivered,
    state = sim:state_summary()
})

-- Should have delivered at least the A->D message
indras.assert.ge(sim.stats.messages_delivered, 2, "Messages should be delivered after hub recovery")

-- ============================================
-- Analyze event timeline
-- ============================================
local events = sim:event_log()
local relay_events = {}
local delivery_events = {}

for i = 1, #events do
    local event = events[i]
    if event.type == "Relay" then
        table.insert(relay_events, event)
    elseif event.type == "Delivered" then
        table.insert(delivery_events, event)
    end
end

indras.log.info("Event summary", {
    trace_id = ctx.trace_id,
    relay_count = #relay_events,
    delivery_count = #delivery_events
})

-- Log delivery timeline
for i, event in ipairs(delivery_events) do
    indras.log.info("Delivery " .. i, {
        trace_id = ctx.trace_id,
        to = event.to,
        tick = event.tick
    })
end

indras.log.info("Hub failure scenario PASSED", {
    trace_id = ctx.trace_id,
    final_tick = sim.tick,
    total_sent = sim.stats.messages_sent,
    total_delivered = sim.stats.messages_delivered,
    total_dropped = sim.stats.messages_dropped,
    delivery_rate = sim.stats:delivery_rate()
})

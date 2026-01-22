-- ABC Relay Test
--
-- Tests basic store-and-forward routing through an offline peer.
-- Topology: A - B - C (triangle mesh)
-- Scenario: A sends to C while C is offline, then C comes online

local ctx = indras.correlation.new_root()
ctx = ctx:with_tag("scenario", "abc_relay")

indras.log.info("Starting ABC relay test", {
    trace_id = ctx.trace_id,
    scenario = "abc_relay"
})

-- Create triangle topology
local mesh = indras.Mesh.from_edges({
    {'A', 'B'},
    {'B', 'C'},
    {'A', 'C'}
})

indras.log.debug("Created mesh", {
    trace_id = ctx.trace_id,
    peers = mesh:peer_count(),
    edges = mesh:edge_count()
})

-- Create simulation with manual control (no random state changes)
local sim = indras.Simulation.new(mesh, indras.SimConfig.manual())

-- A and B come online, C stays offline
local peer_a = indras.PeerId.new('A')
local peer_b = indras.PeerId.new('B')
local peer_c = indras.PeerId.new('C')

sim:force_online(peer_a)
sim:force_online(peer_b)

indras.log.info("Initial state", {
    trace_id = ctx.trace_id,
    a_online = sim:is_online(peer_a),
    b_online = sim:is_online(peer_b),
    c_online = sim:is_online(peer_c)
})

-- A sends to C - message should be held since C is offline
sim:send_message(peer_a, peer_c, "Hello C!")

indras.log.debug("Message sent, running simulation", {
    trace_id = ctx.trace_id,
    from = "A",
    to = "C"
})

sim:run_ticks(5)

-- Message should not be delivered yet
indras.assert.eq(sim.stats.messages_delivered, 0, "Message should not be delivered while C is offline")
indras.assert.eq(sim.stats.messages_sent, 1, "One message should be sent")

indras.log.info("After initial ticks (C offline)", {
    trace_id = ctx.trace_id,
    tick = sim.tick,
    delivered = sim.stats.messages_delivered,
    state = sim:state_summary()
})

-- C comes online
sim:force_online(peer_c)

indras.log.info("C coming online", {
    trace_id = ctx.trace_id,
    tick = sim.tick
})

-- Run more ticks for delivery
sim:run_ticks(10)

-- Message should now be delivered
indras.assert.eq(sim.stats.messages_delivered, 1, "Message should be delivered after C comes online")

indras.log.info("Test completed successfully", {
    trace_id = ctx.trace_id,
    final_tick = sim.tick,
    delivered = sim.stats.messages_delivered,
    delivery_rate = sim.stats:delivery_rate(),
    average_latency = sim.stats:average_latency()
})

-- Return success
return true

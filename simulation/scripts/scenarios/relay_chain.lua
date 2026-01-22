-- Relay Chain Test
--
-- Tests multi-hop message delivery through a chain of peers.
-- Topology: A - B - C - D - E (line)
-- Scenario: A sends to E, requiring multiple relays

local ctx = indras.correlation.new_root()
ctx = ctx:with_tag("scenario", "relay_chain")

indras.log.info("Starting relay chain test", {
    trace_id = ctx.trace_id,
    scenario = "relay_chain"
})

-- Create line topology: A - B - C - D - E
local mesh = indras.MeshBuilder.new(5):line()

indras.log.debug("Created line mesh", {
    trace_id = ctx.trace_id,
    peers = mesh:peer_count(),
    edges = mesh:edge_count(),
    topology = mesh:visualize()
})

-- Create simulation with manual control
local sim = indras.Simulation.new(mesh, indras.SimConfig.manual())

-- Bring all peers online
local peers = mesh:peers()
for _, peer in ipairs(peers) do
    sim:force_online(peer)
end

indras.log.info("All peers online", {
    trace_id = ctx.trace_id,
    peer_count = #peers
})

-- A sends to E (not directly connected, requires 4 hops)
local peer_a = indras.PeerId.new('A')
local peer_e = indras.PeerId.new('E')

indras.log.info("Sending message A -> E", {
    trace_id = ctx.trace_id,
    from = "A",
    to = "E",
    are_connected = mesh:are_connected(peer_a, peer_e)
})

sim:send_message(peer_a, peer_e, "Multi-hop message")

-- Run simulation
sim:run_ticks(20)

-- Check delivery
indras.assert.eq(sim.stats.messages_delivered, 1, "Message should be delivered")
indras.assert.eq(sim.stats.relayed_deliveries, 1, "Should be a relayed delivery")

-- Check event log for relay path
local events = sim:event_log()
local relay_count = 0
for _, event in ipairs(events) do
    if event.type == "Relay" then
        relay_count = relay_count + 1
        indras.log.debug("Relay hop", {
            trace_id = ctx.trace_id,
            from = event.from,
            via = event.via,
            to = event.to,
            tick = event.tick
        })
    end
end

indras.log.info("Relay chain completed", {
    trace_id = ctx.trace_id,
    relay_hops = relay_count,
    total_hops = sim.stats.total_hops,
    average_hops = sim.stats:average_hops(),
    latency = sim.stats:average_latency()
})

-- Should have multiple relay hops (A -> B -> C -> D -> E)
indras.assert.ge(relay_count, 1, "Should have at least one relay hop")

indras.log.info("Relay chain test passed", {
    trace_id = ctx.trace_id
})

return true

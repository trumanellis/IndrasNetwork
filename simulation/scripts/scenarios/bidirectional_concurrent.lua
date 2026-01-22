-- Bidirectional Concurrent Messaging Scenario
-- Tests simultaneous messages in opposite directions
--
-- Topology: A - B - C (line)
--
-- Scenario:
-- 1. A sends to C
-- 2. C sends to A (simultaneously)
-- 3. Both messages should be delivered correctly
-- 4. Verify no deadlocks or race conditions

local ctx = indras.correlation.new_root()

indras.log.info("Starting bidirectional concurrent scenario", {
    trace_id = ctx.trace_id,
    scenario = "bidirectional_concurrent",
    description = "Tests simultaneous messages in opposite directions"
})

-- Create line topology
local mesh = indras.Mesh.from_edges({
    {'A', 'B'},
    {'B', 'C'}
})

indras.log.info("Created line topology", {
    trace_id = ctx.trace_id,
    peers = mesh:peer_count(),
    edges = mesh:edge_count(),
    topology = "A-B-C"
})

local a = indras.PeerId.new('A')
local b = indras.PeerId.new('B')
local c = indras.PeerId.new('C')

local config = indras.SimConfig.manual()
config.trace_routing = true
local sim = indras.Simulation.new(mesh, config)

-- All peers online
sim:force_online(a)
sim:force_online(b)
sim:force_online(c)

indras.log.info("All peers online", {
    trace_id = ctx.trace_id,
    tick = sim.tick
})

-- ============================================
-- Send messages in both directions simultaneously
-- ============================================
indras.log.info("Sending bidirectional messages", { trace_id = ctx.trace_id })

sim:send_message(a, c, "Message A->C")
sim:send_message(c, a, "Message C->A")

indras.log.info("Both messages queued at same tick", {
    trace_id = ctx.trace_id,
    tick = sim.tick,
    sent = sim.stats.messages_sent
})

indras.assert.eq(sim.stats.messages_sent, 2, "Two messages should be sent")

-- ============================================
-- Run simulation
-- ============================================
sim:run_ticks(10)

-- ============================================
-- Verify both messages delivered
-- ============================================
indras.assert.eq(sim.stats.messages_delivered, 2, "Both messages should be delivered")

indras.log.info("Both messages delivered", {
    trace_id = ctx.trace_id,
    delivered = sim.stats.messages_delivered,
    tick = sim.tick
})

-- ============================================
-- Analyze event ordering
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

indras.log.info("Relay events:", { trace_id = ctx.trace_id, count = #relay_events })
for i, event in ipairs(relay_events) do
    indras.log.info("  Relay " .. i, {
        trace_id = ctx.trace_id,
        packet_id = event.packet_id,
        from = event.from,
        via = event.via,
        to = event.to,
        tick = event.tick
    })
end

indras.log.info("Delivery events:", { trace_id = ctx.trace_id, count = #delivery_events })
for i, event in ipairs(delivery_events) do
    indras.log.info("  Delivery " .. i, {
        trace_id = ctx.trace_id,
        packet_id = event.packet_id,
        to = event.to,
        tick = event.tick
    })
end

-- ============================================
-- Test multiple concurrent pairs
-- ============================================
indras.log.info("Testing multiple concurrent message pairs", { trace_id = ctx.trace_id })

-- Send 5 pairs of bidirectional messages
for i = 1, 5 do
    sim:send_message(a, c, "Batch A->C #" .. i)
    sim:send_message(c, a, "Batch C->A #" .. i)
end

indras.assert.eq(sim.stats.messages_sent, 12, "Should have 12 messages total (2 + 10)")

sim:run_ticks(20)

indras.assert.eq(sim.stats.messages_delivered, 12, "All 12 messages should be delivered")

indras.log.info("All concurrent messages delivered", {
    trace_id = ctx.trace_id,
    total_sent = sim.stats.messages_sent,
    total_delivered = sim.stats.messages_delivered
})

-- ============================================
-- Test with intermediate node traffic
-- ============================================
indras.log.info("Testing with B also sending messages", { trace_id = ctx.trace_id })

-- B sends to both A and C while they're exchanging
sim:send_message(b, a, "B->A")
sim:send_message(b, c, "B->C")
sim:send_message(a, c, "A->C concurrent with B")
sim:send_message(c, a, "C->A concurrent with B")

sim:run_ticks(10)

indras.assert.eq(sim.stats.messages_delivered, 16, "All messages including B's should be delivered")

-- ============================================
-- Verify no deadlocks (all messages processed)
-- ============================================
local final_state = sim:state_summary()
indras.log.info("Final state check", {
    trace_id = ctx.trace_id,
    state = final_state
})

-- Check that relay queues are empty (no stuck messages)
-- The state_summary shows "X relay queued" - we want 0
indras.assert.eq(sim.stats.messages_sent, sim.stats.messages_delivered + sim.stats.messages_dropped,
    "All sent messages should be either delivered or dropped")

indras.log.info("Bidirectional concurrent scenario PASSED", {
    trace_id = ctx.trace_id,
    final_tick = sim.tick,
    total_sent = sim.stats.messages_sent,
    total_delivered = sim.stats.messages_delivered,
    total_dropped = sim.stats.messages_dropped,
    delivery_rate = sim.stats:delivery_rate(),
    average_latency = sim.stats:average_latency(),
    average_hops = sim.stats:average_hops()
})

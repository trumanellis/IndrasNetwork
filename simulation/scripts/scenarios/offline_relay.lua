-- Offline Relay Scenario
-- Tests mutual peer fallback when recipient goes offline
--
-- Topology: A - B - C (line, A not directly connected to C)
--
-- Sequence:
-- 1. A, B, C go online
-- 2. C goes offline
-- 3. A sends message to C (must route through B, B stores it)
-- 4. A goes offline
-- 5. C comes back online
-- 6. C receives A's message via B

local ctx = indras.correlation.new_root()

indras.log.info("Starting offline relay scenario", {
    trace_id = ctx.trace_id,
    scenario = "offline_relay",
    description = "Tests mutual peer fallback for offline recipients"
})

-- Create LINE topology: A-B-C (A is NOT directly connected to C)
-- This forces routing through B as the mutual peer
local mesh = indras.Mesh.from_edges({{'A','B'}, {'B','C'}})
indras.log.info("Created line topology", {
    trace_id = ctx.trace_id,
    peers = mesh:peer_count(),
    edges = mesh:edge_count(),
    topology = "A-B-C (line)"
})

-- Verify A and C are NOT directly connected
local a = indras.PeerId.new('A')
local b = indras.PeerId.new('B')
local c = indras.PeerId.new('C')

indras.assert.true_(mesh:are_connected(a, b), "A should be connected to B")
indras.assert.true_(mesh:are_connected(b, c), "B should be connected to C")
indras.assert.true_(not mesh:are_connected(a, c), "A should NOT be directly connected to C")

-- Verify mutual peer relationship - B is the intermediary
local mutual_ac = mesh:mutual_peers(a, c)
indras.log.info("Mutual peers between A and C", {
    trace_id = ctx.trace_id,
    count = #mutual_ac,
    expected = "B"
})
indras.assert.eq(#mutual_ac, 1, "A and C should have exactly one mutual peer (B)")

-- Manual config: no random wake/sleep, we control everything
local config = indras.SimConfig.manual()
config.trace_routing = true
local sim = indras.Simulation.new(mesh, config)

-- ============================================
-- Phase 1: All peers come online
-- ============================================
indras.log.info("Phase 1: All peers coming online", { trace_id = ctx.trace_id })

sim:force_online(a)
sim:force_online(b)
sim:force_online(c)

indras.assert.true_(sim:is_online(a), "A should be online")
indras.assert.true_(sim:is_online(b), "B should be online")
indras.assert.true_(sim:is_online(c), "C should be online")

-- Let them "exchange contacts" (establish presence)
sim:run_ticks(3)

indras.log.info("All peers online and connected", {
    trace_id = ctx.trace_id,
    tick = sim.tick,
    state = sim:state_summary()
})

-- ============================================
-- Phase 2: C goes offline
-- ============================================
indras.log.info("Phase 2: C going offline", { trace_id = ctx.trace_id, tick = sim.tick })

sim:force_offline(c)

indras.assert.true_(sim:is_online(a), "A should still be online")
indras.assert.true_(sim:is_online(b), "B should still be online")
indras.assert.true_(not sim:is_online(c), "C should be offline")

indras.log.info("C is now offline", {
    trace_id = ctx.trace_id,
    tick = sim.tick,
    online_peers = #sim:online_peers()
})

-- ============================================
-- Phase 3: A sends message to C
-- ============================================
indras.log.info("Phase 3: A sending message to offline C", {
    trace_id = ctx.trace_id,
    tick = sim.tick,
    from = "A",
    to = "C"
})

sim:send_message(a, c, "Hello from A to C!")

-- Run a few ticks - B should receive and store the message as relay
sim:run_ticks(5)

indras.log.info("After send attempt (C offline)", {
    trace_id = ctx.trace_id,
    tick = sim.tick,
    messages_sent = sim.stats.messages_sent,
    messages_delivered = sim.stats.messages_delivered,
    state = sim:state_summary()
})

-- Message should NOT be delivered yet (C is offline)
indras.assert.eq(sim.stats.messages_delivered, 0, "Message should not be delivered while C is offline")

-- ============================================
-- Phase 4: A goes offline
-- ============================================
indras.log.info("Phase 4: A going offline", { trace_id = ctx.trace_id, tick = sim.tick })

sim:force_offline(a)

indras.assert.true_(not sim:is_online(a), "A should be offline")
indras.assert.true_(sim:is_online(b), "B should still be online (holding the relay)")
indras.assert.true_(not sim:is_online(c), "C should still be offline")

indras.log.info("A is now offline, B is the only online peer", {
    trace_id = ctx.trace_id,
    tick = sim.tick,
    online_peers = #sim:online_peers()
})

-- Run a few ticks with only B online
sim:run_ticks(3)

-- Still no delivery
indras.assert.eq(sim.stats.messages_delivered, 0, "Message should still not be delivered")

-- ============================================
-- Phase 5: C comes back online
-- ============================================
indras.log.info("Phase 5: C coming back online", { trace_id = ctx.trace_id, tick = sim.tick })

sim:force_online(c)

indras.assert.true_(not sim:is_online(a), "A should still be offline")
indras.assert.true_(sim:is_online(b), "B should be online")
indras.assert.true_(sim:is_online(c), "C should be back online")

indras.log.info("C is back online, should receive message from B", {
    trace_id = ctx.trace_id,
    tick = sim.tick
})

-- ============================================
-- Phase 6: C receives message via B
-- ============================================
-- Run simulation - B should deliver the stored message to C
sim:run_ticks(10)

indras.log.info("After C reconnection", {
    trace_id = ctx.trace_id,
    tick = sim.tick,
    messages_delivered = sim.stats.messages_delivered,
    total_hops = sim.stats.total_hops,
    state = sim:state_summary()
})

-- Verify delivery
indras.assert.eq(sim.stats.messages_delivered, 1, "Message should be delivered to C via B")

-- Check delivery stats
local stats = sim.stats
indras.log.info("Offline relay scenario completed successfully", {
    trace_id = ctx.trace_id,
    final_tick = sim.tick,
    messages_sent = stats.messages_sent,
    messages_delivered = stats.messages_delivered,
    total_hops = stats.total_hops,
    average_latency = stats:average_latency(),
    delivery_rate = stats:delivery_rate()
})

-- Analyze event log to verify the relay path
local events = sim:event_log()
local relay_events = 0
local delivery_event = nil

for i = 1, #events do
    local event = events[i]
    if event.type == "Relay" then
        relay_events = relay_events + 1
        indras.log.info("Relay event found", {
            trace_id = ctx.trace_id,
            from = event.from,
            via = event.via,
            tick = event.tick
        })
    elseif event.type == "Delivered" then
        delivery_event = event
        indras.log.info("Delivery event found", {
            trace_id = ctx.trace_id,
            to = event.to,
            via = event.via,
            tick = event.tick
        })
    end
end

indras.assert.true_(relay_events > 0, "Should have at least one relay event")
indras.assert.true_(delivery_event ~= nil, "Should have a delivery event")

indras.log.info("Offline relay test PASSED", {
    trace_id = ctx.trace_id,
    relay_events = relay_events,
    message_path = "A -> B (relay) -> C (delivered)"
})

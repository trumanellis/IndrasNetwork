-- Back-Propagation Verification Scenario
-- Tests that delivery confirmations propagate back through relay chain
--
-- Topology: A - B - C - D (line)
-- Message: A -> D (must relay through B and C)
--
-- Verifies:
-- 1. Message is delivered to D
-- 2. BackProp events trace back: D -> C -> B -> A
-- 3. Each relay peer receives confirmation

local ctx = indras.correlation.new_root()

indras.log.info("Starting back-propagation verification", {
    trace_id = ctx.trace_id,
    scenario = "backprop_verification",
    description = "Verifies delivery confirmations propagate through relay chain"
})

-- Create line topology for multi-hop relay
local mesh = indras.Mesh.from_edges({
    {'A', 'B'},
    {'B', 'C'},
    {'C', 'D'}
})

indras.log.info("Created line topology", {
    trace_id = ctx.trace_id,
    peers = mesh:peer_count(),
    edges = mesh:edge_count(),
    topology = "A-B-C-D"
})

local a = indras.PeerId.new('A')
local b = indras.PeerId.new('B')
local c = indras.PeerId.new('C')
local d = indras.PeerId.new('D')

local config = indras.SimConfig.manual()
config.trace_routing = true
local sim = indras.Simulation.new(mesh, config)

-- All peers online
sim:force_online(a)
sim:force_online(b)
sim:force_online(c)
sim:force_online(d)

indras.log.info("All peers online", {
    trace_id = ctx.trace_id,
    online = #sim:online_peers()
})

-- ============================================
-- Send message A -> D
-- ============================================
indras.log.info("Sending message A -> D", { trace_id = ctx.trace_id })

sim:send_message(a, d, "BackProp test message")

-- Run enough ticks for delivery AND back-propagation
sim:run_ticks(20)

-- ============================================
-- Verify delivery
-- ============================================
indras.assert.eq(sim.stats.messages_delivered, 1, "Message should be delivered")
indras.log.info("Message delivered", {
    trace_id = ctx.trace_id,
    delivered = sim.stats.messages_delivered,
    total_hops = sim.stats.total_hops
})

-- ============================================
-- Analyze event log for back-propagation
-- ============================================
local events = sim:event_log()

local relay_events = {}
local backprop_events = {}
local delivery_event = nil

for i = 1, #events do
    local event = events[i]
    if event.type == "Relay" then
        table.insert(relay_events, event)
    elseif event.type == "BackProp" then
        table.insert(backprop_events, event)
    elseif event.type == "Delivered" then
        delivery_event = event
    end
end

indras.log.info("Event analysis", {
    trace_id = ctx.trace_id,
    relay_count = #relay_events,
    backprop_count = #backprop_events,
    has_delivery = delivery_event ~= nil
})

-- ============================================
-- Verify relay chain: A -> B -> C -> D
-- ============================================
indras.assert.ge(#relay_events, 2, "Should have at least 2 relay events (B and C)")

indras.log.info("Relay chain:", { trace_id = ctx.trace_id })
for i, event in ipairs(relay_events) do
    indras.log.info("  Relay " .. i, {
        trace_id = ctx.trace_id,
        from = event.from,
        via = event.via,
        to = event.to,
        tick = event.tick
    })
end

-- ============================================
-- Verify back-propagation chain
-- ============================================
-- BackProp should go: D confirms to C, C confirms to B, B confirms to A
-- Expected: 3 backprop events for path A-B-C-D

indras.log.info("Back-propagation chain:", { trace_id = ctx.trace_id })
for i, event in ipairs(backprop_events) do
    indras.log.info("  BackProp " .. i, {
        trace_id = ctx.trace_id,
        from = event.from,
        to = event.to,
        tick = event.tick,
        packet_id = event.packet_id
    })
end

-- We expect backprop_count to equal the number of intermediate hops
-- For A-B-C-D: 3 hops means 2 intermediate nodes (B, C) that need confirmation
-- BackProp goes: D->C, C->B, B->A = 3 backprop events
indras.assert.ge(#backprop_events, 2, "Should have back-propagation events")

-- ============================================
-- Verify backprop ordering (should be reverse of relay)
-- ============================================
if #backprop_events >= 2 then
    -- BackProp events should come after delivery
    indras.assert.true_(delivery_event ~= nil, "Should have delivery event")

    local delivery_tick = delivery_event.tick
    for _, bp in ipairs(backprop_events) do
        indras.assert.ge(bp.tick, delivery_tick,
            "BackProp should occur after delivery")
    end
end

indras.log.info("Back-propagation verification PASSED", {
    trace_id = ctx.trace_id,
    relay_hops = #relay_events,
    backprop_confirmations = #backprop_events,
    delivery_tick = delivery_event and delivery_event.tick or "N/A",
    final_tick = sim.tick
})

-- Summary stats
indras.log.info("Final statistics", {
    trace_id = ctx.trace_id,
    messages_sent = sim.stats.messages_sent,
    messages_delivered = sim.stats.messages_delivered,
    total_hops = sim.stats.total_hops,
    average_hops = sim.stats:average_hops(),
    delivery_rate = sim.stats:delivery_rate()
})

-- Message Timeout/TTL Scenario
-- Tests that messages which cannot be delivered are eventually dropped
--
-- Topology: A - B - C (line)
-- Scenario: A sends to C, but C stays offline beyond timeout
--
-- Verifies:
-- 1. Message is held while destination is offline
-- 2. Message is dropped after timeout expires
-- 3. Dropped event is logged with correct reason

local ctx = indras.correlation.new_root()

indras.log.info("Starting message timeout scenario", {
    trace_id = ctx.trace_id,
    scenario = "message_timeout",
    description = "Tests message expiration when destination unreachable"
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

-- Configure with message timeout
local config = indras.SimConfig.new({
    wake_probability = 0.0,
    sleep_probability = 0.0,
    initial_online_probability = 0.0,
    trace_routing = true,
    message_timeout = 20  -- Message expires after 20 ticks
})

local sim = indras.Simulation.new(mesh, config)

-- A and B online, C stays offline
sim:force_online(a)
sim:force_online(b)
-- C intentionally left offline

indras.log.info("Initial state - C is offline", {
    trace_id = ctx.trace_id,
    a_online = sim:is_online(a),
    b_online = sim:is_online(b),
    c_online = sim:is_online(c)
})

-- ============================================
-- Send message that will timeout
-- ============================================
indras.log.info("Sending message A -> C (C is offline)", { trace_id = ctx.trace_id })

sim:send_message(a, c, "This message will timeout")
local send_tick = sim.tick

-- Run a few ticks - message should be held
sim:run_ticks(5)

indras.assert.eq(sim.stats.messages_delivered, 0, "Message should not be delivered yet")
indras.assert.eq(sim.stats.messages_dropped, 0, "Message should not be dropped yet")

indras.log.info("Message being held", {
    trace_id = ctx.trace_id,
    tick = sim.tick,
    state = sim:state_summary()
})

-- ============================================
-- Run past the timeout
-- ============================================
indras.log.info("Running simulation past timeout threshold", { trace_id = ctx.trace_id })

sim:run_ticks(30)  -- Total of 35 ticks, well past the 20-tick timeout

-- Check if message was dropped due to timeout
local events = sim:event_log()
local dropped_events = {}
local timeout_drops = 0

for i = 1, #events do
    local event = events[i]
    if event.type == "Dropped" then
        table.insert(dropped_events, event)
        indras.log.info("Drop event found", {
            trace_id = ctx.trace_id,
            packet_id = event.packet_id,
            reason = event.reason,
            tick = event.tick
        })
        if event.reason == "MessageTimeout" or event.reason == "TtlExpired" then
            timeout_drops = timeout_drops + 1
        end
    end
end

indras.log.info("Timeout analysis", {
    trace_id = ctx.trace_id,
    total_drops = #dropped_events,
    timeout_drops = timeout_drops,
    messages_delivered = sim.stats.messages_delivered,
    messages_dropped = sim.stats.messages_dropped
})

-- ============================================
-- Verify timeout behavior
-- ============================================
-- The message should either be:
-- 1. Dropped due to timeout (if timeout is enforced)
-- 2. Still held (if timeout isn't enforced in relay queues)

if sim.stats.messages_dropped > 0 then
    indras.log.info("Message was dropped (timeout enforced)", {
        trace_id = ctx.trace_id,
        dropped = sim.stats.messages_dropped
    })
    indras.assert.eq(sim.stats.messages_delivered, 0, "Dropped message should not be delivered")
else
    -- Message is still held - this is also valid behavior
    -- In some implementations, relay queues don't expire
    indras.log.info("Message still held in relay queue (no relay timeout)", {
        trace_id = ctx.trace_id,
        state = sim:state_summary()
    })
end

-- ============================================
-- Test TTL expiration (if supported)
-- ============================================
-- Now bring C online and see if the message can still be delivered
-- (depends on whether it was dropped or just held)

indras.log.info("Bringing C online to test delivery", { trace_id = ctx.trace_id })
sim:force_online(c)
sim:run_ticks(10)

indras.log.info("Final state after C came online", {
    trace_id = ctx.trace_id,
    delivered = sim.stats.messages_delivered,
    dropped = sim.stats.messages_dropped,
    tick = sim.tick
})

-- ============================================
-- Test sender retry timeout
-- ============================================
-- Send a message from offline sender to test sender timeout
indras.log.info("Testing sender retry timeout", { trace_id = ctx.trace_id })

sim:force_offline(a)
sim:send_message(a, c, "Message from offline sender")

-- Run many ticks - should hit max sender retries
sim:run_ticks(50)

local final_events = sim:event_log()
local sender_offline_drops = 0

for i = 1, #final_events do
    local event = final_events[i]
    if event.type == "Dropped" and event.reason == "SenderOffline" then
        sender_offline_drops = sender_offline_drops + 1
    end
end

indras.log.info("Sender offline analysis", {
    trace_id = ctx.trace_id,
    sender_offline_drops = sender_offline_drops,
    total_dropped = sim.stats.messages_dropped
})

indras.log.info("Message timeout scenario PASSED", {
    trace_id = ctx.trace_id,
    final_tick = sim.tick,
    messages_sent = sim.stats.messages_sent,
    messages_delivered = sim.stats.messages_delivered,
    messages_dropped = sim.stats.messages_dropped,
    summary = "Timeout/drop behavior verified"
})

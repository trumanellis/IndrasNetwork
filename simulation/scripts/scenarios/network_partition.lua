-- Network Partition Scenario
-- Tests message delivery across a network partition and recovery
--
-- Topology: Two clusters connected by a bridge node
--   Cluster 1: A - B - C (bridge)
--   Cluster 2: C (bridge) - D - E
--
-- Sequence:
-- 1. All nodes online, verify cross-cluster delivery works
-- 2. Bridge node C goes offline (partition)
-- 3. Attempt to send from A to E (should be held)
-- 4. C comes back online (partition heals)
-- 5. Message should be delivered

local ctx = indras.correlation.new_root()

indras.log.info("Starting network partition scenario", {
    trace_id = ctx.trace_id,
    scenario = "network_partition",
    description = "Tests cross-partition delivery and recovery"
})

-- Create two-cluster topology with C as bridge
-- A-B-C-D-E (line with C as the bridge between clusters)
local mesh = indras.Mesh.from_edges({
    {'A', 'B'},  -- Cluster 1
    {'B', 'C'},  -- Bridge connection
    {'C', 'D'},  -- Bridge connection
    {'D', 'E'}   -- Cluster 2
})

indras.log.info("Created bridged topology", {
    trace_id = ctx.trace_id,
    peers = mesh:peer_count(),
    edges = mesh:edge_count(),
    topology = "A-B-C-D-E (C is bridge)"
})

local a = indras.PeerId.new('A')
local b = indras.PeerId.new('B')
local c = indras.PeerId.new('C')
local d = indras.PeerId.new('D')
local e = indras.PeerId.new('E')

-- Verify C is the only path between A and E
indras.assert.true_(not mesh:are_connected(a, e), "A should not be directly connected to E")
indras.assert.true_(not mesh:are_connected(b, d), "B should not be directly connected to D")

local config = indras.SimConfig.manual()
config.trace_routing = true
local sim = indras.Simulation.new(mesh, config)

-- ============================================
-- Phase 1: Verify cross-cluster delivery works
-- ============================================
indras.log.info("Phase 1: Testing cross-cluster delivery", { trace_id = ctx.trace_id })

-- All nodes online
for _, peer in ipairs({a, b, c, d, e}) do
    sim:force_online(peer)
end

sim:send_message(a, e, "Cross-cluster test")
sim:run_ticks(10)

indras.assert.eq(sim.stats.messages_delivered, 1, "Cross-cluster message should be delivered")
indras.log.info("Cross-cluster delivery verified", {
    trace_id = ctx.trace_id,
    delivered = sim.stats.messages_delivered,
    hops = sim.stats.total_hops
})

-- ============================================
-- Phase 2: Create partition (bridge goes offline)
-- ============================================
indras.log.info("Phase 2: Creating network partition", { trace_id = ctx.trace_id })

sim:force_offline(c)

indras.assert.true_(not sim:is_online(c), "Bridge C should be offline")
indras.log.info("Partition created - C is offline", {
    trace_id = ctx.trace_id,
    tick = sim.tick,
    online_peers = #sim:online_peers()
})

-- ============================================
-- Phase 3: Attempt cross-partition message
-- ============================================
indras.log.info("Phase 3: Sending message across partition", { trace_id = ctx.trace_id })

local pre_send_delivered = sim.stats.messages_delivered
sim:send_message(a, e, "Message during partition")
sim:run_ticks(10)

-- Message should NOT be delivered (partition blocks it)
indras.assert.eq(sim.stats.messages_delivered, pre_send_delivered,
    "Message should not be delivered during partition")

indras.log.info("Message held during partition", {
    trace_id = ctx.trace_id,
    tick = sim.tick,
    delivered = sim.stats.messages_delivered,
    state = sim:state_summary()
})

-- ============================================
-- Phase 4: Heal partition (bridge comes back)
-- ============================================
indras.log.info("Phase 4: Healing partition - C coming online", { trace_id = ctx.trace_id })

sim:force_online(c)
indras.assert.true_(sim:is_online(c), "Bridge C should be online")

-- ============================================
-- Phase 5: Message should be delivered
-- ============================================
sim:run_ticks(15)

indras.assert.eq(sim.stats.messages_delivered, pre_send_delivered + 1,
    "Message should be delivered after partition heals")

indras.log.info("Network partition scenario PASSED", {
    trace_id = ctx.trace_id,
    final_tick = sim.tick,
    total_delivered = sim.stats.messages_delivered,
    total_hops = sim.stats.total_hops
})

-- Analyze relay path
local events = sim:event_log()
local partition_relays = 0
for i = 1, #events do
    local event = events[i]
    if event.type == "Relay" and event.tick > 10 then
        partition_relays = partition_relays + 1
        indras.log.info("Post-partition relay", {
            trace_id = ctx.trace_id,
            from = event.from,
            via = event.via,
            tick = event.tick
        })
    end
end

indras.log.info("Partition recovery complete", {
    trace_id = ctx.trace_id,
    post_partition_relays = partition_relays
})

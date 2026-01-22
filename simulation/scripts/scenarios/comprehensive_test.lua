-- Comprehensive Test
--
-- Tests all mlua bindings and features to ensure everything works correctly.

local ctx = indras.correlation.new_root()
indras.log.info("Starting comprehensive Lua feature test", { trace_id = ctx.trace_id })

-- ============================================================
-- Test 1: PeerId API
-- ============================================================
indras.log.debug("Testing PeerId API", { trace_id = ctx.trace_id })

-- PeerId.new()
local peer_a = indras.PeerId.new('A')
local peer_b = indras.PeerId.new('B')
local peer_z = indras.PeerId.new('Z')

indras.assert.eq(tostring(peer_a), "A", "PeerId tostring should work")
indras.assert.eq(peer_a.char, "A", "PeerId char field should work")

-- PeerId equality
indras.assert.true_(peer_a == indras.PeerId.new('A'), "PeerId equality should work")
indras.assert.true_(peer_a ~= peer_b, "PeerId inequality should work")

-- PeerId ordering
indras.assert.true_(peer_a < peer_b, "PeerId < should work")
indras.assert.true_(peer_b <= peer_b, "PeerId <= should work")

-- PeerId.range_to()
local peers = indras.PeerId.range_to('D')
indras.assert.eq(#peers, 4, "PeerId.range_to should generate correct count")
indras.assert.eq(tostring(peers[1]), "A", "First peer should be A")
indras.assert.eq(tostring(peers[4]), "D", "Last peer should be D")

indras.log.debug("PeerId API tests passed", { trace_id = ctx.trace_id })

-- ============================================================
-- Test 2: Priority API
-- ============================================================
indras.log.debug("Testing Priority API", { trace_id = ctx.trace_id })

local p_low = indras.Priority.low()
local p_normal = indras.Priority.normal()
local p_high = indras.Priority.high()
local p_critical = indras.Priority.critical()

indras.assert.eq(tostring(p_low), "low", "Priority tostring should work")
indras.assert.eq(p_high.value, "high", "Priority value field should work")
indras.assert.true_(p_low == indras.Priority.low(), "Priority equality should work")

indras.log.debug("Priority API tests passed", { trace_id = ctx.trace_id })

-- ============================================================
-- Test 3: MeshBuilder API
-- ============================================================
indras.log.debug("Testing MeshBuilder API", { trace_id = ctx.trace_id })

-- ring()
local ring = indras.MeshBuilder.new(4):ring()
indras.assert.eq(ring:peer_count(), 4, "Ring should have 4 peers")
indras.assert.eq(ring:edge_count(), 4, "Ring with 4 peers should have 4 edges")

-- full_mesh()
local full = indras.MeshBuilder.new(4):full_mesh()
indras.assert.eq(full:peer_count(), 4, "Full mesh should have 4 peers")
indras.assert.eq(full:edge_count(), 6, "Full mesh with 4 peers should have C(4,2)=6 edges")

-- line()
local line = indras.MeshBuilder.new(5):line()
indras.assert.eq(line:peer_count(), 5, "Line should have 5 peers")
indras.assert.eq(line:edge_count(), 4, "Line with 5 peers should have 4 edges")

-- star()
local star = indras.MeshBuilder.new(5):star()
indras.assert.eq(star:peer_count(), 5, "Star should have 5 peers")
indras.assert.eq(star:edge_count(), 4, "Star with 5 peers should have 4 edges")

-- random()
local random_mesh = indras.MeshBuilder.new(5):random(0.5)
indras.assert.eq(random_mesh:peer_count(), 5, "Random mesh should have 5 peers")
indras.assert.ge(random_mesh:edge_count(), 1, "Random mesh should have at least some edges")

indras.log.debug("MeshBuilder API tests passed", { trace_id = ctx.trace_id })

-- ============================================================
-- Test 4: Mesh API
-- ============================================================
indras.log.debug("Testing Mesh API", { trace_id = ctx.trace_id })

-- Mesh.new() and Mesh.from_edges()
local empty_mesh = indras.Mesh.new()
indras.assert.eq(empty_mesh:peer_count(), 0, "Empty mesh should have 0 peers")

local custom = indras.Mesh.from_edges({{'A', 'B'}, {'B', 'C'}, {'C', 'D'}})
indras.assert.eq(custom:peer_count(), 4, "Custom mesh should have 4 peers")
indras.assert.eq(custom:edge_count(), 3, "Custom mesh should have 3 edges")

-- peers()
local mesh_peers = custom:peers()
indras.assert.eq(#mesh_peers, 4, "Should get 4 peers")

-- neighbors()
local b_neighbors = custom:neighbors(indras.PeerId.new('B'))
indras.assert.eq(#b_neighbors, 2, "B should have 2 neighbors (A and C)")

-- are_connected()
indras.assert.true_(custom:are_connected(indras.PeerId.new('A'), indras.PeerId.new('B')), "A-B should be connected")
indras.assert.false_(custom:are_connected(indras.PeerId.new('A'), indras.PeerId.new('C')), "A-C should not be directly connected")

-- mutual_peers()
local mutual = custom:mutual_peers(indras.PeerId.new('A'), indras.PeerId.new('C'))
indras.assert.eq(#mutual, 1, "A and C should share B as mutual peer")
indras.assert.eq(tostring(mutual[1]), "B", "Mutual peer should be B")

-- visualize()
local viz = custom:visualize()
indras.assert.true_(string.find(viz, "Peers:") ~= nil, "Visualize should include 'Peers:'")

-- tostring
local mesh_str = tostring(custom)
indras.assert.true_(string.find(mesh_str, "Mesh") ~= nil, "Mesh tostring should work")

indras.log.debug("Mesh API tests passed", { trace_id = ctx.trace_id })

-- ============================================================
-- Test 5: SimConfig API
-- ============================================================
indras.log.debug("Testing SimConfig API", { trace_id = ctx.trace_id })

-- SimConfig.default()
local cfg_default = indras.SimConfig.default()
indras.assert.approx(cfg_default.wake_probability, 0.3, 0.01, "Default wake probability")
indras.assert.approx(cfg_default.sleep_probability, 0.2, 0.01, "Default sleep probability")
indras.assert.eq(cfg_default.max_ticks, 100, "Default max_ticks")

-- SimConfig.manual()
local cfg_manual = indras.SimConfig.manual()
indras.assert.eq(cfg_manual.wake_probability, 0.0, "Manual wake probability should be 0")
indras.assert.eq(cfg_manual.sleep_probability, 0.0, "Manual sleep probability should be 0")

-- SimConfig.new() with custom values
local cfg_custom = indras.SimConfig.new({
    wake_probability = 0.5,
    sleep_probability = 0.1,
    max_ticks = 200,
    trace_routing = false
})
indras.assert.approx(cfg_custom.wake_probability, 0.5, 0.01, "Custom wake probability")
indras.assert.eq(cfg_custom.max_ticks, 200, "Custom max_ticks")

-- Field setters
cfg_custom.wake_probability = 0.7
indras.assert.approx(cfg_custom.wake_probability, 0.7, 0.01, "Wake probability setter should work")

-- tostring
local cfg_str = tostring(cfg_default)
indras.assert.true_(string.find(cfg_str, "SimConfig") ~= nil, "SimConfig tostring should work")

indras.log.debug("SimConfig API tests passed", { trace_id = ctx.trace_id })

-- ============================================================
-- Test 6: Simulation API
-- ============================================================
indras.log.debug("Testing Simulation API", { trace_id = ctx.trace_id })

local mesh = indras.MeshBuilder.new(3):full_mesh()
local sim = indras.Simulation.new(mesh, indras.SimConfig.manual())

-- tick field
indras.assert.eq(sim.tick, 0, "Initial tick should be 0")

-- step()
sim:step()
indras.assert.eq(sim.tick, 1, "Tick should be 1 after step")

-- run_ticks()
sim:run_ticks(5)
indras.assert.eq(sim.tick, 6, "Tick should be 6 after run_ticks(5)")

-- force_online() / force_offline() / is_online()
local pa = indras.PeerId.new('A')
local pb = indras.PeerId.new('B')
local pc = indras.PeerId.new('C')

sim:force_online(pa)
indras.assert.true_(sim:is_online(pa), "A should be online")

sim:force_offline(pa)
indras.assert.false_(sim:is_online(pa), "A should be offline after force_offline")

-- online_peers() / offline_peers()
sim:force_online(pa)
sim:force_online(pb)
local online = sim:online_peers()
local offline = sim:offline_peers()
indras.assert.eq(#online, 2, "Should have 2 online peers")
indras.assert.eq(#offline, 1, "Should have 1 offline peer")

-- send_message() with string payload
sim:send_message(pa, pb, "Hello from A")
sim:run_ticks(5)

-- send_message() with byte array payload
sim:send_message(pb, pa, {72, 101, 108, 108, 111})  -- "Hello" as bytes
sim:run_ticks(5)

-- state_summary()
local summary = sim:state_summary()
indras.assert.true_(string.find(summary, "Tick") ~= nil, "State summary should include 'Tick'")

-- mesh field
local sim_mesh = sim.mesh
indras.assert.eq(sim_mesh:peer_count(), 3, "Simulation mesh should have 3 peers")

-- event_log()
local events = sim:event_log()
indras.assert.gt(#events, 0, "Should have some events in log")

-- Check event structure
local found_awake = false
for _, event in ipairs(events) do
    if event.type == "Awake" then
        found_awake = true
        indras.assert.not_nil(event.peer, "Awake event should have peer")
        indras.assert.not_nil(event.tick, "Awake event should have tick")
    end
end
indras.assert.true_(found_awake, "Should have at least one Awake event")

-- tostring
local sim_str = tostring(sim)
indras.assert.true_(string.find(sim_str, "Simulation") ~= nil, "Simulation tostring should work")

indras.log.debug("Simulation API tests passed", { trace_id = ctx.trace_id })

-- ============================================================
-- Test 7: SimStats API
-- ============================================================
indras.log.debug("Testing SimStats API", { trace_id = ctx.trace_id })

local stats = sim.stats

-- Field accessors
indras.assert.ge(stats.messages_sent, 0, "messages_sent should be >= 0")
indras.assert.ge(stats.messages_delivered, 0, "messages_delivered should be >= 0")
indras.assert.ge(stats.messages_dropped, 0, "messages_dropped should be >= 0")
indras.assert.ge(stats.total_hops, 0, "total_hops should be >= 0")
indras.assert.ge(stats.direct_deliveries, 0, "direct_deliveries should be >= 0")
indras.assert.ge(stats.relayed_deliveries, 0, "relayed_deliveries should be >= 0")
indras.assert.ge(stats.wake_events, 0, "wake_events should be >= 0")
indras.assert.ge(stats.sleep_events, 0, "sleep_events should be >= 0")

-- Computed methods
local dr = stats:delivery_rate()
indras.assert.ge(dr, 0.0, "delivery_rate should be >= 0")
indras.assert.le(dr, 1.0, "delivery_rate should be <= 1")

local drop_rate = stats:drop_rate()
indras.assert.ge(drop_rate, 0.0, "drop_rate should be >= 0")

local avg_lat = stats:average_latency()
indras.assert.ge(avg_lat, 0.0, "average_latency should be >= 0")

local avg_hops = stats:average_hops()
indras.assert.ge(avg_hops, 0.0, "average_hops should be >= 0")

-- to_table()
local stats_table = stats:to_table()
indras.assert.not_nil(stats_table.messages_sent, "to_table should have messages_sent")
indras.assert.not_nil(stats_table.messages_delivered, "to_table should have messages_delivered")

-- tostring
local stats_str = tostring(stats)
indras.assert.true_(string.find(stats_str, "SimStats") ~= nil, "SimStats tostring should work")

indras.log.debug("SimStats API tests passed", { trace_id = ctx.trace_id })

-- ============================================================
-- Test 8: Correlation Context API
-- ============================================================
indras.log.debug("Testing Correlation Context API", { trace_id = ctx.trace_id })

-- new_root()
local root = indras.correlation.new_root()
indras.assert.not_nil(root.trace_id, "Root should have trace_id")
indras.assert.not_nil(root.span_id, "Root should have span_id")
indras.assert.nil_(root.parent_span_id, "Root should not have parent_span_id")
indras.assert.eq(root.hop_count, 0, "Root hop_count should be 0")

-- child()
local child = root:child()
indras.assert.eq(child.trace_id, root.trace_id, "Child should have same trace_id")
indras.assert.ne(child.span_id, root.span_id, "Child should have different span_id")
indras.assert.eq(child.parent_span_id, root.span_id, "Child parent should be root span")
indras.assert.eq(child.hop_count, 1, "Child hop_count should be 1")

-- with_packet_id()
local with_pid = root:with_packet_id("A#42")
indras.assert.eq(with_pid.packet_id, "A#42", "with_packet_id should set packet_id")

-- with_tag()
local with_tag = root:with_tag("scenario", "test")
indras.assert.true_(string.find(with_tag.packet_id, "scenario=test") ~= nil, "with_tag should set tag")

-- to_traceparent() / from_traceparent()
local tp = root:to_traceparent()
indras.assert.true_(string.sub(tp, 1, 3) == "00-", "Traceparent should start with '00-'")

local parsed = indras.correlation.from_traceparent(tp)
indras.assert.not_nil(parsed, "Should be able to parse traceparent")
indras.assert.eq(parsed.trace_id, root.trace_id, "Parsed trace_id should match")

-- to_table()
local ctx_table = root:to_table()
indras.assert.not_nil(ctx_table.trace_id, "to_table should have trace_id")
indras.assert.not_nil(ctx_table.span_id, "to_table should have span_id")
indras.assert.eq(ctx_table.hop_count, 0, "to_table hop_count should be 0")

-- tostring
local ctx_str = tostring(root)
indras.assert.true_(string.find(ctx_str, "CorrelationContext") ~= nil, "CorrelationContext tostring should work")

indras.log.debug("Correlation Context API tests passed", { trace_id = ctx.trace_id })

-- ============================================================
-- Test 9: Logging API
-- ============================================================
indras.log.debug("Testing Logging API", { trace_id = ctx.trace_id })

-- All log levels (just verify they don't error)
indras.log.trace("Trace message", { level = "trace" })
indras.log.debug("Debug message", { level = "debug" })
indras.log.info("Info message", { level = "info" })
indras.log.warn("Warn message", { level = "warn" })
indras.log.error("Error message", { level = "error" })

-- Log with various field types
indras.log.info("Test with all field types", {
    string_field = "hello",
    number_field = 42,
    float_field = 3.14,
    bool_field = true,
    nil_field = nil,
    array_field = {1, 2, 3},
    object_field = { nested = "value" }
})

-- log.print()
indras.log.print("Direct print message")

indras.log.debug("Logging API tests passed", { trace_id = ctx.trace_id })

-- ============================================================
-- Test 10: Assertions API
-- ============================================================
indras.log.debug("Testing Assertions API", { trace_id = ctx.trace_id })

-- assert.eq / assert.ne
indras.assert.eq(1, 1)
indras.assert.eq("a", "a")
indras.assert.ne(1, 2)
indras.assert.ne("a", "b")

-- assert.gt / assert.ge / assert.lt / assert.le
indras.assert.gt(2, 1)
indras.assert.ge(2, 2)
indras.assert.ge(2, 1)
indras.assert.lt(1, 2)
indras.assert.le(1, 1)
indras.assert.le(1, 2)

-- assert.true_ / assert.false_
indras.assert.true_(true)
indras.assert.true_(1 == 1)
indras.assert.false_(false)
indras.assert.false_(1 == 2)

-- assert.nil_ / assert.not_nil
indras.assert.nil_(nil)
indras.assert.not_nil(1)
indras.assert.not_nil("hello")

-- assert.approx
indras.assert.approx(1.0, 1.0000001, 0.001)
indras.assert.approx(3.14159, 3.14, 0.01)

-- assert.contains
indras.assert.contains({1, 2, 3}, 2)
indras.assert.contains({"a", "b", "c"}, "b")

-- assert.len
indras.assert.len({1, 2, 3}, 3)
indras.assert.len({}, 0)

indras.log.debug("Assertions API tests passed", { trace_id = ctx.trace_id })

-- ============================================================
-- Test 11: Event Constants
-- ============================================================
indras.log.debug("Testing Event Constants", { trace_id = ctx.trace_id })

indras.assert.eq(indras.events.AWAKE, "Awake")
indras.assert.eq(indras.events.SLEEP, "Sleep")
indras.assert.eq(indras.events.SEND, "Send")
indras.assert.eq(indras.events.RELAY, "Relay")
indras.assert.eq(indras.events.DELIVERED, "Delivered")
indras.assert.eq(indras.events.BACKPROP, "BackProp")
indras.assert.eq(indras.events.DROPPED, "Dropped")

indras.assert.eq(indras.events.DropReason.TTL_EXPIRED, "TtlExpired")
indras.assert.eq(indras.events.DropReason.NO_ROUTE, "NoRoute")
indras.assert.eq(indras.events.DropReason.DUPLICATE, "Duplicate")
indras.assert.eq(indras.events.DropReason.EXPIRED, "Expired")
indras.assert.eq(indras.events.DropReason.SENDER_OFFLINE, "SenderOffline")

indras.log.debug("Event Constants tests passed", { trace_id = ctx.trace_id })

-- ============================================================
-- Test 12: Full Integration Scenario
-- ============================================================
indras.log.debug("Testing Full Integration Scenario", { trace_id = ctx.trace_id })

local integration_ctx = indras.correlation.new_root()
integration_ctx = integration_ctx:with_tag("test", "integration")

local int_mesh = indras.Mesh.from_edges({{'A', 'B'}, {'B', 'C'}, {'A', 'C'}})
local int_sim = indras.Simulation.new(int_mesh, indras.SimConfig.manual())

int_sim:force_online(indras.PeerId.new('A'))
int_sim:force_online(indras.PeerId.new('B'))
-- C is offline

int_sim:send_message(indras.PeerId.new('A'), indras.PeerId.new('C'), "Test message")
int_sim:run_ticks(5)

-- Message should be held
indras.assert.eq(int_sim.stats.messages_delivered, 0, "Message should not be delivered while C offline")

-- C comes online
int_sim:force_online(indras.PeerId.new('C'))
int_sim:run_ticks(10)

-- Message should be delivered
indras.assert.eq(int_sim.stats.messages_delivered, 1, "Message should be delivered after C online")
indras.assert.approx(int_sim.stats:delivery_rate(), 1.0, 0.01, "Delivery rate should be 100%")

indras.log.info("Full integration test passed", {
    trace_id = integration_ctx.trace_id,
    delivered = int_sim.stats.messages_delivered,
    latency = int_sim.stats:average_latency()
})

-- ============================================================
-- Summary
-- ============================================================
indras.log.info("All comprehensive tests passed!", {
    trace_id = ctx.trace_id,
    tests_run = 12
})

return true

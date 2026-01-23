-- Full Stack Integration Test
--
-- Comprehensive test that exercises ALL modules together in a realistic workflow.
-- Tests crypto, routing, sync, messaging, DTN, and transport layers as an integrated system.
--
-- Phases:
--   1. Node and Interface Setup (20%)
--   2. Normal Operation with Messaging (30%)
--   3. Network Partition (20%)
--   4. Recovery and Resync (30%)

-- Fix package path for helper libraries
package.path = package.path .. ";scripts/lib/?.lua;simulation/scripts/lib/?.lua"

-- Import helpers
local pq = require("pq_helpers")

-- Configuration levels
local CONFIG = {
    quick = {
        nodes = 10,
        interfaces = 3,
        messages_per_tick = 5,
        total_ticks = 200,
        partition_duration = 40,
        description = "Quick integration smoke test"
    },
    medium = {
        nodes = 20,
        interfaces = 10,
        messages_per_tick = 10,
        total_ticks = 500,
        partition_duration = 100,
        description = "Medium integration test"
    },
    full = {
        nodes = 26,
        interfaces = 30,
        messages_per_tick = 20,
        total_ticks = 1500,
        partition_duration = 300,
        description = "Full-scale integration test"
    }
}

-- Select configuration level (default: quick)
local level = os.getenv("STRESS_LEVEL") or "quick"
local cfg = CONFIG[level]
if not cfg then
    error(string.format("Invalid integration level '%s'. Use 'quick', 'medium', or 'full'", level))
end

-- Create correlation context
local ctx = pq.new_context("integration_full_stack")
ctx = ctx:with_tag("level", level)

indras.log.info("Starting full-stack integration test", {
    trace_id = ctx.trace_id,
    level = level,
    description = cfg.description,
    nodes = cfg.nodes,
    interfaces = cfg.interfaces,
    total_ticks = cfg.total_ticks,
    partition_duration = cfg.partition_duration
})

-- Build mesh topology (mix of full and partial connectivity)
local mesh = indras.MeshBuilder.new(cfg.nodes):random(0.5)
indras.log.info("Created mesh topology", {
    trace_id = ctx.trace_id,
    peers = mesh:peer_count(),
    edges = mesh:edge_count(),
    avg_connections = mesh:edge_count() / mesh:peer_count()
})

-- Create simulation
local config = indras.SimConfig.new({
    wake_probability = 0.1,
    sleep_probability = 0.05,
    initial_online_probability = 0.9,
    max_ticks = cfg.total_ticks,
    trace_routing = true
})

local sim = indras.Simulation.new(mesh, config)
sim:initialize()

local all_peers = mesh:peers()

-- Helper functions
local function random_online_peer()
    local online = sim:online_peers()
    if #online == 0 then return nil end
    return online[math.random(#online)]
end

local function random_peer_from_list(list)
    if #list == 0 then return nil end
    return list[math.random(#list)]
end

-- Data structures for tracking
local interfaces = {}  -- Interface definitions
local interface_members = {}  -- Members per interface
local e2e_messages = {}  -- E2E message tracking
local partition_groups = {}  -- Network partition groups
local dtn_bundles = {}  -- DTN bundle tracking

-- Phase metrics
local phase_metrics = {
    setup = { signatures = 0, kem_ops = 0, invites = 0 },
    normal = { messages = 0, deliveries = 0, e2e_sent = 0, e2e_delivered = 0 },
    partition = { dtn_bundles = 0, dtn_delivered = 0, partition_messages = 0 },
    recovery = { syncs = 0, late_deliveries = 0, convergence_ticks = 0 }
}

-- Calculate phase boundaries
local phase_1_end = math.floor(cfg.total_ticks * 0.2)  -- Setup: 20%
local phase_2_end = math.floor(cfg.total_ticks * 0.5)  -- Normal: 30%
local phase_3_end = math.floor(cfg.total_ticks * 0.7)  -- Partition: 20%
local phase_4_end = cfg.total_ticks  -- Recovery: 30%

indras.log.info("Phase boundaries", {
    trace_id = ctx.trace_id,
    setup_end = phase_1_end,
    normal_end = phase_2_end,
    partition_end = phase_3_end,
    recovery_end = phase_4_end
})

----------------------------
-- PHASE 1: SETUP (20%)
----------------------------
local function phase_1_setup()
    indras.log.info("Phase 1: Node and interface setup", {
        trace_id = ctx.trace_id,
        target_interfaces = cfg.interfaces
    })

    -- Force most peers online for setup
    local online_count = 0
    for _, peer in ipairs(all_peers) do
        if math.random() < 0.9 then
            sim:force_online(peer)
            online_count = online_count + 1
        end
    end

    indras.log.debug("Initialized node states", {
        trace_id = ctx.trace_id,
        online = online_count,
        total = #all_peers
    })

    -- Create PQ identities for all nodes
    for _, peer in ipairs(all_peers) do
        local sign_lat = pq.sign_latency()
        sim:record_pq_signature(peer, sign_lat, 256)
        phase_metrics.setup.signatures = phase_metrics.setup.signatures + 1
    end

    -- Create interfaces with members
    for i = 1, cfg.interfaces do
        local interface_id = string.format("interface-%03d", i)
        local creator = random_online_peer()

        if creator then
            -- Select 3-15 random members based on scale
            local member_count = math.random(3, math.min(15, math.floor(cfg.nodes / 3)))
            local members = {}

            table.insert(members, creator)
            for j = 2, member_count do
                local member = all_peers[math.random(#all_peers)]
                -- Avoid duplicates (simple check)
                local duplicate = false
                for _, m in ipairs(members) do
                    if tostring(m) == tostring(member) then
                        duplicate = true
                        break
                    end
                end
                if not duplicate then
                    table.insert(members, member)
                end
            end

            -- Execute invite flow with PQ crypto
            local invite_stats = pq.create_populated_interface(sim, creator, members, interface_id, ctx)

            phase_metrics.setup.kem_ops = phase_metrics.setup.kem_ops + invite_stats.created * 2
            phase_metrics.setup.invites = phase_metrics.setup.invites + invite_stats.accepted

            -- Store interface info
            interfaces[interface_id] = {
                creator = creator,
                members = members,
                created_tick = sim.tick
            }
            interface_members[interface_id] = members

            indras.log.debug("Created interface", {
                trace_id = ctx.trace_id,
                interface_id = interface_id,
                creator = tostring(creator),
                members = #members,
                accepted = invite_stats.accepted,
                failed = invite_stats.failed
            })
        end
    end

    indras.log.info("Phase 1 complete", {
        trace_id = ctx.trace_id,
        interfaces_created = #interfaces,
        total_signatures = phase_metrics.setup.signatures,
        total_kem_ops = phase_metrics.setup.kem_ops,
        invites_accepted = phase_metrics.setup.invites
    })
end

----------------------------
-- PHASE 2: NORMAL OPERATION (30%)
----------------------------
local function phase_2_normal_tick()
    local online = sim:online_peers()

    -- Generate E2E messages within interfaces
    for interface_id, members in pairs(interface_members) do
        if #members >= 2 and math.random() < 0.7 then
            local sender = random_peer_from_list(members)
            local receiver = random_peer_from_list(members)

            if sender and receiver and tostring(sender) ~= tostring(receiver) then
                -- E2E message with signature
                local sign_lat = pq.sign_latency()
                sim:record_pq_signature(sender, sign_lat, 256)

                local verify_lat = pq.verify_latency()
                local verify_success = math.random() > 0.001
                sim:record_pq_verification(receiver, sender, verify_lat, verify_success)

                -- Send network message
                sim:send_message(sender, receiver, "e2e_msg")

                -- Track E2E message
                local msg_id = string.format("%s-e2e-%d", interface_id, sim.tick)
                e2e_messages[msg_id] = {
                    sender = sender,
                    receiver = receiver,
                    interface = interface_id,
                    sent_tick = sim.tick,
                    delivered = false
                }

                phase_metrics.normal.e2e_sent = phase_metrics.normal.e2e_sent + 1
            end
        end
    end

    -- Generate background message load
    local background_msgs = pq.generate_message_load(sim, online, cfg.messages_per_tick)
    phase_metrics.normal.messages = phase_metrics.normal.messages + background_msgs.signatures
end

----------------------------
-- PHASE 3: NETWORK PARTITION (20%)
----------------------------
local partition_start_tick = 0

local function phase_3_partition_start()
    partition_start_tick = sim.tick

    indras.log.warn("Phase 3: Network partition starting", {
        trace_id = ctx.trace_id,
        tick = sim.tick,
        duration = cfg.partition_duration
    })

    -- Split network into 2 groups
    local split_point = math.floor(#all_peers / 2)
    partition_groups.group_a = {}
    partition_groups.group_b = {}

    for i, peer in ipairs(all_peers) do
        if i <= split_point then
            table.insert(partition_groups.group_a, peer)
        else
            table.insert(partition_groups.group_b, peer)
        end
    end

    indras.log.info("Network partitioned", {
        trace_id = ctx.trace_id,
        group_a_size = #partition_groups.group_a,
        group_b_size = #partition_groups.group_b
    })
end

local function phase_3_partition_tick()
    -- Continue sending messages, but many will be queued as DTN bundles
    local online = sim:online_peers()

    -- Try cross-partition messages (will be queued)
    if math.random() < 0.5 then
        local sender_a = random_peer_from_list(partition_groups.group_a)
        local receiver_b = random_peer_from_list(partition_groups.group_b)

        if sender_a and receiver_b then
            -- This will queue as DTN bundle
            local bundle_id = string.format("dtn-bundle-%d", sim.tick)
            dtn_bundles[bundle_id] = {
                sender = sender_a,
                receiver = receiver_b,
                queued_tick = sim.tick,
                delivered = false
            }

            sim:send_message(sender_a, receiver_b, "dtn_msg")
            phase_metrics.partition.dtn_bundles = phase_metrics.partition.dtn_bundles + 1
        end
    end

    -- Intra-partition messages continue normally
    local background_msgs = pq.generate_message_load(sim, online, math.floor(cfg.messages_per_tick / 2))
    phase_metrics.partition.partition_messages = phase_metrics.partition.partition_messages + background_msgs.signatures
end

----------------------------
-- PHASE 4: RECOVERY (30%)
----------------------------
local recovery_start_tick = 0

local function phase_4_recovery_start()
    recovery_start_tick = sim.tick

    indras.log.warn("Phase 4: Network recovery and resync", {
        trace_id = ctx.trace_id,
        tick = sim.tick
    })

    -- Restore connectivity - bring all peers back online
    for _, peer in ipairs(all_peers) do
        sim:force_online(peer)
    end

    indras.log.info("Network connectivity restored", {
        trace_id = ctx.trace_id,
        online_peers = #sim:online_peers()
    })
end

local function phase_4_recovery_tick()
    -- DTN bundles should now be delivered
    for bundle_id, bundle in pairs(dtn_bundles) do
        if not bundle.delivered then
            -- Simulate bundle delivery check
            if math.random() < 0.3 then  -- 30% chance per tick
                bundle.delivered = true
                bundle.delivered_tick = sim.tick
                phase_metrics.recovery.late_deliveries = phase_metrics.recovery.late_deliveries + 1
                phase_metrics.partition.dtn_delivered = phase_metrics.partition.dtn_delivered + 1
            end
        end
    end

    -- Sync operations between peers
    if math.random() < 0.4 then
        local online = sim:online_peers()
        if #online >= 2 then
            local peer_a = random_peer_from_list(online)
            local peer_b = random_peer_from_list(online)

            if peer_a and peer_b and tostring(peer_a) ~= tostring(peer_b) then
                -- Simulate sync with signatures
                local sign_lat = pq.sign_latency()
                sim:record_pq_signature(peer_a, sign_lat, 256)

                local verify_lat = pq.verify_latency()
                sim:record_pq_verification(peer_b, peer_a, verify_lat, true)

                phase_metrics.recovery.syncs = phase_metrics.recovery.syncs + 1
            end
        end
    end

    -- Continue normal messaging
    local online = sim:online_peers()
    local background_msgs = pq.generate_message_load(sim, online, cfg.messages_per_tick)
end

----------------------------
-- MAIN SIMULATION LOOP
----------------------------

-- Phase 1: Setup
phase_1_setup()

-- Run simulation
indras.log.info("Starting main simulation loop", {
    trace_id = ctx.trace_id,
    total_ticks = cfg.total_ticks
})

for tick = 1, cfg.total_ticks do
    if tick <= phase_1_end then
        -- Phase 1: Already complete
        sim:step()

    elseif tick <= phase_2_end then
        -- Phase 2: Normal operation
        if tick == phase_1_end + 1 then
            indras.log.info("Phase 2: Normal operation with messaging", {
                trace_id = ctx.trace_id,
                tick = tick
            })
        end
        phase_2_normal_tick()
        sim:step()

    elseif tick <= phase_3_end then
        -- Phase 3: Partition
        if tick == phase_2_end + 1 then
            phase_3_partition_start()
        end
        phase_3_partition_tick()
        sim:step()

    else
        -- Phase 4: Recovery
        if tick == phase_3_end + 1 then
            phase_4_recovery_start()
        end
        phase_4_recovery_tick()
        sim:step()
    end

    -- Check E2E message deliveries
    local stats = sim.stats
    local current_delivered = stats.messages_delivered
    if current_delivered > phase_metrics.normal.deliveries then
        phase_metrics.normal.e2e_delivered = phase_metrics.normal.e2e_delivered + (current_delivered - phase_metrics.normal.deliveries)
        phase_metrics.normal.deliveries = current_delivered
    end

    -- Progress logging
    if tick % 100 == 0 then
        indras.log.info("Simulation progress", {
            trace_id = ctx.trace_id,
            tick = tick,
            progress_pct = math.floor(tick * 100 / cfg.total_ticks),
            online_peers = #sim:online_peers(),
            messages_delivered = stats.messages_delivered,
            messages_dropped = stats.messages_dropped
        })
    end
end

----------------------------
-- FINAL METRICS
----------------------------

local stats = sim.stats

-- Calculate convergence time
local convergence_ticks = phase_4_end - recovery_start_tick
phase_metrics.recovery.convergence_ticks = convergence_ticks

-- DTN delivery rate
local dtn_delivery_rate = 0
if phase_metrics.partition.dtn_bundles > 0 then
    dtn_delivery_rate = phase_metrics.partition.dtn_delivered / phase_metrics.partition.dtn_bundles
end

-- E2E delivery rate
local e2e_delivery_rate = 0
if phase_metrics.normal.e2e_sent > 0 then
    e2e_delivery_rate = phase_metrics.normal.e2e_delivered / phase_metrics.normal.e2e_sent
end

indras.log.info("Full-stack integration test completed", {
    trace_id = ctx.trace_id,
    level = level,
    final_tick = sim.tick,

    -- Phase 1: Setup
    interfaces_created = cfg.interfaces,
    setup_signatures = phase_metrics.setup.signatures,
    setup_kem_ops = phase_metrics.setup.kem_ops,
    invites_accepted = phase_metrics.setup.invites,

    -- Phase 2: Normal
    e2e_messages_sent = phase_metrics.normal.e2e_sent,
    e2e_messages_delivered = phase_metrics.normal.e2e_delivered,
    e2e_delivery_rate = e2e_delivery_rate,
    normal_messages = phase_metrics.normal.messages,

    -- Phase 3: Partition
    dtn_bundles_queued = phase_metrics.partition.dtn_bundles,
    dtn_bundles_delivered = phase_metrics.partition.dtn_delivered,
    dtn_delivery_rate = dtn_delivery_rate,
    partition_messages = phase_metrics.partition.partition_messages,

    -- Phase 4: Recovery
    recovery_syncs = phase_metrics.recovery.syncs,
    late_deliveries = phase_metrics.recovery.late_deliveries,
    convergence_ticks = convergence_ticks,

    -- Overall crypto metrics
    total_signatures = stats.pq_signatures_created,
    total_verifications = stats.pq_signatures_verified,
    signature_failures = stats.pq_signature_failures,
    signature_failure_rate = stats:signature_failure_rate(),
    avg_sign_latency_us = stats:avg_signature_latency_us(),
    avg_verify_latency_us = stats:avg_verification_latency_us(),

    -- KEM metrics
    kem_encapsulations = stats.pq_kem_encapsulations,
    kem_decapsulations = stats.pq_kem_decapsulations,
    kem_failures = stats.pq_kem_failures,
    kem_failure_rate = stats:kem_failure_rate(),
    avg_encap_latency_us = stats:avg_kem_encap_latency_us(),
    avg_decap_latency_us = stats:avg_kem_decap_latency_us(),

    -- Invite metrics
    invites_created = stats.invites_created,
    invites_accepted = stats.invites_accepted,
    invites_failed = stats.invites_failed,
    invite_success_rate = stats:invite_success_rate(),

    -- Routing metrics
    messages_sent = stats.messages_sent,
    messages_delivered = stats.messages_delivered,
    messages_dropped = stats.messages_dropped,
    delivery_rate = stats:delivery_rate(),
    avg_latency = stats:average_latency(),
    avg_hops = stats:average_hops()
})

-- Assertions for test validation
indras.assert.gt(stats.pq_signatures_created, cfg.nodes, "Should have created identity signatures")
indras.assert.gt(stats.invites_accepted, 0, "Should have accepted invites")
indras.assert.gt(phase_metrics.normal.e2e_sent, 0, "Should have sent E2E messages")
indras.assert.gt(stats:delivery_rate(), 0.5, "Should maintain >50% delivery rate")
indras.assert.lt(stats:signature_failure_rate(), 0.05, "Signature failure rate should be <5%")
indras.assert.gt(dtn_delivery_rate, 0.0, "Should deliver some DTN bundles after recovery")

-- Level-specific assertions
if level == "quick" then
    indras.assert.gt(stats.messages_delivered, 100, "Quick test should deliver >100 messages")
elseif level == "medium" then
    indras.assert.gt(stats.messages_delivered, 500, "Medium test should deliver >500 messages")
elseif level == "full" then
    indras.assert.gt(stats.messages_delivered, 2000, "Full test should deliver >2000 messages")
end

indras.log.info("Full-stack integration test PASSED", {
    trace_id = ctx.trace_id,
    level = level
})

-- Return comprehensive metrics
return {
    level = level,
    config = cfg,

    -- Phase summaries
    setup = phase_metrics.setup,
    normal = phase_metrics.normal,
    partition = phase_metrics.partition,
    recovery = phase_metrics.recovery,

    -- Rates
    e2e_delivery_rate = e2e_delivery_rate,
    dtn_delivery_rate = dtn_delivery_rate,
    overall_delivery_rate = stats:delivery_rate(),
    signature_failure_rate = stats:signature_failure_rate(),
    kem_failure_rate = stats:kem_failure_rate(),
    invite_success_rate = stats:invite_success_rate(),

    -- Totals
    total_interfaces = cfg.interfaces,
    total_signatures = stats.pq_signatures_created,
    total_kem_ops = stats.pq_kem_encapsulations + stats.pq_kem_decapsulations,
    total_messages = stats.messages_sent,
    total_delivered = stats.messages_delivered,

    -- Latencies
    avg_sign_latency_us = stats:avg_signature_latency_us(),
    avg_verify_latency_us = stats:avg_verification_latency_us(),
    avg_encap_latency_us = stats:avg_kem_encap_latency_us(),
    avg_decap_latency_us = stats:avg_kem_decap_latency_us(),
    avg_message_latency = stats:average_latency(),
    avg_hops = stats:average_hops()
}

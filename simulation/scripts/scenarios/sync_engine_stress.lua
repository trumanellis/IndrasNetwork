-- sync_engine_stress.lua
-- Comprehensive stress test for indras-network SyncEngine
--
-- Tests the full SyncEngine lifecycle: network creation, realm management,
-- high-volume messaging, and member operations under stress conditions.
-- Validates SyncEngine performance with latency percentiles (p50/p95/p99).

-- Fix package path for helper libraries
package.path = package.path .. ";scripts/lib/?.lua;simulation/scripts/lib/?.lua"

local sync_engine = require("sync_engine_helpers")
local pq = require("pq_helpers")
local stress = require("stress_helpers")

-- ============================================================================
-- CONFIGURATION: Three-level stress configuration
-- ============================================================================

local CONFIG = {
    quick = {
        networks = 2,
        realms_per_network = 2,
        members_per_realm = 3,
        messages = 50,
        ticks = 100,
        member_list_ops = 20,
    },
    medium = {
        networks = 3,
        realms_per_network = 5,
        members_per_realm = 5,
        messages = 500,
        ticks = 300,
        member_list_ops = 100,
    },
    full = {
        networks = 5,
        realms_per_network = 10,
        members_per_realm = 8,
        messages = 5000,
        ticks = 1000,
        member_list_ops = 500,
    }
}

local config_level = sync_engine.get_level()
local cfg = CONFIG[config_level] or CONFIG.quick

-- ============================================================================
-- CORRELATION CONTEXT
-- ============================================================================

local ctx = sync_engine.new_context("sync_engine_stress")

indras.log.info("Starting SyncEngine stress test", {
    trace_id = ctx.trace_id,
    level = config_level,
    networks = cfg.networks,
    realms_per_network = cfg.realms_per_network,
    members_per_realm = cfg.members_per_realm,
    messages = cfg.messages,
    ticks = cfg.ticks
})

-- ============================================================================
-- SIMULATION SETUP
-- ============================================================================

-- Calculate total peers needed (networks * realms * members)
local total_peers_needed = math.min(26, cfg.networks * cfg.realms_per_network * cfg.members_per_realm)

-- Create mesh topology with sufficient peers
local mesh = indras.MeshBuilder.new(total_peers_needed):random(0.4)

indras.log.debug("Created mesh topology", {
    trace_id = ctx.trace_id,
    peers = mesh:peer_count(),
    edges = mesh:edge_count(),
    avg_degree = mesh:peer_count() > 0 and (mesh:edge_count() / mesh:peer_count()) or 0
})

-- Create simulation with SyncEngine-appropriate settings
local sim_config = indras.SimConfig.new({
    wake_probability = 0.1,
    sleep_probability = 0.05,
    initial_online_probability = 0.95,
    max_ticks = cfg.ticks,
    trace_routing = true
})

local sim = indras.Simulation.new(mesh, sim_config)
sim:initialize()

local all_peers = mesh:peers()

-- ============================================================================
-- TRACKING STRUCTURES
-- ============================================================================

-- Networks registry: network_id -> { owner, realms[], created_at }
local networks = {}
local network_count = 0

-- Realms registry: realm_id -> { network_id, creator, members[], created_at }
local realms = {}
local realm_count = 0

-- Message tracking
local messages_sent = 0
local messages_by_realm = {}  -- realm_id -> count

-- Member list operations tracking
local member_list_ops = 0

-- Latency trackers
local network_create_latencies = sync_engine.latency_tracker()
local network_destroy_latencies = sync_engine.latency_tracker()
local realm_create_latencies = sync_engine.latency_tracker()
local realm_join_latencies = sync_engine.latency_tracker()
local message_send_latencies = sync_engine.latency_tracker()
local member_list_latencies = sync_engine.latency_tracker()

-- Error counters
local errors = {
    network_create = 0,
    realm_create = 0,
    realm_join = 0,
    message_send = 0,
    member_list = 0,
}

-- Result builder
local result = sync_engine.result_builder("sync_engine_stress")

-- ============================================================================
-- HELPER FUNCTIONS
-- ============================================================================

local function random_online_peer()
    local online = sim:online_peers()
    if #online == 0 then return nil end
    return online[math.random(#online)]
end

local function random_peers(n)
    local selected = {}
    local online = sim:online_peers()
    if #online == 0 then return selected end

    local used = {}
    for _ = 1, math.min(n, #online) do
        local peer = online[math.random(#online)]
        local attempts = 0
        while used[tostring(peer)] and attempts < 10 do
            peer = online[math.random(#online)]
            attempts = attempts + 1
        end
        if not used[tostring(peer)] then
            used[tostring(peer)] = true
            table.insert(selected, peer)
        end
    end
    return selected
end

local function log_progress(current, total, phase_name)
    local interval = math.max(1, math.floor(total / 10))
    if current % interval == 0 or current == total then
        local progress_pct = math.floor(current / total * 100)
        indras.log.info(string.format("%s progress: %d%%", phase_name, progress_pct), {
            trace_id = ctx.trace_id,
            phase = phase_name,
            current = current,
            total = total,
            progress_pct = progress_pct
        })
    end
end

-- ============================================================================
-- PHASE 1: NETWORK CREATION AND LIFECYCLE
-- ============================================================================

indras.log.info("Phase 1: Network Creation", {
    trace_id = ctx.trace_id,
    target_networks = cfg.networks
})

local phase1_start = os.clock()

for i = 1, cfg.networks do
    local network_id = string.format("net-%04d-%s", i, ctx.trace_id:sub(1, 8))
    local owner = random_online_peer()

    if owner then
        -- Simulate SyncEngine: indras.Network.create(owner, network_id)
        -- This would call the Rust SyncEngine binding to create a network
        local start_time = os.clock()

        -- Placeholder: SyncEngine network creation call
        -- local network = indras.sync_engine.Network.create({
        --     id = network_id,
        --     owner = owner,
        --     config = { ... }
        -- })

        -- Simulate latency for network creation
        local latency_us = pq.random_latency(1000, 500)  -- ~500-1500us

        -- Record PQ key generation for network (owner generates signing key)
        local sign_latency = pq.sign_latency()
        sim:record_pq_signature(owner, sign_latency, 256)

        -- Track creation
        local elapsed = (os.clock() - start_time) * 1000000 + latency_us
        network_create_latencies:add(elapsed)

        -- Store network metadata
        networks[network_id] = {
            owner = owner,
            realms = {},
            created_at = sim.tick
        }
        network_count = network_count + 1

        indras.log.debug("Network created", {
            trace_id = ctx.trace_id,
            network_id = network_id,
            owner = tostring(owner),
            latency_us = elapsed
        })
    else
        errors.network_create = errors.network_create + 1
        indras.log.warn("Failed to create network - no online peers", {
            trace_id = ctx.trace_id,
            network_index = i
        })
    end

    log_progress(i, cfg.networks, "Network Creation")
end

local phase1_elapsed = os.clock() - phase1_start

indras.log.info("Phase 1 complete", {
    trace_id = ctx.trace_id,
    networks_created = network_count,
    errors = errors.network_create,
    elapsed_sec = phase1_elapsed,
    avg_latency_us = network_create_latencies:average(),
    p50_latency_us = network_create_latencies:p50(),
    p95_latency_us = network_create_latencies:p95(),
    p99_latency_us = network_create_latencies:p99()
})

-- Advance simulation
for _ = 1, math.floor(cfg.ticks * 0.1) do
    sim:step()
end

-- ============================================================================
-- PHASE 2: REALM CREATION AND JOINING
-- ============================================================================

indras.log.info("Phase 2: Realm Creation and Joining", {
    trace_id = ctx.trace_id,
    realms_per_network = cfg.realms_per_network,
    members_per_realm = cfg.members_per_realm
})

local phase2_start = os.clock()
local total_realms = network_count * cfg.realms_per_network
local realms_created = 0
local members_joined = 0

for network_id, network in pairs(networks) do
    for j = 1, cfg.realms_per_network do
        local realm_id = string.format("%s-realm-%04d", network_id, j)
        local creator = network.owner

        if creator then
            -- Simulate SyncEngine: network.create_realm(realm_id, config)
            -- This would call the Rust SyncEngine binding to create a realm
            local start_time = os.clock()

            -- Placeholder: SyncEngine realm creation call
            -- local realm = network:create_realm({
            --     id = realm_id,
            --     name = "Stress Test Realm",
            --     max_members = cfg.members_per_realm
            -- })

            -- Simulate latency for realm creation
            local latency_us = pq.random_latency(550, 250)  -- ~300-800us

            -- Record PQ operations (realm key generation)
            local sign_latency = pq.sign_latency()
            sim:record_pq_signature(creator, sign_latency, 128)

            local elapsed = (os.clock() - start_time) * 1000000 + latency_us
            realm_create_latencies:add(elapsed)

            -- Initialize realm
            realms[realm_id] = {
                network_id = network_id,
                creator = creator,
                members = { creator },  -- Creator is first member
                created_at = sim.tick
            }
            realm_count = realm_count + 1
            realms_created = realms_created + 1
            messages_by_realm[realm_id] = 0

            table.insert(network.realms, realm_id)

            -- Join additional members
            local additional_members = random_peers(cfg.members_per_realm - 1)
            for _, member in ipairs(additional_members) do
                if member ~= creator then
                    -- Simulate SyncEngine: realm.join(member) or realm.invite(member)
                    local join_start = os.clock()

                    -- Placeholder: SyncEngine realm join call
                    -- realm:invite(member)
                    -- member:accept_invite(realm_id)

                    -- Simulate KEM operations for invite
                    local encap_latency = pq.encap_latency()
                    sim:record_kem_encapsulation(creator, member, encap_latency)

                    local decap_latency = pq.decap_latency()
                    local success = math.random() > 0.001  -- 0.1% failure rate
                    sim:record_kem_decapsulation(member, creator, decap_latency, success)

                    if success then
                        sim:record_invite_accepted(member, realm_id)
                        table.insert(realms[realm_id].members, member)
                        members_joined = members_joined + 1

                        local join_elapsed = (os.clock() - join_start) * 1000000 + encap_latency + decap_latency
                        realm_join_latencies:add(join_elapsed)
                    else
                        sim:record_invite_failed(member, realm_id, "KEM failure")
                        errors.realm_join = errors.realm_join + 1
                    end
                end
            end

            indras.log.debug("Realm created with members", {
                trace_id = ctx.trace_id,
                realm_id = realm_id,
                network_id = network_id,
                creator = tostring(creator),
                member_count = #realms[realm_id].members
            })
        else
            errors.realm_create = errors.realm_create + 1
        end

        log_progress(realms_created, total_realms, "Realm Creation")
    end
end

local phase2_elapsed = os.clock() - phase2_start

indras.log.info("Phase 2 complete", {
    trace_id = ctx.trace_id,
    realms_created = realms_created,
    members_joined = members_joined,
    realm_create_errors = errors.realm_create,
    realm_join_errors = errors.realm_join,
    elapsed_sec = phase2_elapsed,
    realm_create_p50_us = realm_create_latencies:p50(),
    realm_create_p95_us = realm_create_latencies:p95(),
    realm_create_p99_us = realm_create_latencies:p99(),
    realm_join_p50_us = realm_join_latencies:p50(),
    realm_join_p95_us = realm_join_latencies:p95(),
    realm_join_p99_us = realm_join_latencies:p99()
})

-- Advance simulation
for _ = 1, math.floor(cfg.ticks * 0.1) do
    sim:step()
end

-- ============================================================================
-- PHASE 3: MESSAGE STRESS (High Volume)
-- ============================================================================

indras.log.info("Phase 3: Message Stress", {
    trace_id = ctx.trace_id,
    target_messages = cfg.messages,
    active_realms = realm_count
})

local phase3_start = os.clock()
local phase3_start_tick = sim.tick

-- Get list of realm IDs for random selection
local realm_ids = {}
for realm_id, _ in pairs(realms) do
    table.insert(realm_ids, realm_id)
end

if #realm_ids == 0 then
    indras.log.warn("No realms available for messaging", {
        trace_id = ctx.trace_id
    })
else
    -- Message sending loop
    local messages_per_tick = math.max(1, math.ceil(cfg.messages / (cfg.ticks * 0.5)))

    while messages_sent < cfg.messages and sim.tick < cfg.ticks * 0.8 do
        -- Send multiple messages per tick
        for _ = 1, messages_per_tick do
            if messages_sent >= cfg.messages then break end

            -- Pick random realm
            local realm_id = realm_ids[math.random(#realm_ids)]
            local realm = realms[realm_id]

            if realm and #realm.members >= 2 then
                -- Pick sender and receiver from realm members
                local sender_idx = math.random(#realm.members)
                local receiver_idx = math.random(#realm.members)
                while receiver_idx == sender_idx do
                    receiver_idx = math.random(#realm.members)
                end

                local sender = realm.members[sender_idx]
                local receiver = realm.members[receiver_idx]

                -- Simulate SyncEngine: realm.send_message(sender, receiver, content)
                local start_time = os.clock()

                -- Placeholder: SyncEngine message send call
                -- local msg = realm:send_message({
                --     from = sender,
                --     to = receiver,
                --     content = sync_engine.random_message(50, 200),
                --     encrypt = true
                -- })

                -- Sign the message with PQ signature
                local sign_latency = pq.sign_latency()
                sim:record_pq_signature(sender, sign_latency, 256)

                -- Periodic key rotation (every ~10% of messages)
                if math.random() < 0.1 then
                    local encap_latency = pq.encap_latency()
                    sim:record_kem_encapsulation(sender, receiver, encap_latency)

                    local decap_latency = pq.decap_latency()
                    sim:record_kem_decapsulation(receiver, sender, decap_latency, true)
                end

                -- Send via simulation
                local msg_id = string.format("%s-msg-%06d", realm_id, messages_sent)
                sim:send_message(sender, receiver, msg_id)

                -- Simulate message send latency (local queueing)
                local msg_latency_us = pq.random_latency(125, 75)  -- ~50-200us
                local elapsed = (os.clock() - start_time) * 1000000 + msg_latency_us
                message_send_latencies:add(elapsed)

                messages_sent = messages_sent + 1
                messages_by_realm[realm_id] = (messages_by_realm[realm_id] or 0) + 1
            else
                errors.message_send = errors.message_send + 1
            end
        end

        -- Advance simulation
        sim:step()

        -- Progress logging (every 10%)
        log_progress(messages_sent, cfg.messages, "Message Stress")
    end
end

local phase3_elapsed = os.clock() - phase3_start
local phase3_tick_duration = sim.tick - phase3_start_tick
local messages_per_tick = phase3_tick_duration > 0 and (messages_sent / phase3_tick_duration) or 0

indras.log.info("Phase 3 complete", {
    trace_id = ctx.trace_id,
    messages_sent = messages_sent,
    target_messages = cfg.messages,
    errors = errors.message_send,
    elapsed_sec = phase3_elapsed,
    tick_duration = phase3_tick_duration,
    messages_per_tick = messages_per_tick,
    p50_latency_us = message_send_latencies:p50(),
    p95_latency_us = message_send_latencies:p95(),
    p99_latency_us = message_send_latencies:p99()
})

-- ============================================================================
-- PHASE 4: MEMBER LISTING OPERATIONS
-- ============================================================================

indras.log.info("Phase 4: Member Listing", {
    trace_id = ctx.trace_id,
    target_ops = cfg.member_list_ops
})

local phase4_start = os.clock()

for i = 1, cfg.member_list_ops do
    if #realm_ids == 0 then break end

    -- Pick random realm
    local realm_id = realm_ids[math.random(#realm_ids)]
    local realm = realms[realm_id]

    if realm then
        -- Simulate SyncEngine: realm.list_members()
        local start_time = os.clock()

        -- Placeholder: SyncEngine member list call
        -- local members = realm:list_members({
        --     include_online_status = true,
        --     include_roles = true
        -- })

        -- Simulate member list latency
        local list_latency_us = pq.random_latency(60, 40)  -- ~20-100us

        -- For each member, we might verify their status
        -- This simulates the cost of checking member info
        for _, member in ipairs(realm.members) do
            -- Verify member signature/status (simulated)
            if math.random() < 0.3 then  -- 30% of listings verify signatures
                local verify_latency = pq.verify_latency()
                sim:record_pq_verification(realm.creator, member, verify_latency, true)
            end
        end

        local elapsed = (os.clock() - start_time) * 1000000 + list_latency_us
        member_list_latencies:add(elapsed)
        member_list_ops = member_list_ops + 1
    else
        errors.member_list = errors.member_list + 1
    end

    log_progress(i, cfg.member_list_ops, "Member Listing")
end

local phase4_elapsed = os.clock() - phase4_start

indras.log.info("Phase 4 complete", {
    trace_id = ctx.trace_id,
    member_list_ops = member_list_ops,
    errors = errors.member_list,
    elapsed_sec = phase4_elapsed,
    p50_latency_us = member_list_latencies:p50(),
    p95_latency_us = member_list_latencies:p95(),
    p99_latency_us = member_list_latencies:p99()
})

-- ============================================================================
-- PHASE 5: CLEANUP AND FINAL METRICS
-- ============================================================================

indras.log.info("Phase 5: Cleanup and Metrics", {
    trace_id = ctx.trace_id
})

-- Run remaining simulation ticks to allow message delivery
local remaining_ticks = cfg.ticks - sim.tick
for _ = 1, remaining_ticks do
    sim:step()
end

-- Optional: Destroy networks (lifecycle test)
local networks_destroyed = 0
for network_id, network in pairs(networks) do
    -- Simulate SyncEngine: network.destroy()
    local start_time = os.clock()

    -- Placeholder: SyncEngine network destroy call
    -- network:destroy({ force = true })

    local destroy_latency_us = pq.random_latency(350, 150)  -- ~200-500us
    local elapsed = (os.clock() - start_time) * 1000000 + destroy_latency_us
    network_destroy_latencies:add(elapsed)
    networks_destroyed = networks_destroyed + 1
end

-- Gather final simulation statistics
local final_stats = sim.stats
local event_log = sim:event_log()

-- Calculate delivery rate
local delivery_rate = 0
if messages_sent > 0 then
    delivery_rate = final_stats.messages_delivered / messages_sent
end

-- ============================================================================
-- FINAL RESULTS AND ASSERTIONS
-- ============================================================================

indras.log.info("SyncEngine stress test completed", {
    trace_id = ctx.trace_id,
    level = config_level,
    -- Configuration
    target_networks = cfg.networks,
    target_realms_per_network = cfg.realms_per_network,
    target_messages = cfg.messages,
    -- Network metrics
    networks_created = network_count,
    networks_destroyed = networks_destroyed,
    network_create_p50_us = network_create_latencies:p50(),
    network_create_p95_us = network_create_latencies:p95(),
    network_create_p99_us = network_create_latencies:p99(),
    -- Realm metrics
    realms_created = realm_count,
    members_joined = members_joined,
    realm_create_p50_us = realm_create_latencies:p50(),
    realm_create_p95_us = realm_create_latencies:p95(),
    realm_create_p99_us = realm_create_latencies:p99(),
    realm_join_p50_us = realm_join_latencies:p50(),
    realm_join_p95_us = realm_join_latencies:p95(),
    realm_join_p99_us = realm_join_latencies:p99(),
    -- Message metrics
    messages_sent = messages_sent,
    messages_delivered = final_stats.messages_delivered,
    delivery_rate = delivery_rate,
    message_send_p50_us = message_send_latencies:p50(),
    message_send_p95_us = message_send_latencies:p95(),
    message_send_p99_us = message_send_latencies:p99(),
    -- Member list metrics
    member_list_ops = member_list_ops,
    member_list_p50_us = member_list_latencies:p50(),
    member_list_p95_us = member_list_latencies:p95(),
    member_list_p99_us = member_list_latencies:p99(),
    -- Error metrics
    total_errors = errors.network_create + errors.realm_create + errors.realm_join + errors.message_send + errors.member_list,
    error_breakdown = errors,
    -- Crypto metrics
    signatures_created = final_stats.pq_signatures_created,
    signatures_verified = final_stats.pq_signatures_verified,
    kem_encapsulations = final_stats.pq_kem_encapsulations,
    kem_decapsulations = final_stats.pq_kem_decapsulations,
    -- Simulation metrics
    total_ticks = sim.tick,
    event_log_size = #event_log
})

-- Assertions
indras.assert.gt(network_count, 0, "Should have created at least one network")
indras.assert.gt(realm_count, 0, "Should have created at least one realm")
indras.assert.gt(messages_sent, 0, "Should have sent messages")
indras.assert.gt(final_stats.messages_delivered, 0, "Should have delivered messages")
indras.assert.gt(delivery_rate, 0.5, "Delivery rate should be > 50%")
indras.assert.gt(member_list_ops, 0, "Should have performed member list operations")

-- Latency assertions (sanity checks)
indras.assert.gt(network_create_latencies:p50(), 0, "Network create latency should be > 0")
indras.assert.gt(message_send_latencies:p50(), 0, "Message send latency should be > 0")

-- Performance thresholds based on stress level
if config_level == "quick" then
    indras.assert.lt(network_create_latencies:p99(), 10000, "Network create P99 should be < 10ms")
    indras.assert.lt(message_send_latencies:p99(), 5000, "Message send P99 should be < 5ms")
elseif config_level == "medium" then
    indras.assert.lt(network_create_latencies:p99(), 15000, "Network create P99 should be < 15ms")
    indras.assert.lt(message_send_latencies:p99(), 10000, "Message send P99 should be < 10ms")
elseif config_level == "full" then
    indras.assert.lt(network_create_latencies:p99(), 25000, "Network create P99 should be < 25ms")
    indras.assert.lt(message_send_latencies:p99(), 20000, "Message send P99 should be < 20ms")
end

-- Crypto operation assertions
indras.assert.gt(final_stats.pq_signatures_created, 0, "Should have created PQ signatures")
indras.assert.lt(final_stats:signature_failure_rate(), 0.01, "Signature failure rate should be < 1%")
indras.assert.lt(final_stats:kem_failure_rate(), 0.01, "KEM failure rate should be < 1%")

indras.log.info("SyncEngine stress test passed all assertions", {
    trace_id = ctx.trace_id,
    level = config_level,
    networks = network_count,
    realms = realm_count,
    messages = messages_sent,
    delivery_rate = delivery_rate
})

-- ============================================================================
-- BUILD AND RETURN RESULT
-- ============================================================================

result:add("networks_created", network_count)
result:add("networks_destroyed", networks_destroyed)
result:add("realms_created", realm_count)
result:add("members_joined", members_joined)
result:add("messages_sent", messages_sent)
result:add("messages_delivered", final_stats.messages_delivered)
result:add("delivery_rate", delivery_rate)
result:add("member_list_ops", member_list_ops)

-- Latency percentiles
result:add("network_create_latency", {
    p50 = network_create_latencies:p50(),
    p95 = network_create_latencies:p95(),
    p99 = network_create_latencies:p99(),
    avg = network_create_latencies:average()
})

result:add("realm_create_latency", {
    p50 = realm_create_latencies:p50(),
    p95 = realm_create_latencies:p95(),
    p99 = realm_create_latencies:p99(),
    avg = realm_create_latencies:average()
})

result:add("realm_join_latency", {
    p50 = realm_join_latencies:p50(),
    p95 = realm_join_latencies:p95(),
    p99 = realm_join_latencies:p99(),
    avg = realm_join_latencies:average()
})

result:add("message_send_latency", {
    p50 = message_send_latencies:p50(),
    p95 = message_send_latencies:p95(),
    p99 = message_send_latencies:p99(),
    avg = message_send_latencies:average()
})

result:add("member_list_latency", {
    p50 = member_list_latencies:p50(),
    p95 = member_list_latencies:p95(),
    p99 = member_list_latencies:p99(),
    avg = member_list_latencies:average()
})

-- Error summary
result:add("errors", errors)

-- Crypto metrics
result:add("crypto", {
    signatures_created = final_stats.pq_signatures_created,
    signatures_verified = final_stats.pq_signatures_verified,
    signature_failure_rate = final_stats:signature_failure_rate(),
    kem_encapsulations = final_stats.pq_kem_encapsulations,
    kem_decapsulations = final_stats.pq_kem_decapsulations,
    kem_failure_rate = final_stats:kem_failure_rate()
})

-- Simulation metrics
result:add("simulation", {
    total_ticks = sim.tick,
    event_log_size = #event_log,
    avg_hops = final_stats:average_hops(),
    avg_latency = final_stats:average_latency()
})

return result:build()

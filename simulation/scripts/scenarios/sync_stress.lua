-- Sync Stress Test
--
-- Stress test for indras-sync module (Automerge CRDT document synchronization).
-- Simulates concurrent document edits, offline/online scenarios, and convergence testing.

local ctx = indras.correlation.new_root()
ctx = ctx:with_tag("scenario", "sync_stress")

-- Configuration levels
local CONFIG = {
    quick = {
        peer_count = 5,
        sync_ops = 100,
        max_ticks = 100,
        edit_rate = 2,           -- edits per tick
        offline_percentage = 0.2 -- 20% go offline in phase 2
    },
    medium = {
        peer_count = 15,
        sync_ops = 1000,
        max_ticks = 300,
        edit_rate = 5,
        offline_percentage = 0.3
    },
    full = {
        peer_count = 26,
        sync_ops = 10000,
        max_ticks = 1000,
        edit_rate = 15,
        offline_percentage = 0.4
    }
}

-- Select configuration (default to quick if not specified)
local config_level = os.getenv("STRESS_LEVEL") or "quick"
local config = CONFIG[config_level]

if not config then
    indras.log.error("Invalid SYNC_LEVEL", {
        trace_id = ctx.trace_id,
        level = config_level,
        valid_levels = {"quick", "medium", "full"}
    })
    error("Invalid SYNC_LEVEL: " .. config_level)
end

indras.log.info("Starting sync stress test", {
    trace_id = ctx.trace_id,
    level = config_level,
    peers = config.peer_count,
    target_sync_ops = config.sync_ops,
    duration = config.max_ticks,
    edit_rate = config.edit_rate,
    offline_percentage = config.offline_percentage
})

-- Create mesh topology (full mesh for sync scenarios)
local mesh = indras.MeshBuilder.new(config.peer_count):full_mesh()

indras.log.debug("Created full mesh for sync", {
    trace_id = ctx.trace_id,
    peers = mesh:peer_count(),
    edges = mesh:edge_count()
})

-- Create simulation
local sim_config = indras.SimConfig.new({
    wake_probability = 0.0,      -- Manual control for offline scenarios
    sleep_probability = 0.0,
    initial_online_probability = 1.0,  -- Start all online
    max_ticks = config.max_ticks,
    trace_routing = true
})

local sim = indras.Simulation.new(mesh, sim_config)
sim:initialize()

-- Get all peers
local all_peers = mesh:peers()

-- Sync state tracking
-- Track which "document versions" each peer has seen
local peer_document_state = {}
local document_version_counter = 0
local convergence_achieved_tick = nil

for _, peer in ipairs(all_peers) do
    peer_document_state[tostring(peer)] = {}
end

-- Metrics tracking
local metrics = {
    sync_operations = 0,
    convergence_ticks = 0,
    offline_sync_success = 0,
    message_overhead = 0,
    edits_created = 0,
    edits_propagated = 0,
    offline_peers_list = {}
}

-- Helper functions
local function random_online_peer()
    local online = sim:online_peers()
    if #online == 0 then return nil end
    return online[math.random(#online)]
end

local function random_offline_peer()
    local offline = sim:offline_peers()
    if #offline == 0 then return nil end
    return offline[math.random(#offline)]
end

-- Create a document edit (represented as a version number)
local function create_edit(peer)
    document_version_counter = document_version_counter + 1
    local version = document_version_counter

    -- Peer immediately sees their own edit
    local peer_key = tostring(peer)
    peer_document_state[peer_key][version] = true

    metrics.edits_created = metrics.edits_created + 1
    metrics.sync_operations = metrics.sync_operations + 1

    indras.log.debug("Created document edit", {
        trace_id = ctx.trace_id,
        peer = peer_key,
        version = version,
        tick = sim.tick
    })

    return version
end

-- Propagate edit to connected peers (sync message)
local function propagate_edit(from_peer, to_peer, version)
    local from_key = tostring(from_peer)
    local to_key = tostring(to_peer)

    -- Check if recipient already has this version
    if peer_document_state[to_key][version] then
        return  -- Already synced
    end

    -- Send sync message
    sim:send_message(from_peer, to_peer, string.format("sync_v%d", version))
    metrics.message_overhead = metrics.message_overhead + 1

    -- Record that recipient now has this version
    peer_document_state[to_key][version] = true
    metrics.edits_propagated = metrics.edits_propagated + 1
    metrics.sync_operations = metrics.sync_operations + 1

    indras.log.debug("Propagated edit", {
        trace_id = ctx.trace_id,
        from = from_key,
        to = to_key,
        version = version,
        tick = sim.tick
    })
end

-- Check if all online peers have converged (all have all versions)
local function check_convergence()
    local online = sim:online_peers()
    if #online == 0 then return false end

    -- Check if all online peers have all versions
    for version = 1, document_version_counter do
        for _, peer in ipairs(online) do
            local peer_key = tostring(peer)
            if not peer_document_state[peer_key][version] then
                return false  -- Missing version
            end
        end
    end

    return true
end

-- Broadcast edits from a peer to all connected online peers
local function broadcast_edits(peer)
    local peer_key = tostring(peer)
    local online = sim:online_peers()

    -- Find all versions this peer has
    for version, _ in pairs(peer_document_state[peer_key]) do
        -- Propagate to all other online peers
        for _, other in ipairs(online) do
            if other ~= peer then
                propagate_edit(peer, other, version)
            end
        end
    end
end

-- Phase tracking
local current_phase = 1
local phase_transition_ticks = {
    phase2 = math.floor(config.max_ticks * 0.3),  -- 30% through
    phase3 = math.floor(config.max_ticks * 0.6)   -- 60% through
}

-- Phase 1: All peers online, concurrent edits
local function phase1_concurrent_edits()
    if sim.tick > phase_transition_ticks.phase2 then
        current_phase = 2
        indras.log.info("Transitioning to phase 2", {
            trace_id = ctx.trace_id,
            tick = sim.tick,
            edits_so_far = metrics.edits_created
        })
        return
    end

    -- Generate concurrent edits
    for _ = 1, config.edit_rate do
        local peer = random_online_peer()
        if peer then
            local version = create_edit(peer)
            -- Immediately broadcast to connected peers
            broadcast_edits(peer)
        end
    end
end

-- Phase 2: Some peers go offline, continue edits
local function phase2_offline_scenario()
    if sim.tick == phase_transition_ticks.phase2 then
        -- Take some peers offline
        local online = sim:online_peers()
        local target_offline = math.floor(#online * config.offline_percentage)

        for i = 1, target_offline do
            local victim = online[i]
            if victim then
                sim:force_offline(victim)
                table.insert(metrics.offline_peers_list, tostring(victim))
                indras.log.info("Taking peer offline for sync test", {
                    trace_id = ctx.trace_id,
                    peer = tostring(victim),
                    tick = sim.tick
                })
            end
        end
    end

    if sim.tick > phase_transition_ticks.phase3 then
        current_phase = 3
        indras.log.info("Transitioning to phase 3", {
            trace_id = ctx.trace_id,
            tick = sim.tick,
            offline_peers = #metrics.offline_peers_list
        })
        return
    end

    -- Continue creating edits among online peers
    for _ = 1, config.edit_rate do
        local peer = random_online_peer()
        if peer then
            local version = create_edit(peer)
            broadcast_edits(peer)
        end
    end
end

-- Phase 3: Bring offline peers back, verify convergence
local function phase3_rejoin_and_converge()
    if sim.tick == phase_transition_ticks.phase3 then
        -- Bring offline peers back online
        local offline = sim:offline_peers()

        for _, peer in ipairs(offline) do
            sim:force_online(peer)
            indras.log.info("Bringing peer back online", {
                trace_id = ctx.trace_id,
                peer = tostring(peer),
                tick = sim.tick
            })
        end
    end

    -- Continue some edits but focus on sync
    if math.random() < 0.3 then  -- Reduced edit rate
        local peer = random_online_peer()
        if peer then
            create_edit(peer)
        end
    end

    -- Aggressive sync broadcasts
    local online = sim:online_peers()
    for _, peer in ipairs(online) do
        if math.random() < 0.8 then  -- 80% chance to sync each tick
            broadcast_edits(peer)
        end
    end

    -- Check for convergence
    if not convergence_achieved_tick and check_convergence() then
        convergence_achieved_tick = sim.tick
        metrics.convergence_ticks = convergence_achieved_tick - phase_transition_ticks.phase3

        indras.log.info("Convergence achieved", {
            trace_id = ctx.trace_id,
            tick = convergence_achieved_tick,
            convergence_duration = metrics.convergence_ticks,
            total_versions = document_version_counter
        })

        -- Count successful offline peer syncs
        for _, peer_key in ipairs(metrics.offline_peers_list) do
            local has_all = true
            for version = 1, document_version_counter do
                if not peer_document_state[peer_key][version] then
                    has_all = false
                    break
                end
            end
            if has_all then
                metrics.offline_sync_success = metrics.offline_sync_success + 1
            end
        end
    end
end

-- Main simulation loop
indras.log.info("Running sync stress simulation", {
    trace_id = ctx.trace_id,
    ticks = config.max_ticks
})

for tick = 1, config.max_ticks do
    -- Execute phase logic
    if current_phase == 1 then
        phase1_concurrent_edits()
    elseif current_phase == 2 then
        phase2_offline_scenario()
    elseif current_phase == 3 then
        phase3_rejoin_and_converge()
    end

    -- Advance simulation
    sim:step()

    -- Progress logging
    if tick % math.floor(config.max_ticks / 10) == 0 then
        local stats = sim.stats
        local convergence_status = check_convergence() and "converged" or "divergent"

        indras.log.info("Sync progress checkpoint", {
            trace_id = ctx.trace_id,
            tick = tick,
            phase = current_phase,
            online_count = #sim:online_peers(),
            edits_created = metrics.edits_created,
            sync_operations = metrics.sync_operations,
            message_overhead = metrics.message_overhead,
            convergence = convergence_status,
            messages_delivered = stats.messages_delivered
        })
    end
end

-- Final statistics
local stats = sim.stats

-- Calculate final metrics
local final_convergence = check_convergence()
if not convergence_achieved_tick and final_convergence then
    convergence_achieved_tick = sim.tick
    metrics.convergence_ticks = sim.tick - phase_transition_ticks.phase3
end

local offline_sync_rate = 0
if #metrics.offline_peers_list > 0 then
    offline_sync_rate = metrics.offline_sync_success / #metrics.offline_peers_list
end

local edit_propagation_rate = 0
if metrics.edits_created > 0 then
    -- Each edit should reach all peers
    local expected_propagations = metrics.edits_created * (config.peer_count - 1)
    edit_propagation_rate = metrics.edits_propagated / expected_propagations
end

indras.log.info("Sync stress test completed", {
    trace_id = ctx.trace_id,
    level = config_level,
    final_tick = sim.tick,
    -- Sync metrics
    sync_operations = metrics.sync_operations,
    convergence_ticks = metrics.convergence_ticks,
    convergence_achieved = final_convergence,
    offline_sync_success = metrics.offline_sync_success,
    offline_sync_rate = offline_sync_rate,
    message_overhead = metrics.message_overhead,
    -- Document metrics
    total_edits = metrics.edits_created,
    edits_propagated = metrics.edits_propagated,
    edit_propagation_rate = edit_propagation_rate,
    document_versions = document_version_counter,
    -- Network metrics
    messages_sent = stats.messages_sent,
    messages_delivered = stats.messages_delivered,
    messages_dropped = stats.messages_dropped,
    delivery_rate = stats:delivery_rate(),
    average_latency = stats:average_latency(),
    average_hops = stats:average_hops()
})

-- Assertions
indras.assert.gt(metrics.edits_created, 0, "Should have created document edits")
indras.assert.gt(metrics.sync_operations, 0, "Should have performed sync operations")
indras.assert.gt(stats.messages_delivered, 0, "Should have delivered sync messages")

-- Verify convergence was achieved
indras.assert.eq(final_convergence, true, "All online peers should converge")

-- Verify offline peers successfully resynced
if #metrics.offline_peers_list > 0 then
    indras.assert.gt(metrics.offline_sync_success, 0, "At least some offline peers should resync")
    indras.assert.ge(offline_sync_rate, 0.8, "At least 80% of offline peers should resync successfully")
end

-- Verify reasonable message overhead (not excessive)
local messages_per_edit = metrics.message_overhead / metrics.edits_created
indras.assert.lt(messages_per_edit, config.peer_count * 2, "Message overhead should be reasonable")

indras.log.info("Sync stress test passed", {
    trace_id = ctx.trace_id,
    convergence_achieved = final_convergence,
    offline_sync_rate = offline_sync_rate,
    edit_propagation_rate = edit_propagation_rate,
    messages_per_edit = messages_per_edit
})

return {
    -- Sync metrics
    sync_operations = metrics.sync_operations,
    convergence_ticks = metrics.convergence_ticks,
    offline_sync_success = metrics.offline_sync_success,
    message_overhead = metrics.message_overhead,
    -- Derived metrics
    offline_sync_rate = offline_sync_rate,
    edit_propagation_rate = edit_propagation_rate,
    messages_per_edit = messages_per_edit,
    convergence_achieved = final_convergence,
    -- Totals
    total_edits = metrics.edits_created,
    total_document_versions = document_version_counter,
    -- Network stats
    delivery_rate = stats:delivery_rate(),
    average_latency = stats:average_latency()
}

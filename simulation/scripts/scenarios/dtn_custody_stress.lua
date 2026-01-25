-- DTN Custody Transfer Stress Test
--
-- Stress tests DTN custody transfer mechanisms including:
-- - Custody acceptance/rejection decisions based on capacity and policy
-- - Custody timeout handling when nodes fail to forward bundles
-- - Multi-hop custody transfer chains with integrity verification
-- - Custody conflict resolution when multiple nodes claim custody
--
-- This scenario extends DTN stress testing to focus specifically on
-- custody transfer semantics and edge cases.

local ctx = indras.correlation.new_root()
ctx = ctx:with_tag("scenario", "dtn_custody_stress")

-- Configuration levels
local CONFIG = {
    quick = {
        peers = 12,
        bundles = 60,
        ticks = 250,
        offline_rate = 0.70,           -- 70% offline: challenged network
        bundle_ttl = 120,               -- ticks before bundle expires
        custody_timeout = 30,           -- ticks before custody times out
        custody_capacity = 5,           -- max bundles a node can hold custody for
        rejection_prob = 0.15,          -- probability of custody rejection
        failure_prob = 0.10,            -- probability of node failure during custody
        conflict_prob = 0.05,           -- probability of custody conflict injection
        spray_factor = 2,
        wake_prob = 0.08,
        sleep_prob = 0.12
    },
    medium = {
        peers = 24,
        bundles = 400,
        ticks = 600,
        offline_rate = 0.80,
        bundle_ttl = 200,
        custody_timeout = 50,
        custody_capacity = 8,
        rejection_prob = 0.20,
        failure_prob = 0.15,
        conflict_prob = 0.08,
        spray_factor = 3,
        wake_prob = 0.05,
        sleep_prob = 0.15
    },
    full = {
        peers = 30,
        bundles = 2000,
        ticks = 1500,
        offline_rate = 0.85,
        bundle_ttl = 400,
        custody_timeout = 80,
        custody_capacity = 12,
        rejection_prob = 0.25,
        failure_prob = 0.20,
        conflict_prob = 0.10,
        spray_factor = 4,
        wake_prob = 0.03,
        sleep_prob = 0.20
    }
}

-- Select test level (default to quick if not specified)
local test_level = os.getenv("STRESS_LEVEL") or "quick"
local config = CONFIG[test_level]

if not config then
    error("Invalid test level: " .. tostring(test_level))
end

indras.log.info("Starting DTN custody transfer stress test", {
    trace_id = ctx.trace_id,
    level = test_level,
    peers = config.peers,
    bundles = config.bundles,
    ticks = config.ticks,
    offline_rate = config.offline_rate,
    bundle_ttl = config.bundle_ttl,
    custody_timeout = config.custody_timeout,
    custody_capacity = config.custody_capacity,
    rejection_prob = config.rejection_prob,
    failure_prob = config.failure_prob,
    conflict_prob = config.conflict_prob
})

-- Create sparse mesh topology (challenged network)
local mesh = indras.MeshBuilder.new(config.peers):random(0.25)

indras.log.debug("Created sparse mesh for custody stress", {
    trace_id = ctx.trace_id,
    peers = mesh:peer_count(),
    edges = mesh:edge_count(),
    avg_connectivity = mesh:edge_count() / mesh:peer_count()
})

-- Create simulation with challenged network conditions
local sim_config = indras.SimConfig.new({
    wake_probability = config.wake_prob,
    sleep_probability = config.sleep_prob,
    initial_online_probability = 1.0 - config.offline_rate,
    max_ticks = config.ticks,
    trace_routing = true
})

local sim = indras.Simulation.new(mesh, sim_config)
sim:initialize()

local all_peers = mesh:peers()

-- Node custody state tracking
local NodeCustodyState = {}
NodeCustodyState.__index = NodeCustodyState

function NodeCustodyState.new(peer)
    return setmetatable({
        peer = peer,
        peer_str = tostring(peer),
        held_bundles = {},          -- bundle_id -> tick_acquired
        custody_count = 0,
        total_accepted = 0,
        total_rejected = 0,
        total_timeouts = 0,
        total_transferred = 0,
        failed_during_custody = false
    }, NodeCustodyState)
end

function NodeCustodyState:can_accept()
    return self.custody_count < config.custody_capacity
end

function NodeCustodyState:accept_custody(bundle_id, tick)
    if not self:can_accept() then
        return false
    end
    self.held_bundles[bundle_id] = tick
    self.custody_count = self.custody_count + 1
    self.total_accepted = self.total_accepted + 1
    return true
end

function NodeCustodyState:release_custody(bundle_id)
    if self.held_bundles[bundle_id] then
        self.held_bundles[bundle_id] = nil
        self.custody_count = self.custody_count - 1
        self.total_transferred = self.total_transferred + 1
        return true
    end
    return false
end

function NodeCustodyState:check_timeouts(current_tick)
    local timed_out = {}
    for bundle_id, acquired_tick in pairs(self.held_bundles) do
        if current_tick - acquired_tick >= config.custody_timeout then
            table.insert(timed_out, bundle_id)
        end
    end
    return timed_out
end

function NodeCustodyState:force_timeout(bundle_id)
    if self.held_bundles[bundle_id] then
        self.held_bundles[bundle_id] = nil
        self.custody_count = self.custody_count - 1
        self.total_timeouts = self.total_timeouts + 1
        return true
    end
    return false
end

-- Initialize node custody states
local node_states = {}
for _, peer in ipairs(all_peers) do
    node_states[tostring(peer)] = NodeCustodyState.new(peer)
end

-- CustodyBundle class with enhanced custody tracking
local CustodyBundle = {}
CustodyBundle.__index = CustodyBundle

function CustodyBundle.new(id, source, destination, created_tick, ttl)
    return setmetatable({
        id = id,
        source = source,
        destination = destination,
        created_tick = created_tick,
        ttl = ttl,
        expires_tick = created_tick + ttl,
        -- Delivery state
        delivered = false,
        delivered_tick = nil,
        expired = false,
        -- Custody state
        current_custodian = nil,       -- current holder of custody
        custody_acquired_tick = nil,   -- when current custody was acquired
        -- Tracking counts
        acceptance_count = 0,
        rejection_count = 0,
        timeout_count = 0,
        conflict_count = 0,
        -- Custody chain for integrity verification
        custody_chain = {},            -- ordered list of custody transfers
        copies = {},                   -- peers holding copies (for epidemic spreading)
        -- Conflict tracking
        conflicting_claims = {}        -- nodes that claimed custody simultaneously
    }, CustodyBundle)
end

function CustodyBundle:add_copy(peer, tick)
    local peer_str = tostring(peer)
    if not self.copies[peer_str] then
        self.copies[peer_str] = tick
    end
end

function CustodyBundle:copy_count()
    local count = 0
    for _ in pairs(self.copies) do
        count = count + 1
    end
    return count
end

function CustodyBundle:has_expired(current_tick)
    return current_tick >= self.expires_tick
end

function CustodyBundle:record_acceptance(from_peer, to_peer, tick)
    self.acceptance_count = self.acceptance_count + 1

    local from_str = from_peer and tostring(from_peer) or "origin"
    local to_str = tostring(to_peer)

    table.insert(self.custody_chain, {
        type = "acceptance",
        from = from_str,
        to = to_str,
        tick = tick,
        chain_position = #self.custody_chain + 1
    })

    self.current_custodian = to_peer
    self.custody_acquired_tick = tick
end

function CustodyBundle:record_rejection(from_peer, to_peer, tick, reason)
    self.rejection_count = self.rejection_count + 1

    table.insert(self.custody_chain, {
        type = "rejection",
        from = tostring(from_peer),
        to = tostring(to_peer),
        tick = tick,
        reason = reason
    })
end

function CustodyBundle:record_timeout(peer, tick)
    self.timeout_count = self.timeout_count + 1

    table.insert(self.custody_chain, {
        type = "timeout",
        node = tostring(peer),
        tick = tick
    })

    -- Custody returns to previous holder or becomes orphaned
    self.current_custodian = nil
    self.custody_acquired_tick = nil
end

function CustodyBundle:record_conflict(claimants, tick)
    self.conflict_count = self.conflict_count + 1

    local claimant_strs = {}
    for _, c in ipairs(claimants) do
        table.insert(claimant_strs, tostring(c))
    end

    table.insert(self.custody_chain, {
        type = "conflict",
        claimants = claimant_strs,
        tick = tick
    })

    table.insert(self.conflicting_claims, {
        claimants = claimant_strs,
        tick = tick,
        resolved = false
    })
end

function CustodyBundle:resolve_conflict(winner, tick)
    if #self.conflicting_claims > 0 then
        local latest_conflict = self.conflicting_claims[#self.conflicting_claims]
        latest_conflict.resolved = true
        latest_conflict.winner = tostring(winner)
        latest_conflict.resolved_tick = tick

        table.insert(self.custody_chain, {
            type = "conflict_resolution",
            winner = tostring(winner),
            tick = tick
        })

        self.current_custodian = winner
        self.custody_acquired_tick = tick
    end
end

function CustodyBundle:chain_length()
    local length = 0
    for _, entry in ipairs(self.custody_chain) do
        if entry.type == "acceptance" then
            length = length + 1
        end
    end
    return length
end

function CustodyBundle:verify_chain_integrity()
    -- Verify the custody chain has no gaps or inconsistencies
    local last_holder = nil
    local issues = {}

    for i, entry in ipairs(self.custody_chain) do
        if entry.type == "acceptance" then
            if last_holder and entry.from ~= last_holder and entry.from ~= "origin" then
                table.insert(issues, {
                    position = i,
                    issue = "chain_gap",
                    expected_from = last_holder,
                    actual_from = entry.from
                })
            end
            last_holder = entry.to
        elseif entry.type == "timeout" then
            if last_holder ~= entry.node then
                table.insert(issues, {
                    position = i,
                    issue = "timeout_mismatch",
                    expected_holder = last_holder,
                    timeout_node = entry.node
                })
            end
            last_holder = nil
        elseif entry.type == "conflict_resolution" then
            last_holder = entry.winner
        end
    end

    return #issues == 0, issues
end

-- Bundle registry
local bundles = {}
local next_bundle_id = 1

-- Metrics tracking
local Metrics = {
    bundles_created = 0,
    bundles_delivered = 0,
    bundles_expired = 0,
    -- Custody-specific metrics
    custody_acceptances = 0,
    custody_rejections = 0,
    custody_timeouts = 0,
    custody_conflicts = 0,
    custody_conflicts_resolved = 0,
    custody_transfers = 0,
    -- Chain metrics
    total_chain_length = 0,
    max_chain_length = 0,
    chains_verified = 0,
    chains_with_issues = 0,
    -- Failure metrics
    node_failures_during_custody = 0
}

-- Helper functions
local function random_online_peer()
    local online = sim:online_peers()
    if #online == 0 then return nil end
    return online[math.random(#online)]
end

local function random_peer()
    return all_peers[math.random(#all_peers)]
end

local function create_bundle(source, destination, tick)
    local bundle = CustodyBundle.new(
        string.format("custody-bundle-%d", next_bundle_id),
        source,
        destination,
        tick,
        config.bundle_ttl
    )
    next_bundle_id = next_bundle_id + 1

    -- Source initially has custody
    bundle:add_copy(source, tick)
    bundle:record_acceptance(nil, source, tick)

    local source_state = node_states[tostring(source)]
    if source_state then
        source_state:accept_custody(bundle.id, tick)
    end

    bundles[bundle.id] = bundle
    Metrics.bundles_created = Metrics.bundles_created + 1
    Metrics.custody_acceptances = Metrics.custody_acceptances + 1

    indras.log.debug("Custody bundle created", {
        trace_id = ctx.trace_id,
        bundle_id = bundle.id,
        source = tostring(source),
        destination = tostring(destination),
        ttl = config.bundle_ttl,
        expires_tick = bundle.expires_tick
    })

    return bundle
end

local function attempt_custody_transfer(bundle, from_peer, to_peer, current_tick)
    if bundle.delivered or bundle.expired then
        return false, "bundle_inactive"
    end

    local from_str = tostring(from_peer)
    local to_str = tostring(to_peer)
    local to_state = node_states[to_str]

    if not to_state then
        return false, "unknown_node"
    end

    -- Check if target node is online
    if not sim:is_online(to_peer) then
        return false, "node_offline"
    end

    -- Check capacity-based rejection
    if not to_state:can_accept() then
        bundle:record_rejection(from_peer, to_peer, current_tick, "capacity_exceeded")
        Metrics.custody_rejections = Metrics.custody_rejections + 1
        to_state.total_rejected = to_state.total_rejected + 1

        indras.log.debug("Custody rejected (capacity)", {
            trace_id = ctx.trace_id,
            bundle_id = bundle.id,
            from = from_str,
            to = to_str,
            current_custody = to_state.custody_count,
            capacity = config.custody_capacity
        })

        return false, "capacity_exceeded"
    end

    -- Random policy-based rejection
    if math.random() < config.rejection_prob then
        bundle:record_rejection(from_peer, to_peer, current_tick, "policy_rejection")
        Metrics.custody_rejections = Metrics.custody_rejections + 1
        to_state.total_rejected = to_state.total_rejected + 1

        indras.log.debug("Custody rejected (policy)", {
            trace_id = ctx.trace_id,
            bundle_id = bundle.id,
            from = from_str,
            to = to_str
        })

        return false, "policy_rejection"
    end

    -- Release custody from sender
    local from_state = node_states[from_str]
    if from_state then
        from_state:release_custody(bundle.id)
    end

    -- Accept custody at receiver
    to_state:accept_custody(bundle.id, current_tick)
    bundle:add_copy(to_peer, current_tick)
    bundle:record_acceptance(from_peer, to_peer, current_tick)

    Metrics.custody_acceptances = Metrics.custody_acceptances + 1
    Metrics.custody_transfers = Metrics.custody_transfers + 1

    -- Send network message to simulate transfer
    sim:send_message(from_peer, to_peer, "custody_transfer")

    indras.log.debug("Custody transferred", {
        trace_id = ctx.trace_id,
        bundle_id = bundle.id,
        from = from_str,
        to = to_str,
        chain_length = bundle:chain_length()
    })

    return true, "accepted"
end

local function inject_custody_conflict(bundle, current_tick)
    -- Simulate a scenario where multiple nodes claim custody
    local online = sim:online_peers()
    if #online < 2 then return false end

    -- Select 2-3 random nodes as conflicting claimants
    local num_claimants = math.min(math.random(2, 3), #online)
    local claimants = {}
    local used = {}

    for _ = 1, num_claimants do
        local idx = math.random(#online)
        local attempts = 0
        while used[idx] and attempts < 10 do
            idx = math.random(#online)
            attempts = attempts + 1
        end
        if not used[idx] then
            used[idx] = true
            table.insert(claimants, online[idx])
        end
    end

    if #claimants < 2 then return false end

    bundle:record_conflict(claimants, current_tick)
    Metrics.custody_conflicts = Metrics.custody_conflicts + 1

    indras.log.warn("Custody conflict injected", {
        trace_id = ctx.trace_id,
        bundle_id = bundle.id,
        num_claimants = #claimants,
        tick = current_tick
    })

    -- Resolve conflict by selecting random winner
    local winner = claimants[math.random(#claimants)]
    local winner_state = node_states[tostring(winner)]

    if winner_state and winner_state:can_accept() then
        winner_state:accept_custody(bundle.id, current_tick)
        bundle:resolve_conflict(winner, current_tick)
        Metrics.custody_conflicts_resolved = Metrics.custody_conflicts_resolved + 1

        indras.log.info("Custody conflict resolved", {
            trace_id = ctx.trace_id,
            bundle_id = bundle.id,
            winner = tostring(winner),
            tick = current_tick
        })
    end

    return true
end

local function process_custody_timeouts(current_tick)
    for peer_str, state in pairs(node_states) do
        local timed_out = state:check_timeouts(current_tick)

        for _, bundle_id in ipairs(timed_out) do
            local bundle = bundles[bundle_id]
            if bundle and not bundle.delivered and not bundle.expired then
                state:force_timeout(bundle_id)
                bundle:record_timeout(state.peer, current_tick)
                Metrics.custody_timeouts = Metrics.custody_timeouts + 1

                indras.log.warn("Custody timeout", {
                    trace_id = ctx.trace_id,
                    bundle_id = bundle_id,
                    node = peer_str,
                    tick = current_tick,
                    held_for = current_tick - (bundle.custody_acquired_tick or 0)
                })
            end
        end
    end
end

local function simulate_node_failures(current_tick)
    -- Simulate random node failures during custody
    for peer_str, state in pairs(node_states) do
        if state.custody_count > 0 and not state.failed_during_custody then
            if math.random() < config.failure_prob / config.ticks then
                -- Node fails while holding custody
                state.failed_during_custody = true
                Metrics.node_failures_during_custody = Metrics.node_failures_during_custody + 1

                -- Force all held bundles to timeout
                for bundle_id, _ in pairs(state.held_bundles) do
                    local bundle = bundles[bundle_id]
                    if bundle then
                        state:force_timeout(bundle_id)
                        bundle:record_timeout(state.peer, current_tick)
                        Metrics.custody_timeouts = Metrics.custody_timeouts + 1
                    end
                end

                sim:force_offline(state.peer)

                indras.log.error("Node failed during custody", {
                    trace_id = ctx.trace_id,
                    node = peer_str,
                    bundles_affected = state.custody_count,
                    tick = current_tick
                })
            end
        end
    end
end

local function epidemic_custody_spray(bundle, current_tick)
    if bundle.delivered or bundle.expired then return end
    if not bundle.current_custodian then return end

    local custodian = bundle.current_custodian
    local custodian_str = tostring(custodian)

    if not sim:is_online(custodian) then return end

    local neighbors = mesh:neighbors(custodian)
    if #neighbors == 0 then return end

    -- Shuffle neighbors
    for i = #neighbors, 2, -1 do
        local j = math.random(i)
        neighbors[i], neighbors[j] = neighbors[j], neighbors[i]
    end

    local transferred = 0
    for _, neighbor in ipairs(neighbors) do
        if transferred >= config.spray_factor then break end

        if sim:is_online(neighbor) and not bundle.copies[tostring(neighbor)] then
            local success, reason = attempt_custody_transfer(bundle, custodian, neighbor, current_tick)
            if success then
                transferred = transferred + 1
                break  -- Only one node can have custody at a time
            end
        end
    end
end

local function attempt_delivery(bundle, current_tick)
    if bundle.delivered or bundle.expired then return end

    local dest_str = tostring(bundle.destination)
    if bundle.copies[dest_str] and sim:is_online(bundle.destination) then
        bundle.delivered = true
        bundle.delivered_tick = current_tick
        Metrics.bundles_delivered = Metrics.bundles_delivered + 1

        -- Release custody at destination
        local dest_state = node_states[dest_str]
        if dest_state then
            dest_state:release_custody(bundle.id)
        end

        local latency = current_tick - bundle.created_tick
        local chain_len = bundle:chain_length()
        local valid, issues = bundle:verify_chain_integrity()

        Metrics.total_chain_length = Metrics.total_chain_length + chain_len
        Metrics.max_chain_length = math.max(Metrics.max_chain_length, chain_len)
        Metrics.chains_verified = Metrics.chains_verified + 1

        if not valid then
            Metrics.chains_with_issues = Metrics.chains_with_issues + 1
            indras.log.warn("Custody chain has integrity issues", {
                trace_id = ctx.trace_id,
                bundle_id = bundle.id,
                issues = #issues
            })
        end

        indras.log.info("Custody bundle delivered", {
            trace_id = ctx.trace_id,
            bundle_id = bundle.id,
            source = tostring(bundle.source),
            destination = dest_str,
            latency_ticks = latency,
            chain_length = chain_len,
            acceptances = bundle.acceptance_count,
            rejections = bundle.rejection_count,
            timeouts = bundle.timeout_count,
            conflicts = bundle.conflict_count,
            chain_valid = valid
        })
    end
end

local function expire_bundles(current_tick)
    for _, bundle in pairs(bundles) do
        if not bundle.expired and not bundle.delivered and bundle:has_expired(current_tick) then
            bundle.expired = true
            Metrics.bundles_expired = Metrics.bundles_expired + 1

            -- Release custody if held
            if bundle.current_custodian then
                local custodian_str = tostring(bundle.current_custodian)
                local state = node_states[custodian_str]
                if state then
                    state:release_custody(bundle.id)
                end
            end

            indras.log.warn("Custody bundle expired", {
                trace_id = ctx.trace_id,
                bundle_id = bundle.id,
                source = tostring(bundle.source),
                destination = tostring(bundle.destination),
                chain_length = bundle:chain_length(),
                acceptances = bundle.acceptance_count,
                rejections = bundle.rejection_count,
                timeouts = bundle.timeout_count
            })
        end
    end
end

-- Phase 1: Custody chain building
indras.log.info("Phase 1: Building custody chains", {
    trace_id = ctx.trace_id,
    phase_ticks = math.floor(config.ticks * 0.35)
})

local phase1_end = math.floor(config.ticks * 0.35)
local bundles_per_tick = math.ceil(config.bundles / phase1_end)

for tick = 1, phase1_end do
    -- Create new bundles
    for _ = 1, bundles_per_tick do
        if Metrics.bundles_created < config.bundles then
            local source = random_online_peer()
            local destination = random_peer()

            if source and destination and source ~= destination then
                create_bundle(source, destination, tick)
            end
        end
    end

    -- Process custody operations
    for _, bundle in pairs(bundles) do
        if not bundle.delivered and not bundle.expired then
            epidemic_custody_spray(bundle, tick)
            attempt_delivery(bundle, tick)
        end
    end

    process_custody_timeouts(tick)
    expire_bundles(tick)
    sim:step()

    -- Progress logging
    if tick % 50 == 0 then
        indras.log.info("Phase 1 progress", {
            trace_id = ctx.trace_id,
            tick = tick,
            online_count = #sim:online_peers(),
            bundles_created = Metrics.bundles_created,
            bundles_delivered = Metrics.bundles_delivered,
            custody_acceptances = Metrics.custody_acceptances,
            custody_rejections = Metrics.custody_rejections,
            custody_timeouts = Metrics.custody_timeouts
        })
    end
end

-- Phase 2: Custody stress with failures and conflicts
indras.log.info("Phase 2: Custody stress with failures", {
    trace_id = ctx.trace_id,
    phase_ticks = math.floor(config.ticks * 0.40)
})

local phase2_end = phase1_end + math.floor(config.ticks * 0.40)

for tick = phase1_end + 1, phase2_end do
    -- Inject custody conflicts periodically
    if math.random() < config.conflict_prob then
        local active_bundles = {}
        for _, bundle in pairs(bundles) do
            if not bundle.delivered and not bundle.expired then
                table.insert(active_bundles, bundle)
            end
        end
        if #active_bundles > 0 then
            local target = active_bundles[math.random(#active_bundles)]
            inject_custody_conflict(target, tick)
        end
    end

    -- Simulate node failures
    simulate_node_failures(tick)

    -- Process custody operations
    for _, bundle in pairs(bundles) do
        if not bundle.delivered and not bundle.expired then
            epidemic_custody_spray(bundle, tick)
            attempt_delivery(bundle, tick)
        end
    end

    process_custody_timeouts(tick)
    expire_bundles(tick)
    sim:step()

    if tick % 50 == 0 then
        indras.log.info("Phase 2 progress", {
            trace_id = ctx.trace_id,
            tick = tick,
            online_count = #sim:online_peers(),
            bundles_delivered = Metrics.bundles_delivered,
            bundles_expired = Metrics.bundles_expired,
            custody_conflicts = Metrics.custody_conflicts,
            custody_timeouts = Metrics.custody_timeouts,
            node_failures = Metrics.node_failures_during_custody
        })
    end
end

-- Phase 3: Custody recovery
indras.log.info("Phase 3: Custody recovery", {
    trace_id = ctx.trace_id,
    phase_ticks = config.ticks - phase2_end
})

-- Increase wake probability and reset failed nodes
for peer_str, state in pairs(node_states) do
    if state.failed_during_custody then
        state.failed_during_custody = false
        sim:force_online(state.peer)
    end
end

for tick = phase2_end + 1, config.ticks do
    -- Wake up offline peers periodically
    if tick % 8 == 0 then
        local offline = sim:offline_peers()
        if #offline > 0 then
            local wake_count = math.min(2, #offline)
            for i = 1, wake_count do
                sim:force_online(offline[i])
            end
        end
    end

    -- Process custody operations with recovery focus
    for _, bundle in pairs(bundles) do
        if not bundle.delivered and not bundle.expired then
            -- Re-establish custody for orphaned bundles
            if not bundle.current_custodian then
                local online = sim:online_peers()
                if #online > 0 then
                    local new_custodian = online[math.random(#online)]
                    local state = node_states[tostring(new_custodian)]
                    if state and state:can_accept() then
                        state:accept_custody(bundle.id, tick)
                        bundle:record_acceptance(nil, new_custodian, tick)
                        Metrics.custody_acceptances = Metrics.custody_acceptances + 1
                    end
                end
            end

            epidemic_custody_spray(bundle, tick)
            attempt_delivery(bundle, tick)
        end
    end

    process_custody_timeouts(tick)
    expire_bundles(tick)
    sim:step()

    if tick % 50 == 0 then
        indras.log.info("Phase 3 progress", {
            trace_id = ctx.trace_id,
            tick = tick,
            online_count = #sim:online_peers(),
            bundles_delivered = Metrics.bundles_delivered,
            bundles_expired = Metrics.bundles_expired,
            custody_transfers = Metrics.custody_transfers
        })
    end
end

-- Calculate final metrics
local custody_acceptance_rate = (Metrics.custody_acceptances + Metrics.custody_rejections) > 0
    and (Metrics.custody_acceptances / (Metrics.custody_acceptances + Metrics.custody_rejections))
    or 0

local custody_rejection_rate = (Metrics.custody_acceptances + Metrics.custody_rejections) > 0
    and (Metrics.custody_rejections / (Metrics.custody_acceptances + Metrics.custody_rejections))
    or 0

local custody_timeout_rate = Metrics.custody_acceptances > 0
    and (Metrics.custody_timeouts / Metrics.custody_acceptances)
    or 0

local bundle_delivery_rate = Metrics.bundles_created > 0
    and (Metrics.bundles_delivered / Metrics.bundles_created)
    or 0

local avg_chain_length = Metrics.chains_verified > 0
    and (Metrics.total_chain_length / Metrics.chains_verified)
    or 0

local chain_integrity_rate = Metrics.chains_verified > 0
    and ((Metrics.chains_verified - Metrics.chains_with_issues) / Metrics.chains_verified)
    or 0

local conflict_resolution_rate = Metrics.custody_conflicts > 0
    and (Metrics.custody_conflicts_resolved / Metrics.custody_conflicts)
    or 0

-- Calculate average delivery latency
local total_latency = 0
local delivered_count = 0
for _, bundle in pairs(bundles) do
    if bundle.delivered then
        total_latency = total_latency + (bundle.delivered_tick - bundle.created_tick)
        delivered_count = delivered_count + 1
    end
end
local avg_delivery_latency = delivered_count > 0 and (total_latency / delivered_count) or 0

-- Network stats
local stats = sim.stats

indras.log.info("DTN custody stress test completed", {
    trace_id = ctx.trace_id,
    test_level = test_level,
    final_tick = sim.tick,
    -- Bundle metrics
    bundles_created = Metrics.bundles_created,
    bundles_delivered = Metrics.bundles_delivered,
    bundles_expired = Metrics.bundles_expired,
    bundle_delivery_rate = bundle_delivery_rate,
    avg_delivery_latency = avg_delivery_latency,
    -- Custody metrics
    custody_acceptances = Metrics.custody_acceptances,
    custody_rejections = Metrics.custody_rejections,
    custody_timeouts = Metrics.custody_timeouts,
    custody_transfers = Metrics.custody_transfers,
    custody_acceptance_rate = custody_acceptance_rate,
    custody_rejection_rate = custody_rejection_rate,
    custody_timeout_rate = custody_timeout_rate,
    -- Conflict metrics
    custody_conflicts = Metrics.custody_conflicts,
    custody_conflicts_resolved = Metrics.custody_conflicts_resolved,
    conflict_resolution_rate = conflict_resolution_rate,
    -- Chain metrics
    avg_chain_length = avg_chain_length,
    max_chain_length = Metrics.max_chain_length,
    chains_verified = Metrics.chains_verified,
    chain_integrity_rate = chain_integrity_rate,
    -- Failure metrics
    node_failures_during_custody = Metrics.node_failures_during_custody,
    -- Network metrics
    messages_sent = stats.messages_sent,
    messages_delivered = stats.messages_delivered,
    messages_dropped = stats.messages_dropped,
    network_delivery_rate = stats:delivery_rate()
})

-- Assertions
indras.assert.eq(Metrics.bundles_created, config.bundles, "Should create all configured bundles")
indras.assert.gt(Metrics.bundles_delivered, 0, "Should deliver at least some bundles")
indras.assert.gt(Metrics.custody_transfers, 0, "Should have custody transfers")
indras.assert.gt(Metrics.custody_acceptances, 0, "Should have custody acceptances")

-- Custody acceptance rate should be reasonable given rejection probability
indras.assert.gt(custody_acceptance_rate, 0.5, "Custody acceptance rate should be > 50%")

-- Bundle delivery rate should be reasonable
indras.assert.gt(bundle_delivery_rate, 0.05, "Bundle delivery rate should be > 5%")

-- Chain integrity should be high
indras.assert.gt(chain_integrity_rate, 0.8, "Chain integrity rate should be > 80%")

-- Custody timeouts should not dominate
indras.assert.lt(custody_timeout_rate, 0.5, "Custody timeout rate should be < 50%")

-- Conflict resolution should work
if Metrics.custody_conflicts > 0 then
    indras.assert.gt(conflict_resolution_rate, 0.7, "Conflict resolution rate should be > 70%")
end

indras.log.info("DTN custody stress test passed", {
    trace_id = ctx.trace_id,
    bundle_delivery_rate = bundle_delivery_rate,
    custody_acceptance_rate = custody_acceptance_rate,
    custody_rejection_rate = custody_rejection_rate,
    custody_timeout_rate = custody_timeout_rate,
    avg_chain_length = avg_chain_length,
    chain_integrity_rate = chain_integrity_rate,
    conflict_resolution_rate = conflict_resolution_rate
})

return {
    -- Bundle metrics
    bundles_created = Metrics.bundles_created,
    bundles_delivered = Metrics.bundles_delivered,
    bundles_expired = Metrics.bundles_expired,
    bundle_delivery_rate = bundle_delivery_rate,
    avg_delivery_latency = avg_delivery_latency,
    -- Custody metrics
    custody_acceptances = Metrics.custody_acceptances,
    custody_rejections = Metrics.custody_rejections,
    custody_timeouts = Metrics.custody_timeouts,
    custody_transfers = Metrics.custody_transfers,
    custody_acceptance_rate = custody_acceptance_rate,
    custody_rejection_rate = custody_rejection_rate,
    custody_timeout_rate = custody_timeout_rate,
    -- Conflict metrics
    custody_conflicts = Metrics.custody_conflicts,
    custody_conflicts_resolved = Metrics.custody_conflicts_resolved,
    conflict_resolution_rate = conflict_resolution_rate,
    -- Chain metrics
    avg_chain_length = avg_chain_length,
    max_chain_length = Metrics.max_chain_length,
    chain_integrity_rate = chain_integrity_rate,
    -- Failure metrics
    node_failures_during_custody = Metrics.node_failures_during_custody,
    -- Network metrics
    network_delivery_rate = stats:delivery_rate(),
    total_ticks = sim.tick
}

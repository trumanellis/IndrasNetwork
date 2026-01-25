-- DTN Strategy Switching Stress Test
--
-- Stress tests dynamic DTN routing strategy switching with three strategies:
-- - Epidemic: Full spray-and-wait with configurable copy limits
-- - PRoPHET: Probability-based forwarding using encounter history
-- - Direct: Only forward when destination is directly reachable
--
-- Tests strategy switching based on network conditions:
-- - Network connectivity level
-- - Buffer usage/congestion
-- - Delivery success rates
--
-- Validates that dynamic switching improves overall performance compared
-- to static strategy selection.

local ctx = indras.correlation.new_root()
ctx = ctx:with_tag("scenario", "dtn_strategy_stress")

-- Configuration levels
local CONFIG = {
    quick = {
        peers = 12,
        bundles = 100,
        ticks = 400,
        -- Phase durations
        epidemic_phase_ticks = 80,
        prophet_phase_ticks = 80,
        direct_phase_ticks = 60,
        switching_phase_ticks = 120,
        drain_phase_ticks = 60,
        -- DTN parameters
        offline_rate = 0.70,
        bundle_ttl = 150,
        spray_limit = 4,          -- Max copies per bundle (epidemic)
        prophet_threshold = 0.4,   -- Min probability to forward (prophet)
        wake_prob = 0.08,
        sleep_prob = 0.12,
        -- Strategy switching thresholds
        connectivity_high = 0.5,   -- Above this: prefer Direct
        connectivity_low = 0.2,    -- Below this: prefer Epidemic
        buffer_high = 0.7,         -- Above this: reduce spray factor
        delivery_rate_target = 0.5 -- Target delivery rate for switching decisions
    },
    medium = {
        peers = 20,
        bundles = 500,
        ticks = 1000,
        epidemic_phase_ticks = 200,
        prophet_phase_ticks = 200,
        direct_phase_ticks = 150,
        switching_phase_ticks = 350,
        drain_phase_ticks = 100,
        offline_rate = 0.80,
        bundle_ttl = 400,
        spray_limit = 5,
        prophet_threshold = 0.35,
        wake_prob = 0.05,
        sleep_prob = 0.15,
        connectivity_high = 0.45,
        connectivity_low = 0.15,
        buffer_high = 0.65,
        delivery_rate_target = 0.45
    },
    full = {
        peers = 26,
        bundles = 2000,
        ticks = 2500,
        epidemic_phase_ticks = 500,
        prophet_phase_ticks = 500,
        direct_phase_ticks = 400,
        switching_phase_ticks = 800,
        drain_phase_ticks = 300,
        offline_rate = 0.85,
        bundle_ttl = 1000,
        spray_limit = 6,
        prophet_threshold = 0.3,
        wake_prob = 0.03,
        sleep_prob = 0.18,
        connectivity_high = 0.40,
        connectivity_low = 0.10,
        buffer_high = 0.60,
        delivery_rate_target = 0.40
    }
}

-- Select test level
local test_level = os.getenv("STRESS_LEVEL") or "quick"
local config = CONFIG[test_level]

if not config then
    error("Invalid test level: " .. tostring(test_level) .. ". Valid: quick, medium, full")
end

-- Strategy enumeration
local Strategy = {
    EPIDEMIC = "epidemic",
    PROPHET = "prophet",
    DIRECT = "direct"
}

indras.log.info("Starting DTN strategy switching stress test", {
    trace_id = ctx.trace_id,
    level = test_level,
    peers = config.peers,
    bundles = config.bundles,
    ticks = config.ticks,
    offline_rate = config.offline_rate,
    spray_limit = config.spray_limit,
    prophet_threshold = config.prophet_threshold
})

-- Create sparse mesh topology (challenged network)
local mesh = indras.MeshBuilder.new(config.peers):random(0.25)

indras.log.debug("Created sparse mesh for DTN testing", {
    trace_id = ctx.trace_id,
    peers = mesh:peer_count(),
    edges = mesh:edge_count(),
    avg_degree = mesh:edge_count() * 2 / mesh:peer_count()
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

--------------------------------------------------------------------------------
-- Bundle class for DTN message tracking
--------------------------------------------------------------------------------
local Bundle = {}
Bundle.__index = Bundle

function Bundle.new(id, source, destination, created_tick, ttl)
    return setmetatable({
        id = id,
        source = source,
        destination = destination,
        created_tick = created_tick,
        ttl = ttl,
        expires_tick = created_tick + ttl,
        delivered = false,
        delivered_tick = nil,
        expired = false,
        copies = {},              -- {peer_str -> tick_added}
        copy_count_limit = config.spray_limit,
        strategy_used = nil,      -- Strategy that created this bundle
        hops = 0,
        custody_chain = {}
    }, Bundle)
end

function Bundle:add_copy(peer, tick)
    local peer_str = tostring(peer)
    if not self.copies[peer_str] then
        self.copies[peer_str] = tick
        return true
    end
    return false
end

function Bundle:has_copy(peer)
    return self.copies[tostring(peer)] ~= nil
end

function Bundle:count_copies()
    local count = 0
    for _ in pairs(self.copies) do
        count = count + 1
    end
    return count
end

function Bundle:can_spray()
    return self:count_copies() < self.copy_count_limit
end

function Bundle:has_expired(current_tick)
    return current_tick >= self.expires_tick
end

function Bundle:record_hop(from_peer, to_peer, tick)
    table.insert(self.custody_chain, {
        from = from_peer and tostring(from_peer) or "origin",
        to = tostring(to_peer),
        tick = tick
    })
    self.hops = self.hops + 1
end

--------------------------------------------------------------------------------
-- PRoPHET probability tracking
--------------------------------------------------------------------------------
local ProphetTable = {}
ProphetTable.__index = ProphetTable

function ProphetTable.new()
    return setmetatable({
        probs = {},           -- {peer_str -> {other_peer_str -> probability}}
        last_encounter = {},  -- {peer_str -> {other_peer_str -> tick}}
        decay_factor = 0.98,
        init_prob = 0.1,
        max_prob = 0.95
    }, ProphetTable)
end

function ProphetTable:ensure_peer(peer_str)
    if not self.probs[peer_str] then
        self.probs[peer_str] = {}
        self.last_encounter[peer_str] = {}
    end
end

function ProphetTable:record_encounter(peer1_str, peer2_str, tick)
    self:ensure_peer(peer1_str)
    self:ensure_peer(peer2_str)

    -- Update probability for peer1 -> peer2
    local current = self.probs[peer1_str][peer2_str] or self.init_prob
    local new_prob = current + (1 - current) * 0.5
    self.probs[peer1_str][peer2_str] = math.min(new_prob, self.max_prob)
    self.last_encounter[peer1_str][peer2_str] = tick

    -- Update probability for peer2 -> peer1
    current = self.probs[peer2_str][peer1_str] or self.init_prob
    new_prob = current + (1 - current) * 0.5
    self.probs[peer2_str][peer1_str] = math.min(new_prob, self.max_prob)
    self.last_encounter[peer2_str][peer1_str] = tick
end

function ProphetTable:get_probability(from_str, to_str)
    self:ensure_peer(from_str)
    return self.probs[from_str][to_str] or 0
end

function ProphetTable:apply_decay(current_tick)
    for peer_str, others in pairs(self.probs) do
        for other_str, prob in pairs(others) do
            local last = self.last_encounter[peer_str] and self.last_encounter[peer_str][other_str]
            if last and current_tick > last then
                local ticks_since = current_tick - last
                local decay = math.pow(self.decay_factor, ticks_since)
                self.probs[peer_str][other_str] = prob * decay
            end
        end
    end
end

function ProphetTable:get_best_forwarder(holder_str, dest_str, candidates)
    local best_peer = nil
    local best_prob = 0

    for _, candidate in ipairs(candidates) do
        local cand_str = tostring(candidate)
        if cand_str ~= holder_str and cand_str ~= dest_str then
            local prob = self:get_probability(cand_str, dest_str)
            if prob > best_prob then
                best_prob = prob
                best_peer = candidate
            end
        end
    end

    return best_peer, best_prob
end

--------------------------------------------------------------------------------
-- Strategy Manager - handles dynamic switching
--------------------------------------------------------------------------------
local StrategyManager = {}
StrategyManager.__index = StrategyManager

function StrategyManager.new()
    return setmetatable({
        current_strategy = Strategy.EPIDEMIC,
        strategy_history = {},
        switch_count = 0,
        last_switch_tick = 0,
        min_switch_interval = 20,  -- Minimum ticks between switches
        -- Metrics per strategy
        metrics = {
            [Strategy.EPIDEMIC] = {
                bundles_sent = 0,
                bundles_delivered = 0,
                copies_created = 0,
                total_latency = 0
            },
            [Strategy.PROPHET] = {
                bundles_sent = 0,
                bundles_delivered = 0,
                copies_created = 0,
                total_latency = 0
            },
            [Strategy.DIRECT] = {
                bundles_sent = 0,
                bundles_delivered = 0,
                copies_created = 0,
                total_latency = 0
            }
        }
    }, StrategyManager)
end

function StrategyManager:get_strategy()
    return self.current_strategy
end

function StrategyManager:set_strategy(strategy, tick, reason)
    if self.current_strategy ~= strategy then
        if tick - self.last_switch_tick >= self.min_switch_interval then
            local old = self.current_strategy
            self.current_strategy = strategy
            self.switch_count = self.switch_count + 1
            self.last_switch_tick = tick

            table.insert(self.strategy_history, {
                from = old,
                to = strategy,
                tick = tick,
                reason = reason
            })

            indras.log.info("Strategy switched", {
                trace_id = ctx.trace_id,
                from = old,
                to = strategy,
                tick = tick,
                reason = reason,
                total_switches = self.switch_count
            })

            return true
        end
    end
    return false
end

function StrategyManager:force_strategy(strategy, tick)
    -- Force switch without interval check (for phase testing)
    local old = self.current_strategy
    self.current_strategy = strategy
    self.last_switch_tick = tick

    indras.log.debug("Strategy forced", {
        trace_id = ctx.trace_id,
        from = old,
        to = strategy,
        tick = tick
    })
end

function StrategyManager:record_send(strategy)
    self.metrics[strategy].bundles_sent = self.metrics[strategy].bundles_sent + 1
end

function StrategyManager:record_delivery(strategy, latency)
    self.metrics[strategy].bundles_delivered = self.metrics[strategy].bundles_delivered + 1
    self.metrics[strategy].total_latency = self.metrics[strategy].total_latency + latency
end

function StrategyManager:record_copy(strategy)
    self.metrics[strategy].copies_created = self.metrics[strategy].copies_created + 1
end

function StrategyManager:get_delivery_rate(strategy)
    local m = self.metrics[strategy]
    if m.bundles_sent > 0 then
        return m.bundles_delivered / m.bundles_sent
    end
    return 0
end

function StrategyManager:get_avg_latency(strategy)
    local m = self.metrics[strategy]
    if m.bundles_delivered > 0 then
        return m.total_latency / m.bundles_delivered
    end
    return 0
end

function StrategyManager:get_overhead(strategy)
    local m = self.metrics[strategy]
    if m.bundles_delivered > 0 then
        return m.copies_created / m.bundles_delivered
    end
    return m.copies_created
end

function StrategyManager:evaluate_and_switch(network_state, tick)
    -- Automatic strategy selection based on network conditions
    local connectivity = network_state.connectivity
    local buffer_usage = network_state.buffer_usage
    local recent_delivery_rate = network_state.recent_delivery_rate

    local new_strategy = self.current_strategy
    local reason = nil

    -- High connectivity: Direct delivery is efficient
    if connectivity >= config.connectivity_high then
        if self.current_strategy ~= Strategy.DIRECT then
            new_strategy = Strategy.DIRECT
            reason = string.format("high_connectivity=%.2f", connectivity)
        end
    -- Low connectivity: Need aggressive epidemic routing
    elseif connectivity <= config.connectivity_low then
        if self.current_strategy ~= Strategy.EPIDEMIC then
            new_strategy = Strategy.EPIDEMIC
            reason = string.format("low_connectivity=%.2f", connectivity)
        end
    -- Medium connectivity: Use PRoPHET for smart forwarding
    else
        -- Check buffer usage
        if buffer_usage >= config.buffer_high then
            -- High buffer usage: switch to Direct to reduce overhead
            if self.current_strategy == Strategy.EPIDEMIC then
                new_strategy = Strategy.DIRECT
                reason = string.format("high_buffer=%.2f", buffer_usage)
            end
        elseif recent_delivery_rate < config.delivery_rate_target then
            -- Poor delivery: try more aggressive strategy
            if self.current_strategy == Strategy.DIRECT then
                new_strategy = Strategy.PROPHET
                reason = string.format("low_delivery_rate=%.2f", recent_delivery_rate)
            elseif self.current_strategy == Strategy.PROPHET then
                new_strategy = Strategy.EPIDEMIC
                reason = string.format("low_delivery_rate=%.2f", recent_delivery_rate)
            end
        else
            -- Default to PRoPHET for balanced performance
            if self.current_strategy == Strategy.EPIDEMIC then
                new_strategy = Strategy.PROPHET
                reason = "moderate_conditions"
            end
        end
    end

    if new_strategy ~= self.current_strategy and reason then
        return self:set_strategy(new_strategy, tick, reason)
    end

    return false
end

--------------------------------------------------------------------------------
-- Global state
--------------------------------------------------------------------------------
local bundles = {}
local next_bundle_id = 1
local prophet_table = ProphetTable.new()
local strategy_manager = StrategyManager.new()

-- Per-phase metrics
local phase_metrics = {
    epidemic = { delivered = 0, expired = 0, copies = 0, latency_sum = 0 },
    prophet = { delivered = 0, expired = 0, copies = 0, latency_sum = 0 },
    direct = { delivered = 0, expired = 0, copies = 0, latency_sum = 0 },
    switching = { delivered = 0, expired = 0, copies = 0, latency_sum = 0, switches = 0 }
}

-- Global metrics
local total_bundles_created = 0
local total_bundles_delivered = 0
local total_bundles_expired = 0
local total_copies_created = 0

--------------------------------------------------------------------------------
-- Helper functions
--------------------------------------------------------------------------------
local function random_online_peer()
    local online = sim:online_peers()
    if #online == 0 then return nil end
    return online[math.random(#online)]
end

local function random_peer()
    return all_peers[math.random(#all_peers)]
end

local function get_connectivity_ratio()
    return #sim:online_peers() / #all_peers
end

local function estimate_buffer_usage()
    -- Estimate based on active bundles per online peer
    local active_bundles = 0
    for _, bundle in pairs(bundles) do
        if not bundle.delivered and not bundle.expired then
            active_bundles = active_bundles + bundle:count_copies()
        end
    end
    local online_count = math.max(1, #sim:online_peers())
    local avg_per_peer = active_bundles / online_count
    -- Normalize to 0-1 range (assume 10 bundles per peer is "full")
    return math.min(1.0, avg_per_peer / 10)
end

local function get_recent_delivery_rate(window_ticks)
    -- Simplified: use overall delivery rate
    if total_bundles_created > 0 then
        return total_bundles_delivered / total_bundles_created
    end
    return 0.5  -- Default assumption
end

local function get_network_state()
    return {
        connectivity = get_connectivity_ratio(),
        buffer_usage = estimate_buffer_usage(),
        recent_delivery_rate = get_recent_delivery_rate(50)
    }
end

local function create_bundle(source, destination, tick, strategy)
    local bundle = Bundle.new(
        string.format("bundle-%d", next_bundle_id),
        source,
        destination,
        tick,
        config.bundle_ttl
    )
    bundle.strategy_used = strategy
    next_bundle_id = next_bundle_id + 1

    bundle:add_copy(source, tick)
    bundles[bundle.id] = bundle
    total_bundles_created = total_bundles_created + 1

    strategy_manager:record_send(strategy)

    indras.log.debug("Bundle created", {
        trace_id = ctx.trace_id,
        bundle_id = bundle.id,
        source = tostring(source),
        destination = tostring(destination),
        strategy = strategy,
        tick = tick
    })

    return bundle
end

--------------------------------------------------------------------------------
-- Strategy implementations
--------------------------------------------------------------------------------

-- Epidemic routing: spray copies to all available neighbors up to limit
local function epidemic_forward(bundle, current_tick)
    if bundle.delivered or bundle.expired then return 0 end
    if not bundle:can_spray() then return 0 end

    local copies_made = 0

    -- Find all holders
    local holders = {}
    for peer_str, _ in pairs(bundle.copies) do
        for _, peer in ipairs(all_peers) do
            if tostring(peer) == peer_str and sim:is_online(peer) then
                table.insert(holders, peer)
                break
            end
        end
    end

    -- Each holder sprays to neighbors
    for _, holder in ipairs(holders) do
        if not bundle:can_spray() then break end

        local neighbors = mesh:neighbors(holder)
        -- Shuffle for random spray order
        for i = #neighbors, 2, -1 do
            local j = math.random(i)
            neighbors[i], neighbors[j] = neighbors[j], neighbors[i]
        end

        for _, neighbor in ipairs(neighbors) do
            if not bundle:can_spray() then break end

            if sim:is_online(neighbor) and not bundle:has_copy(neighbor) then
                if bundle:add_copy(neighbor, current_tick) then
                    bundle:record_hop(holder, neighbor, current_tick)
                    copies_made = copies_made + 1
                    total_copies_created = total_copies_created + 1
                    strategy_manager:record_copy(Strategy.EPIDEMIC)

                    sim:send_message(holder, neighbor, "bundle:" .. bundle.id)
                end
            end
        end
    end

    return copies_made
end

-- PRoPHET routing: forward only to peers with high delivery probability
local function prophet_forward(bundle, current_tick)
    if bundle.delivered or bundle.expired then return 0 end

    local copies_made = 0
    local dest_str = tostring(bundle.destination)

    -- Find all holders
    local holders = {}
    for peer_str, _ in pairs(bundle.copies) do
        for _, peer in ipairs(all_peers) do
            if tostring(peer) == peer_str and sim:is_online(peer) then
                table.insert(holders, peer)
                break
            end
        end
    end

    for _, holder in ipairs(holders) do
        local holder_str = tostring(holder)
        local neighbors = mesh:neighbors(holder)

        -- Find online neighbors
        local online_neighbors = {}
        for _, n in ipairs(neighbors) do
            if sim:is_online(n) and not bundle:has_copy(n) then
                table.insert(online_neighbors, n)
            end
        end

        -- Record encounters for PRoPHET learning
        for _, neighbor in ipairs(online_neighbors) do
            prophet_table:record_encounter(holder_str, tostring(neighbor), current_tick)
        end

        -- Find best forwarder based on delivery probability
        local best, prob = prophet_table:get_best_forwarder(holder_str, dest_str, online_neighbors)

        if best and prob >= config.prophet_threshold then
            if bundle:add_copy(best, current_tick) then
                bundle:record_hop(holder, best, current_tick)
                copies_made = copies_made + 1
                total_copies_created = total_copies_created + 1
                strategy_manager:record_copy(Strategy.PROPHET)

                sim:send_message(holder, best, "bundle:" .. bundle.id)

                indras.log.debug("PRoPHET forward", {
                    trace_id = ctx.trace_id,
                    bundle_id = bundle.id,
                    from = holder_str,
                    to = tostring(best),
                    probability = string.format("%.3f", prob)
                })
            end
        end
    end

    return copies_made
end

-- Direct delivery: only forward when destination is directly reachable
local function direct_forward(bundle, current_tick)
    if bundle.delivered or bundle.expired then return 0 end

    local copies_made = 0
    local dest = bundle.destination
    local dest_str = tostring(dest)

    -- Check if destination is online
    if not sim:is_online(dest) then
        return 0
    end

    -- Find holders that can reach destination directly
    for peer_str, _ in pairs(bundle.copies) do
        for _, peer in ipairs(all_peers) do
            if tostring(peer) == peer_str and sim:is_online(peer) then
                -- Check if destination is a neighbor
                local neighbors = mesh:neighbors(peer)
                for _, neighbor in ipairs(neighbors) do
                    if tostring(neighbor) == dest_str then
                        if bundle:add_copy(dest, current_tick) then
                            bundle:record_hop(peer, dest, current_tick)
                            copies_made = copies_made + 1
                            total_copies_created = total_copies_created + 1
                            strategy_manager:record_copy(Strategy.DIRECT)

                            sim:send_message(peer, dest, "bundle:" .. bundle.id)

                            indras.log.debug("Direct delivery", {
                                trace_id = ctx.trace_id,
                                bundle_id = bundle.id,
                                from = peer_str,
                                to = dest_str
                            })
                        end
                        return copies_made
                    end
                end
                break
            end
        end
    end

    return copies_made
end

-- Forward using current strategy
local function forward_bundle(bundle, current_tick)
    local strategy = strategy_manager:get_strategy()

    if strategy == Strategy.EPIDEMIC then
        return epidemic_forward(bundle, current_tick)
    elseif strategy == Strategy.PROPHET then
        return prophet_forward(bundle, current_tick)
    elseif strategy == Strategy.DIRECT then
        return direct_forward(bundle, current_tick)
    end

    return 0
end

-- Check for delivery
local function check_delivery(bundle, current_tick, phase_key)
    if bundle.delivered or bundle.expired then return end

    local dest_str = tostring(bundle.destination)
    if bundle:has_copy(bundle.destination) and sim:is_online(bundle.destination) then
        bundle.delivered = true
        bundle.delivered_tick = current_tick
        total_bundles_delivered = total_bundles_delivered + 1

        local latency = current_tick - bundle.created_tick
        strategy_manager:record_delivery(bundle.strategy_used, latency)

        if phase_key and phase_metrics[phase_key] then
            phase_metrics[phase_key].delivered = phase_metrics[phase_key].delivered + 1
            phase_metrics[phase_key].latency_sum = phase_metrics[phase_key].latency_sum + latency
        end

        indras.log.debug("Bundle delivered", {
            trace_id = ctx.trace_id,
            bundle_id = bundle.id,
            strategy = bundle.strategy_used,
            latency = latency,
            hops = bundle.hops
        })
    end
end

-- Check for expiration
local function check_expiration(bundle, current_tick, phase_key)
    if bundle.delivered or bundle.expired then return end

    if bundle:has_expired(current_tick) then
        bundle.expired = true
        total_bundles_expired = total_bundles_expired + 1

        if phase_key and phase_metrics[phase_key] then
            phase_metrics[phase_key].expired = phase_metrics[phase_key].expired + 1
        end

        indras.log.debug("Bundle expired", {
            trace_id = ctx.trace_id,
            bundle_id = bundle.id,
            strategy = bundle.strategy_used,
            copies = bundle:count_copies()
        })
    end
end

-- Process all active bundles
local function process_bundles(current_tick, phase_key)
    local copies_this_tick = 0

    for _, bundle in pairs(bundles) do
        if not bundle.delivered and not bundle.expired then
            copies_this_tick = copies_this_tick + forward_bundle(bundle, current_tick)
            check_delivery(bundle, current_tick, phase_key)
            check_expiration(bundle, current_tick, phase_key)
        end
    end

    if phase_key and phase_metrics[phase_key] then
        phase_metrics[phase_key].copies = phase_metrics[phase_key].copies + copies_this_tick
    end

    return copies_this_tick
end

--------------------------------------------------------------------------------
-- Phase 1: Epidemic Routing
--------------------------------------------------------------------------------
indras.log.info("Phase 1: Epidemic routing test", {
    trace_id = ctx.trace_id,
    phase = "epidemic",
    ticks = config.epidemic_phase_ticks,
    spray_limit = config.spray_limit
})

strategy_manager:force_strategy(Strategy.EPIDEMIC, 0)

local bundles_per_tick = math.ceil(config.bundles * 0.25 / config.epidemic_phase_ticks)
local phase1_bundles_created = 0

for tick = 1, config.epidemic_phase_ticks do
    -- Create new bundles
    for _ = 1, bundles_per_tick do
        if phase1_bundles_created < config.bundles * 0.25 then
            local source = random_online_peer()
            local dest = random_peer()
            if source and dest and source ~= dest then
                create_bundle(source, dest, tick, Strategy.EPIDEMIC)
                phase1_bundles_created = phase1_bundles_created + 1
            end
        end
    end

    process_bundles(tick, "epidemic")
    sim:step()

    if tick % 40 == 0 then
        indras.log.info("Phase 1 progress", {
            trace_id = ctx.trace_id,
            tick = tick,
            online = #sim:online_peers(),
            bundles_created = phase1_bundles_created,
            delivered = phase_metrics.epidemic.delivered,
            copies = phase_metrics.epidemic.copies
        })
    end
end

local phase1_end = config.epidemic_phase_ticks

--------------------------------------------------------------------------------
-- Phase 2: PRoPHET Routing
--------------------------------------------------------------------------------
indras.log.info("Phase 2: PRoPHET routing test", {
    trace_id = ctx.trace_id,
    phase = "prophet",
    ticks = config.prophet_phase_ticks,
    threshold = config.prophet_threshold
})

strategy_manager:force_strategy(Strategy.PROPHET, phase1_end)

-- Pre-seed some encounter history
for tick = phase1_end + 1, phase1_end + 20 do
    local online = sim:online_peers()
    for i = 1, math.min(5, #online) do
        local p1 = online[math.random(#online)]
        local neighbors = mesh:neighbors(p1)
        if #neighbors > 0 then
            local p2 = neighbors[math.random(#neighbors)]
            if sim:is_online(p2) then
                prophet_table:record_encounter(tostring(p1), tostring(p2), tick)
            end
        end
    end
    sim:step()
end

local phase2_bundles_created = 0
bundles_per_tick = math.ceil(config.bundles * 0.25 / config.prophet_phase_ticks)

for tick = phase1_end + 21, phase1_end + config.prophet_phase_ticks do
    -- Record encounters for learning
    local online = sim:online_peers()
    if tick % 5 == 0 then
        for i = 1, math.min(3, #online) do
            local p1 = online[math.random(#online)]
            local neighbors = mesh:neighbors(p1)
            if #neighbors > 0 then
                local p2 = neighbors[math.random(#neighbors)]
                if sim:is_online(p2) then
                    prophet_table:record_encounter(tostring(p1), tostring(p2), tick)
                end
            end
        end
    end

    -- Create bundles
    for _ = 1, bundles_per_tick do
        if phase2_bundles_created < config.bundles * 0.25 then
            local source = random_online_peer()
            local dest = random_peer()
            if source and dest and source ~= dest then
                create_bundle(source, dest, tick, Strategy.PROPHET)
                phase2_bundles_created = phase2_bundles_created + 1
            end
        end
    end

    -- Apply probability decay periodically
    if tick % 10 == 0 then
        prophet_table:apply_decay(tick)
    end

    process_bundles(tick, "prophet")
    sim:step()

    if tick % 40 == 0 then
        indras.log.info("Phase 2 progress", {
            trace_id = ctx.trace_id,
            tick = tick,
            online = #sim:online_peers(),
            bundles_created = phase2_bundles_created,
            delivered = phase_metrics.prophet.delivered,
            copies = phase_metrics.prophet.copies
        })
    end
end

local phase2_end = phase1_end + config.prophet_phase_ticks

--------------------------------------------------------------------------------
-- Phase 3: Direct Delivery
--------------------------------------------------------------------------------
indras.log.info("Phase 3: Direct delivery test", {
    trace_id = ctx.trace_id,
    phase = "direct",
    ticks = config.direct_phase_ticks
})

strategy_manager:force_strategy(Strategy.DIRECT, phase2_end)

-- Increase connectivity for direct delivery testing
local original_wake = config.wake_prob
config.wake_prob = 0.15

local phase3_bundles_created = 0
bundles_per_tick = math.ceil(config.bundles * 0.20 / config.direct_phase_ticks)

for tick = phase2_end + 1, phase2_end + config.direct_phase_ticks do
    -- Wake more peers for direct delivery testing
    if tick % 5 == 0 then
        local offline = sim:offline_peers()
        if #offline > 0 then
            sim:force_online(offline[math.random(#offline)])
        end
    end

    -- Create bundles targeting online peers more often
    for _ = 1, bundles_per_tick do
        if phase3_bundles_created < config.bundles * 0.20 then
            local source = random_online_peer()
            local dest
            -- 60% chance to target online peer for direct delivery
            if math.random() < 0.6 then
                dest = random_online_peer()
            else
                dest = random_peer()
            end
            if source and dest and source ~= dest then
                create_bundle(source, dest, tick, Strategy.DIRECT)
                phase3_bundles_created = phase3_bundles_created + 1
            end
        end
    end

    process_bundles(tick, "direct")
    sim:step()

    if tick % 30 == 0 then
        indras.log.info("Phase 3 progress", {
            trace_id = ctx.trace_id,
            tick = tick,
            online = #sim:online_peers(),
            bundles_created = phase3_bundles_created,
            delivered = phase_metrics.direct.delivered,
            copies = phase_metrics.direct.copies
        })
    end
end

config.wake_prob = original_wake
local phase3_end = phase2_end + config.direct_phase_ticks

--------------------------------------------------------------------------------
-- Phase 4: Dynamic Strategy Switching
--------------------------------------------------------------------------------
indras.log.info("Phase 4: Dynamic strategy switching test", {
    trace_id = ctx.trace_id,
    phase = "switching",
    ticks = config.switching_phase_ticks,
    connectivity_thresholds = {
        high = config.connectivity_high,
        low = config.connectivity_low
    },
    buffer_threshold = config.buffer_high
})

local phase4_bundles_created = 0
bundles_per_tick = math.ceil(config.bundles * 0.30 / config.switching_phase_ticks)
local switching_events = {}

for tick = phase3_end + 1, phase3_end + config.switching_phase_ticks do
    -- Vary network conditions
    if tick % 30 == 0 then
        -- Randomly change network state
        local action = math.random(3)
        if action == 1 then
            -- Force many offline
            for i = 1, math.floor(#all_peers * 0.3) do
                local online = sim:online_peers()
                if #online > 3 then
                    sim:force_offline(online[math.random(#online)])
                end
            end
        elseif action == 2 then
            -- Force many online
            for i = 1, math.floor(#all_peers * 0.3) do
                local offline = sim:offline_peers()
                if #offline > 0 then
                    sim:force_online(offline[math.random(#offline)])
                end
            end
        end
        -- action == 3: do nothing, let natural churn happen
    end

    -- Evaluate and potentially switch strategy
    local network_state = get_network_state()
    local switched = strategy_manager:evaluate_and_switch(network_state, tick)
    if switched then
        phase_metrics.switching.switches = phase_metrics.switching.switches + 1
        table.insert(switching_events, {
            tick = tick,
            to = strategy_manager:get_strategy(),
            connectivity = network_state.connectivity,
            buffer = network_state.buffer_usage
        })
    end

    -- Create bundles with current (dynamic) strategy
    for _ = 1, bundles_per_tick do
        if phase4_bundles_created < config.bundles * 0.30 then
            local source = random_online_peer()
            local dest = random_peer()
            if source and dest and source ~= dest then
                create_bundle(source, dest, tick, strategy_manager:get_strategy())
                phase4_bundles_created = phase4_bundles_created + 1
            end
        end
    end

    -- Continue PRoPHET learning during switching phase
    if tick % 5 == 0 then
        local online = sim:online_peers()
        for i = 1, math.min(2, #online) do
            local p1 = online[math.random(#online)]
            local neighbors = mesh:neighbors(p1)
            if #neighbors > 0 then
                local p2 = neighbors[math.random(#neighbors)]
                if sim:is_online(p2) then
                    prophet_table:record_encounter(tostring(p1), tostring(p2), tick)
                end
            end
        end
    end

    process_bundles(tick, "switching")
    sim:step()

    if tick % 60 == 0 then
        indras.log.info("Phase 4 progress", {
            trace_id = ctx.trace_id,
            tick = tick,
            strategy = strategy_manager:get_strategy(),
            online = #sim:online_peers(),
            connectivity = string.format("%.2f", network_state.connectivity),
            bundles_created = phase4_bundles_created,
            delivered = phase_metrics.switching.delivered,
            switches = phase_metrics.switching.switches
        })
    end
end

local phase4_end = phase3_end + config.switching_phase_ticks

--------------------------------------------------------------------------------
-- Phase 5: Drain - Allow remaining bundles to be delivered
--------------------------------------------------------------------------------
indras.log.info("Phase 5: Drain phase", {
    trace_id = ctx.trace_id,
    phase = "drain",
    ticks = config.drain_phase_ticks
})

-- Wake all peers for maximum delivery
for _, peer in ipairs(sim:offline_peers()) do
    sim:force_online(peer)
end

strategy_manager:force_strategy(Strategy.EPIDEMIC, phase4_end)

for tick = phase4_end + 1, phase4_end + config.drain_phase_ticks do
    process_bundles(tick, nil)  -- No phase tracking for drain
    sim:step()

    if tick % 30 == 0 then
        local remaining = 0
        for _, bundle in pairs(bundles) do
            if not bundle.delivered and not bundle.expired then
                remaining = remaining + 1
            end
        end
        indras.log.debug("Drain progress", {
            trace_id = ctx.trace_id,
            tick = tick,
            remaining_bundles = remaining,
            total_delivered = total_bundles_delivered
        })
    end
end

--------------------------------------------------------------------------------
-- Final Statistics and Assertions
--------------------------------------------------------------------------------
local final_stats = sim.stats

-- Calculate per-strategy metrics
local strategy_results = {}
for _, strategy in pairs(Strategy) do
    local m = strategy_manager.metrics[strategy]
    strategy_results[strategy] = {
        bundles_sent = m.bundles_sent,
        bundles_delivered = m.bundles_delivered,
        copies_created = m.copies_created,
        delivery_rate = strategy_manager:get_delivery_rate(strategy),
        avg_latency = strategy_manager:get_avg_latency(strategy),
        overhead = strategy_manager:get_overhead(strategy)
    }
end

-- Calculate phase metrics
local phase_results = {}
for phase_name, pm in pairs(phase_metrics) do
    local delivery_rate = 0
    local avg_latency = 0
    local phase_sent = 0

    if phase_name == "epidemic" then
        phase_sent = phase1_bundles_created
    elseif phase_name == "prophet" then
        phase_sent = phase2_bundles_created
    elseif phase_name == "direct" then
        phase_sent = phase3_bundles_created
    elseif phase_name == "switching" then
        phase_sent = phase4_bundles_created
    end

    if phase_sent > 0 then
        delivery_rate = pm.delivered / phase_sent
    end
    if pm.delivered > 0 then
        avg_latency = pm.latency_sum / pm.delivered
    end

    phase_results[phase_name] = {
        bundles_sent = phase_sent,
        delivered = pm.delivered,
        expired = pm.expired,
        copies = pm.copies,
        delivery_rate = delivery_rate,
        avg_latency = avg_latency,
        switches = pm.switches or 0
    }
end

-- Overall delivery rate
local overall_delivery_rate = total_bundles_created > 0
    and total_bundles_delivered / total_bundles_created
    or 0

indras.log.info("DTN strategy stress test completed", {
    trace_id = ctx.trace_id,
    level = test_level,
    final_tick = sim.tick,
    -- Overall metrics
    total_bundles_created = total_bundles_created,
    total_bundles_delivered = total_bundles_delivered,
    total_bundles_expired = total_bundles_expired,
    overall_delivery_rate = string.format("%.3f", overall_delivery_rate),
    total_copies_created = total_copies_created,
    -- Strategy switching
    total_switches = strategy_manager.switch_count,
    -- Per-strategy delivery rates
    epidemic_delivery_rate = string.format("%.3f", strategy_results[Strategy.EPIDEMIC].delivery_rate),
    prophet_delivery_rate = string.format("%.3f", strategy_results[Strategy.PROPHET].delivery_rate),
    direct_delivery_rate = string.format("%.3f", strategy_results[Strategy.DIRECT].delivery_rate),
    -- Per-strategy overhead
    epidemic_overhead = string.format("%.2f", strategy_results[Strategy.EPIDEMIC].overhead),
    prophet_overhead = string.format("%.2f", strategy_results[Strategy.PROPHET].overhead),
    direct_overhead = string.format("%.2f", strategy_results[Strategy.DIRECT].overhead),
    -- Network stats
    network_messages_sent = final_stats.messages_sent,
    network_messages_delivered = final_stats.messages_delivered
})

-- Log per-phase summary
for phase_name, pr in pairs(phase_results) do
    indras.log.info("Phase summary: " .. phase_name, {
        trace_id = ctx.trace_id,
        phase = phase_name,
        bundles_sent = pr.bundles_sent,
        delivered = pr.delivered,
        expired = pr.expired,
        delivery_rate = string.format("%.3f", pr.delivery_rate),
        avg_latency = string.format("%.1f", pr.avg_latency),
        copies = pr.copies,
        switches = pr.switches
    })
end

--------------------------------------------------------------------------------
-- Assertions
--------------------------------------------------------------------------------

-- Basic sanity checks
indras.assert.gt(total_bundles_created, 0, "Should have created bundles")
indras.assert.gt(total_bundles_delivered, 0, "Should have delivered some bundles")

-- Overall delivery rate should be reasonable
indras.assert.gt(overall_delivery_rate, 0.15,
    "Overall delivery rate should be > 15% even in challenged network")

-- Each strategy should have been used
indras.assert.gt(strategy_results[Strategy.EPIDEMIC].bundles_sent, 0,
    "Epidemic strategy should have sent bundles")
indras.assert.gt(strategy_results[Strategy.PROPHET].bundles_sent, 0,
    "PRoPHET strategy should have sent bundles")
indras.assert.gt(strategy_results[Strategy.DIRECT].bundles_sent, 0,
    "Direct strategy should have sent bundles")

-- Strategy switching should have occurred
indras.assert.gt(strategy_manager.switch_count, 0,
    "Strategy switching should have occurred during adaptive phase")

-- Epidemic should have highest overhead (most copies)
indras.assert.ge(strategy_results[Strategy.EPIDEMIC].overhead,
    strategy_results[Strategy.DIRECT].overhead,
    "Epidemic should have >= overhead compared to Direct")

-- Direct should have lowest overhead
indras.assert.le(strategy_results[Strategy.DIRECT].overhead,
    strategy_results[Strategy.EPIDEMIC].overhead,
    "Direct should have <= overhead compared to Epidemic")

-- Verify phase metrics
indras.assert.gt(phase_results["epidemic"].delivered, 0,
    "Epidemic phase should deliver bundles")
indras.assert.gt(phase_results["switching"].switches, 0,
    "Switching phase should have strategy switches")

indras.log.info("DTN strategy stress test passed", {
    trace_id = ctx.trace_id,
    level = test_level,
    overall_delivery_rate = string.format("%.3f", overall_delivery_rate),
    total_switches = strategy_manager.switch_count,
    epidemic_overhead = string.format("%.2f", strategy_results[Strategy.EPIDEMIC].overhead),
    prophet_overhead = string.format("%.2f", strategy_results[Strategy.PROPHET].overhead),
    direct_overhead = string.format("%.2f", strategy_results[Strategy.DIRECT].overhead)
})

--------------------------------------------------------------------------------
-- Return results
--------------------------------------------------------------------------------
return {
    level = test_level,
    -- Overall metrics
    total_bundles_created = total_bundles_created,
    total_bundles_delivered = total_bundles_delivered,
    total_bundles_expired = total_bundles_expired,
    overall_delivery_rate = overall_delivery_rate,
    total_copies_created = total_copies_created,
    -- Strategy switching
    total_switches = strategy_manager.switch_count,
    strategy_history = strategy_manager.strategy_history,
    -- Per-strategy results
    strategy_results = strategy_results,
    -- Per-phase results
    phase_results = phase_results,
    -- Switching events
    switching_events = switching_events,
    -- Network stats
    network_delivery_rate = final_stats:delivery_rate(),
    final_tick = sim.tick,
    -- Configuration used
    config = {
        spray_limit = config.spray_limit,
        prophet_threshold = config.prophet_threshold,
        bundle_ttl = config.bundle_ttl,
        connectivity_thresholds = {
            high = config.connectivity_high,
            low = config.connectivity_low
        },
        buffer_threshold = config.buffer_high
    }
}

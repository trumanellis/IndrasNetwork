-- DTN (Delay-Tolerant Networking) Stress Test
--
-- Stress tests delay-tolerant networking with bundle protocol, epidemic routing,
-- and custody transfer. Simulates challenged networks with high offline rates,
-- intermittent connectivity, and message relaying.
--
-- DTN characteristics:
-- - High offline/challenged connectivity
-- - Bundle messages with TTL (time-to-live)
-- - Custody transfer (relay nodes hold messages)
-- - Epidemic routing (spray copies to available peers)

local ctx = indras.correlation.new_root()
ctx = ctx:with_tag("scenario", "dtn_stress")

-- Configuration levels
local CONFIG = {
    quick = {
        peers = 10,
        bundles = 50,
        ticks = 200,
        offline_rate = 0.80,  -- 80% offline: very challenged network
        bundle_ttl = 100,      -- ticks
        spray_factor = 3,      -- copies per bundle
        wake_prob = 0.05,      -- low wake rate
        sleep_prob = 0.15      -- high sleep rate
    },
    medium = {
        peers = 20,
        bundles = 500,
        ticks = 500,
        offline_rate = 0.90,  -- 90% offline: extremely challenged
        bundle_ttl = 250,
        spray_factor = 5,
        wake_prob = 0.03,
        sleep_prob = 0.20
    },
    full = {
        peers = 26,
        bundles = 5000,
        ticks = 2000,
        offline_rate = 0.95,  -- 95% offline: near-total disruption
        bundle_ttl = 1000,
        spray_factor = 7,
        wake_prob = 0.02,
        sleep_prob = 0.25
    }
}

-- Select test level (default to quick if not specified)
local test_level = os.getenv("STRESS_LEVEL") or "quick"
local config = CONFIG[test_level]

if not config then
    error("Invalid test level: " .. tostring(test_level))
end

indras.log.info("Starting DTN stress test", {
    trace_id = ctx.trace_id,
    level = test_level,
    peers = config.peers,
    bundles = config.bundles,
    ticks = config.ticks,
    offline_rate = config.offline_rate,
    bundle_ttl = config.bundle_ttl,
    spray_factor = config.spray_factor
})

-- Create sparse mesh topology (challenged network)
local mesh = indras.MeshBuilder.new(config.peers):random(0.2)

indras.log.debug("Created sparse mesh", {
    trace_id = ctx.trace_id,
    peers = mesh:peer_count(),
    edges = mesh:edge_count(),
    avg_connectivity = mesh:edge_count() / mesh:peer_count()
})

-- Create simulation with extreme offline conditions
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

-- DTN Bundle tracking
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
        copies = {},  -- peers holding copies
        custody_chain = {}  -- custody transfer chain
    }, Bundle)
end

function Bundle:add_copy(peer, tick)
    if not self.copies[tostring(peer)] then
        self.copies[tostring(peer)] = tick
    end
end

function Bundle:transfer_custody(from_peer, to_peer, tick)
    table.insert(self.custody_chain, {
        from = from_peer and tostring(from_peer) or "origin",
        to = tostring(to_peer),
        tick = tick
    })
end

function Bundle:copy_count()
    local count = 0
    for _ in pairs(self.copies) do
        count = count + 1
    end
    return count
end

function Bundle:has_expired(current_tick)
    return current_tick >= self.expires_tick
end

-- Bundle registry
local bundles = {}
local next_bundle_id = 1

-- DTN metrics
local bundles_created = 0
local bundles_delivered = 0
local bundles_expired = 0
local custody_transfers = 0
local spray_copies_total = 0

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
    local bundle = Bundle.new(
        string.format("bundle-%d", next_bundle_id),
        source,
        destination,
        tick,
        config.bundle_ttl
    )
    next_bundle_id = next_bundle_id + 1

    bundle:add_copy(source, tick)
    bundles[bundle.id] = bundle
    bundles_created = bundles_created + 1

    indras.log.debug("Bundle created", {
        trace_id = ctx.trace_id,
        bundle_id = bundle.id,
        source = tostring(source),
        destination = tostring(destination),
        ttl = config.bundle_ttl,
        expires_tick = bundle.expires_tick
    })

    return bundle
end

local function epidemic_spray(bundle, current_tick)
    local source_str = tostring(bundle.source)

    -- Find peers with copies
    local holders = {}
    for peer_str, _ in pairs(bundle.copies) do
        for _, peer in ipairs(all_peers) do
            if tostring(peer) == peer_str then
                table.insert(holders, peer)
                break
            end
        end
    end

    -- Each holder sprays to online neighbors
    for _, holder in ipairs(holders) do
        if sim:is_online(holder) then
            local neighbors = mesh:neighbors(holder)
            local sprayed = 0

            -- Shuffle neighbors for random spray
            for i = #neighbors, 2, -1 do
                local j = math.random(i)
                neighbors[i], neighbors[j] = neighbors[j], neighbors[i]
            end

            for _, neighbor in ipairs(neighbors) do
                if sprayed >= config.spray_factor then
                    break
                end

                if sim:is_online(neighbor) and not bundle.copies[tostring(neighbor)] then
                    -- Spray copy to neighbor
                    bundle:add_copy(neighbor, current_tick)
                    bundle:transfer_custody(holder, neighbor, current_tick)
                    custody_transfers = custody_transfers + 1
                    spray_copies_total = spray_copies_total + 1
                    sprayed = sprayed + 1

                    -- Send network message to simulate transfer
                    sim:send_message(holder, neighbor, "bundle_transfer")
                end
            end
        end
    end
end

local function attempt_delivery(bundle, current_tick)
    if bundle.delivered or bundle.expired then
        return
    end

    -- Check if destination has a copy
    local dest_str = tostring(bundle.destination)
    if bundle.copies[dest_str] and sim:is_online(bundle.destination) then
        bundle.delivered = true
        bundle.delivered_tick = current_tick
        bundles_delivered = bundles_delivered + 1

        local latency = current_tick - bundle.created_tick

        indras.log.info("Bundle delivered", {
            trace_id = ctx.trace_id,
            bundle_id = bundle.id,
            source = tostring(bundle.source),
            destination = dest_str,
            latency_ticks = latency,
            custody_transfers = #bundle.custody_chain,
            copies_made = bundle:copy_count()
        })
    end
end

local function expire_bundles(current_tick)
    for _, bundle in pairs(bundles) do
        if not bundle.expired and not bundle.delivered and bundle:has_expired(current_tick) then
            bundle.expired = true
            bundles_expired = bundles_expired + 1

            indras.log.warn("Bundle expired", {
                trace_id = ctx.trace_id,
                bundle_id = bundle.id,
                source = tostring(bundle.source),
                destination = tostring(bundle.destination),
                copies_made = bundle:copy_count(),
                custody_transfers = #bundle.custody_chain
            })
        end
    end
end

-- Phase 1: Create bundles in challenged network
indras.log.info("Phase 1: Creating bundles in challenged network", {
    trace_id = ctx.trace_id,
    phase_ticks = math.floor(config.ticks * 0.3)
})

local phase1_end = math.floor(config.ticks * 0.3)
local bundles_per_tick = math.ceil(config.bundles / phase1_end)

for tick = 1, phase1_end do
    -- Create new bundles
    for _ = 1, bundles_per_tick do
        if bundles_created < config.bundles then
            local source = random_online_peer()
            local destination = random_peer()

            if source and destination and source ~= destination then
                create_bundle(source, destination, tick)
            end
        end
    end

    -- Epidemic spray for existing bundles
    for _, bundle in pairs(bundles) do
        if not bundle.delivered and not bundle.expired then
            epidemic_spray(bundle, tick)
            attempt_delivery(bundle, tick)
        end
    end

    expire_bundles(tick)
    sim:step()

    -- Progress logging
    if tick % 50 == 0 then
        indras.log.info("Phase 1 progress", {
            trace_id = ctx.trace_id,
            tick = tick,
            online_count = #sim:online_peers(),
            bundles_created = bundles_created,
            bundles_delivered = bundles_delivered,
            bundles_expired = bundles_expired,
            custody_transfers = custody_transfers
        })
    end
end

-- Phase 2: Continue with high offline churn
indras.log.info("Phase 2: High offline churn", {
    trace_id = ctx.trace_id,
    phase_ticks = math.floor(config.ticks * 0.4)
})

local phase2_end = phase1_end + math.floor(config.ticks * 0.4)

for tick = phase1_end + 1, phase2_end do
    -- Epidemic spray and delivery attempts
    for _, bundle in pairs(bundles) do
        if not bundle.delivered and not bundle.expired then
            epidemic_spray(bundle, tick)
            attempt_delivery(bundle, tick)
        end
    end

    expire_bundles(tick)
    sim:step()

    if tick % 50 == 0 then
        indras.log.info("Phase 2 progress", {
            trace_id = ctx.trace_id,
            tick = tick,
            online_count = #sim:online_peers(),
            bundles_delivered = bundles_delivered,
            bundles_expired = bundles_expired,
            custody_transfers = custody_transfers
        })
    end
end

-- Phase 3: Gradually wake peers to enable delivery
indras.log.info("Phase 3: Gradual peer wake-up", {
    trace_id = ctx.trace_id,
    phase_ticks = config.ticks - phase2_end
})

-- Increase wake probability for final phase
local original_wake_prob = config.wake_prob
config.wake_prob = 0.15  -- Higher wake rate

for tick = phase2_end + 1, config.ticks do
    -- Force some offline peers online to enable delivery
    if tick % 10 == 0 then
        local offline = sim:offline_peers()
        if #offline > 0 then
            local wake_count = math.min(3, #offline)
            for i = 1, wake_count do
                sim:force_online(offline[i])
            end
        end
    end

    -- Continue epidemic spray and delivery
    for _, bundle in pairs(bundles) do
        if not bundle.delivered and not bundle.expired then
            epidemic_spray(bundle, tick)
            attempt_delivery(bundle, tick)
        end
    end

    expire_bundles(tick)
    sim:step()

    if tick % 50 == 0 then
        indras.log.info("Phase 3 progress", {
            trace_id = ctx.trace_id,
            tick = tick,
            online_count = #sim:online_peers(),
            bundles_delivered = bundles_delivered,
            bundles_expired = bundles_expired,
            custody_transfers = custody_transfers
        })
    end
end

-- Calculate final metrics
local bundle_delivery_rate = bundles_created > 0 and (bundles_delivered / bundles_created) or 0
local custody_success_rate = custody_transfers > 0 and (bundles_delivered / custody_transfers) or 0
local spray_copies_avg = bundles_created > 0 and (spray_copies_total / bundles_created) or 0

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

indras.log.info("DTN stress test completed", {
    trace_id = ctx.trace_id,
    test_level = test_level,
    final_tick = sim.tick,
    -- DTN bundle metrics
    bundles_created = bundles_created,
    bundles_delivered = bundles_delivered,
    bundles_expired = bundles_expired,
    bundle_delivery_rate = bundle_delivery_rate,
    -- Custody metrics
    custody_transfers = custody_transfers,
    custody_success_rate = custody_success_rate,
    -- Spray metrics
    spray_copies_total = spray_copies_total,
    spray_copies_avg = spray_copies_avg,
    -- Latency metrics
    avg_delivery_latency = avg_delivery_latency,
    -- Network metrics
    messages_sent = stats.messages_sent,
    messages_delivered = stats.messages_delivered,
    messages_dropped = stats.messages_dropped,
    network_delivery_rate = stats:delivery_rate()
})

-- Assertions
indras.assert.eq(bundles_created, config.bundles, "Should create all configured bundles")
indras.assert.gt(bundles_delivered, 0, "Should deliver at least some bundles")
indras.assert.gt(custody_transfers, 0, "Should have custody transfers")

-- Delivery rate should be reasonable given high offline rate
-- Even in extreme DTN scenarios, epidemic routing should achieve some delivery
indras.assert.gt(bundle_delivery_rate, 0.1, "Bundle delivery rate should be > 10%")

-- Spray factor should result in multiple copies
indras.assert.gt(spray_copies_avg, 1.0, "Should create multiple copies per bundle")

indras.log.info("DTN stress test passed", {
    trace_id = ctx.trace_id,
    bundle_delivery_rate = bundle_delivery_rate,
    avg_delivery_latency = avg_delivery_latency,
    spray_copies_avg = spray_copies_avg
})

return {
    bundles_created = bundles_created,
    bundles_delivered = bundles_delivered,
    bundle_delivery_rate = bundle_delivery_rate,
    bundles_expired = bundles_expired,
    custody_transfers = custody_transfers,
    custody_success_rate = custody_success_rate,
    bundle_expirations = bundles_expired,
    avg_delivery_latency = avg_delivery_latency,
    spray_copies_avg = spray_copies_avg,
    network_delivery_rate = stats:delivery_rate(),
    total_ticks = sim.tick
}

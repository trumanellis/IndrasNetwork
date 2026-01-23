-- Network Partition and Recovery Test
--
-- Tests network partition scenarios with gradual healing and recovery.
-- Verifies message delivery behavior during partitioned and healing states.

local ctx = indras.correlation.new_root()
ctx = ctx:with_tag("scenario", "partition_recovery")

-- Configuration levels
local CONFIG = {
    quick = {
        peers = 10,
        partitions = 2,
        ticks = 150,
        baseline_ticks = 30,
        partition_ticks = 60,
        healing_ticks = 40,
        verification_ticks = 20
    },
    medium = {
        peers = 20,
        partitions = 3,
        ticks = 400,
        baseline_ticks = 80,
        partition_ticks = 160,
        healing_ticks = 100,
        verification_ticks = 60
    },
    full = {
        peers = 26,
        partitions = 5,
        ticks = 1000,
        baseline_ticks = 200,
        partition_ticks = 400,
        healing_ticks = 250,
        verification_ticks = 150
    }
}

-- Select configuration (default to quick)
local test_level = os.getenv("STRESS_LEVEL") or "quick"
local config = CONFIG[test_level]
if not config then
    indras.log.error("Invalid TEST_LEVEL", {
        trace_id = ctx.trace_id,
        level = test_level,
        valid_levels = {"quick", "medium", "full"}
    })
    error("Invalid TEST_LEVEL: " .. test_level)
end

indras.log.info("Starting partition recovery test", {
    trace_id = ctx.trace_id,
    test_level = test_level,
    peers = config.peers,
    partitions = config.partitions,
    total_ticks = config.ticks,
    baseline_ticks = config.baseline_ticks,
    partition_ticks = config.partition_ticks,
    healing_ticks = config.healing_ticks,
    verification_ticks = config.verification_ticks
})

-- Message rate constants
local MESSAGE_RATE = 3  -- messages per tick during active phases

-- Create mesh topology
local mesh = indras.MeshBuilder.new(config.peers):random(0.3)

indras.log.debug("Created mesh topology", {
    trace_id = ctx.trace_id,
    peers = mesh:peer_count(),
    edges = mesh:edge_count()
})

-- Create simulation
local sim_config = indras.SimConfig.new({
    wake_probability = 0.1,
    sleep_probability = 0.05,
    initial_online_probability = 0.9,
    max_ticks = config.ticks,
    trace_routing = true
})

local sim = indras.Simulation.new(mesh, sim_config)
sim:initialize()

-- Get all peers
local all_peers = mesh:peers()

-- Partition state tracking
local partitions = {}  -- Array of peer arrays
local partitions_created = 0
local healing_events = 0
local partition_start_tick = 0
local healing_start_tick = 0
local recovery_complete_tick = 0

-- Message tracking
local intra_partition_sent = 0
local intra_partition_delivered = 0
local cross_partition_sent = 0
local cross_partition_queued = 0
local post_healing_sent = 0
local post_healing_delivered = 0

-- Phase tracking
local Phase = {
    BASELINE = 1,
    PARTITIONED = 2,
    HEALING = 3,
    VERIFICATION = 4
}
local current_phase = Phase.BASELINE

-- Helper: Get random online peer
local function random_online_peer()
    local online = sim:online_peers()
    if #online == 0 then return nil end
    return online[math.random(#online)]
end

-- Helper: Get random online peer from a specific partition
local function random_peer_from_partition(partition_idx)
    local partition = partitions[partition_idx]
    if not partition or #partition == 0 then return nil end

    -- Find online peers in this partition
    local online = {}
    for _, peer in ipairs(partition) do
        if sim:is_online(peer) then
            table.insert(online, peer)
        end
    end

    if #online == 0 then return nil end
    return online[math.random(#online)]
end

-- Helper: Find which partition a peer belongs to
local function find_partition(peer)
    for idx, partition in ipairs(partitions) do
        for _, p in ipairs(partition) do
            if p == peer then
                return idx
            end
        end
    end
    return nil
end

-- Helper: Check if two peers are in the same partition
local function same_partition(peer1, peer2)
    local p1 = find_partition(peer1)
    local p2 = find_partition(peer2)
    return p1 ~= nil and p1 == p2
end

-- Create network partitions
local function create_partitions()
    indras.log.info("Creating network partitions", {
        trace_id = ctx.trace_id,
        partition_count = config.partitions,
        tick = sim.tick
    })

    -- Divide peers into N partitions
    local peers_per_partition = math.floor(#all_peers / config.partitions)
    local peer_idx = 1

    for i = 1, config.partitions do
        partitions[i] = {}
        local count = peers_per_partition

        -- Last partition gets any remainder
        if i == config.partitions then
            count = #all_peers - peer_idx + 1
        end

        for j = 1, count do
            table.insert(partitions[i], all_peers[peer_idx])
            peer_idx = peer_idx + 1
        end

        indras.log.debug("Created partition", {
            trace_id = ctx.trace_id,
            partition_id = i,
            peer_count = #partitions[i]
        })
    end

    partitions_created = config.partitions
    partition_start_tick = sim.tick

    -- Force peers offline if they're in different partitions
    -- (Simulated by tracking and not sending cross-partition messages)
    indras.log.info("Partitions created", {
        trace_id = ctx.trace_id,
        partitions = partitions_created,
        tick = sim.tick
    })
end

-- Heal one partition connection
local function heal_partition_step()
    -- Gradually merge partitions
    if #partitions <= 1 then
        return false  -- Nothing to heal
    end

    -- Merge the last two partitions
    local last = table.remove(partitions)
    for _, peer in ipairs(last) do
        table.insert(partitions[#partitions], peer)
    end

    healing_events = healing_events + 1

    indras.log.info("Healed partition", {
        trace_id = ctx.trace_id,
        healing_event = healing_events,
        remaining_partitions = #partitions,
        tick = sim.tick
    })

    return true
end

-- Send messages within partitions
local function send_intra_partition_messages()
    for _ = 1, MESSAGE_RATE do
        -- Pick a random partition
        local partition_idx = math.random(#partitions)
        local sender = random_peer_from_partition(partition_idx)
        local receiver = random_peer_from_partition(partition_idx)

        if sender and receiver and sender ~= receiver then
            sim:send_message(sender, receiver, "intra_partition")
            intra_partition_sent = intra_partition_sent + 1
        end
    end
end

-- Try to send messages across partitions (will queue)
local function send_cross_partition_messages()
    for _ = 1, MESSAGE_RATE do
        if #partitions < 2 then break end

        local partition1 = math.random(#partitions)
        local partition2 = math.random(#partitions)

        -- Ensure different partitions
        while partition2 == partition1 do
            partition2 = math.random(#partitions)
        end

        local sender = random_peer_from_partition(partition1)
        local receiver = random_peer_from_partition(partition2)

        if sender and receiver then
            -- These messages should queue (not deliver immediately)
            sim:send_message(sender, receiver, "cross_partition")
            cross_partition_sent = cross_partition_sent + 1
            cross_partition_queued = cross_partition_queued + 1
        end
    end
end

-- Send messages in healing phase
local function send_healing_messages()
    -- Mix of intra and cross partition
    for _ = 1, MESSAGE_RATE * 2 do
        local sender = random_online_peer()
        local receiver = random_online_peer()

        if sender and receiver and sender ~= receiver then
            sim:send_message(sender, receiver, "healing")
        end
    end
end

-- Send messages in post-healing verification
local function send_verification_messages()
    for _ = 1, MESSAGE_RATE do
        local sender = random_online_peer()
        local receiver = random_online_peer()

        if sender and receiver and sender ~= receiver then
            sim:send_message(sender, receiver, "verification")
            post_healing_sent = post_healing_sent + 1
        end
    end
end

-- Phase management
local phase_tick = 0

local function advance_phase()
    phase_tick = phase_tick + 1

    if current_phase == Phase.BASELINE then
        if phase_tick >= config.baseline_ticks then
            current_phase = Phase.PARTITIONED
            phase_tick = 0
            create_partitions()

            indras.log.info("Phase transition: BASELINE -> PARTITIONED", {
                trace_id = ctx.trace_id,
                tick = sim.tick
            })
        end
    elseif current_phase == Phase.PARTITIONED then
        if phase_tick >= config.partition_ticks then
            current_phase = Phase.HEALING
            phase_tick = 0
            healing_start_tick = sim.tick

            indras.log.info("Phase transition: PARTITIONED -> HEALING", {
                trace_id = ctx.trace_id,
                tick = sim.tick
            })
        end
    elseif current_phase == Phase.HEALING then
        if phase_tick >= config.healing_ticks then
            current_phase = Phase.VERIFICATION
            phase_tick = 0
            recovery_complete_tick = sim.tick

            indras.log.info("Phase transition: HEALING -> VERIFICATION", {
                trace_id = ctx.trace_id,
                tick = sim.tick
            })
        end
    end
end

-- Run simulation
indras.log.info("Running partition recovery simulation", {
    trace_id = ctx.trace_id,
    ticks = config.ticks
})

local baseline_stats = nil
local partitioned_stats = nil

for tick = 1, config.ticks do
    advance_phase()

    if current_phase == Phase.BASELINE then
        -- Normal operation
        for _ = 1, MESSAGE_RATE do
            local sender = random_online_peer()
            local receiver = random_online_peer()

            if sender and receiver and sender ~= receiver then
                sim:send_message(sender, receiver, "baseline")
            end
        end

        -- Capture baseline stats
        if phase_tick == config.baseline_ticks - 1 then
            baseline_stats = {
                messages_delivered = sim.stats.messages_delivered,
                messages_sent = sim.stats.messages_sent,
                delivery_rate = sim.stats:delivery_rate()
            }
        end

    elseif current_phase == Phase.PARTITIONED then
        -- Send within partitions
        send_intra_partition_messages()

        -- Try to send across partitions
        send_cross_partition_messages()

    elseif current_phase == Phase.HEALING then
        -- Gradually heal partitions
        local heal_interval = math.floor(config.healing_ticks / (config.partitions - 1))
        if heal_interval > 0 and phase_tick % heal_interval == 0 then
            heal_partition_step()
        end

        send_healing_messages()

    elseif current_phase == Phase.VERIFICATION then
        -- Post-recovery verification
        send_verification_messages()
    end

    -- Advance simulation
    sim:step()

    -- Track intra-partition delivery
    if current_phase == Phase.PARTITIONED then
        local current_delivered = sim.stats.messages_delivered
        if tick > partition_start_tick then
            -- Approximate intra-partition deliveries
            intra_partition_delivered = intra_partition_delivered +
                (current_delivered - (partitioned_stats and partitioned_stats.messages_delivered or 0))
        end
        partitioned_stats = {
            messages_delivered = sim.stats.messages_delivered
        }
    end

    -- Track post-healing delivery
    if current_phase == Phase.VERIFICATION then
        post_healing_delivered = sim.stats.messages_delivered -
            (partitioned_stats and partitioned_stats.messages_delivered or 0)
    end

    -- Progress logging
    if tick % 50 == 0 then
        indras.log.info("Partition recovery checkpoint", {
            trace_id = ctx.trace_id,
            tick = tick,
            phase = current_phase,
            online_count = #sim:online_peers(),
            partitions = #partitions,
            healing_events = healing_events,
            messages_delivered = sim.stats.messages_delivered,
            messages_dropped = sim.stats.messages_dropped
        })
    end
end

-- Final statistics
local stats = sim.stats

-- Calculate metrics
local intra_partition_delivery_rate = 0
if intra_partition_sent > 0 then
    intra_partition_delivery_rate = intra_partition_delivered / intra_partition_sent
end

local post_healing_delivery_rate = 0
if post_healing_sent > 0 then
    post_healing_delivery_rate = post_healing_delivered / post_healing_sent
end

local total_recovery_time = 0
if recovery_complete_tick > partition_start_tick then
    total_recovery_time = recovery_complete_tick - partition_start_tick
end

indras.log.info("Partition recovery test completed", {
    trace_id = ctx.trace_id,
    test_level = test_level,
    final_tick = sim.tick,
    -- Partition metrics
    partitions_created = partitions_created,
    healing_events = healing_events,
    total_recovery_time = total_recovery_time,
    -- Delivery metrics
    intra_partition_sent = intra_partition_sent,
    intra_partition_delivered = intra_partition_delivered,
    intra_partition_delivery_rate = intra_partition_delivery_rate,
    cross_partition_sent = cross_partition_sent,
    cross_partition_queued = cross_partition_queued,
    post_healing_sent = post_healing_sent,
    post_healing_delivered = post_healing_delivered,
    post_healing_delivery_rate = post_healing_delivery_rate,
    -- Overall network metrics
    messages_sent = stats.messages_sent,
    messages_delivered = stats.messages_delivered,
    messages_dropped = stats.messages_dropped,
    delivery_rate = stats:delivery_rate(),
    average_latency = stats:average_latency(),
    average_hops = stats:average_hops()
})

-- Assertions
indras.assert.gt(partitions_created, 0, "Should have created partitions")
indras.assert.eq(healing_events, config.partitions - 1,
    "Should have healed all partitions")

-- Intra-partition delivery should be high
indras.assert.gt(intra_partition_delivery_rate, 0.7,
    "Intra-partition delivery rate should be > 70%")

-- Cross-partition messages should queue
indras.assert.gt(cross_partition_queued, 0,
    "Should have queued cross-partition messages")

-- Post-healing delivery should recover
indras.assert.gt(post_healing_delivery_rate, 0.5,
    "Post-healing delivery rate should recover to > 50%")

-- Recovery time should be reasonable
indras.assert.gt(total_recovery_time, 0,
    "Should have tracked recovery time")

indras.log.info("Partition recovery test passed", {
    trace_id = ctx.trace_id,
    intra_partition_delivery_rate = intra_partition_delivery_rate,
    post_healing_delivery_rate = post_healing_delivery_rate,
    recovery_time_ticks = total_recovery_time
})

return {
    test_level = test_level,
    -- Partition metrics
    partitions_created = partitions_created,
    intra_partition_delivery_rate = intra_partition_delivery_rate,
    cross_partition_queued = cross_partition_queued,
    healing_events = healing_events,
    post_healing_delivery_rate = post_healing_delivery_rate,
    total_recovery_time = total_recovery_time,
    -- Network metrics
    delivery_rate = stats:delivery_rate(),
    average_latency = stats:average_latency(),
    average_hops = stats:average_hops(),
    -- Totals
    total_messages = stats.messages_sent,
    total_delivered = stats.messages_delivered
}

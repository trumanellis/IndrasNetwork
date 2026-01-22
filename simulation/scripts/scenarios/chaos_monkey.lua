-- Chaos Monkey Test
--
-- Stress test with random peer failures during message delivery.
-- Tests network resilience and store-and-forward reliability.

local ctx = indras.correlation.new_root()
ctx = ctx:with_tag("scenario", "chaos_monkey")

local PEER_COUNT = 10
local MESSAGE_COUNT = 20
local SIMULATION_TICKS = 200
local KILL_INTERVAL = 10

indras.log.info("Starting chaos monkey test", {
    trace_id = ctx.trace_id,
    peers = PEER_COUNT,
    messages = MESSAGE_COUNT,
    duration = SIMULATION_TICKS
})

-- Create random mesh topology
local mesh = indras.MeshBuilder.new(PEER_COUNT):random(0.4)

indras.log.debug("Created random mesh", {
    trace_id = ctx.trace_id,
    peers = mesh:peer_count(),
    edges = mesh:edge_count()
})

-- Create simulation with some randomness
local config = indras.SimConfig.new({
    wake_probability = 0.2,
    sleep_probability = 0.1,
    initial_online_probability = 0.7,
    max_ticks = SIMULATION_TICKS,
    trace_routing = true
})

local sim = indras.Simulation.new(mesh, config)
sim:initialize()

-- Get all peers
local all_peers = mesh:peers()

-- Helper: get random online peer
local function random_online_peer()
    local online = sim:online_peers()
    if #online == 0 then
        return nil
    end
    return online[math.random(#online)]
end

-- Helper: get random peer pair
local function random_pair()
    local peers = mesh:peers()
    local from_idx = math.random(#peers)
    local to_idx = math.random(#peers)
    while to_idx == from_idx do
        to_idx = math.random(#peers)
    end
    return peers[from_idx], peers[to_idx]
end

-- Chaos: kill random peers periodically
local kills = 0
local function chaos_tick(tick)
    if tick % KILL_INTERVAL == 0 then
        local victim = random_online_peer()
        if victim then
            sim:force_offline(victim)
            kills = kills + 1
            indras.log.warn("Chaos: killed peer", {
                trace_id = ctx.trace_id,
                peer = tostring(victim),
                tick = tick,
                total_kills = kills
            })
        end
    end
end

-- Send all messages upfront
indras.log.info("Sending messages", {
    trace_id = ctx.trace_id,
    count = MESSAGE_COUNT
})

for i = 1, MESSAGE_COUNT do
    local from, to = random_pair()
    sim:send_message(from, to, "chaos_msg_" .. i)

    indras.log.trace("Queued message", {
        trace_id = ctx.trace_id,
        msg_num = i,
        from = tostring(from),
        to = tostring(to)
    })
end

-- Run simulation with chaos
indras.log.info("Running simulation with chaos", {
    trace_id = ctx.trace_id,
    ticks = SIMULATION_TICKS
})

for tick = 1, SIMULATION_TICKS do
    chaos_tick(tick)
    sim:step()

    -- Log progress every 50 ticks
    if tick % 50 == 0 then
        indras.log.info("Progress checkpoint", {
            trace_id = ctx.trace_id,
            tick = tick,
            delivered = sim.stats.messages_delivered,
            dropped = sim.stats.messages_dropped,
            online_count = #sim:online_peers()
        })
    end
end

-- Final statistics
local stats = sim.stats
indras.log.info("Chaos test completed", {
    trace_id = ctx.trace_id,
    final_tick = sim.tick,
    sent = stats.messages_sent,
    delivered = stats.messages_delivered,
    dropped = stats.messages_dropped,
    expired = stats.messages_expired,
    delivery_rate = stats:delivery_rate(),
    drop_rate = stats:drop_rate(),
    average_latency = stats:average_latency(),
    average_hops = stats:average_hops(),
    total_kills = kills,
    wake_events = stats.wake_events,
    sleep_events = stats.sleep_events
})

-- Verify some messages got through (resilience check)
local delivery_rate = stats:delivery_rate()
indras.assert.gt(delivery_rate, 0.0, "At least some messages should be delivered")

indras.log.info("Chaos monkey test passed", {
    trace_id = ctx.trace_id,
    delivery_rate = delivery_rate
})

return {
    delivery_rate = delivery_rate,
    dropped = stats.messages_dropped,
    delivered = stats.messages_delivered
}

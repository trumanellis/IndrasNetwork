-- Scalability Limit Test
--
-- Finds system performance limits by ramping load until degradation.
-- Measures throughput, latency, and tick rate at increasing peer counts
-- and message rates until performance degrades >50%.

local ctx = indras.correlation.new_root()
ctx = ctx:with_tag("scenario", "scalability_limit")

-- Configuration levels
local CONFIG = {
    quick = {
        max_peers = 26,
        ramp_steps = 5,
        ticks_per_step = 100,
        initial_peers = 10
    },
    medium = {
        max_peers = 26,
        ramp_steps = 10,
        ticks_per_step = 150,
        initial_peers = 10
    },
    full = {
        max_peers = 26,
        ramp_steps = 20,
        ticks_per_step = 200,
        initial_peers = 10
    }
}

-- Select configuration level (default: quick)
local level = os.getenv("STRESS_LEVEL") or "quick"
local config = CONFIG[level]
if not config then
    indras.log.error("Invalid scalability level", {
        trace_id = ctx.trace_id,
        level = level,
        valid_levels = "quick, medium, full"
    })
    error("Invalid SCALABILITY_LEVEL: " .. level)
end

indras.log.info("Starting scalability limit test", {
    trace_id = ctx.trace_id,
    level = level,
    max_peers = config.max_peers,
    ramp_steps = config.ramp_steps,
    ticks_per_step = config.ticks_per_step,
    initial_peers = config.initial_peers
})

-- Calculate peer increment per step
local peer_increment = math.floor((config.max_peers - config.initial_peers) / config.ramp_steps)
if peer_increment < 1 then
    peer_increment = 1
end

-- Baseline message rate (will scale with peer count)
local BASE_MESSAGE_RATE = 2  -- messages per peer per tick

-- Performance tracking
local degradation_curve = {}
local baseline_throughput = nil
local baseline_latency = nil
local baseline_tick_rate = nil
local breaking_point_step = nil
local max_sustainable_peers = 0
local max_sustainable_rate = 0

-- Helper: Calculate degradation percentage
local function calculate_degradation(current_throughput, current_latency, current_tick_rate)
    if not baseline_throughput or not baseline_latency or not baseline_tick_rate then
        return 0
    end

    local throughput_deg = 0
    if baseline_throughput > 0 then
        throughput_deg = math.max(0, (baseline_throughput - current_throughput) / baseline_throughput * 100)
    end

    local latency_deg = 0
    if baseline_latency > 0 and current_latency > baseline_latency then
        latency_deg = (current_latency - baseline_latency) / baseline_latency * 100
    end

    local tick_rate_deg = 0
    if baseline_tick_rate > 0 then
        tick_rate_deg = math.max(0, (baseline_tick_rate - current_tick_rate) / baseline_tick_rate * 100)
    end

    -- Average degradation across metrics
    return (throughput_deg + latency_deg + tick_rate_deg) / 3
end

-- Helper: Get wall-clock time in microseconds
local function get_time_us()
    local socket_ok, socket = pcall(require, "socket")
    if socket_ok and socket.gettime then
        return math.floor(socket.gettime() * 1000000)
    end
    -- Fallback to os.clock (less accurate)
    return math.floor(os.clock() * 1000000)
end

-- Helper: Random online peer
local function random_online_peer(sim)
    local online = sim:online_peers()
    if #online == 0 then return nil end
    return online[math.random(#online)]
end

-- Helper: Generate messages for current step
local function generate_messages(sim, message_rate)
    local sent_count = 0
    for _ = 1, message_rate do
        local sender = random_online_peer(sim)
        local receiver = random_online_peer(sim)

        if sender and receiver and sender ~= receiver then
            sim:send_message(sender, receiver, "scalability_test")
            sent_count = sent_count + 1
        end
    end
    return sent_count
end

-- Run scalability ramp test
for step = 1, config.ramp_steps do
    -- Calculate peer count and message rate for this step
    local peer_count = config.initial_peers + (step - 1) * peer_increment
    if peer_count > config.max_peers then
        peer_count = config.max_peers
    end

    local message_rate = peer_count * BASE_MESSAGE_RATE

    indras.log.info("Starting ramp step", {
        trace_id = ctx.trace_id,
        step = step,
        peer_count = peer_count,
        message_rate = message_rate,
        ticks = config.ticks_per_step
    })

    -- Create mesh topology for this step
    local mesh = indras.MeshBuilder.new(peer_count):random(0.3)

    -- Create simulation
    local sim_config = indras.SimConfig.new({
        wake_probability = 0.05,
        sleep_probability = 0.02,
        initial_online_probability = 0.95,
        max_ticks = config.ticks_per_step,
        trace_routing = false  -- Disable for performance
    })

    local sim = indras.Simulation.new(mesh, sim_config)
    sim:initialize()

    -- Warm-up phase (10 ticks)
    for _ = 1, 10 do
        generate_messages(sim, message_rate)
        sim:step()
    end

    -- Reset stats after warm-up
    local initial_sent = sim.stats.messages_sent
    local initial_delivered = sim.stats.messages_delivered

    -- Measure step performance
    local step_start_time = get_time_us()
    local tick_times = {}

    for tick = 1, config.ticks_per_step do
        local tick_start = get_time_us()

        generate_messages(sim, message_rate)
        sim:step()

        local tick_end = get_time_us()
        table.insert(tick_times, tick_end - tick_start)
    end

    local step_end_time = get_time_us()
    local step_duration_us = step_end_time - step_start_time

    -- Calculate step metrics
    local stats = sim.stats
    local messages_sent_this_step = stats.messages_sent - initial_sent
    local messages_delivered_this_step = stats.messages_delivered - initial_delivered

    local delivery_rate = 0
    if messages_sent_this_step > 0 then
        delivery_rate = messages_delivered_this_step / messages_sent_this_step
    end

    local avg_latency = stats:average_latency()

    -- Calculate tick rate (ticks per second)
    local tick_rate = 0
    if step_duration_us > 0 then
        tick_rate = (config.ticks_per_step * 1000000.0) / step_duration_us
    end

    -- Calculate throughput (messages delivered per second)
    local throughput = 0
    if step_duration_us > 0 then
        throughput = (messages_delivered_this_step * 1000000.0) / step_duration_us
    end

    -- Calculate average tick time
    local total_tick_time = 0
    for _, t in ipairs(tick_times) do
        total_tick_time = total_tick_time + t
    end
    local avg_tick_time_us = total_tick_time / #tick_times

    -- Set baseline on first step
    if step == 1 then
        baseline_throughput = throughput
        baseline_latency = avg_latency
        baseline_tick_rate = tick_rate

        indras.log.info("Baseline established", {
            trace_id = ctx.trace_id,
            throughput = baseline_throughput,
            latency = baseline_latency,
            tick_rate = baseline_tick_rate
        })
    end

    -- Calculate degradation
    local degradation_percent = calculate_degradation(throughput, avg_latency, tick_rate)

    -- Record step data
    local step_data = {
        step = step,
        peer_count = peer_count,
        message_rate = message_rate,
        messages_sent = messages_sent_this_step,
        messages_delivered = messages_delivered_this_step,
        delivery_rate = delivery_rate,
        avg_latency = avg_latency,
        throughput = throughput,
        tick_rate = tick_rate,
        avg_tick_time_us = avg_tick_time_us,
        degradation_percent = degradation_percent
    }

    table.insert(degradation_curve, step_data)

    indras.log.info("Ramp step completed", {
        trace_id = ctx.trace_id,
        step = step,
        peer_count = peer_count,
        message_rate = message_rate,
        messages_sent = messages_sent_this_step,
        messages_delivered = messages_delivered_this_step,
        delivery_rate = delivery_rate,
        avg_latency = avg_latency,
        throughput = throughput,
        tick_rate = tick_rate,
        avg_tick_time_us = avg_tick_time_us,
        degradation_percent = degradation_percent
    })

    -- Check for breaking point (>50% degradation)
    if degradation_percent > 50 and not breaking_point_step then
        breaking_point_step = step

        indras.log.warn("Breaking point detected", {
            trace_id = ctx.trace_id,
            step = step,
            peer_count = peer_count,
            degradation_percent = degradation_percent,
            throughput = throughput,
            baseline_throughput = baseline_throughput
        })

        -- Last sustainable point was previous step
        if step > 1 then
            local prev_step = degradation_curve[step - 1]
            max_sustainable_peers = prev_step.peer_count
            max_sustainable_rate = prev_step.message_rate
        end

        break  -- Stop ramping after breaking point
    end

    -- Update max sustainable values if still performing well
    if degradation_percent <= 50 then
        max_sustainable_peers = peer_count
        max_sustainable_rate = message_rate
    end
end

-- Final summary
indras.log.info("Scalability limit test completed", {
    trace_id = ctx.trace_id,
    level = level,
    total_steps = #degradation_curve,
    breaking_point_step = breaking_point_step or "none",
    max_sustainable_peers = max_sustainable_peers,
    max_sustainable_rate = max_sustainable_rate,
    baseline_throughput = baseline_throughput,
    baseline_latency = baseline_latency,
    baseline_tick_rate = baseline_tick_rate
})

-- Log degradation curve
for i, step_data in ipairs(degradation_curve) do
    indras.log.info("Degradation curve point", {
        trace_id = ctx.trace_id,
        step = i,
        peer_count = step_data.peer_count,
        throughput = step_data.throughput,
        degradation_percent = step_data.degradation_percent
    })
end

-- Assertions
indras.assert.gt(#degradation_curve, 0, "Should have collected degradation data")
indras.assert.gt(max_sustainable_peers, 0, "Should have determined max sustainable peers")

if breaking_point_step then
    indras.log.info("System limits identified", {
        trace_id = ctx.trace_id,
        max_sustainable_peers = max_sustainable_peers,
        max_sustainable_rate = max_sustainable_rate,
        breaking_point_step = breaking_point_step
    })
else
    indras.log.info("No breaking point found within test parameters", {
        trace_id = ctx.trace_id,
        max_tested_peers = max_sustainable_peers,
        max_tested_rate = max_sustainable_rate,
        note = "System remained stable throughout test"
    })
end

-- Return comprehensive metrics
return {
    level = level,
    max_sustainable_peers = max_sustainable_peers,
    max_sustainable_rate = max_sustainable_rate,
    breaking_point_step = breaking_point_step,
    baseline_throughput = baseline_throughput,
    baseline_latency = baseline_latency,
    baseline_tick_rate = baseline_tick_rate,
    degradation_curve = degradation_curve,
    total_steps = #degradation_curve
}

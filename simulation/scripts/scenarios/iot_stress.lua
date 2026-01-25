-- IoT (Internet of Things) Stress Test
--
-- Stress tests IoT device constraints: duty cycling (power-aware scheduling),
-- compact wire formats (bandwidth-constrained messaging), and low memory
-- constraints (limited buffers). Simulates resource-constrained devices with
-- intermittent connectivity patterns.
--
-- IoT characteristics:
-- - Duty cycling: Active/PreSleep/Sleeping/Waking state machines
-- - Bandwidth limits: Message size tracking and throttling
-- - Memory pressure: Limited pending message buffers per device
-- - Power-aware scheduling: Nodes only active during brief windows

local ctx = indras.correlation.new_root()
ctx = ctx:with_tag("scenario", "iot_stress")

-- Configuration levels
local CONFIG = {
    quick = {
        peers = 10,
        messages = 100,
        ticks = 200,
        -- Duty cycle parameters
        active_window_ticks = 5,       -- ticks per active window
        sleep_duration_ticks = 15,     -- ticks between active windows
        presleep_ticks = 2,            -- ticks in presleep state
        waking_ticks = 2,              -- ticks to wake up
        -- Bandwidth constraints (bytes)
        max_message_size = 256,
        bandwidth_limit_per_tick = 512,
        -- Memory constraints
        max_pending_messages = 5,
        buffer_size_bytes = 1024
    },
    medium = {
        peers = 20,
        messages = 500,
        ticks = 500,
        -- Duty cycle parameters
        active_window_ticks = 4,
        sleep_duration_ticks = 20,
        presleep_ticks = 2,
        waking_ticks = 3,
        -- Bandwidth constraints (bytes)
        max_message_size = 192,
        bandwidth_limit_per_tick = 384,
        -- Memory constraints
        max_pending_messages = 4,
        buffer_size_bytes = 768
    },
    full = {
        peers = 26,
        messages = 2000,
        ticks = 1500,
        -- Duty cycle parameters
        active_window_ticks = 3,
        sleep_duration_ticks = 30,
        presleep_ticks = 3,
        waking_ticks = 4,
        -- Bandwidth constraints (bytes)
        max_message_size = 128,
        bandwidth_limit_per_tick = 256,
        -- Memory constraints
        max_pending_messages = 3,
        buffer_size_bytes = 512
    }
}

-- Select test level (default to quick if not specified)
local test_level = os.getenv("STRESS_LEVEL") or "quick"
local config = CONFIG[test_level]

if not config then
    error("Invalid test level: " .. tostring(test_level))
end

indras.log.info("Starting IoT stress test", {
    trace_id = ctx.trace_id,
    level = test_level,
    peers = config.peers,
    messages = config.messages,
    ticks = config.ticks,
    active_window = config.active_window_ticks,
    sleep_duration = config.sleep_duration_ticks,
    max_message_size = config.max_message_size,
    max_pending = config.max_pending_messages
})

-- Create sparse mesh topology (IoT networks often have limited connectivity)
local mesh = indras.MeshBuilder.new(config.peers):random(0.25)

indras.log.debug("Created IoT mesh topology", {
    trace_id = ctx.trace_id,
    peers = mesh:peer_count(),
    edges = mesh:edge_count(),
    avg_connectivity = mesh:edge_count() / mesh:peer_count()
})

-- Create simulation with IoT-appropriate settings
local sim_config = indras.SimConfig.new({
    wake_probability = 0.0,              -- We control wake/sleep via duty cycling
    sleep_probability = 0.0,             -- Disable random sleep
    initial_online_probability = 0.3,    -- Only some nodes start active
    max_ticks = config.ticks,
    trace_routing = true
})

local sim = indras.Simulation.new(mesh, sim_config)
sim:initialize()

local all_peers = mesh:peers()

-------------------------------------------------------------------------------
-- IoT Device State Machine
-------------------------------------------------------------------------------

-- Device states
local STATE_ACTIVE = "Active"
local STATE_PRESLEEP = "PreSleep"
local STATE_SLEEPING = "Sleeping"
local STATE_WAKING = "Waking"

-- IoT Device class
local IoTDevice = {}
IoTDevice.__index = IoTDevice

function IoTDevice.new(peer, config, start_offset)
    local device = setmetatable({
        peer = peer,
        peer_str = tostring(peer),
        state = STATE_SLEEPING,
        state_tick = 0,
        -- Duty cycle timing (offset start for network staggering)
        cycle_offset = start_offset or 0,
        active_window = config.active_window_ticks,
        sleep_duration = config.sleep_duration_ticks,
        presleep_duration = config.presleep_ticks,
        waking_duration = config.waking_ticks,
        -- Bandwidth tracking
        bandwidth_limit = config.bandwidth_limit_per_tick,
        bandwidth_used = 0,
        max_message_size = config.max_message_size,
        -- Memory/buffer tracking
        max_pending = config.max_pending_messages,
        buffer_size = config.buffer_size_bytes,
        pending_messages = {},
        buffer_used = 0,
        -- Metrics
        messages_sent = 0,
        messages_received = 0,
        messages_dropped_size = 0,
        messages_dropped_buffer = 0,
        messages_dropped_bandwidth = 0,
        active_ticks = 0,
        sleep_ticks = 0,
        state_transitions = 0
    }, IoTDevice)
    return device
end

function IoTDevice:cycle_position(tick)
    -- Calculate position within duty cycle
    local cycle_length = self.active_window + self.presleep_duration +
                         self.sleep_duration + self.waking_duration
    return (tick + self.cycle_offset) % cycle_length
end

function IoTDevice:expected_state(tick)
    local pos = self:cycle_position(tick)

    if pos < self.active_window then
        return STATE_ACTIVE
    elseif pos < self.active_window + self.presleep_duration then
        return STATE_PRESLEEP
    elseif pos < self.active_window + self.presleep_duration + self.sleep_duration then
        return STATE_SLEEPING
    else
        return STATE_WAKING
    end
end

function IoTDevice:update_state(tick, sim_ref)
    local new_state = self:expected_state(tick)

    if new_state ~= self.state then
        local old_state = self.state
        self.state = new_state
        self.state_tick = tick
        self.state_transitions = self.state_transitions + 1

        -- Update simulation online/offline status
        if new_state == STATE_ACTIVE then
            sim_ref:force_online(self.peer)
        elseif new_state == STATE_SLEEPING or new_state == STATE_PRESLEEP then
            -- PreSleep: still online but preparing to sleep
            -- Sleeping: fully offline
            if new_state == STATE_SLEEPING then
                sim_ref:force_offline(self.peer)
            end
        elseif new_state == STATE_WAKING then
            -- Waking: coming online but not ready yet
            sim_ref:force_offline(self.peer)
        end

        return true, old_state, new_state
    end

    return false
end

function IoTDevice:is_active()
    return self.state == STATE_ACTIVE
end

function IoTDevice:can_send()
    -- Can only send during active state
    return self.state == STATE_ACTIVE
end

function IoTDevice:can_receive()
    -- Can receive during active or presleep states
    return self.state == STATE_ACTIVE or self.state == STATE_PRESLEEP
end

function IoTDevice:reset_bandwidth()
    self.bandwidth_used = 0
end

function IoTDevice:check_message_constraints(size)
    -- Check message size limit
    if size > self.max_message_size then
        return false, "size_exceeded"
    end

    -- Check bandwidth limit
    if self.bandwidth_used + size > self.bandwidth_limit then
        return false, "bandwidth_exceeded"
    end

    -- Check buffer space
    if #self.pending_messages >= self.max_pending then
        return false, "buffer_full"
    end

    if self.buffer_used + size > self.buffer_size then
        return false, "buffer_overflow"
    end

    return true, nil
end

function IoTDevice:queue_message(msg_id, size, destination, tick)
    local ok, reason = self:check_message_constraints(size)
    if not ok then
        if reason == "size_exceeded" then
            self.messages_dropped_size = self.messages_dropped_size + 1
        elseif reason == "bandwidth_exceeded" then
            self.messages_dropped_bandwidth = self.messages_dropped_bandwidth + 1
        else
            self.messages_dropped_buffer = self.messages_dropped_buffer + 1
        end
        return false, reason
    end

    table.insert(self.pending_messages, {
        id = msg_id,
        size = size,
        destination = destination,
        queued_tick = tick
    })
    self.buffer_used = self.buffer_used + size
    self.bandwidth_used = self.bandwidth_used + size

    return true
end

function IoTDevice:dequeue_message()
    if #self.pending_messages == 0 then
        return nil
    end

    local msg = table.remove(self.pending_messages, 1)
    self.buffer_used = self.buffer_used - msg.size
    return msg
end

function IoTDevice:tick_metrics(tick)
    if self:is_active() then
        self.active_ticks = self.active_ticks + 1
    else
        self.sleep_ticks = self.sleep_ticks + 1
    end
end

-------------------------------------------------------------------------------
-- IoT Device Registry
-------------------------------------------------------------------------------

local devices = {}
local devices_by_str = {}

-- Initialize all devices with staggered duty cycles
for i, peer in ipairs(all_peers) do
    -- Stagger start offsets to prevent all devices waking simultaneously
    local cycle_length = config.active_window_ticks + config.presleep_ticks +
                         config.sleep_duration_ticks + config.waking_ticks
    local offset = ((i - 1) * math.floor(cycle_length / config.peers)) % cycle_length

    local device = IoTDevice.new(peer, config, offset)
    devices[i] = device
    devices_by_str[device.peer_str] = device
end

indras.log.debug("Initialized IoT devices", {
    trace_id = ctx.trace_id,
    device_count = #devices
})

-------------------------------------------------------------------------------
-- Message Generation and Tracking
-------------------------------------------------------------------------------

local Message = {}
Message.__index = Message

function Message.new(id, source, destination, size, created_tick)
    return setmetatable({
        id = id,
        source = source,
        destination = destination,
        size = size,
        created_tick = created_tick,
        delivered = false,
        delivered_tick = nil,
        dropped = false,
        drop_reason = nil,
        attempts = 0
    }, Message)
end

local messages = {}
local next_message_id = 1

-- Metrics
local metrics = {
    -- Message counts
    messages_created = 0,
    messages_delivered = 0,
    messages_dropped = 0,
    -- Drop reasons
    dropped_size = 0,
    dropped_buffer = 0,
    dropped_bandwidth = 0,
    dropped_offline = 0,
    -- Duty cycle metrics
    total_state_transitions = 0,
    active_windows_used = 0,
    messages_sent_during_active = 0,
    messages_queued_presleep = 0,
    -- Bandwidth metrics
    total_bytes_sent = 0,
    total_bytes_dropped = 0,
    bandwidth_throttle_events = 0,
    -- Memory metrics
    peak_buffer_usage = 0,
    buffer_overflow_events = 0,
    -- Delivery metrics
    delivery_latencies = {},
    delivery_during_first_active = 0
}

local function random_message_size()
    -- Generate random message size (50-150% of max to test constraints)
    local base = config.max_message_size * 0.5
    local variance = config.max_message_size * 0.5
    return math.floor(base + math.random() * variance)
end

local function get_device(peer)
    return devices_by_str[tostring(peer)]
end

local function random_active_device()
    local active = {}
    for _, device in ipairs(devices) do
        if device:is_active() then
            table.insert(active, device)
        end
    end
    if #active == 0 then return nil end
    return active[math.random(#active)]
end

local function random_device()
    return devices[math.random(#devices)]
end

-------------------------------------------------------------------------------
-- Phase 1: Duty Cycle Testing
-------------------------------------------------------------------------------

indras.log.info("Phase 1: Testing duty cycle constraints", {
    trace_id = ctx.trace_id,
    phase_ticks = math.floor(config.ticks * 0.35),
    active_window = config.active_window_ticks,
    sleep_duration = config.sleep_duration_ticks
})

local phase1_end = math.floor(config.ticks * 0.35)
local messages_per_phase = math.floor(config.messages * 0.35)
local messages_per_tick = math.max(1, math.ceil(messages_per_phase / phase1_end))

for tick = 1, phase1_end do
    -- Update all device states
    for _, device in ipairs(devices) do
        local changed, old_state, new_state = device:update_state(tick, sim)
        if changed then
            metrics.total_state_transitions = metrics.total_state_transitions + 1

            if new_state == STATE_ACTIVE then
                metrics.active_windows_used = metrics.active_windows_used + 1
            end

            indras.log.debug("Device state transition", {
                trace_id = ctx.trace_id,
                device = device.peer_str,
                old_state = old_state,
                new_state = new_state,
                tick = tick
            })
        end
        device:reset_bandwidth()
        device:tick_metrics(tick)
    end

    -- Generate messages from active devices
    for _ = 1, messages_per_tick do
        if metrics.messages_created >= messages_per_phase then
            break
        end

        local sender_device = random_active_device()
        local receiver_device = random_device()

        if sender_device and receiver_device and
           sender_device.peer ~= receiver_device.peer and
           sender_device:can_send() then

            local size = random_message_size()
            local msg_id = string.format("iot-msg-%d", next_message_id)
            next_message_id = next_message_id + 1

            local msg = Message.new(msg_id, sender_device.peer,
                                   receiver_device.peer, size, tick)

            -- Try to queue on sender
            local queued, reason = sender_device:queue_message(
                msg_id, size, receiver_device.peer, tick)

            if queued then
                messages[msg_id] = msg
                metrics.messages_created = metrics.messages_created + 1
                metrics.messages_sent_during_active = metrics.messages_sent_during_active + 1

                -- Actually send via simulation
                sim:send_message(sender_device.peer, receiver_device.peer, "iot_message")
                sender_device.messages_sent = sender_device.messages_sent + 1
                metrics.total_bytes_sent = metrics.total_bytes_sent + size

                -- Check if receiver can accept
                if receiver_device:can_receive() then
                    msg.delivered = true
                    msg.delivered_tick = tick
                    metrics.messages_delivered = metrics.messages_delivered + 1
                    receiver_device.messages_received = receiver_device.messages_received + 1
                    table.insert(metrics.delivery_latencies, tick - msg.created_tick)

                    if tick - msg.created_tick <= config.active_window_ticks then
                        metrics.delivery_during_first_active =
                            metrics.delivery_during_first_active + 1
                    end
                end
            else
                msg.dropped = true
                msg.drop_reason = reason
                metrics.messages_dropped = metrics.messages_dropped + 1
                metrics.total_bytes_dropped = metrics.total_bytes_dropped + size

                if reason == "size_exceeded" then
                    metrics.dropped_size = metrics.dropped_size + 1
                elseif reason == "bandwidth_exceeded" then
                    metrics.dropped_bandwidth = metrics.dropped_bandwidth + 1
                    metrics.bandwidth_throttle_events =
                        metrics.bandwidth_throttle_events + 1
                else
                    metrics.dropped_buffer = metrics.dropped_buffer + 1
                    metrics.buffer_overflow_events =
                        metrics.buffer_overflow_events + 1
                end
            end
        end
    end

    -- Track peak buffer usage
    for _, device in ipairs(devices) do
        if device.buffer_used > metrics.peak_buffer_usage then
            metrics.peak_buffer_usage = device.buffer_used
        end
    end

    sim:step()

    -- Progress logging
    if tick % 50 == 0 then
        local active_count = 0
        for _, device in ipairs(devices) do
            if device:is_active() then
                active_count = active_count + 1
            end
        end

        indras.log.info("Phase 1 progress", {
            trace_id = ctx.trace_id,
            tick = tick,
            active_devices = active_count,
            messages_created = metrics.messages_created,
            messages_delivered = metrics.messages_delivered,
            messages_dropped = metrics.messages_dropped,
            state_transitions = metrics.total_state_transitions
        })
    end
end

-------------------------------------------------------------------------------
-- Phase 2: Bandwidth and Size Constraint Testing
-------------------------------------------------------------------------------

indras.log.info("Phase 2: Testing bandwidth and message size constraints", {
    trace_id = ctx.trace_id,
    phase_ticks = math.floor(config.ticks * 0.35),
    max_message_size = config.max_message_size,
    bandwidth_limit = config.bandwidth_limit_per_tick
})

local phase2_start = phase1_end + 1
local phase2_end = phase1_end + math.floor(config.ticks * 0.35)
local phase2_messages = math.floor(config.messages * 0.35)
local phase2_target = metrics.messages_created + phase2_messages

-- In phase 2, we deliberately test size and bandwidth limits
local function oversized_message_size()
    -- 40% chance of oversized message
    if math.random() < 0.4 then
        return config.max_message_size + math.random(50, 150)
    end
    return random_message_size()
end

for tick = phase2_start, phase2_end do
    -- Update device states
    for _, device in ipairs(devices) do
        device:update_state(tick, sim)
        device:reset_bandwidth()
        device:tick_metrics(tick)
    end

    -- Generate burst of messages to stress bandwidth
    local burst_size = math.random(3, 8)
    for _ = 1, burst_size do
        if metrics.messages_created >= phase2_target then
            break
        end

        local sender_device = random_active_device()
        local receiver_device = random_device()

        if sender_device and receiver_device and
           sender_device.peer ~= receiver_device.peer and
           sender_device:can_send() then

            local size = oversized_message_size()
            local msg_id = string.format("iot-msg-%d", next_message_id)
            next_message_id = next_message_id + 1

            local msg = Message.new(msg_id, sender_device.peer,
                                   receiver_device.peer, size, tick)

            local queued, reason = sender_device:queue_message(
                msg_id, size, receiver_device.peer, tick)

            if queued then
                messages[msg_id] = msg
                metrics.messages_created = metrics.messages_created + 1

                sim:send_message(sender_device.peer, receiver_device.peer, "iot_bandwidth_test")
                sender_device.messages_sent = sender_device.messages_sent + 1
                metrics.total_bytes_sent = metrics.total_bytes_sent + size

                if receiver_device:can_receive() then
                    msg.delivered = true
                    msg.delivered_tick = tick
                    metrics.messages_delivered = metrics.messages_delivered + 1
                    receiver_device.messages_received = receiver_device.messages_received + 1
                    table.insert(metrics.delivery_latencies, tick - msg.created_tick)
                end
            else
                msg.dropped = true
                msg.drop_reason = reason
                metrics.messages_dropped = metrics.messages_dropped + 1
                metrics.total_bytes_dropped = metrics.total_bytes_dropped + size

                if reason == "size_exceeded" then
                    metrics.dropped_size = metrics.dropped_size + 1
                elseif reason == "bandwidth_exceeded" then
                    metrics.dropped_bandwidth = metrics.dropped_bandwidth + 1
                    metrics.bandwidth_throttle_events =
                        metrics.bandwidth_throttle_events + 1
                else
                    metrics.dropped_buffer = metrics.dropped_buffer + 1
                    metrics.buffer_overflow_events =
                        metrics.buffer_overflow_events + 1
                end
            end
        end
    end

    -- Track peak buffer usage
    for _, device in ipairs(devices) do
        if device.buffer_used > metrics.peak_buffer_usage then
            metrics.peak_buffer_usage = device.buffer_used
        end
    end

    sim:step()

    if tick % 50 == 0 then
        indras.log.info("Phase 2 progress", {
            trace_id = ctx.trace_id,
            tick = tick,
            messages_created = metrics.messages_created,
            dropped_size = metrics.dropped_size,
            dropped_bandwidth = metrics.dropped_bandwidth,
            bandwidth_throttle_events = metrics.bandwidth_throttle_events
        })
    end
end

-------------------------------------------------------------------------------
-- Phase 3: Memory Pressure Testing
-------------------------------------------------------------------------------

indras.log.info("Phase 3: Testing memory pressure and buffer constraints", {
    trace_id = ctx.trace_id,
    phase_ticks = config.ticks - phase2_end,
    max_pending = config.max_pending_messages,
    buffer_size = config.buffer_size_bytes
})

local phase3_start = phase2_end + 1
local phase3_target = config.messages

for tick = phase3_start, config.ticks do
    -- Update device states
    for _, device in ipairs(devices) do
        device:update_state(tick, sim)
        device:reset_bandwidth()
        device:tick_metrics(tick)
    end

    -- Generate messages with focus on buffer exhaustion
    -- Send multiple small messages to fill buffers
    local small_msg_count = math.random(5, 10)
    for _ = 1, small_msg_count do
        if metrics.messages_created >= phase3_target then
            break
        end

        local sender_device = random_active_device()
        local receiver_device = random_device()

        if sender_device and receiver_device and
           sender_device.peer ~= receiver_device.peer and
           sender_device:can_send() then

            -- Use smaller messages to test buffer count limits
            local size = math.floor(config.max_message_size * 0.3 +
                                   math.random() * config.max_message_size * 0.3)
            local msg_id = string.format("iot-msg-%d", next_message_id)
            next_message_id = next_message_id + 1

            local msg = Message.new(msg_id, sender_device.peer,
                                   receiver_device.peer, size, tick)

            local queued, reason = sender_device:queue_message(
                msg_id, size, receiver_device.peer, tick)

            if queued then
                messages[msg_id] = msg
                metrics.messages_created = metrics.messages_created + 1

                sim:send_message(sender_device.peer, receiver_device.peer, "iot_memory_test")
                sender_device.messages_sent = sender_device.messages_sent + 1
                metrics.total_bytes_sent = metrics.total_bytes_sent + size

                if receiver_device:can_receive() then
                    msg.delivered = true
                    msg.delivered_tick = tick
                    metrics.messages_delivered = metrics.messages_delivered + 1
                    receiver_device.messages_received = receiver_device.messages_received + 1
                    table.insert(metrics.delivery_latencies, tick - msg.created_tick)
                end
            else
                msg.dropped = true
                msg.drop_reason = reason
                metrics.messages_dropped = metrics.messages_dropped + 1
                metrics.total_bytes_dropped = metrics.total_bytes_dropped + size

                if reason == "size_exceeded" then
                    metrics.dropped_size = metrics.dropped_size + 1
                elseif reason == "bandwidth_exceeded" then
                    metrics.dropped_bandwidth = metrics.dropped_bandwidth + 1
                elseif reason == "buffer_full" or reason == "buffer_overflow" then
                    metrics.dropped_buffer = metrics.dropped_buffer + 1
                    metrics.buffer_overflow_events =
                        metrics.buffer_overflow_events + 1
                end
            end
        end
    end

    -- Process message delivery for queued messages
    for _, device in ipairs(devices) do
        if device:can_send() then
            local msg_data = device:dequeue_message()
            while msg_data do
                -- Message was already sent, just tracking queue drain
                msg_data = device:dequeue_message()
            end
        end

        if device.buffer_used > metrics.peak_buffer_usage then
            metrics.peak_buffer_usage = device.buffer_used
        end
    end

    sim:step()

    if tick % 50 == 0 then
        local total_pending = 0
        local total_buffer_used = 0
        for _, device in ipairs(devices) do
            total_pending = total_pending + #device.pending_messages
            total_buffer_used = total_buffer_used + device.buffer_used
        end

        indras.log.info("Phase 3 progress", {
            trace_id = ctx.trace_id,
            tick = tick,
            messages_created = metrics.messages_created,
            dropped_buffer = metrics.dropped_buffer,
            total_pending = total_pending,
            total_buffer_used = total_buffer_used,
            buffer_overflow_events = metrics.buffer_overflow_events
        })
    end
end

-------------------------------------------------------------------------------
-- Final Metrics Calculation
-------------------------------------------------------------------------------

-- Calculate duty cycle efficiency
local total_device_ticks = 0
local total_active_ticks = 0
for _, device in ipairs(devices) do
    total_device_ticks = total_device_ticks + device.active_ticks + device.sleep_ticks
    total_active_ticks = total_active_ticks + device.active_ticks
end
local duty_cycle_efficiency = total_device_ticks > 0 and
    (total_active_ticks / total_device_ticks) or 0

-- Calculate average delivery latency
local avg_latency = 0
if #metrics.delivery_latencies > 0 then
    local sum = 0
    for _, lat in ipairs(metrics.delivery_latencies) do
        sum = sum + lat
    end
    avg_latency = sum / #metrics.delivery_latencies
end

-- Calculate delivery rate
local delivery_rate = metrics.messages_created > 0 and
    (metrics.messages_delivered / metrics.messages_created) or 0

-- Calculate drop rates by reason
local size_drop_rate = metrics.messages_created > 0 and
    (metrics.dropped_size / metrics.messages_created) or 0
local bandwidth_drop_rate = metrics.messages_created > 0 and
    (metrics.dropped_bandwidth / metrics.messages_created) or 0
local buffer_drop_rate = metrics.messages_created > 0 and
    (metrics.dropped_buffer / metrics.messages_created) or 0

-- Calculate bandwidth efficiency
local bandwidth_efficiency = (metrics.total_bytes_sent + metrics.total_bytes_dropped) > 0 and
    (metrics.total_bytes_sent / (metrics.total_bytes_sent + metrics.total_bytes_dropped)) or 0

-- Get simulation stats
local stats = sim.stats

indras.log.info("IoT stress test completed", {
    trace_id = ctx.trace_id,
    test_level = test_level,
    final_tick = sim.tick,
    -- Message metrics
    messages_created = metrics.messages_created,
    messages_delivered = metrics.messages_delivered,
    messages_dropped = metrics.messages_dropped,
    delivery_rate = delivery_rate,
    avg_latency_ticks = avg_latency,
    -- Drop breakdown
    dropped_size = metrics.dropped_size,
    dropped_bandwidth = metrics.dropped_bandwidth,
    dropped_buffer = metrics.dropped_buffer,
    size_drop_rate = size_drop_rate,
    bandwidth_drop_rate = bandwidth_drop_rate,
    buffer_drop_rate = buffer_drop_rate,
    -- Duty cycle metrics
    duty_cycle_efficiency = duty_cycle_efficiency,
    total_state_transitions = metrics.total_state_transitions,
    active_windows_used = metrics.active_windows_used,
    messages_sent_during_active = metrics.messages_sent_during_active,
    -- Bandwidth metrics
    total_bytes_sent = metrics.total_bytes_sent,
    total_bytes_dropped = metrics.total_bytes_dropped,
    bandwidth_efficiency = bandwidth_efficiency,
    bandwidth_throttle_events = metrics.bandwidth_throttle_events,
    -- Memory metrics
    peak_buffer_usage = metrics.peak_buffer_usage,
    buffer_overflow_events = metrics.buffer_overflow_events,
    -- Network metrics
    network_messages_sent = stats.messages_sent,
    network_delivery_rate = stats:delivery_rate()
})

-------------------------------------------------------------------------------
-- Assertions
-------------------------------------------------------------------------------

-- Basic functionality assertions
indras.assert.gt(metrics.messages_created, 0, "Should create messages")
indras.assert.gt(metrics.messages_delivered, 0, "Should deliver some messages")

-- Duty cycle assertions
indras.assert.gt(metrics.total_state_transitions, 0,
    "Devices should transition through duty cycle states")
indras.assert.gt(duty_cycle_efficiency, 0,
    "Should have measurable duty cycle efficiency")
indras.assert.lt(duty_cycle_efficiency, 0.5,
    "Duty cycle should keep devices mostly sleeping (< 50% active)")

-- Constraint enforcement assertions
-- We expect drops due to constraints being enforced
local total_constraint_drops = metrics.dropped_size +
                               metrics.dropped_bandwidth +
                               metrics.dropped_buffer

if test_level == "medium" or test_level == "full" then
    -- Stricter constraints should cause more drops
    indras.assert.gt(total_constraint_drops, 0,
        "Constraints should cause message drops in medium/full mode")
end

-- Bandwidth should be throttled
if metrics.bandwidth_throttle_events > 0 then
    indras.log.info("Bandwidth throttling verified", {
        trace_id = ctx.trace_id,
        throttle_events = metrics.bandwidth_throttle_events
    })
end

-- Buffer limits should be enforced
if metrics.buffer_overflow_events > 0 then
    indras.log.info("Buffer overflow protection verified", {
        trace_id = ctx.trace_id,
        overflow_events = metrics.buffer_overflow_events
    })
end

-- Delivery rate should be reasonable given constraints
-- IoT scenarios with duty cycling have lower delivery rates due to:
-- - Sparse mesh (25% connectivity)
-- - Devices only active ~20% of the time
-- - Bandwidth and buffer constraints causing drops
local min_delivery_rate = 0.10
if test_level == "quick" then
    min_delivery_rate = 0.15
elseif test_level == "full" then
    min_delivery_rate = 0.05
end

indras.assert.ge(delivery_rate, min_delivery_rate,
    string.format("Delivery rate should be at least %.0f%% under IoT constraints",
                  min_delivery_rate * 100))

-- Peak buffer usage should not exceed configured limits
indras.assert.le(metrics.peak_buffer_usage, config.buffer_size_bytes * config.peers,
    "Peak buffer usage should not exceed total configured buffer space")

indras.log.info("IoT stress test passed", {
    trace_id = ctx.trace_id,
    level = test_level,
    delivery_rate = delivery_rate,
    duty_cycle_efficiency = duty_cycle_efficiency,
    bandwidth_efficiency = bandwidth_efficiency,
    constraint_drops = total_constraint_drops
})

return {
    -- Test info
    level = test_level,
    -- Delivery metrics
    messages_created = metrics.messages_created,
    messages_delivered = metrics.messages_delivered,
    messages_dropped = metrics.messages_dropped,
    delivery_rate = delivery_rate,
    avg_latency_ticks = avg_latency,
    -- Drop breakdown
    dropped_size = metrics.dropped_size,
    dropped_bandwidth = metrics.dropped_bandwidth,
    dropped_buffer = metrics.dropped_buffer,
    -- Duty cycle metrics
    duty_cycle_efficiency = duty_cycle_efficiency,
    total_state_transitions = metrics.total_state_transitions,
    active_windows_used = metrics.active_windows_used,
    -- Bandwidth metrics
    total_bytes_sent = metrics.total_bytes_sent,
    total_bytes_dropped = metrics.total_bytes_dropped,
    bandwidth_efficiency = bandwidth_efficiency,
    bandwidth_throttle_events = metrics.bandwidth_throttle_events,
    -- Memory metrics
    peak_buffer_usage = metrics.peak_buffer_usage,
    buffer_overflow_events = metrics.buffer_overflow_events,
    -- Network stats
    network_delivery_rate = stats:delivery_rate(),
    total_ticks = sim.tick
}

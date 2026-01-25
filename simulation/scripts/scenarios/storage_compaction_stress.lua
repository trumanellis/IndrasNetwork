-- Storage Compaction Stress Test
--
-- Stress tests append-only log compaction behavior including:
-- - High write volume to trigger compaction thresholds
-- - Compaction triggers based on log size
-- - Data integrity verification after compaction cycles
-- - Concurrent read/write access during compaction windows
--
-- Since direct storage API bindings may not exist, this simulates compaction:
-- - Tracks event log size (simulated)
-- - Triggers "compaction" when size exceeds threshold
-- - Verifies message integrity after compaction
-- - Measures throughput impact during compaction

local ctx = indras.correlation.new_root()
ctx = ctx:with_tag("scenario", "storage_compaction_stress")

-- Configuration levels
local CONFIG = {
    quick = {
        peer_count = 8,
        max_ticks = 200,
        events_per_tick = 10,
        log_compaction_threshold = 500,    -- events before compaction triggers
        compaction_duration = 5,           -- ticks for compaction to complete
        verification_samples = 50,         -- messages to verify after compaction
        concurrent_access_rate = 0.7,      -- probability of read/write during compaction
    },
    medium = {
        peer_count = 15,
        max_ticks = 500,
        events_per_tick = 25,
        log_compaction_threshold = 2000,
        compaction_duration = 10,
        verification_samples = 200,
        concurrent_access_rate = 0.8,
    },
    full = {
        peer_count = 26,
        max_ticks = 2000,
        events_per_tick = 50,
        log_compaction_threshold = 10000,
        compaction_duration = 20,
        verification_samples = 1000,
        concurrent_access_rate = 0.9,
    }
}

-- Select test level (default: quick)
local test_level = os.getenv("STRESS_LEVEL") or "quick"
local config = CONFIG[test_level] or CONFIG.quick

indras.log.info("Starting storage compaction stress test", {
    trace_id = ctx.trace_id,
    level = test_level,
    peer_count = config.peer_count,
    max_ticks = config.max_ticks,
    events_per_tick = config.events_per_tick,
    log_compaction_threshold = config.log_compaction_threshold,
    compaction_duration = config.compaction_duration
})

--------------------------------------------------------------------------------
-- EventLog Class: Simulates append-only log with compaction
--------------------------------------------------------------------------------

local EventLog = {}
EventLog.__index = EventLog

function EventLog.new(compaction_threshold, compaction_duration)
    local self = setmetatable({}, EventLog)
    self.entries = {}                          -- all log entries
    self.compacted_entries = {}                -- entries that survived compaction
    self.log_size = 0                          -- current log size (simulated bytes)
    self.compaction_threshold = compaction_threshold
    self.compaction_duration = compaction_duration
    self.compaction_in_progress = false
    self.compaction_start_tick = nil
    self.compaction_end_tick = nil
    self.compaction_count = 0                  -- total compactions performed
    self.entries_before_compaction = 0         -- entries at compaction start
    self.entries_after_compaction = 0          -- entries after compaction
    self.next_entry_id = 1
    self.integrity_checksums = {}              -- checksums for verification
    return self
end

function EventLog:append(event_type, payload, tick)
    local entry_id = self.next_entry_id
    self.next_entry_id = self.next_entry_id + 1

    local entry = {
        id = entry_id,
        type = event_type,
        payload = payload,
        tick = tick,
        compacted = false,
        checksum = self:_compute_checksum(entry_id, event_type, payload)
    }

    table.insert(self.entries, entry)
    self.integrity_checksums[entry_id] = entry.checksum

    -- Simulate log size growth (variable based on payload)
    local entry_size = 64 + #tostring(payload)
    self.log_size = self.log_size + entry_size

    return entry_id, entry.checksum
end

function EventLog:_compute_checksum(id, event_type, payload)
    -- Simple checksum simulation: combine fields into a hash-like value
    local hash = id * 31
    for i = 1, #event_type do
        hash = (hash * 31 + string.byte(event_type, i)) % 2147483647
    end
    local payload_str = tostring(payload)
    for i = 1, math.min(#payload_str, 100) do
        hash = (hash * 31 + string.byte(payload_str, i)) % 2147483647
    end
    return hash
end

function EventLog:should_compact()
    return not self.compaction_in_progress and self.log_size >= self.compaction_threshold
end

function EventLog:start_compaction(tick)
    if self.compaction_in_progress then
        return false
    end

    self.compaction_in_progress = true
    self.compaction_start_tick = tick
    self.compaction_end_tick = tick + self.compaction_duration
    self.entries_before_compaction = #self.entries
    self.compaction_count = self.compaction_count + 1

    indras.log.info("Compaction started", {
        trace_id = ctx.trace_id,
        compaction_number = self.compaction_count,
        tick = tick,
        log_size = self.log_size,
        entry_count = #self.entries,
        threshold = self.compaction_threshold
    })

    return true
end

function EventLog:is_compacting()
    return self.compaction_in_progress
end

function EventLog:update_compaction(tick)
    if not self.compaction_in_progress then
        return false
    end

    if tick >= self.compaction_end_tick then
        -- Compaction complete: merge/compact entries
        self:_perform_compaction()
        self.compaction_in_progress = false
        self.entries_after_compaction = #self.entries

        indras.log.info("Compaction completed", {
            trace_id = ctx.trace_id,
            compaction_number = self.compaction_count,
            tick = tick,
            entries_before = self.entries_before_compaction,
            entries_after = self.entries_after_compaction,
            compaction_ratio = self.entries_before_compaction > 0
                and (1 - self.entries_after_compaction / self.entries_before_compaction) or 0,
            new_log_size = self.log_size
        })

        return true
    end

    return false
end

function EventLog:_perform_compaction()
    -- Simulate compaction: remove duplicate/obsolete entries
    -- In a real system, this would merge events, remove superseded entries, etc.

    local compacted = {}
    local seen_payloads = {}
    local removed_count = 0

    -- Keep most recent entries, remove duplicates
    for i = #self.entries, 1, -1 do
        local entry = self.entries[i]
        local payload_key = entry.type .. ":" .. tostring(entry.payload)

        if not seen_payloads[payload_key] then
            seen_payloads[payload_key] = true
            entry.compacted = true
            table.insert(compacted, 1, entry)
        else
            -- Entry is superseded, will be removed
            removed_count = removed_count + 1
        end
    end

    -- Also remove ~30% of old entries to simulate further compaction
    local final_entries = {}
    for i, entry in ipairs(compacted) do
        if i > #compacted * 0.3 or math.random() > 0.3 then
            table.insert(final_entries, entry)
        else
            removed_count = removed_count + 1
            -- Remove from checksum tracking (entry deleted)
            self.integrity_checksums[entry.id] = nil
        end
    end

    self.entries = final_entries
    self.compacted_entries = final_entries

    -- Recalculate log size after compaction
    self.log_size = 0
    for _, entry in ipairs(self.entries) do
        self.log_size = self.log_size + 64 + #tostring(entry.payload)
    end

    indras.log.debug("Compaction removed entries", {
        trace_id = ctx.trace_id,
        removed = removed_count,
        remaining = #self.entries
    })
end

function EventLog:verify_integrity(entry_id)
    -- Verify entry checksum matches stored value
    local stored_checksum = self.integrity_checksums[entry_id]
    if not stored_checksum then
        -- Entry was removed during compaction (expected for some entries)
        return nil, "entry_removed"
    end

    for _, entry in ipairs(self.entries) do
        if entry.id == entry_id then
            local computed = self:_compute_checksum(entry.id, entry.type, entry.payload)
            if computed == stored_checksum then
                return true, "valid"
            else
                return false, "checksum_mismatch"
            end
        end
    end

    return nil, "entry_not_found"
end

function EventLog:get_entry_count()
    return #self.entries
end

function EventLog:get_log_size()
    return self.log_size
end

function EventLog:get_stats()
    return {
        entry_count = #self.entries,
        log_size = self.log_size,
        compaction_count = self.compaction_count,
        compacting = self.compaction_in_progress
    }
end

--------------------------------------------------------------------------------
-- Simulation Setup
--------------------------------------------------------------------------------

-- Create mesh topology
local mesh = indras.MeshBuilder.new(config.peer_count):random(0.4)

indras.log.debug("Created compaction stress mesh", {
    trace_id = ctx.trace_id,
    peers = mesh:peer_count(),
    edges = mesh:edge_count()
})

-- Create simulation with moderate network dynamics
local sim_config = indras.SimConfig.new({
    wake_probability = 0.1,
    sleep_probability = 0.05,
    initial_online_probability = 0.9,
    max_ticks = config.max_ticks,
    trace_routing = false  -- Reduce overhead for high-volume testing
})

local sim = indras.Simulation.new(mesh, sim_config)
sim:initialize()

local all_peers = mesh:peers()

-- Create event log per peer (simulating distributed storage)
local peer_logs = {}
for _, peer in ipairs(all_peers) do
    peer_logs[tostring(peer)] = EventLog.new(
        config.log_compaction_threshold,
        config.compaction_duration
    )
end

--------------------------------------------------------------------------------
-- Metrics Tracking
--------------------------------------------------------------------------------

local metrics = {
    -- Write metrics
    total_events_written = 0,
    events_per_phase = {0, 0, 0, 0},
    write_throughput_samples = {},

    -- Compaction metrics
    total_compactions = 0,
    compaction_triggers = {},  -- tick when each compaction triggered
    compaction_durations = {},

    -- Integrity metrics
    integrity_checks = 0,
    integrity_valid = 0,
    integrity_removed = 0,  -- entries removed by compaction (expected)
    integrity_failures = 0, -- actual corruption (unexpected)

    -- Concurrent access metrics
    reads_during_compaction = 0,
    writes_during_compaction = 0,
    concurrent_access_success = 0,
    concurrent_access_failures = 0,

    -- Throughput impact
    throughput_normal = {},
    throughput_during_compaction = {},
}

--------------------------------------------------------------------------------
-- Helper Functions
--------------------------------------------------------------------------------

local function random_online_peer()
    local online = sim:online_peers()
    if #online == 0 then return nil end
    return online[math.random(#online)]
end

local function get_random_pair()
    local sender = random_online_peer()
    local receiver = random_online_peer()
    if sender and receiver and sender ~= receiver then
        return sender, receiver
    end
    return nil, nil
end

local function generate_payload(tick)
    -- Generate variable-size payload
    local payload_size = math.random(50, 200)
    local payload = string.format("tick_%d_data_%d_", tick, math.random(10000))
    while #payload < payload_size do
        payload = payload .. string.char(math.random(65, 90))
    end
    return payload
end

local function write_event(peer, event_type, tick)
    local log = peer_logs[tostring(peer)]
    if not log then return nil end

    local payload = generate_payload(tick)
    local entry_id, checksum = log:append(event_type, payload, tick)

    metrics.total_events_written = metrics.total_events_written + 1

    return entry_id, checksum
end

local function check_and_trigger_compaction(tick)
    local compactions_triggered = 0

    for peer_str, log in pairs(peer_logs) do
        if log:should_compact() then
            log:start_compaction(tick)
            compactions_triggered = compactions_triggered + 1
            metrics.total_compactions = metrics.total_compactions + 1
            table.insert(metrics.compaction_triggers, tick)
        end

        -- Update ongoing compaction
        if log:is_compacting() then
            log:update_compaction(tick)
        end
    end

    return compactions_triggered
end

local function is_any_compacting()
    for _, log in pairs(peer_logs) do
        if log:is_compacting() then
            return true
        end
    end
    return false
end

local function count_compacting_peers()
    local count = 0
    for _, log in pairs(peer_logs) do
        if log:is_compacting() then
            count = count + 1
        end
    end
    return count
end

local function verify_sample_integrity()
    local samples_checked = 0
    local valid = 0
    local removed = 0
    local failures = 0

    for peer_str, log in pairs(peer_logs) do
        -- Sample random entries from this log
        local entries_to_check = math.min(
            math.ceil(config.verification_samples / config.peer_count),
            log:get_entry_count()
        )

        for _ = 1, entries_to_check do
            if log:get_entry_count() > 0 then
                local entry = log.entries[math.random(log:get_entry_count())]
                local is_valid, status = log:verify_integrity(entry.id)

                samples_checked = samples_checked + 1

                if is_valid == true then
                    valid = valid + 1
                elseif is_valid == nil and status == "entry_removed" then
                    removed = removed + 1
                else
                    failures = failures + 1
                    indras.log.warn("Integrity failure detected", {
                        trace_id = ctx.trace_id,
                        peer = peer_str,
                        entry_id = entry.id,
                        status = status
                    })
                end
            end
        end
    end

    return samples_checked, valid, removed, failures
end

--------------------------------------------------------------------------------
-- Phase Definitions
--------------------------------------------------------------------------------

local phase1_end = math.floor(config.max_ticks * 0.30)  -- Steady writes
local phase2_end = math.floor(config.max_ticks * 0.55)  -- Compaction triggers
local phase3_end = math.floor(config.max_ticks * 0.80)  -- Concurrent access
local phase4_end = config.max_ticks                      -- Verification

indras.log.info("Running storage compaction stress simulation", {
    trace_id = ctx.trace_id,
    phase1_end = phase1_end,
    phase2_end = phase2_end,
    phase3_end = phase3_end,
    phase4_end = phase4_end
})

--------------------------------------------------------------------------------
-- Main Simulation Loop
--------------------------------------------------------------------------------

for tick = 1, config.max_ticks do
    local phase = 1
    local events_this_tick = 0
    local compactions_triggered = 0
    local compacting_peers = count_compacting_peers()

    -- Determine current phase
    if tick <= phase1_end then
        phase = 1
    elseif tick <= phase2_end then
        phase = 2
    elseif tick <= phase3_end then
        phase = 3
    else
        phase = 4
    end

    -- Phase 1: Steady high-volume writes to build up log
    if phase == 1 then
        for _ = 1, config.events_per_tick do
            local sender, receiver = get_random_pair()
            if sender and receiver then
                write_event(sender, "message_sent", tick)
                write_event(receiver, "message_received", tick)
                sim:send_message(sender, receiver, string.format("msg_%d", tick))
                events_this_tick = events_this_tick + 2
            end
        end

        -- Track throughput during normal operation
        table.insert(metrics.throughput_normal, events_this_tick)

    -- Phase 2: Continue writes, compaction should trigger
    elseif phase == 2 then
        -- Higher write rate to trigger compaction faster
        local burst_rate = math.ceil(config.events_per_tick * 1.5)

        for _ = 1, burst_rate do
            local sender, receiver = get_random_pair()
            if sender and receiver then
                write_event(sender, "burst_event", tick)
                sim:send_message(sender, receiver, string.format("burst_%d", tick))
                events_this_tick = events_this_tick + 1
            end
        end

        -- Check for compaction triggers
        compactions_triggered = check_and_trigger_compaction(tick)

        if compacting_peers > 0 then
            table.insert(metrics.throughput_during_compaction, events_this_tick)
        else
            table.insert(metrics.throughput_normal, events_this_tick)
        end

    -- Phase 3: Concurrent access during compaction
    elseif phase == 3 then
        -- Check/trigger compaction
        compactions_triggered = check_and_trigger_compaction(tick)

        -- Test concurrent reads and writes during compaction
        for _ = 1, config.events_per_tick do
            local sender, receiver = get_random_pair()
            if sender and receiver then
                local sender_log = peer_logs[tostring(sender)]

                -- Attempt write during compaction
                if sender_log:is_compacting() and math.random() < config.concurrent_access_rate then
                    local success = pcall(function()
                        write_event(sender, "concurrent_write", tick)
                    end)

                    metrics.writes_during_compaction = metrics.writes_during_compaction + 1
                    if success then
                        metrics.concurrent_access_success = metrics.concurrent_access_success + 1
                    else
                        metrics.concurrent_access_failures = metrics.concurrent_access_failures + 1
                    end
                    events_this_tick = events_this_tick + 1
                else
                    write_event(sender, "normal_write", tick)
                    events_this_tick = events_this_tick + 1
                end

                -- Attempt read during compaction
                if sender_log:is_compacting() and math.random() < config.concurrent_access_rate then
                    metrics.reads_during_compaction = metrics.reads_during_compaction + 1

                    local success = pcall(function()
                        if sender_log:get_entry_count() > 0 then
                            local random_entry = sender_log.entries[
                                math.random(sender_log:get_entry_count())
                            ]
                            sender_log:verify_integrity(random_entry.id)
                        end
                    end)

                    if success then
                        metrics.concurrent_access_success = metrics.concurrent_access_success + 1
                    else
                        metrics.concurrent_access_failures = metrics.concurrent_access_failures + 1
                    end
                end

                sim:send_message(sender, receiver, string.format("concurrent_%d", tick))
            end
        end

        if compacting_peers > 0 then
            table.insert(metrics.throughput_during_compaction, events_this_tick)
        else
            table.insert(metrics.throughput_normal, events_this_tick)
        end

    -- Phase 4: Verification after compaction
    else
        -- Reduced writes, focus on verification
        local reduced_rate = math.ceil(config.events_per_tick * 0.3)

        for _ = 1, reduced_rate do
            local sender, receiver = get_random_pair()
            if sender and receiver then
                write_event(sender, "verification_phase", tick)
                events_this_tick = events_this_tick + 1
            end
        end

        -- Complete any ongoing compaction
        check_and_trigger_compaction(tick)

        -- Periodic integrity verification
        if tick % 10 == 0 then
            local checked, valid, removed, failures = verify_sample_integrity()
            metrics.integrity_checks = metrics.integrity_checks + checked
            metrics.integrity_valid = metrics.integrity_valid + valid
            metrics.integrity_removed = metrics.integrity_removed + removed
            metrics.integrity_failures = metrics.integrity_failures + failures
        end

        table.insert(metrics.throughput_normal, events_this_tick)
    end

    metrics.events_per_phase[phase] = metrics.events_per_phase[phase] + events_this_tick

    -- Advance simulation
    sim:step()

    -- Progress logging
    if tick % math.floor(config.max_ticks / 10) == 0 or compactions_triggered > 0 then
        local total_log_size = 0
        local total_entries = 0
        for _, log in pairs(peer_logs) do
            total_log_size = total_log_size + log:get_log_size()
            total_entries = total_entries + log:get_entry_count()
        end

        indras.log.info("Compaction stress progress", {
            trace_id = ctx.trace_id,
            tick = tick,
            phase = phase,
            online_count = #sim:online_peers(),
            events_written = metrics.total_events_written,
            total_log_size = total_log_size,
            total_entries = total_entries,
            compactions = metrics.total_compactions,
            compacting_peers = compacting_peers,
            compactions_this_tick = compactions_triggered
        })
    end
end

--------------------------------------------------------------------------------
-- Final Verification and Statistics
--------------------------------------------------------------------------------

-- Final comprehensive integrity check
indras.log.info("Running final integrity verification", {
    trace_id = ctx.trace_id,
    samples = config.verification_samples
})

local final_checked, final_valid, final_removed, final_failures = verify_sample_integrity()
metrics.integrity_checks = metrics.integrity_checks + final_checked
metrics.integrity_valid = metrics.integrity_valid + final_valid
metrics.integrity_removed = metrics.integrity_removed + final_removed
metrics.integrity_failures = metrics.integrity_failures + final_failures

-- Calculate derived metrics
local function calculate_average(samples)
    if #samples == 0 then return 0 end
    local sum = 0
    for _, v in ipairs(samples) do
        sum = sum + v
    end
    return sum / #samples
end

local avg_throughput_normal = calculate_average(metrics.throughput_normal)
local avg_throughput_compaction = calculate_average(metrics.throughput_during_compaction)
local throughput_impact = avg_throughput_normal > 0
    and (1 - avg_throughput_compaction / avg_throughput_normal) or 0

-- Aggregate log statistics
local total_log_size = 0
local total_entries = 0
local total_compactions = 0
for _, log in pairs(peer_logs) do
    local stats = log:get_stats()
    total_log_size = total_log_size + stats.log_size
    total_entries = total_entries + stats.entry_count
    total_compactions = total_compactions + stats.compaction_count
end

-- Network statistics
local stats = sim.stats

-- Integrity rate
local integrity_rate = metrics.integrity_checks > 0
    and (metrics.integrity_valid / (metrics.integrity_checks - metrics.integrity_removed)) or 1.0

-- Concurrent access success rate
local concurrent_total = metrics.concurrent_access_success + metrics.concurrent_access_failures
local concurrent_success_rate = concurrent_total > 0
    and (metrics.concurrent_access_success / concurrent_total) or 1.0

indras.log.info("Storage compaction stress test completed", {
    trace_id = ctx.trace_id,
    final_tick = sim.tick,
    test_level = test_level,

    -- Write metrics
    total_events_written = metrics.total_events_written,
    phase1_events = metrics.events_per_phase[1],
    phase2_events = metrics.events_per_phase[2],
    phase3_events = metrics.events_per_phase[3],
    phase4_events = metrics.events_per_phase[4],

    -- Compaction metrics
    total_compactions = total_compactions,
    compaction_triggers = #metrics.compaction_triggers,
    final_log_size = total_log_size,
    final_entry_count = total_entries,

    -- Throughput metrics
    avg_throughput_normal = avg_throughput_normal,
    avg_throughput_during_compaction = avg_throughput_compaction,
    throughput_impact_percentage = throughput_impact * 100,

    -- Integrity metrics
    integrity_checks = metrics.integrity_checks,
    integrity_valid = metrics.integrity_valid,
    integrity_removed = metrics.integrity_removed,
    integrity_failures = metrics.integrity_failures,
    integrity_rate = integrity_rate,

    -- Concurrent access metrics
    reads_during_compaction = metrics.reads_during_compaction,
    writes_during_compaction = metrics.writes_during_compaction,
    concurrent_success_rate = concurrent_success_rate,

    -- Network metrics
    messages_sent = stats.messages_sent,
    messages_delivered = stats.messages_delivered,
    delivery_rate = stats:delivery_rate(),
    average_latency = stats:average_latency()
})

--------------------------------------------------------------------------------
-- Assertions
--------------------------------------------------------------------------------

-- Verify events were written
indras.assert.gt(metrics.total_events_written, 0, "Should have written events")

-- Verify compaction occurred
indras.assert.gt(total_compactions, 0,
    "Should have triggered at least one compaction cycle")

-- Verify data integrity after compaction (no corruption)
indras.assert.eq(metrics.integrity_failures, 0,
    "Should have no integrity failures after compaction")

-- Verify integrity rate is high (accounting for legitimately removed entries)
if metrics.integrity_checks > metrics.integrity_removed then
    indras.assert.ge(integrity_rate, 0.99,
        "Integrity rate should be >= 99% for surviving entries")
end

-- Verify concurrent access during compaction worked
if metrics.writes_during_compaction > 0 then
    indras.assert.ge(concurrent_success_rate, 0.95,
        "Concurrent access success rate should be >= 95%")
end

-- Verify throughput impact is reasonable (compaction shouldn't kill performance)
if #metrics.throughput_during_compaction > 0 then
    indras.assert.lt(throughput_impact, 0.5,
        "Throughput impact during compaction should be < 50%")
end

-- Verify network delivery
indras.assert.gt(stats:delivery_rate(), 0.0, "Should deliver some messages")

indras.log.info("Storage compaction stress test passed", {
    trace_id = ctx.trace_id,
    total_events_written = metrics.total_events_written,
    total_compactions = total_compactions,
    integrity_rate = integrity_rate,
    concurrent_success_rate = concurrent_success_rate,
    throughput_impact = throughput_impact,
    delivery_rate = stats:delivery_rate()
})

--------------------------------------------------------------------------------
-- Return Results for External Analysis
--------------------------------------------------------------------------------

return {
    level = test_level,

    -- Write metrics
    total_events_written = metrics.total_events_written,
    events_per_phase = metrics.events_per_phase,

    -- Compaction metrics
    total_compactions = total_compactions,
    compaction_triggers = #metrics.compaction_triggers,
    final_log_size = total_log_size,
    final_entry_count = total_entries,

    -- Throughput metrics
    avg_throughput_normal = avg_throughput_normal,
    avg_throughput_during_compaction = avg_throughput_compaction,
    throughput_impact = throughput_impact,

    -- Integrity metrics
    integrity_checks = metrics.integrity_checks,
    integrity_valid = metrics.integrity_valid,
    integrity_removed = metrics.integrity_removed,
    integrity_failures = metrics.integrity_failures,
    integrity_rate = integrity_rate,

    -- Concurrent access metrics
    reads_during_compaction = metrics.reads_during_compaction,
    writes_during_compaction = metrics.writes_during_compaction,
    concurrent_success_rate = concurrent_success_rate,

    -- Network metrics
    delivery_rate = stats:delivery_rate(),
    messages_delivered = stats.messages_delivered,
    average_latency = stats:average_latency(),
    average_hops = stats:average_hops()
}

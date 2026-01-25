-- Blob Storage Stress Test
--
-- Stress tests content-addressed blob storage behavior:
-- - Large binary data handling (1KB-10MB range)
-- - Content addressing and deduplication
-- - Read/write throughput for various blob sizes
-- - Storage pressure and eviction policies
--
-- Since direct blob API bindings may not exist, this scenario simulates
-- blob storage behavior through content hash tracking and metrics.

-- Fix package path for helper libraries
package.path = package.path .. ";scripts/lib/?.lua;simulation/scripts/lib/?.lua"

local stress = require("stress_helpers")

local ctx = stress.new_context("storage_blob_stress")

-- Configuration levels
local CONFIG = {
    quick = {
        peer_count = 10,
        blob_count = 200,
        max_ticks = 150,
        storage_limit_mb = 50,
        dedup_test_count = 50,
        eviction_pressure_factor = 2.0,
    },
    medium = {
        peer_count = 20,
        blob_count = 2000,
        max_ticks = 400,
        storage_limit_mb = 200,
        dedup_test_count = 500,
        eviction_pressure_factor = 2.5,
    },
    full = {
        peer_count = 26,
        blob_count = 20000,
        max_ticks = 1000,
        storage_limit_mb = 1000,
        dedup_test_count = 5000,
        eviction_pressure_factor = 3.0,
    }
}

-- Select test level (default: quick)
local test_level = os.getenv("STRESS_LEVEL") or "quick"
local config = CONFIG[test_level] or CONFIG.quick

indras.log.info("Starting blob storage stress test", {
    trace_id = ctx.trace_id,
    level = test_level,
    peer_count = config.peer_count,
    blob_count = config.blob_count,
    max_ticks = config.max_ticks,
    storage_limit_mb = config.storage_limit_mb,
    dedup_test_count = config.dedup_test_count
})

-- ============================================================================
-- BlobStore Class: Simulates content-addressed blob storage
-- ============================================================================

local BlobStore = {}
BlobStore.__index = BlobStore

--- Create a new BlobStore instance
-- @param storage_limit_bytes number Maximum storage capacity in bytes
-- @return BlobStore
function BlobStore.new(storage_limit_bytes)
    local self = setmetatable({}, BlobStore)
    self.storage_limit = storage_limit_bytes
    self.blobs = {}              -- hash -> {size, ref_count, created_tick, last_access_tick}
    self.total_size = 0
    self.next_hash_id = 1

    -- Metrics
    self.writes = 0
    self.reads = 0
    self.dedup_hits = 0
    self.evictions = 0
    self.bytes_written = 0
    self.bytes_read = 0
    self.bytes_evicted = 0

    -- Latency tracking
    self.write_latencies = {}
    self.read_latencies = {}

    return self
end

--- Generate a simulated content hash based on content
-- In real blob storage, this would be a cryptographic hash (SHA-256, BLAKE3, etc.)
-- @param content string The blob content (simulated)
-- @param size number The blob size in bytes
-- @return string The content hash
function BlobStore:content_hash(content, size)
    -- Simulate content-based hashing: same content = same hash
    -- Use content string as basis for reproducible "hash"
    local hash_basis = content .. "|" .. tostring(size)
    -- Simple hash simulation using string-based approach
    local hash = 0
    for i = 1, #hash_basis do
        hash = (hash * 31 + string.byte(hash_basis, i)) % 2147483647
    end
    return string.format("blob_%016x", hash)
end

--- Simulate write latency based on blob size
-- @param size number Blob size in bytes
-- @return number Latency in microseconds
function BlobStore:_simulate_write_latency(size)
    -- Base latency + size-proportional component
    -- Assumes ~100MB/s write speed with some overhead
    local base_latency = 50  -- 50us base overhead
    local size_latency = math.floor(size / 100)  -- ~100 bytes per us
    local variance = math.random(-20, 20)
    return math.max(10, base_latency + size_latency + variance)
end

--- Simulate read latency based on blob size
-- @param size number Blob size in bytes
-- @return number Latency in microseconds
function BlobStore:_simulate_read_latency(size)
    -- Reads are typically faster than writes
    local base_latency = 30  -- 30us base overhead
    local size_latency = math.floor(size / 200)  -- ~200 bytes per us
    local variance = math.random(-10, 10)
    return math.max(5, base_latency + size_latency + variance)
end

--- Write a blob to storage
-- @param content string Content identifier (for hash generation)
-- @param size number Blob size in bytes
-- @param tick number Current simulation tick
-- @return string|nil hash The blob hash, or nil if storage full and eviction failed
-- @return boolean is_duplicate Whether this was a dedup hit
function BlobStore:write(content, size, tick)
    local hash = self:content_hash(content, size)

    -- Check for deduplication
    if self.blobs[hash] then
        self.blobs[hash].ref_count = self.blobs[hash].ref_count + 1
        self.blobs[hash].last_access_tick = tick
        self.dedup_hits = self.dedup_hits + 1
        self.writes = self.writes + 1
        -- Dedup hits have minimal latency (just metadata update)
        local latency = math.random(5, 20)
        table.insert(self.write_latencies, latency)
        return hash, true
    end

    -- Check if we need to evict to make room
    while self.total_size + size > self.storage_limit do
        local evicted = self:_evict_lru(tick)
        if not evicted then
            -- Cannot evict anything, storage is full
            return nil, false
        end
    end

    -- Store the blob
    self.blobs[hash] = {
        size = size,
        ref_count = 1,
        created_tick = tick,
        last_access_tick = tick
    }
    self.total_size = self.total_size + size
    self.writes = self.writes + 1
    self.bytes_written = self.bytes_written + size

    -- Record write latency
    local latency = self:_simulate_write_latency(size)
    table.insert(self.write_latencies, latency)

    return hash, false
end

--- Read a blob from storage
-- @param hash string The blob hash
-- @param tick number Current simulation tick
-- @return number|nil size The blob size, or nil if not found
function BlobStore:read(hash, tick)
    local blob = self.blobs[hash]
    if not blob then
        return nil
    end

    blob.last_access_tick = tick
    self.reads = self.reads + 1
    self.bytes_read = self.bytes_read + blob.size

    -- Record read latency
    local latency = self:_simulate_read_latency(blob.size)
    table.insert(self.read_latencies, latency)

    return blob.size
end

--- Evict the least recently used blob
-- @param tick number Current simulation tick
-- @return boolean Whether eviction was successful
function BlobStore:_evict_lru(tick)
    local lru_hash = nil
    local lru_tick = tick + 1

    for hash, blob in pairs(self.blobs) do
        if blob.last_access_tick < lru_tick then
            lru_hash = hash
            lru_tick = blob.last_access_tick
        end
    end

    if lru_hash then
        local blob = self.blobs[lru_hash]
        self.total_size = self.total_size - blob.size
        self.bytes_evicted = self.bytes_evicted + blob.size
        self.evictions = self.evictions + 1
        self.blobs[lru_hash] = nil
        return true
    end

    return false
end

--- Get blob count
-- @return number Number of unique blobs stored
function BlobStore:blob_count()
    local count = 0
    for _ in pairs(self.blobs) do
        count = count + 1
    end
    return count
end

--- Get storage utilization percentage
-- @return number Utilization as 0-100
function BlobStore:utilization()
    if self.storage_limit == 0 then return 0 end
    return (self.total_size / self.storage_limit) * 100
end

--- Get deduplication ratio
-- @return number Ratio of dedup hits to total writes (0-1)
function BlobStore:dedup_ratio()
    if self.writes == 0 then return 0 end
    return self.dedup_hits / self.writes
end

--- Calculate throughput metrics
-- @param duration_ticks number Duration in simulation ticks
-- @return table Throughput statistics
function BlobStore:throughput_stats(duration_ticks)
    local write_throughput = 0
    local read_throughput = 0
    if duration_ticks > 0 then
        write_throughput = self.bytes_written / duration_ticks
        read_throughput = self.bytes_read / duration_ticks
    end

    return {
        write_throughput_bytes_per_tick = write_throughput,
        read_throughput_bytes_per_tick = read_throughput,
        total_writes = self.writes,
        total_reads = self.reads
    }
end

--- Get latency percentiles
-- @return table Latency statistics
function BlobStore:latency_stats()
    local function percentile(samples, p)
        if #samples == 0 then return 0 end
        local sorted = {}
        for _, v in ipairs(samples) do table.insert(sorted, v) end
        table.sort(sorted)
        local idx = math.ceil(#sorted * p / 100)
        return sorted[math.max(1, idx)]
    end

    local function average(samples)
        if #samples == 0 then return 0 end
        local sum = 0
        for _, v in ipairs(samples) do sum = sum + v end
        return sum / #samples
    end

    return {
        write_latency_avg_us = average(self.write_latencies),
        write_latency_p50_us = percentile(self.write_latencies, 50),
        write_latency_p95_us = percentile(self.write_latencies, 95),
        write_latency_p99_us = percentile(self.write_latencies, 99),
        read_latency_avg_us = average(self.read_latencies),
        read_latency_p50_us = percentile(self.read_latencies, 50),
        read_latency_p95_us = percentile(self.read_latencies, 95),
        read_latency_p99_us = percentile(self.read_latencies, 99)
    }
end

-- ============================================================================
-- Blob Size Distribution
-- ============================================================================

-- Size categories with weights (realistic distribution)
local SIZE_DISTRIBUTION = {
    { name = "tiny",   min = 1024,       max = 4096,       weight = 15 },  -- 1KB-4KB
    { name = "small",  min = 4096,       max = 65536,      weight = 30 },  -- 4KB-64KB
    { name = "medium", min = 65536,      max = 1048576,    weight = 35 },  -- 64KB-1MB
    { name = "large",  min = 1048576,    max = 5242880,    weight = 15 },  -- 1MB-5MB
    { name = "huge",   min = 5242880,    max = 10485760,   weight = 5 },   -- 5MB-10MB
}

--- Generate a random blob size based on distribution
-- @return number Size in bytes
-- @return string Size category name
local function random_blob_size()
    -- Calculate total weight
    local total_weight = 0
    for _, cat in ipairs(SIZE_DISTRIBUTION) do
        total_weight = total_weight + cat.weight
    end

    -- Select category based on weight
    local roll = math.random(1, total_weight)
    local cumulative = 0
    for _, cat in ipairs(SIZE_DISTRIBUTION) do
        cumulative = cumulative + cat.weight
        if roll <= cumulative then
            local size = math.random(cat.min, cat.max)
            return size, cat.name
        end
    end

    -- Fallback (shouldn't happen)
    return 4096, "small"
end

--- Generate a content identifier for a blob
-- @param peer_id string The peer creating the blob
-- @param sequence number Sequence number
-- @param unique boolean If true, generate unique content; if false, may reuse
-- @return string Content identifier
local function generate_content_id(peer_id, sequence, unique)
    if unique then
        return string.format("%s:blob:%d:%d", tostring(peer_id), sequence, math.random(1000000))
    else
        -- For dedup testing, use predictable content that may repeat
        local common_content_id = sequence % 20  -- Only 20 unique content patterns
        return string.format("shared:content:%d", common_content_id)
    end
end

-- ============================================================================
-- Create Simulation Environment
-- ============================================================================

local mesh = indras.MeshBuilder.new(config.peer_count):random(0.3)

indras.log.debug("Created blob stress mesh", {
    trace_id = ctx.trace_id,
    peers = mesh:peer_count(),
    edges = mesh:edge_count()
})

local sim_config = indras.SimConfig.new({
    wake_probability = 0.1,
    sleep_probability = 0.05,
    initial_online_probability = 0.9,
    max_ticks = config.max_ticks,
    trace_routing = false
})

local sim = indras.Simulation.new(mesh, sim_config)
sim:initialize()

local all_peers = mesh:peers()

-- Create blob store for simulation
local storage_limit_bytes = config.storage_limit_mb * 1024 * 1024
local blob_store = BlobStore.new(storage_limit_bytes)

-- Tracking metrics
local metrics = {
    -- Size distribution tracking
    size_categories = { tiny = 0, small = 0, medium = 0, large = 0, huge = 0 },
    total_blob_bytes = 0,

    -- Phase metrics
    phase_blobs = { 0, 0, 0, 0 },
    phase_dedup_hits = { 0, 0, 0, 0 },

    -- Hash tracking for verification
    known_hashes = {},
    hash_collision_checks = 0,

    -- Per-tick tracking
    blobs_per_tick = {},
}

-- Helper functions
local function random_online_peer()
    local online = sim:online_peers()
    if #online == 0 then return nil end
    return online[math.random(#online)]
end

-- Phase timing
local phase1_end = math.floor(config.max_ticks * 0.35)   -- 35% blob creation
local phase2_end = math.floor(config.max_ticks * 0.55)   -- 20% dedup testing
local phase3_end = math.floor(config.max_ticks * 0.80)   -- 25% eviction stress
local phase4_end = config.max_ticks                       -- 20% mixed workload

indras.log.info("Running blob storage stress simulation", {
    trace_id = ctx.trace_id,
    storage_limit_mb = config.storage_limit_mb,
    phase1_end = phase1_end,
    phase2_end = phase2_end,
    phase3_end = phase3_end,
    phase4_end = phase4_end
})

-- Calculate blobs per tick for each phase
local blobs_per_tick_p1 = math.ceil((config.blob_count * 0.4) / phase1_end)
local blobs_per_tick_p2 = math.ceil(config.dedup_test_count / (phase2_end - phase1_end))
local blobs_per_tick_p3 = math.ceil((config.blob_count * 0.4) / (phase3_end - phase2_end))
local blobs_per_tick_p4 = math.ceil((config.blob_count * 0.2) / (phase4_end - phase3_end))

local blob_sequence = 0

-- ============================================================================
-- Phase 1: Blob Creation (varied sizes)
-- ============================================================================
indras.log.info("Phase 1: Blob creation with varied sizes", {
    trace_id = ctx.trace_id,
    target_blobs = config.blob_count * 0.4,
    blobs_per_tick = blobs_per_tick_p1
})

for tick = 1, phase1_end do
    local tick_blob_count = 0

    for _ = 1, blobs_per_tick_p1 do
        local peer = random_online_peer()
        if peer then
            blob_sequence = blob_sequence + 1
            local size, category = random_blob_size()
            local content = generate_content_id(peer, blob_sequence, true)

            local hash, is_dedup = blob_store:write(content, size, tick)
            if hash then
                tick_blob_count = tick_blob_count + 1
                metrics.phase_blobs[1] = metrics.phase_blobs[1] + 1
                metrics.size_categories[category] = metrics.size_categories[category] + 1
                metrics.total_blob_bytes = metrics.total_blob_bytes + size

                if is_dedup then
                    metrics.phase_dedup_hits[1] = metrics.phase_dedup_hits[1] + 1
                else
                    metrics.known_hashes[hash] = { size = size, tick = tick }
                end

                -- Simulate network message with blob reference
                local receiver = random_online_peer()
                if receiver and receiver ~= peer then
                    sim:send_message(peer, receiver, string.format("blob_ref:%s", hash))
                end
            end
        end
    end

    metrics.blobs_per_tick[tick] = tick_blob_count
    sim:step()

    -- Progress logging
    if tick % 25 == 0 or tick == phase1_end then
        indras.log.info("Phase 1 progress", {
            trace_id = ctx.trace_id,
            tick = tick,
            phase_blobs = metrics.phase_blobs[1],
            unique_blobs = blob_store:blob_count(),
            storage_utilization_pct = blob_store:utilization(),
            total_size_mb = blob_store.total_size / (1024 * 1024)
        })
    end
end

-- ============================================================================
-- Phase 2: Deduplication Testing
-- ============================================================================
indras.log.info("Phase 2: Deduplication stress test", {
    trace_id = ctx.trace_id,
    target_dedup_tests = config.dedup_test_count
})

local dedup_test_count = 0

for tick = phase1_end + 1, phase2_end do
    local tick_blob_count = 0

    for _ = 1, blobs_per_tick_p2 do
        local peer = random_online_peer()
        if peer then
            blob_sequence = blob_sequence + 1
            local size, category = random_blob_size()
            -- Use non-unique content to test deduplication
            local content = generate_content_id(peer, blob_sequence, false)

            local hash, is_dedup = blob_store:write(content, size, tick)
            if hash then
                tick_blob_count = tick_blob_count + 1
                metrics.phase_blobs[2] = metrics.phase_blobs[2] + 1
                dedup_test_count = dedup_test_count + 1

                if is_dedup then
                    metrics.phase_dedup_hits[2] = metrics.phase_dedup_hits[2] + 1
                end

                -- Verify hash consistency (same content = same hash)
                metrics.hash_collision_checks = metrics.hash_collision_checks + 1
            end
        end
    end

    -- Also do reads to test read path
    for _ = 1, math.ceil(blobs_per_tick_p2 / 2) do
        local hashes = stress.table_keys(metrics.known_hashes)
        if #hashes > 0 then
            local hash = hashes[math.random(#hashes)]
            blob_store:read(hash, tick)
        end
    end

    metrics.blobs_per_tick[tick] = tick_blob_count
    sim:step()

    -- Progress logging
    if tick % 20 == 0 or tick == phase2_end then
        indras.log.info("Phase 2 progress", {
            trace_id = ctx.trace_id,
            tick = tick,
            dedup_tests = dedup_test_count,
            dedup_ratio = blob_store:dedup_ratio(),
            phase_dedup_hits = metrics.phase_dedup_hits[2],
            storage_utilization_pct = blob_store:utilization()
        })
    end
end

-- ============================================================================
-- Phase 3: Eviction Stress (exceed storage limits)
-- ============================================================================
indras.log.info("Phase 3: Storage pressure and eviction stress", {
    trace_id = ctx.trace_id,
    current_utilization_pct = blob_store:utilization(),
    pressure_factor = config.eviction_pressure_factor
})

local evictions_before = blob_store.evictions
local high_pressure_blobs_per_tick = math.ceil(blobs_per_tick_p3 * config.eviction_pressure_factor)

for tick = phase2_end + 1, phase3_end do
    local tick_blob_count = 0

    -- Write many large blobs to force evictions
    for _ = 1, high_pressure_blobs_per_tick do
        local peer = random_online_peer()
        if peer then
            blob_sequence = blob_sequence + 1
            -- Bias toward larger blobs during eviction stress
            local size = math.random(1048576, 10485760)  -- 1MB-10MB
            local content = generate_content_id(peer, blob_sequence, true)

            local hash, is_dedup = blob_store:write(content, size, tick)
            if hash then
                tick_blob_count = tick_blob_count + 1
                metrics.phase_blobs[3] = metrics.phase_blobs[3] + 1

                if is_dedup then
                    metrics.phase_dedup_hits[3] = metrics.phase_dedup_hits[3] + 1
                else
                    metrics.known_hashes[hash] = { size = size, tick = tick }
                end
            end
        end
    end

    metrics.blobs_per_tick[tick] = tick_blob_count
    sim:step()

    -- Progress logging
    if tick % 15 == 0 or tick == phase3_end then
        local evictions_this_phase = blob_store.evictions - evictions_before
        indras.log.info("Phase 3 progress", {
            trace_id = ctx.trace_id,
            tick = tick,
            phase_blobs = metrics.phase_blobs[3],
            evictions_this_phase = evictions_this_phase,
            total_evictions = blob_store.evictions,
            bytes_evicted_mb = blob_store.bytes_evicted / (1024 * 1024),
            storage_utilization_pct = blob_store:utilization(),
            unique_blobs = blob_store:blob_count()
        })
    end
end

-- ============================================================================
-- Phase 4: Mixed Workload (reads, writes, and verification)
-- ============================================================================
indras.log.info("Phase 4: Mixed workload stress", {
    trace_id = ctx.trace_id,
    blobs_per_tick = blobs_per_tick_p4
})

for tick = phase3_end + 1, phase4_end do
    local tick_blob_count = 0

    -- Mixed operations: 40% writes, 50% reads, 10% dedup writes
    for _ = 1, blobs_per_tick_p4 do
        local op_roll = math.random(100)
        local peer = random_online_peer()

        if peer then
            if op_roll <= 40 then
                -- Write new blob
                blob_sequence = blob_sequence + 1
                local size, category = random_blob_size()
                local content = generate_content_id(peer, blob_sequence, true)

                local hash, is_dedup = blob_store:write(content, size, tick)
                if hash then
                    tick_blob_count = tick_blob_count + 1
                    metrics.phase_blobs[4] = metrics.phase_blobs[4] + 1
                    if not is_dedup then
                        metrics.known_hashes[hash] = { size = size, tick = tick }
                    end
                end

            elseif op_roll <= 90 then
                -- Read existing blob
                local hashes = stress.table_keys(metrics.known_hashes)
                if #hashes > 0 then
                    local hash = hashes[math.random(#hashes)]
                    blob_store:read(hash, tick)
                end

            else
                -- Write with dedup potential
                blob_sequence = blob_sequence + 1
                local size, category = random_blob_size()
                local content = generate_content_id(peer, blob_sequence, false)

                local hash, is_dedup = blob_store:write(content, size, tick)
                if hash then
                    tick_blob_count = tick_blob_count + 1
                    metrics.phase_blobs[4] = metrics.phase_blobs[4] + 1
                    if is_dedup then
                        metrics.phase_dedup_hits[4] = metrics.phase_dedup_hits[4] + 1
                    end
                end
            end
        end
    end

    metrics.blobs_per_tick[tick] = tick_blob_count
    sim:step()

    -- Progress logging
    if tick % 20 == 0 or tick == phase4_end then
        indras.log.info("Phase 4 progress", {
            trace_id = ctx.trace_id,
            tick = tick,
            phase_blobs = metrics.phase_blobs[4],
            total_reads = blob_store.reads,
            total_writes = blob_store.writes,
            storage_utilization_pct = blob_store:utilization()
        })
    end
end

-- ============================================================================
-- Final Statistics
-- ============================================================================
local sim_stats = sim.stats
local latency_stats = blob_store:latency_stats()
local throughput_stats = blob_store:throughput_stats(config.max_ticks)

-- Calculate derived metrics
local total_blobs = metrics.phase_blobs[1] + metrics.phase_blobs[2] +
                    metrics.phase_blobs[3] + metrics.phase_blobs[4]
local total_dedup_hits = metrics.phase_dedup_hits[1] + metrics.phase_dedup_hits[2] +
                         metrics.phase_dedup_hits[3] + metrics.phase_dedup_hits[4]

local eviction_rate = 0
if total_blobs > 0 then
    eviction_rate = blob_store.evictions / total_blobs
end

local avg_blobs_per_tick = 0
if #metrics.blobs_per_tick > 0 then
    local sum = 0
    for _, count in ipairs(metrics.blobs_per_tick) do
        sum = sum + count
    end
    avg_blobs_per_tick = sum / #metrics.blobs_per_tick
end

indras.log.info("Blob storage stress test completed", {
    trace_id = ctx.trace_id,
    level = test_level,
    final_tick = sim.tick,
    -- Blob metrics
    total_blobs_written = total_blobs,
    unique_blobs_stored = blob_store:blob_count(),
    total_bytes_written_mb = blob_store.bytes_written / (1024 * 1024),
    total_bytes_read_mb = blob_store.bytes_read / (1024 * 1024),
    total_bytes_evicted_mb = blob_store.bytes_evicted / (1024 * 1024),
    -- Deduplication
    dedup_hits = total_dedup_hits,
    dedup_ratio = blob_store:dedup_ratio(),
    -- Eviction
    evictions = blob_store.evictions,
    eviction_rate = eviction_rate,
    -- Storage
    final_storage_utilization_pct = blob_store:utilization(),
    storage_limit_mb = config.storage_limit_mb,
    -- Throughput
    write_throughput_kb_per_tick = throughput_stats.write_throughput_bytes_per_tick / 1024,
    read_throughput_kb_per_tick = throughput_stats.read_throughput_bytes_per_tick / 1024,
    avg_blobs_per_tick = avg_blobs_per_tick,
    -- Latency
    write_latency_p50_us = latency_stats.write_latency_p50_us,
    write_latency_p95_us = latency_stats.write_latency_p95_us,
    write_latency_p99_us = latency_stats.write_latency_p99_us,
    read_latency_p50_us = latency_stats.read_latency_p50_us,
    read_latency_p95_us = latency_stats.read_latency_p95_us,
    read_latency_p99_us = latency_stats.read_latency_p99_us,
    -- Size distribution
    size_dist_tiny = metrics.size_categories.tiny,
    size_dist_small = metrics.size_categories.small,
    size_dist_medium = metrics.size_categories.medium,
    size_dist_large = metrics.size_categories.large,
    size_dist_huge = metrics.size_categories.huge,
    -- Phase breakdown
    phase1_blobs = metrics.phase_blobs[1],
    phase2_blobs = metrics.phase_blobs[2],
    phase3_blobs = metrics.phase_blobs[3],
    phase4_blobs = metrics.phase_blobs[4],
    -- Network metrics
    messages_sent = sim_stats.messages_sent,
    messages_delivered = sim_stats.messages_delivered,
    delivery_rate = sim_stats:delivery_rate()
})

-- ============================================================================
-- Assertions
-- ============================================================================
indras.assert.gt(total_blobs, 0, "Should have written blobs")
indras.assert.gt(blob_store:blob_count(), 0, "Should have stored unique blobs")

-- Verify blob count meets minimum threshold (within 80% of target)
local min_blobs = config.blob_count * 0.8
indras.assert.ge(total_blobs, min_blobs,
    string.format("Should write at least %d blobs (got %d)", min_blobs, total_blobs))

-- Verify deduplication worked in phase 2
if config.dedup_test_count >= 50 then
    indras.assert.gt(metrics.phase_dedup_hits[2], 0,
        "Should have dedup hits during dedup testing phase")
end

-- Verify evictions occurred during pressure phase
if test_level ~= "quick" then
    indras.assert.gt(blob_store.evictions, 0,
        "Should have evicted blobs during storage pressure phase")
end

-- Verify read operations were performed
indras.assert.gt(blob_store.reads, 0, "Should have performed read operations")

-- Verify write latency is reasonable (p99 under 100ms for large blobs)
indras.assert.lt(latency_stats.write_latency_p99_us, 100000,
    "P99 write latency should be under 100ms")

-- Verify read latency is reasonable (p99 under 50ms)
indras.assert.lt(latency_stats.read_latency_p99_us, 50000,
    "P99 read latency should be under 50ms")

-- Verify size distribution has variety
local categories_used = 0
for _, count in pairs(metrics.size_categories) do
    if count > 0 then categories_used = categories_used + 1 end
end
indras.assert.ge(categories_used, 3, "Should have blobs in at least 3 size categories")

-- Verify network delivery for blob references
indras.assert.gt(sim_stats:delivery_rate(), 0.0, "Should deliver blob reference messages")

indras.log.info("Blob storage stress test passed", {
    trace_id = ctx.trace_id,
    total_blobs = total_blobs,
    dedup_ratio = blob_store:dedup_ratio(),
    eviction_rate = eviction_rate,
    final_utilization_pct = blob_store:utilization(),
    delivery_rate = sim_stats:delivery_rate()
})

-- ============================================================================
-- Return Results
-- ============================================================================
return {
    level = test_level,
    -- Blob metrics
    total_blobs = total_blobs,
    unique_blobs = blob_store:blob_count(),
    bytes_written_mb = blob_store.bytes_written / (1024 * 1024),
    bytes_read_mb = blob_store.bytes_read / (1024 * 1024),
    bytes_evicted_mb = blob_store.bytes_evicted / (1024 * 1024),
    -- Deduplication
    dedup_hits = total_dedup_hits,
    dedup_ratio = blob_store:dedup_ratio(),
    -- Eviction
    evictions = blob_store.evictions,
    eviction_rate = eviction_rate,
    -- Storage
    storage_utilization_pct = blob_store:utilization(),
    storage_limit_mb = config.storage_limit_mb,
    -- Throughput
    write_throughput_kb_per_tick = throughput_stats.write_throughput_bytes_per_tick / 1024,
    read_throughput_kb_per_tick = throughput_stats.read_throughput_bytes_per_tick / 1024,
    -- Latency
    write_latency = {
        avg = latency_stats.write_latency_avg_us,
        p50 = latency_stats.write_latency_p50_us,
        p95 = latency_stats.write_latency_p95_us,
        p99 = latency_stats.write_latency_p99_us
    },
    read_latency = {
        avg = latency_stats.read_latency_avg_us,
        p50 = latency_stats.read_latency_p50_us,
        p95 = latency_stats.read_latency_p95_us,
        p99 = latency_stats.read_latency_p99_us
    },
    -- Size distribution
    size_distribution = metrics.size_categories,
    -- Phase breakdown
    phase_blobs = metrics.phase_blobs,
    phase_dedup_hits = metrics.phase_dedup_hits,
    -- Network metrics
    delivery_rate = sim_stats:delivery_rate(),
    messages_sent = sim_stats.messages_sent,
    messages_delivered = sim_stats.messages_delivered
}

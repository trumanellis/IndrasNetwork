-- Messaging Schema Validation Stress Test
--
-- Stress tests schema registry, content validation, and schema migration
-- for the indras-messaging module. Simulates schema behavior since direct
-- schema API bindings may not exist.
--
-- Tests:
-- 1. Schema Registry: registration and lookup operations
-- 2. Content Validation: validate message content against schemas
-- 3. Schema Migration: version transitions under load
-- 4. Validation Throughput: measure validation performance

-- Fix package path for helper libraries
package.path = package.path .. ";scripts/lib/?.lua;simulation/scripts/lib/?.lua"

local pq = require("pq_helpers")
local stress = require("stress_helpers")

-- Configuration levels
local CONFIG = {
    quick = {
        peers = 10,
        schemas = 5,
        messages = 200,
        migrations = 3,
        ticks = 200,
    },
    medium = {
        peers = 20,
        schemas = 20,
        messages = 2000,
        migrations = 10,
        ticks = 500,
    },
    full = {
        peers = 26,
        schemas = 50,
        messages = 20000,
        migrations = 25,
        ticks = 1500,
    }
}

-- Select configuration (default to quick)
local config_level = os.getenv("STRESS_LEVEL") or "quick"
local cfg = CONFIG[config_level] or CONFIG.quick

-- Create correlation context
local ctx = pq.new_context("messaging_schema_stress")
ctx = ctx:with_tag("level", config_level)

indras.log.info("Starting messaging schema validation stress test", {
    trace_id = ctx.trace_id,
    config_level = config_level,
    peers = cfg.peers,
    schemas = cfg.schemas,
    messages = cfg.messages,
    migrations = cfg.migrations,
    ticks = cfg.ticks
})

-- ============================================================================
-- SCHEMA CLASS: Represents a message schema with versioning
-- ============================================================================

local Schema = {}
Schema.__index = Schema

--- Create a new schema
-- @param id string Schema identifier
-- @param content_type string Content type (text, json, binary, custom)
-- @param version number Schema version
-- @return Schema
function Schema.new(id, content_type, version)
    local self = setmetatable({}, Schema)
    self.id = id
    self.content_type = content_type
    self.version = version or 1
    self.created_at = os.time()
    self.fields = {}
    self.validation_complexity = Schema._complexity_for_type(content_type)
    return self
end

--- Get validation complexity based on content type
-- @param content_type string
-- @return number Complexity multiplier (affects validation latency)
function Schema._complexity_for_type(content_type)
    local complexity_map = {
        text = 1.0,       -- Simple text validation
        json = 2.5,       -- JSON parsing and structure validation
        binary = 1.5,     -- Binary format validation
        custom = 4.0,     -- Custom schema with complex rules
    }
    return complexity_map[content_type] or 2.0
end

--- Add a field definition to the schema
-- @param name string Field name
-- @param field_type string Field type (string, number, boolean, array, object)
-- @param required boolean Whether field is required
function Schema:add_field(name, field_type, required)
    table.insert(self.fields, {
        name = name,
        type = field_type,
        required = required or false
    })
    -- Increase complexity for each field
    self.validation_complexity = self.validation_complexity + 0.1 * #self.fields
end

--- Create a new version of this schema (for migration)
-- @return Schema New schema with incremented version
function Schema:new_version()
    local new_schema = Schema.new(self.id, self.content_type, self.version + 1)
    -- Copy fields
    for _, field in ipairs(self.fields) do
        new_schema:add_field(field.name, field.type, field.required)
    end
    return new_schema
end

--- Calculate validation latency in microseconds
-- @return number Latency in microseconds
function Schema:validation_latency()
    local base_latency = 50  -- Base 50us
    local variance = 20
    return stress.random_latency(base_latency * self.validation_complexity, variance * self.validation_complexity)
end

--- Validate content against this schema (simulated)
-- @param content_size number Size of content in bytes
-- @return boolean success, number latency_us
function Schema:validate(content_size)
    local latency = self:validation_latency()
    -- Add latency based on content size (1us per 100 bytes)
    latency = latency + math.floor(content_size / 100)

    -- Determine success based on content type
    -- Different types have different failure modes
    local failure_rate = 0.001  -- Base 0.1% failure rate
    if self.content_type == "json" then
        failure_rate = 0.005    -- JSON has higher parse failure rate
    elseif self.content_type == "custom" then
        failure_rate = 0.003    -- Custom schemas have moderate failure
    end

    local success = math.random() > failure_rate
    return success, latency
end

-- ============================================================================
-- SCHEMA REGISTRY CLASS: Manages schema registration and lookup
-- ============================================================================

local SchemaRegistry = {}
SchemaRegistry.__index = SchemaRegistry

--- Create a new schema registry
-- @return SchemaRegistry
function SchemaRegistry.new()
    local self = setmetatable({}, SchemaRegistry)
    self.schemas = {}           -- id -> { version -> Schema }
    self.lookup_cache = {}      -- Cache for fast lookups
    self.stats = {
        registrations = 0,
        lookups = 0,
        cache_hits = 0,
        cache_misses = 0,
        migrations = 0,
    }
    return self
end

--- Register a schema
-- @param schema Schema The schema to register
-- @return boolean success, number latency_us
function SchemaRegistry:register(schema)
    local latency = stress.random_latency(100, 30)  -- ~100us for registration

    if not self.schemas[schema.id] then
        self.schemas[schema.id] = {}
    end

    self.schemas[schema.id][schema.version] = schema
    self.stats.registrations = self.stats.registrations + 1

    -- Invalidate cache for this schema
    self.lookup_cache[schema.id] = nil

    return true, latency
end

--- Lookup a schema by ID and optional version
-- @param id string Schema ID
-- @param version number Optional version (nil = latest)
-- @return Schema|nil, number latency_us
function SchemaRegistry:lookup(id, version)
    self.stats.lookups = self.stats.lookups + 1

    -- Check cache first
    local cache_key = id .. ":" .. (version or "latest")
    if self.lookup_cache[cache_key] then
        self.stats.cache_hits = self.stats.cache_hits + 1
        return self.lookup_cache[cache_key], stress.random_latency(5, 2)  -- ~5us cache hit
    end

    self.stats.cache_misses = self.stats.cache_misses + 1
    local latency = stress.random_latency(50, 15)  -- ~50us lookup

    local versions = self.schemas[id]
    if not versions then
        return nil, latency
    end

    local schema
    if version then
        schema = versions[version]
    else
        -- Find latest version
        local max_version = 0
        for v, s in pairs(versions) do
            if v > max_version then
                max_version = v
                schema = s
            end
        end
    end

    -- Cache the result
    if schema then
        self.lookup_cache[cache_key] = schema
    end

    return schema, latency
end

--- Get all versions of a schema
-- @param id string Schema ID
-- @return table Array of versions
function SchemaRegistry:get_versions(id)
    local versions = self.schemas[id]
    if not versions then
        return {}
    end

    local result = {}
    for v, _ in pairs(versions) do
        table.insert(result, v)
    end
    table.sort(result)
    return result
end

--- Record a migration event
function SchemaRegistry:record_migration()
    self.stats.migrations = self.stats.migrations + 1
end

--- Get registry statistics
-- @return table Statistics
function SchemaRegistry:get_stats()
    local cache_hit_rate = 0
    if self.stats.lookups > 0 then
        cache_hit_rate = self.stats.cache_hits / self.stats.lookups
    end

    return {
        registrations = self.stats.registrations,
        lookups = self.stats.lookups,
        cache_hits = self.stats.cache_hits,
        cache_misses = self.stats.cache_misses,
        cache_hit_rate = cache_hit_rate,
        migrations = self.stats.migrations,
        schema_count = stress.table_count(self.schemas),
    }
end

-- ============================================================================
-- CONTENT TYPES: Different message content types with varying complexity
-- ============================================================================

local CONTENT_TYPES = {
    {
        name = "text",
        description = "Plain text messages",
        avg_size = 256,
        size_variance = 128,
        weight = 0.4,  -- 40% of messages
    },
    {
        name = "json",
        description = "JSON structured data",
        avg_size = 1024,
        size_variance = 512,
        weight = 0.35,  -- 35% of messages
    },
    {
        name = "binary",
        description = "Binary encoded data",
        avg_size = 2048,
        size_variance = 1024,
        weight = 0.15,  -- 15% of messages
    },
    {
        name = "custom",
        description = "Custom schema format",
        avg_size = 512,
        size_variance = 256,
        weight = 0.1,  -- 10% of messages
    },
}

--- Select a random content type based on weights
-- @return table Content type definition
local function random_content_type()
    local r = math.random()
    local cumulative = 0
    for _, ct in ipairs(CONTENT_TYPES) do
        cumulative = cumulative + ct.weight
        if r <= cumulative then
            return ct
        end
    end
    return CONTENT_TYPES[1]  -- Default to text
end

--- Generate random content size for a content type
-- @param content_type table Content type definition
-- @return number Content size in bytes
local function random_content_size(content_type)
    return math.max(1, math.floor(content_type.avg_size + (math.random() - 0.5) * 2 * content_type.size_variance))
end

-- ============================================================================
-- SIMULATION SETUP
-- ============================================================================

-- Create mesh topology
local mesh = indras.MeshBuilder.new(cfg.peers):random(0.3)

indras.log.debug("Created mesh topology", {
    trace_id = ctx.trace_id,
    peers = mesh:peer_count(),
    edges = mesh:edge_count(),
    avg_degree = mesh:edge_count() / mesh:peer_count()
})

-- Create simulation
local sim_config = indras.SimConfig.new({
    wake_probability = 0.1,
    sleep_probability = 0.05,
    initial_online_probability = 0.95,
    max_ticks = cfg.ticks,
    trace_routing = false  -- Disable for performance
})

local sim = indras.Simulation.new(mesh, sim_config)
sim:initialize()

local all_peers = mesh:peers()

-- Initialize schema registry
local registry = SchemaRegistry.new()

-- Metrics tracking
local validation_latencies = {}
local registration_latencies = {}
local lookup_latencies = {}
local migration_latencies = {}

local validation_tracker = stress.throughput_calculator()
local validation_successes = 0
local validation_failures = 0
local migration_successes = 0
local migration_failures = 0

-- Schema tracking
local active_schemas = {}  -- schema_id -> current_version

-- Helper functions
local function random_online_peer()
    local online = sim:online_peers()
    if #online == 0 then return nil end
    return online[math.random(#online)]
end

-- ============================================================================
-- PHASE 1: SCHEMA REGISTRATION
-- ============================================================================

indras.log.info("Phase 1: Schema registration", {
    trace_id = ctx.trace_id,
    target_schemas = cfg.schemas
})

local phase1_start_tick = sim.tick

for i = 1, cfg.schemas do
    local content_type = random_content_type()
    local schema_id = string.format("schema-%s-%04d", content_type.name, i)

    local schema = Schema.new(schema_id, content_type.name, 1)

    -- Add random fields based on content type
    if content_type.name == "json" then
        schema:add_field("id", "string", true)
        schema:add_field("timestamp", "number", true)
        schema:add_field("data", "object", false)
        schema:add_field("tags", "array", false)
    elseif content_type.name == "custom" then
        schema:add_field("header", "binary", true)
        schema:add_field("payload", "binary", true)
        schema:add_field("checksum", "number", true)
    elseif content_type.name == "text" then
        schema:add_field("content", "string", true)
        schema:add_field("encoding", "string", false)
    elseif content_type.name == "binary" then
        schema:add_field("format", "string", true)
        schema:add_field("data", "binary", true)
    end

    -- Register schema
    local success, latency = registry:register(schema)
    table.insert(registration_latencies, latency)

    active_schemas[schema_id] = {
        schema = schema,
        version = 1,
        content_type = content_type,
    }

    indras.log.debug("Registered schema", {
        trace_id = ctx.trace_id,
        schema_id = schema_id,
        content_type = content_type.name,
        complexity = schema.validation_complexity,
        latency_us = latency
    })

    sim:step()
end

local phase1_end_tick = sim.tick
local registry_stats = registry:get_stats()

indras.log.info("Phase 1 complete: Schema registration", {
    trace_id = ctx.trace_id,
    schemas_registered = registry_stats.registrations,
    avg_registration_latency_us = pq.average(registration_latencies),
    p95_registration_latency_us = pq.percentile(registration_latencies, 95),
    tick_duration = phase1_end_tick - phase1_start_tick
})

-- Advance simulation
for _ = 1, math.floor(cfg.ticks * 0.1) do
    sim:step()
end

-- ============================================================================
-- PHASE 2: VALIDATION STRESS TEST
-- ============================================================================

indras.log.info("Phase 2: Validation stress test", {
    trace_id = ctx.trace_id,
    target_messages = cfg.messages,
    active_schemas = stress.table_count(active_schemas)
})

local phase2_start_tick = sim.tick
validation_tracker:start(phase2_start_tick)

local messages_validated = 0
local schema_ids = stress.table_keys(active_schemas)

while messages_validated < cfg.messages and sim.tick < cfg.ticks * 0.7 do
    local sender = random_online_peer()
    local receiver = random_online_peer()

    if sender and receiver and sender ~= receiver then
        -- Pick a random schema
        local schema_id = schema_ids[math.random(#schema_ids)]
        local schema_info = active_schemas[schema_id]

        -- Lookup schema (simulates real-world behavior)
        local schema, lookup_latency = registry:lookup(schema_id, schema_info.version)
        table.insert(lookup_latencies, lookup_latency)

        if schema then
            -- Generate content and validate
            local content_size = random_content_size(schema_info.content_type)
            local success, validation_latency = schema:validate(content_size)

            table.insert(validation_latencies, validation_latency)
            validation_tracker:record()
            messages_validated = messages_validated + 1

            if success then
                validation_successes = validation_successes + 1

                -- Sign and send message
                local sign_lat = pq.sign_latency()
                sim:record_pq_signature(sender, sign_lat, content_size)

                local msg_id = string.format("%s-msg-%d", schema_id, messages_validated)
                sim:send_message(sender, receiver, msg_id)
            else
                validation_failures = validation_failures + 1
            end
        end
    end

    sim:step()

    -- Progress logging
    if messages_validated % math.max(1, math.floor(cfg.messages / 10)) == 0 then
        local current_registry_stats = registry:get_stats()
        indras.log.info("Validation progress", {
            trace_id = ctx.trace_id,
            messages_validated = messages_validated,
            target = cfg.messages,
            tick = sim.tick,
            validation_success_rate = validation_successes / math.max(1, messages_validated),
            cache_hit_rate = current_registry_stats.cache_hit_rate,
            avg_validation_latency_us = pq.average(validation_latencies)
        })
    end
end

validation_tracker:finish(sim.tick)
local phase2_end_tick = sim.tick

local validation_percentiles = pq.percentiles(validation_latencies)
local lookup_percentiles = pq.percentiles(lookup_latencies)

indras.log.info("Phase 2 complete: Validation stress", {
    trace_id = ctx.trace_id,
    messages_validated = messages_validated,
    validation_successes = validation_successes,
    validation_failures = validation_failures,
    validation_success_rate = validation_successes / math.max(1, messages_validated),
    avg_validation_latency_us = pq.average(validation_latencies),
    p50_validation_latency_us = validation_percentiles.p50,
    p95_validation_latency_us = validation_percentiles.p95,
    p99_validation_latency_us = validation_percentiles.p99,
    avg_lookup_latency_us = pq.average(lookup_latencies),
    p95_lookup_latency_us = lookup_percentiles.p95,
    validations_per_tick = validation_tracker:ops_per_tick(),
    tick_duration = phase2_end_tick - phase2_start_tick
})

-- ============================================================================
-- PHASE 3: SCHEMA MIGRATION STRESS
-- ============================================================================

indras.log.info("Phase 3: Schema migration stress", {
    trace_id = ctx.trace_id,
    target_migrations = cfg.migrations
})

local phase3_start_tick = sim.tick
local migrations_completed = 0
local messages_during_migration = 0

-- Perform migrations while continuing message validation
for migration_num = 1, cfg.migrations do
    -- Select a schema to migrate
    local schema_id = schema_ids[math.random(#schema_ids)]
    local schema_info = active_schemas[schema_id]

    local old_version = schema_info.version
    local old_schema = schema_info.schema

    -- Create new schema version
    local new_schema = old_schema:new_version()

    -- Add a new field to the migrated schema
    new_schema:add_field(string.format("migration_%d_field", migration_num), "string", false)

    -- Measure migration latency (register new version)
    local migration_start = os.clock()
    local success, reg_latency = registry:register(new_schema)

    if success then
        -- Simulate migration transition period
        -- During this time, both versions may be in use
        local transition_ticks = math.random(5, 15)
        local old_version_uses = 0
        local new_version_uses = 0

        for _ = 1, transition_ticks do
            -- Some peers use old version, some use new
            local use_new_version = math.random() < 0.5 + (0.5 * _ / transition_ticks)  -- Gradually shift

            local sender = random_online_peer()
            local receiver = random_online_peer()

            if sender and receiver and sender ~= receiver then
                local version_to_use = use_new_version and new_schema.version or old_version
                local schema, _ = registry:lookup(schema_id, version_to_use)

                if schema then
                    local content_size = random_content_size(schema_info.content_type)
                    local valid, val_latency = schema:validate(content_size)
                    table.insert(validation_latencies, val_latency)
                    messages_during_migration = messages_during_migration + 1

                    if use_new_version then
                        new_version_uses = new_version_uses + 1
                    else
                        old_version_uses = old_version_uses + 1
                    end

                    if valid then
                        sim:send_message(sender, receiver, string.format("migration-%d-msg", migration_num))
                    end
                end
            end

            sim:step()
        end

        -- Complete migration
        local migration_latency = (os.clock() - migration_start) * 1000000  -- Convert to us
        table.insert(migration_latencies, migration_latency)

        -- Update active schema to new version
        schema_info.schema = new_schema
        schema_info.version = new_schema.version

        registry:record_migration()
        migrations_completed = migrations_completed + 1
        migration_successes = migration_successes + 1

        indras.log.debug("Schema migration completed", {
            trace_id = ctx.trace_id,
            schema_id = schema_id,
            old_version = old_version,
            new_version = new_schema.version,
            transition_ticks = transition_ticks,
            old_version_uses = old_version_uses,
            new_version_uses = new_version_uses,
            migration_latency_us = migration_latency
        })
    else
        migration_failures = migration_failures + 1
    end

    -- Continue some validation between migrations
    for _ = 1, 10 do
        sim:step()
    end
end

local phase3_end_tick = sim.tick
local migration_percentiles = pq.percentiles(migration_latencies)

indras.log.info("Phase 3 complete: Schema migration", {
    trace_id = ctx.trace_id,
    migrations_completed = migrations_completed,
    migration_successes = migration_successes,
    migration_failures = migration_failures,
    messages_during_migration = messages_during_migration,
    avg_migration_latency_us = pq.average(migration_latencies),
    p95_migration_latency_us = migration_percentiles.p95,
    tick_duration = phase3_end_tick - phase3_start_tick
})

-- Run remaining simulation ticks
local remaining_ticks = cfg.ticks - sim.tick
for _ = 1, remaining_ticks do
    sim:step()
end

-- ============================================================================
-- FINAL STATISTICS AND ASSERTIONS
-- ============================================================================

local final_stats = sim.stats
local final_registry_stats = registry:get_stats()

-- Calculate final metrics
local total_validations = validation_successes + validation_failures
local validation_success_rate = total_validations > 0 and (validation_successes / total_validations) or 0
local migration_success_rate = cfg.migrations > 0 and (migration_successes / cfg.migrations) or 0

local final_validation_percentiles = pq.percentiles(validation_latencies)
local final_lookup_percentiles = pq.percentiles(lookup_latencies)

indras.log.info("Messaging schema stress test completed", {
    trace_id = ctx.trace_id,
    config_level = config_level,
    total_ticks = sim.tick,
    -- Schema Registry Metrics
    schemas_registered = final_registry_stats.registrations,
    total_lookups = final_registry_stats.lookups,
    cache_hit_rate = final_registry_stats.cache_hit_rate,
    -- Validation Metrics
    total_validations = total_validations,
    validation_successes = validation_successes,
    validation_failures = validation_failures,
    validation_success_rate = validation_success_rate,
    avg_validation_latency_us = pq.average(validation_latencies),
    p50_validation_latency_us = final_validation_percentiles.p50,
    p95_validation_latency_us = final_validation_percentiles.p95,
    p99_validation_latency_us = final_validation_percentiles.p99,
    validation_throughput_per_tick = validation_tracker:ops_per_tick(),
    -- Lookup Metrics
    avg_lookup_latency_us = pq.average(lookup_latencies),
    p95_lookup_latency_us = final_lookup_percentiles.p95,
    -- Migration Metrics
    migrations_attempted = cfg.migrations,
    migrations_completed = migrations_completed,
    migration_success_rate = migration_success_rate,
    avg_migration_latency_us = pq.average(migration_latencies),
    -- Network Metrics
    messages_sent = final_stats.messages_sent,
    messages_delivered = final_stats.messages_delivered,
    delivery_rate = final_stats:delivery_rate(),
    avg_network_latency = final_stats:average_latency()
})

-- Assertions
indras.assert.gt(final_registry_stats.registrations, 0, "Should have registered schemas")
indras.assert.gt(total_validations, 0, "Should have performed validations")
indras.assert.gt(validation_success_rate, 0.95, "Validation success rate should be > 95%")
indras.assert.gt(final_registry_stats.cache_hit_rate, 0.5, "Cache hit rate should be > 50%")
indras.assert.gt(migration_success_rate, 0.9, "Migration success rate should be > 90%")
indras.assert.lt(final_validation_percentiles.p99, 1000, "P99 validation latency should be < 1ms")
indras.assert.lt(final_lookup_percentiles.p95, 100, "P95 lookup latency should be < 100us")

indras.log.info("Messaging schema stress test passed", {
    trace_id = ctx.trace_id,
    validation_success_rate = validation_success_rate,
    cache_hit_rate = final_registry_stats.cache_hit_rate,
    migration_success_rate = migration_success_rate,
    validation_throughput_per_tick = validation_tracker:ops_per_tick()
})

-- Return comprehensive metrics
return {
    -- Configuration
    config_level = config_level,
    peers = cfg.peers,
    target_schemas = cfg.schemas,
    target_messages = cfg.messages,
    target_migrations = cfg.migrations,
    -- Schema Registry
    schemas_registered = final_registry_stats.registrations,
    total_lookups = final_registry_stats.lookups,
    cache_hit_rate = final_registry_stats.cache_hit_rate,
    -- Validation
    total_validations = total_validations,
    validation_success_rate = validation_success_rate,
    validation_latency = {
        avg = pq.average(validation_latencies),
        p50 = final_validation_percentiles.p50,
        p95 = final_validation_percentiles.p95,
        p99 = final_validation_percentiles.p99,
    },
    validation_throughput_per_tick = validation_tracker:ops_per_tick(),
    -- Lookup
    lookup_latency = {
        avg = pq.average(lookup_latencies),
        p50 = final_lookup_percentiles.p50,
        p95 = final_lookup_percentiles.p95,
        p99 = final_lookup_percentiles.p99,
    },
    -- Migration
    migrations_completed = migrations_completed,
    migration_success_rate = migration_success_rate,
    migration_latency = {
        avg = pq.average(migration_latencies),
        p95 = migration_percentiles.p95,
    },
    -- Network
    messages_sent = final_stats.messages_sent,
    messages_delivered = final_stats.messages_delivered,
    delivery_rate = final_stats:delivery_rate(),
    -- Performance
    total_ticks = sim.tick,
}

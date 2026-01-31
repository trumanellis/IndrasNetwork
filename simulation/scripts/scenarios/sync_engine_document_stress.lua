-- SyncEngine Document<T> CRDT Stress Test
--
-- Stress tests the Document<T> typed CRDT wrapper from indras-network.
-- Simulates typed documents with various schemas, concurrent updates,
-- cross-realm synchronization, and document persistence.
--
-- Tests:
-- 1. Document Creation: Create documents with different schemas
-- 2. Concurrent Updates: Multiple realm members updating documents simultaneously
-- 3. Sync Verification: Ensure updates propagate to all realm members
-- 4. Latency Tracking: Measure update and sync latencies
-- 5. Persistence: Test document reload from storage

-- Fix package path for helper libraries
package.path = package.path .. ";scripts/lib/?.lua;simulation/scripts/lib/?.lua"

local pq = require("pq_helpers")
local stress = require("stress_helpers")

-- Configuration levels (quick/medium/full)
local CONFIG = {
    quick = {
        realm_count = 2,           -- Number of realms to create
        members_per_realm = 4,     -- Members per realm
        schemas = 5,               -- Different document schemas
        documents_per_schema = 3,  -- Documents per schema type
        updates_per_document = 20, -- Updates per document
        ticks = 300,
        sync_check_interval = 10,  -- Ticks between sync verification
        persistence_tests = 5,     -- Number of persistence reload tests
    },
    medium = {
        realm_count = 4,
        members_per_realm = 8,
        schemas = 10,
        documents_per_schema = 5,
        updates_per_document = 100,
        ticks = 800,
        sync_check_interval = 20,
        persistence_tests = 15,
    },
    full = {
        realm_count = 6,
        members_per_realm = 12,
        schemas = 20,
        documents_per_schema = 10,
        updates_per_document = 500,
        ticks = 2000,
        sync_check_interval = 50,
        persistence_tests = 30,
    }
}

-- Select configuration (default to quick)
local config_level = os.getenv("STRESS_LEVEL") or "quick"
local cfg = CONFIG[config_level] or CONFIG.quick

-- Create correlation context
local ctx = indras.correlation.new_root()
ctx = ctx:with_tag("scenario", "sync_engine_document_stress")
ctx = ctx:with_tag("level", config_level)

indras.log.info("Starting SyncEngine Document<T> stress test", {
    trace_id = ctx.trace_id,
    config_level = config_level,
    realm_count = cfg.realm_count,
    members_per_realm = cfg.members_per_realm,
    schemas = cfg.schemas,
    documents_per_schema = cfg.documents_per_schema,
    updates_per_document = cfg.updates_per_document,
    ticks = cfg.ticks
})

-- ============================================================================
-- DOCUMENT SCHEMA DEFINITIONS
-- ============================================================================

local SCHEMAS = {
    {
        name = "counter",
        description = "Simple counter document",
        fields = {"value"},
        complexity = 1.0,
        default_value = function() return { value = 0 } end,
        update = function(doc) doc.value = doc.value + 1 end,
        verify = function(doc, expected_updates) return doc.value == expected_updates end,
    },
    {
        name = "key_value",
        description = "Key-value store document",
        fields = {"entries", "version"},
        complexity = 1.5,
        default_value = function() return { entries = {}, version = 0 } end,
        update = function(doc)
            local key = string.format("key_%d", doc.version + 1)
            doc.entries[key] = math.random(1000)
            doc.version = doc.version + 1
        end,
        verify = function(doc, expected_updates) return doc.version == expected_updates end,
    },
    {
        name = "list",
        description = "Ordered list document",
        fields = {"items", "count"},
        complexity = 2.0,
        default_value = function() return { items = {}, count = 0 } end,
        update = function(doc)
            table.insert(doc.items, { id = doc.count + 1, data = math.random() })
            doc.count = doc.count + 1
        end,
        verify = function(doc, expected_updates) return doc.count == expected_updates end,
    },
    {
        name = "nested",
        description = "Nested structure document",
        fields = {"root", "depth", "nodes"},
        complexity = 3.0,
        default_value = function() return { root = {}, depth = 0, nodes = 0 } end,
        update = function(doc)
            local node_id = string.format("node_%d", doc.nodes + 1)
            local level = (doc.nodes % 3) + 1
            if level > doc.depth then doc.depth = level end
            doc.root[node_id] = { level = level, children = {} }
            doc.nodes = doc.nodes + 1
        end,
        verify = function(doc, expected_updates) return doc.nodes == expected_updates end,
    },
    {
        name = "text",
        description = "Collaborative text document",
        fields = {"content", "cursor_positions", "edit_count"},
        complexity = 2.5,
        default_value = function() return { content = "", cursor_positions = {}, edit_count = 0 } end,
        update = function(doc)
            doc.content = doc.content .. string.char(97 + (doc.edit_count % 26))
            doc.edit_count = doc.edit_count + 1
        end,
        verify = function(doc, expected_updates) return doc.edit_count == expected_updates end,
    },
}

--- Get schema by index (cycles through available schemas)
local function get_schema(index)
    return SCHEMAS[((index - 1) % #SCHEMAS) + 1]
end

-- ============================================================================
-- DOCUMENT CLASS: Simulates Document<T> behavior
-- ============================================================================

local Document = {}
Document.__index = Document

--- Create a new document
-- @param doc_id string Unique document identifier
-- @param schema table Schema definition
-- @param realm_id string Realm this document belongs to
-- @return Document
function Document.new(doc_id, schema, realm_id)
    local self = setmetatable({}, Document)
    self.id = doc_id
    self.schema = schema
    self.realm_id = realm_id
    self.state = schema.default_value()
    self.version = 0
    self.update_count = 0
    self.last_updated_by = nil
    self.last_updated_tick = 0
    self.synced_to = {}  -- member_id -> version they have
    self.created_at = os.time()
    self.persisted = false
    self.reload_count = 0
    return self
end

--- Calculate update latency based on schema complexity
-- @return number Latency in microseconds
function Document:update_latency()
    local base = 100 * self.schema.complexity
    local variance = 30 * self.schema.complexity
    return stress.random_latency(base, variance)
end

--- Calculate sync latency (network propagation + merge)
-- @return number Latency in microseconds
function Document:sync_latency()
    local base = 200 * self.schema.complexity
    local variance = 80 * self.schema.complexity
    return stress.random_latency(base, variance)
end

--- Calculate persistence latency
-- @return number Latency in microseconds
function Document:persistence_latency()
    local base = 150
    local size_factor = 1 + (self.update_count * 0.01)  -- Larger docs take longer
    local variance = 50
    return stress.random_latency(base * size_factor, variance)
end

--- Apply an update to the document
-- @param author_id string ID of the member making the update
-- @param tick number Current tick
-- @return number Latency in microseconds
function Document:apply_update(author_id, tick)
    local latency = self:update_latency()

    -- Apply schema-specific update
    self.schema.update(self.state)
    self.version = self.version + 1
    self.update_count = self.update_count + 1
    self.last_updated_by = author_id
    self.last_updated_tick = tick

    -- Author has latest version
    self.synced_to[author_id] = self.version

    return latency
end

--- Sync document state to a member
-- @param member_id string ID of member to sync to
-- @return boolean synced, number latency_us
function Document:sync_to_member(member_id)
    local current_version = self.synced_to[member_id] or 0

    if current_version >= self.version then
        -- Already synced
        return false, 0
    end

    local latency = self:sync_latency()
    self.synced_to[member_id] = self.version

    return true, latency
end

--- Check if all members are synced
-- @param member_ids table Array of member IDs
-- @return boolean all_synced, number synced_count, number total
function Document:check_sync(member_ids)
    local synced = 0
    for _, member_id in ipairs(member_ids) do
        local member_version = self.synced_to[member_id] or 0
        if member_version >= self.version then
            synced = synced + 1
        end
    end
    return synced == #member_ids, synced, #member_ids
end

--- Simulate persistence (save to storage)
-- @return number Latency in microseconds
function Document:persist()
    local latency = self:persistence_latency()
    self.persisted = true
    return latency
end

--- Simulate document reload from storage
-- @return boolean success, number latency_us
function Document:reload()
    if not self.persisted then
        return false, 0
    end

    local latency = self:persistence_latency()
    self.reload_count = self.reload_count + 1

    -- Simulate potential reload failure (rare)
    local success = math.random() > 0.001

    return success, latency
end

--- Verify document state matches expected updates
-- @return boolean valid
function Document:verify_state()
    return self.schema.verify(self.state, self.update_count)
end

-- ============================================================================
-- REALM CLASS: Groups members and their shared documents
-- ============================================================================

local Realm = {}
Realm.__index = Realm

--- Create a new realm
-- @param realm_id string Realm identifier
-- @param member_ids table Array of member IDs
-- @return Realm
function Realm.new(realm_id, member_ids)
    local self = setmetatable({}, Realm)
    self.id = realm_id
    self.members = member_ids
    self.documents = {}  -- doc_id -> Document
    self.online_members = {}  -- Set of online member IDs
    for _, mid in ipairs(member_ids) do
        self.online_members[mid] = true
    end
    return self
end

--- Create a document in this realm
-- @param doc_id string Document identifier
-- @param schema table Schema definition
-- @return Document
function Realm:create_document(doc_id, schema)
    local doc = Document.new(doc_id, schema, self.id)
    self.documents[doc_id] = doc

    -- All members get initial sync
    for _, member_id in ipairs(self.members) do
        doc.synced_to[member_id] = 0
    end

    return doc
end

--- Get a random online member
-- @return string|nil member_id
function Realm:random_online_member()
    local online = {}
    for member_id, is_online in pairs(self.online_members) do
        if is_online then
            table.insert(online, member_id)
        end
    end
    if #online == 0 then return nil end
    return online[math.random(#online)]
end

--- Get online member count
-- @return number
function Realm:online_count()
    local count = 0
    for _, is_online in pairs(self.online_members) do
        if is_online then count = count + 1 end
    end
    return count
end

--- Set member online/offline status
-- @param member_id string
-- @param online boolean
function Realm:set_member_status(member_id, online)
    self.online_members[member_id] = online
end

-- ============================================================================
-- SIMULATION SETUP
-- ============================================================================

-- Calculate total peers needed
local total_peers = math.min(26, cfg.realm_count * cfg.members_per_realm)

-- Create mesh topology
local mesh = indras.MeshBuilder.new(total_peers):random(0.4)

indras.log.debug("Created mesh topology", {
    trace_id = ctx.trace_id,
    peers = mesh:peer_count(),
    edges = mesh:edge_count(),
    avg_degree = mesh:edge_count() / mesh:peer_count()
})

-- Create simulation
local sim_config = indras.SimConfig.new({
    wake_probability = 0.08,
    sleep_probability = 0.04,
    initial_online_probability = 0.9,
    max_ticks = cfg.ticks,
    trace_routing = false  -- Disable for performance
})

local sim = indras.Simulation.new(mesh, sim_config)
sim:initialize()

local all_peers = mesh:peers()

-- Create realms
local realms = {}
local peer_index = 1

for r = 1, cfg.realm_count do
    local realm_id = string.format("realm-%04d", r)
    local members = {}

    for m = 1, cfg.members_per_realm do
        if peer_index <= #all_peers then
            table.insert(members, tostring(all_peers[peer_index]))
            peer_index = peer_index + 1
        end
    end

    if #members > 0 then
        local realm = Realm.new(realm_id, members)
        table.insert(realms, realm)

        indras.log.debug("Created realm", {
            trace_id = ctx.trace_id,
            realm_id = realm_id,
            member_count = #members
        })
    end
end

-- ============================================================================
-- METRICS TRACKING
-- ============================================================================

local update_latencies = {}
local sync_latencies = {}
local persistence_latencies = {}
local reload_latencies = {}

local metrics = {
    documents_created = 0,
    total_updates = 0,
    sync_operations = 0,
    sync_successes = 0,
    sync_failures = 0,
    persistence_operations = 0,
    reload_operations = 0,
    reload_successes = 0,
    reload_failures = 0,
    convergence_checks = 0,
    convergence_successes = 0,
    state_validations = 0,
    state_validation_failures = 0,
}

local throughput_tracker = stress.throughput_calculator()

-- ============================================================================
-- PHASE 1: DOCUMENT CREATION
-- ============================================================================

indras.log.info("Phase 1: Document creation", {
    trace_id = ctx.trace_id,
    target_schemas = cfg.schemas,
    documents_per_schema = cfg.documents_per_schema,
    total_documents = cfg.schemas * cfg.documents_per_schema
})

local phase1_start_tick = sim.tick

-- Create documents across realms
local all_documents = {}  -- { realm = Realm, doc = Document }

for schema_idx = 1, cfg.schemas do
    local schema = get_schema(schema_idx)

    for doc_num = 1, cfg.documents_per_schema do
        -- Round-robin across realms
        local realm = realms[((schema_idx + doc_num) % #realms) + 1]
        local doc_id = string.format("%s-doc-%d", schema.name, (schema_idx - 1) * cfg.documents_per_schema + doc_num)

        local doc = realm:create_document(doc_id, schema)
        table.insert(all_documents, { realm = realm, doc = doc })
        metrics.documents_created = metrics.documents_created + 1

        indras.log.debug("Created document", {
            trace_id = ctx.trace_id,
            doc_id = doc_id,
            schema = schema.name,
            realm_id = realm.id
        })
    end

    sim:step()
end

local phase1_end_tick = sim.tick

indras.narrative("Collaborative documents come to life")
indras.log.info("Phase 1 complete: Document creation", {
    trace_id = ctx.trace_id,
    documents_created = metrics.documents_created,
    realms = #realms,
    tick_duration = phase1_end_tick - phase1_start_tick,
})

-- ============================================================================
-- PHASE 2: CONCURRENT UPDATES
-- ============================================================================

indras.log.info("Phase 2: Concurrent updates", {
    trace_id = ctx.trace_id,
    updates_per_document = cfg.updates_per_document,
    total_target_updates = metrics.documents_created * cfg.updates_per_document
})

local phase2_start_tick = sim.tick
throughput_tracker:start(phase2_start_tick)

-- Track updates per document
local document_updates = {}
for _, entry in ipairs(all_documents) do
    document_updates[entry.doc.id] = 0
end

local updates_this_tick = 0
local target_total_updates = metrics.documents_created * cfg.updates_per_document
local updates_per_tick = math.max(1, math.floor(target_total_updates / (cfg.ticks * 0.5)))

while metrics.total_updates < target_total_updates and sim.tick < cfg.ticks * 0.6 do
    -- Perform updates across multiple documents this tick
    for _ = 1, updates_per_tick do
        if metrics.total_updates >= target_total_updates then break end

        -- Pick a random document that hasn't hit its update limit
        local entry = all_documents[math.random(#all_documents)]
        local doc = entry.doc
        local realm = entry.realm

        if document_updates[doc.id] < cfg.updates_per_document then
            -- Pick a random online member to make the update
            local author = realm:random_online_member()

            if author then
                local latency = doc:apply_update(author, sim.tick)
                table.insert(update_latencies, latency)

                document_updates[doc.id] = document_updates[doc.id] + 1
                metrics.total_updates = metrics.total_updates + 1
                throughput_tracker:record()

                -- Send sync message via simulation
                local receiver_idx = math.random(#realm.members)
                local receiver = realm.members[receiver_idx]
                if receiver ~= author then
                    sim:send_message(all_peers[1], all_peers[math.min(#all_peers, 2)],
                        string.format("doc_update:%s:v%d", doc.id, doc.version))
                end
            end
        end
    end

    sim:step()

    -- Progress logging at 10% intervals
    local progress = metrics.total_updates / target_total_updates
    local progress_pct = math.floor(progress * 10)
    if progress_pct > 0 and metrics.total_updates % math.floor(target_total_updates / 10) < updates_per_tick then
        indras.log.info("Update progress checkpoint", {
            trace_id = ctx.trace_id,
            updates_completed = metrics.total_updates,
            target = target_total_updates,
            progress_pct = progress_pct * 10,
            tick = sim.tick,
            updates_per_tick = throughput_tracker:ops_per_tick()
        })
    end
end

throughput_tracker:finish(sim.tick)
local phase2_end_tick = sim.tick

local update_percentiles = pq.percentiles(update_latencies)

indras.narrative("Multiple hands shape the same document")
indras.log.info("Phase 2 complete: Concurrent updates", {
    trace_id = ctx.trace_id,
    total_updates = metrics.total_updates,
    avg_update_latency_us = pq.average(update_latencies),
    p50_update_latency_us = update_percentiles.p50,
    p95_update_latency_us = update_percentiles.p95,
    p99_update_latency_us = update_percentiles.p99,
    updates_per_tick = throughput_tracker:ops_per_tick(),
    tick_duration = phase2_end_tick - phase2_start_tick,
})

-- ============================================================================
-- PHASE 3: SYNC VERIFICATION
-- ============================================================================

indras.log.info("Phase 3: Sync verification", {
    trace_id = ctx.trace_id,
    sync_check_interval = cfg.sync_check_interval
})

local phase3_start_tick = sim.tick

-- Perform sync operations and verify convergence
local sync_rounds = 0
local max_sync_rounds = math.floor((cfg.ticks * 0.25) / cfg.sync_check_interval)

while sync_rounds < max_sync_rounds and sim.tick < cfg.ticks * 0.85 do
    sync_rounds = sync_rounds + 1

    -- Sync all documents to all members
    for _, entry in ipairs(all_documents) do
        local doc = entry.doc
        local realm = entry.realm

        for _, member_id in ipairs(realm.members) do
            local synced, latency = doc:sync_to_member(member_id)
            if synced then
                table.insert(sync_latencies, latency)
                metrics.sync_operations = metrics.sync_operations + 1
                metrics.sync_successes = metrics.sync_successes + 1
            end
        end
    end

    -- Advance simulation by sync interval
    for _ = 1, cfg.sync_check_interval do
        sim:step()
    end

    -- Check convergence
    local converged_docs = 0
    for _, entry in ipairs(all_documents) do
        local doc = entry.doc
        local realm = entry.realm
        local all_synced, synced_count, total = doc:check_sync(realm.members)

        metrics.convergence_checks = metrics.convergence_checks + 1
        if all_synced then
            converged_docs = converged_docs + 1
            metrics.convergence_successes = metrics.convergence_successes + 1
        end
    end

    -- Progress logging at 10% intervals
    if sync_rounds % math.max(1, math.floor(max_sync_rounds / 10)) == 0 then
        local convergence_rate = converged_docs / #all_documents
        indras.log.info("Sync verification progress", {
            trace_id = ctx.trace_id,
            sync_round = sync_rounds,
            max_rounds = max_sync_rounds,
            converged_documents = converged_docs,
            total_documents = #all_documents,
            convergence_rate = convergence_rate,
            tick = sim.tick,
            sync_operations = metrics.sync_operations
        })
    end
end

local phase3_end_tick = sim.tick

local sync_percentiles = pq.percentiles(sync_latencies)

indras.narrative("Concurrent edits test the document system's resilience")
indras.log.info("Phase 3 complete: Sync verification", {
    trace_id = ctx.trace_id,
    sync_operations = metrics.sync_operations,
    sync_successes = metrics.sync_successes,
    convergence_checks = metrics.convergence_checks,
    convergence_successes = metrics.convergence_successes,
    convergence_rate = metrics.convergence_successes / math.max(1, metrics.convergence_checks),
    avg_sync_latency_us = pq.average(sync_latencies),
    p50_sync_latency_us = sync_percentiles.p50,
    p95_sync_latency_us = sync_percentiles.p95,
    p99_sync_latency_us = sync_percentiles.p99,
    tick_duration = phase3_end_tick - phase3_start_tick,
})

-- ============================================================================
-- PHASE 4: PERSISTENCE TESTING
-- ============================================================================

indras.log.info("Phase 4: Persistence testing", {
    trace_id = ctx.trace_id,
    persistence_tests = cfg.persistence_tests
})

local phase4_start_tick = sim.tick

-- Persist all documents
for _, entry in ipairs(all_documents) do
    local doc = entry.doc
    local latency = doc:persist()
    table.insert(persistence_latencies, latency)
    metrics.persistence_operations = metrics.persistence_operations + 1
end

-- Perform reload tests
for test_num = 1, cfg.persistence_tests do
    -- Pick a random document
    local entry = all_documents[math.random(#all_documents)]
    local doc = entry.doc

    local success, latency = doc:reload()
    if latency > 0 then
        table.insert(reload_latencies, latency)
    end

    metrics.reload_operations = metrics.reload_operations + 1
    if success then
        metrics.reload_successes = metrics.reload_successes + 1
    else
        metrics.reload_failures = metrics.reload_failures + 1
    end

    -- Verify state after reload
    metrics.state_validations = metrics.state_validations + 1
    if not doc:verify_state() then
        metrics.state_validation_failures = metrics.state_validation_failures + 1
    end

    sim:step()

    -- Progress logging
    if test_num % math.max(1, math.floor(cfg.persistence_tests / 10)) == 0 then
        indras.log.info("Persistence test progress", {
            trace_id = ctx.trace_id,
            test_num = test_num,
            total_tests = cfg.persistence_tests,
            reload_success_rate = metrics.reload_successes / math.max(1, metrics.reload_operations),
            tick = sim.tick
        })
    end
end

local phase4_end_tick = sim.tick

local persistence_percentiles = pq.percentiles(persistence_latencies)
local reload_percentiles = pq.percentiles(reload_latencies)

indras.narrative("Every edit preserved — collaborative writing at scale")
indras.log.info("Phase 4 complete: Persistence testing", {
    trace_id = ctx.trace_id,
    persistence_operations = metrics.persistence_operations,
    reload_operations = metrics.reload_operations,
    reload_successes = metrics.reload_successes,
    reload_failures = metrics.reload_failures,
    reload_success_rate = metrics.reload_successes / math.max(1, metrics.reload_operations),
    state_validations = metrics.state_validations,
    state_validation_failures = metrics.state_validation_failures,
    avg_persistence_latency_us = pq.average(persistence_latencies),
    p95_persistence_latency_us = persistence_percentiles.p95,
    avg_reload_latency_us = pq.average(reload_latencies),
    p95_reload_latency_us = reload_percentiles.p95,
    tick_duration = phase4_end_tick - phase4_start_tick,
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

-- Calculate derived metrics
local update_success_rate = metrics.total_updates > 0 and 1.0 or 0  -- All updates succeed in simulation
local sync_success_rate = metrics.sync_operations > 0 and (metrics.sync_successes / metrics.sync_operations) or 0
local convergence_rate = metrics.convergence_checks > 0 and (metrics.convergence_successes / metrics.convergence_checks) or 0
local reload_success_rate = metrics.reload_operations > 0 and (metrics.reload_successes / metrics.reload_operations) or 0
local state_validation_rate = metrics.state_validations > 0 and (1 - metrics.state_validation_failures / metrics.state_validations) or 0

local final_update_percentiles = pq.percentiles(update_latencies)
local final_sync_percentiles = pq.percentiles(sync_latencies)

indras.log.info("SyncEngine Document stress test completed", {
    trace_id = ctx.trace_id,
    config_level = config_level,
    total_ticks = sim.tick,
    -- Document Metrics
    documents_created = metrics.documents_created,
    realms = #realms,
    -- Update Metrics
    total_updates = metrics.total_updates,
    update_throughput_per_tick = throughput_tracker:ops_per_tick(),
    avg_update_latency_us = pq.average(update_latencies),
    p50_update_latency_us = final_update_percentiles.p50,
    p95_update_latency_us = final_update_percentiles.p95,
    p99_update_latency_us = final_update_percentiles.p99,
    -- Sync Metrics
    sync_operations = metrics.sync_operations,
    sync_success_rate = sync_success_rate,
    convergence_rate = convergence_rate,
    avg_sync_latency_us = pq.average(sync_latencies),
    p50_sync_latency_us = final_sync_percentiles.p50,
    p95_sync_latency_us = final_sync_percentiles.p95,
    p99_sync_latency_us = final_sync_percentiles.p99,
    -- Persistence Metrics
    persistence_operations = metrics.persistence_operations,
    reload_operations = metrics.reload_operations,
    reload_success_rate = reload_success_rate,
    state_validation_rate = state_validation_rate,
    avg_persistence_latency_us = pq.average(persistence_latencies),
    avg_reload_latency_us = pq.average(reload_latencies),
    -- Network Metrics
    messages_sent = final_stats.messages_sent,
    messages_delivered = final_stats.messages_delivered,
    delivery_rate = final_stats:delivery_rate(),
    avg_network_latency = final_stats:average_latency()
})

-- Assertions
indras.assert.gt(metrics.documents_created, 0, "Should have created documents")
indras.assert.gt(metrics.total_updates, 0, "Should have performed updates")
indras.assert.gt(metrics.sync_operations, 0, "Should have performed sync operations")
indras.assert.gt(sync_success_rate, 0.95, "Sync success rate should be > 95%")
indras.assert.gt(convergence_rate, 0.9, "Convergence rate should be > 90%")
indras.assert.gt(reload_success_rate, 0.99, "Reload success rate should be > 99%")
indras.assert.eq(metrics.state_validation_failures, 0, "No state validation failures expected")
indras.assert.lt(final_update_percentiles.p99, 1000, "P99 update latency should be < 1ms")
indras.assert.lt(final_sync_percentiles.p99, 2000, "P99 sync latency should be < 2ms")

indras.log.info("SyncEngine Document stress test passed", {
    trace_id = ctx.trace_id,
    documents_created = metrics.documents_created,
    total_updates = metrics.total_updates,
    sync_success_rate = sync_success_rate,
    convergence_rate = convergence_rate,
    reload_success_rate = reload_success_rate,
    update_throughput_per_tick = throughput_tracker:ops_per_tick()
})

-- Return comprehensive metrics
return {
    -- Configuration
    config_level = config_level,
    realm_count = cfg.realm_count,
    members_per_realm = cfg.members_per_realm,
    target_schemas = cfg.schemas,
    documents_per_schema = cfg.documents_per_schema,
    -- Document Metrics
    documents_created = metrics.documents_created,
    total_realms = #realms,
    -- Update Metrics
    total_updates = metrics.total_updates,
    update_throughput_per_tick = throughput_tracker:ops_per_tick(),
    update_latency = {
        avg = pq.average(update_latencies),
        p50 = final_update_percentiles.p50,
        p95 = final_update_percentiles.p95,
        p99 = final_update_percentiles.p99,
    },
    -- Sync Metrics
    sync_operations = metrics.sync_operations,
    sync_success_rate = sync_success_rate,
    convergence_checks = metrics.convergence_checks,
    convergence_rate = convergence_rate,
    sync_latency = {
        avg = pq.average(sync_latencies),
        p50 = final_sync_percentiles.p50,
        p95 = final_sync_percentiles.p95,
        p99 = final_sync_percentiles.p99,
    },
    -- Persistence Metrics
    persistence_operations = metrics.persistence_operations,
    reload_operations = metrics.reload_operations,
    reload_success_rate = reload_success_rate,
    state_validation_rate = state_validation_rate,
    persistence_latency = {
        avg = pq.average(persistence_latencies),
        p95 = persistence_percentiles.p95,
    },
    reload_latency = {
        avg = pq.average(reload_latencies),
        p95 = reload_percentiles.p95,
    },
    -- Network Metrics
    messages_sent = final_stats.messages_sent,
    messages_delivered = final_stats.messages_delivered,
    delivery_rate = final_stats:delivery_rate(),
    -- Performance
    total_ticks = sim.tick,
}

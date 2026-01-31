-- Artifact Sharing Simulation Helpers
--
-- Utility functions for revocable artifact sharing simulation scenarios.
-- Provides KeyRegistry simulation for testing share/recall lifecycle.
--
-- Key Concepts:
-- - Revocable Sharing: Artifacts can be shared with per-artifact encryption keys
-- - Key Registry: CRDT document tracking encrypted keys per artifact
-- - Recall: Sharer can revoke access by removing key from registry
-- - Tombstone: ArtifactRecalled message posted to chat on revocation

local artifact = {}

-- ============================================================================
-- STRESS LEVELS: Artifact sharing specific configurations
-- ============================================================================

artifact.LEVELS = {
    quick = {
        name = "quick",
        members = 4,
        artifacts_per_member = 2,
        recall_ratio = 0.5,  -- 50% of artifacts recalled
        ticks = 100,
        sync_interval = 5,
    },
    medium = {
        name = "medium",
        members = 8,
        artifacts_per_member = 5,
        recall_ratio = 0.5,
        ticks = 300,
        sync_interval = 3,
    },
    full = {
        name = "full",
        members = 16,
        artifacts_per_member = 10,
        recall_ratio = 0.5,
        ticks = 1000,
        sync_interval = 2,
    }
}

-- Event types for JSONL logging
artifact.EVENTS = {
    -- Artifact sharing lifecycle
    ARTIFACT_SHARED_REVOCABLE = "artifact_shared_revocable",
    ARTIFACT_RECALLED = "artifact_recalled",
    RECALL_ACKNOWLEDGED = "recall_acknowledged",

    -- Registry operations
    KEY_STORED = "key_stored",
    KEY_REMOVED = "key_removed",
    REGISTRY_SYNCED = "registry_synced",

    -- Access checks
    ACCESS_GRANTED = "access_granted",
    ACCESS_DENIED = "access_denied",

    -- Permission events
    PERMISSION_DENIED = "permission_denied",
    PERMISSION_GRANTED = "permission_granted",

    -- CRDT sync
    CRDT_CONVERGED = "crdt_converged",
    CRDT_CONFLICT = "crdt_conflict",

    -- ArtifactIndex events
    ARTIFACT_UPLOADED = "artifact_uploaded",
    ARTIFACT_GRANTED = "artifact_granted",
    ARTIFACT_ACCESS_REVOKED = "artifact_access_revoked",
    ARTIFACT_TRANSFERRED = "artifact_transferred",
    ARTIFACT_EXPIRED = "artifact_expired",
    RECOVERY_REQUESTED = "recovery_requested",
    RECOVERY_COMPLETED = "recovery_completed",
}

-- Artifact status values
artifact.STATUS = {
    SHARED = "shared",
    RECALLED = "recalled",
    ACTIVE = "active",
    TRANSFERRED = "transferred",
    EXPIRED = "expired",
}

-- ============================================================================
-- CONFIGURATION HELPERS
-- ============================================================================

--- Get the current stress level from environment
-- @return string The stress level (quick, medium, or full)
function artifact.get_level()
    return os.getenv("STRESS_LEVEL") or "quick"
end

--- Get the artifact configuration for current stress level
-- @return table The level configuration
function artifact.get_config()
    local level = artifact.get_level()
    return artifact.LEVELS[level] or artifact.LEVELS.quick
end

-- ============================================================================
-- CONTEXT AND LOGGING
-- ============================================================================

--- Create a correlation context for an artifact scenario
-- @param scenario_name string Name of the scenario
-- @return CorrelationContext
function artifact.new_context(scenario_name)
    local ctx = indras.correlation.new_root()
    ctx = ctx:with_tag("scenario", scenario_name)
    ctx = ctx:with_tag("subsystem", "artifact_sharing")
    ctx = ctx:with_tag("stress_level", artifact.get_level())
    return ctx
end

--- Encode a Lua table as JSON string
-- Simple JSON encoder for event output
-- @param value any The value to encode
-- @return string JSON string
local function json_encode(value)
    local t = type(value)
    if t == "nil" then
        return "null"
    elseif t == "boolean" then
        return value and "true" or "false"
    elseif t == "number" then
        if value ~= value then -- NaN
            return "null"
        elseif value == math.huge or value == -math.huge then
            return "null"
        else
            return tostring(value)
        end
    elseif t == "string" then
        -- Escape special characters
        local escaped = value:gsub('\\', '\\\\')
            :gsub('"', '\\"')
            :gsub('\n', '\\n')
            :gsub('\r', '\\r')
            :gsub('\t', '\\t')
        return '"' .. escaped .. '"'
    elseif t == "table" then
        -- Check if it's an array
        local is_array = true
        local max_idx = 0
        for k, _ in pairs(value) do
            if type(k) ~= "number" or k < 1 or math.floor(k) ~= k then
                is_array = false
                break
            end
            if k > max_idx then max_idx = k end
        end
        if is_array and max_idx > 0 then
            for i = 1, max_idx do
                if value[i] == nil then
                    is_array = false
                    break
                end
            end
        end

        if is_array and max_idx > 0 then
            local parts = {}
            for i = 1, max_idx do
                parts[i] = json_encode(value[i])
            end
            return "[" .. table.concat(parts, ",") .. "]"
        else
            local parts = {}
            for k, v in pairs(value) do
                local key_str = type(k) == "string" and k or tostring(k)
                table.insert(parts, '"' .. key_str .. '":' .. json_encode(v))
            end
            return "{" .. table.concat(parts, ",") .. "}"
        end
    else
        return '"<' .. t .. '>"'
    end
end

--- Create a context logger with automatic trace_id
-- @param ctx CorrelationContext The correlation context
-- @return table Logger object
function artifact.create_logger(ctx)
    local logger = {}

    function logger.trace(msg, fields)
        fields = fields or {}
        fields.trace_id = ctx.trace_id
        indras.log.trace(msg, fields)
    end

    function logger.debug(msg, fields)
        fields = fields or {}
        fields.trace_id = ctx.trace_id
        indras.log.debug(msg, fields)
    end

    function logger.info(msg, fields)
        fields = fields or {}
        fields.trace_id = ctx.trace_id
        indras.log.info(msg, fields)
    end

    function logger.warn(msg, fields)
        fields = fields or {}
        fields.trace_id = ctx.trace_id
        indras.log.warn(msg, fields)
    end

    function logger.error(msg, fields)
        fields = fields or {}
        fields.trace_id = ctx.trace_id
        indras.log.error(msg, fields)
    end

    --- Log an artifact event with standard fields
    -- Also outputs JSONL to stdout for viewer consumption
    function logger.event(event_type, fields)
        fields = fields or {}
        fields.trace_id = ctx.trace_id
        fields.event_type = event_type

        -- Log to tracing system (goes to file)
        indras.log.info(event_type, fields)

        -- Also output JSONL to stdout for viewer
        local json_line = json_encode(fields)
        indras.log.print(json_line)
    end

    return logger
end

-- ============================================================================
-- KEY REGISTRY (Simulates CRDT artifact key storage)
-- ============================================================================

--- Create a new KeyRegistry for simulation
-- Simulates the CRDT document that tracks artifact encryption keys
-- @return table KeyRegistry object
function artifact.KeyRegistry_new()
    local registry = {
        -- keys[hash] = encrypted_key (base64 or hex string)
        keys = {},
        -- artifacts[hash] = { name, size, mime_type, sharer, status, shared_at, recalled_at }
        artifacts = {},
        -- revocations = array of { hash, revoked_by, tick, timestamp }
        revocations = {},
        -- Statistics
        total_stored = 0,
        total_revoked = 0,
    }

    --- Store an artifact key in the registry
    -- @param hash string Artifact hash (content-addressable ID)
    -- @param artifact_meta table { name, size, mime_type }
    -- @param encrypted_key string The per-artifact encrypted key
    -- @param sharer string Member ID who shared the artifact
    -- @param tick number Current simulation tick
    -- @return boolean Success
    function registry:store(hash, artifact_meta, encrypted_key, sharer, tick)
        if self.keys[hash] then
            return false, "Key already exists for artifact"
        end

        self.keys[hash] = encrypted_key
        self.artifacts[hash] = {
            name = artifact_meta.name,
            size = artifact_meta.size,
            mime_type = artifact_meta.mime_type,
            sharer = sharer,
            status = artifact.STATUS.SHARED,
            shared_at = tick,
            recalled_at = nil,
        }
        self.total_stored = self.total_stored + 1
        return true
    end

    --- Revoke an artifact key from the registry
    -- @param hash string Artifact hash
    -- @param revoked_by string Member ID attempting revocation
    -- @param tick number Current simulation tick
    -- @return boolean success, string|nil error
    function registry:revoke(hash, revoked_by, tick)
        local art = self.artifacts[hash]
        if not art then
            return false, "Artifact not found in registry"
        end

        if art.status == artifact.STATUS.RECALLED then
            return false, "Artifact already recalled"
        end

        -- Only sharer can revoke
        if art.sharer ~= revoked_by then
            return false, "Only the sharer can revoke this artifact"
        end

        -- Remove key and update status
        self.keys[hash] = nil
        art.status = artifact.STATUS.RECALLED
        art.recalled_at = tick

        -- Record revocation
        table.insert(self.revocations, {
            hash = hash,
            revoked_by = revoked_by,
            tick = tick,
            timestamp = os.time(),
        })

        self.total_revoked = self.total_revoked + 1
        return true
    end

    --- Check if an artifact is revoked
    -- @param hash string Artifact hash
    -- @return boolean True if revoked
    function registry:is_revoked(hash)
        local art = self.artifacts[hash]
        if not art then
            return false
        end
        return art.status == artifact.STATUS.RECALLED
    end

    --- Check if a member can revoke an artifact
    -- @param hash string Artifact hash
    -- @param member_id string Member ID
    -- @return boolean True if member can revoke
    function registry:can_revoke(hash, member_id)
        local art = self.artifacts[hash]
        if not art then
            return false
        end
        return art.sharer == member_id and art.status == artifact.STATUS.SHARED
    end

    --- Get the encrypted key for an artifact
    -- @param hash string Artifact hash
    -- @return string|nil The encrypted key, or nil if not found/revoked
    function registry:get_key(hash)
        if self:is_revoked(hash) then
            return nil
        end
        return self.keys[hash]
    end

    --- Get artifact metadata
    -- @param hash string Artifact hash
    -- @return table|nil Artifact metadata
    function registry:get_artifact(hash)
        return self.artifacts[hash]
    end

    --- List all shared artifacts (not recalled)
    -- @return table Array of { hash, artifact } pairs
    function registry:list_shared()
        local result = {}
        for hash, art in pairs(self.artifacts) do
            if art.status == artifact.STATUS.SHARED then
                table.insert(result, { hash = hash, artifact = art })
            end
        end
        return result
    end

    --- List all recalled artifacts
    -- @return table Array of { hash, artifact } pairs
    function registry:list_recalled()
        local result = {}
        for hash, art in pairs(self.artifacts) do
            if art.status == artifact.STATUS.RECALLED then
                table.insert(result, { hash = hash, artifact = art })
            end
        end
        return result
    end

    --- Get registry statistics
    -- @return table Statistics
    function registry:stats()
        local shared_count = 0
        local recalled_count = 0
        for _, art in pairs(self.artifacts) do
            if art.status == artifact.STATUS.SHARED then
                shared_count = shared_count + 1
            else
                recalled_count = recalled_count + 1
            end
        end

        return {
            total_stored = self.total_stored,
            total_revoked = self.total_revoked,
            currently_shared = shared_count,
            currently_recalled = recalled_count,
        }
    end

    --- Clone the registry (for simulating member views)
    -- @return table New registry with same state
    function registry:clone()
        local clone = artifact.KeyRegistry_new()
        for hash, key in pairs(self.keys) do
            clone.keys[hash] = key
        end
        for hash, art in pairs(self.artifacts) do
            clone.artifacts[hash] = {
                name = art.name,
                size = art.size,
                mime_type = art.mime_type,
                sharer = art.sharer,
                status = art.status,
                shared_at = art.shared_at,
                recalled_at = art.recalled_at,
            }
        end
        for _, rev in ipairs(self.revocations) do
            table.insert(clone.revocations, {
                hash = rev.hash,
                revoked_by = rev.revoked_by,
                tick = rev.tick,
                timestamp = rev.timestamp,
            })
        end
        clone.total_stored = self.total_stored
        clone.total_revoked = self.total_revoked
        return clone
    end

    return registry
end

-- Alias for cleaner API
artifact.KeyRegistry = { new = artifact.KeyRegistry_new }

-- ============================================================================
-- ACCESS MODES (for ArtifactIndex)
-- ============================================================================

artifact.ACCESS_MODES = {
    REVOCABLE = "revocable",
    PERMANENT = "permanent",
    TIMED = "timed",
    TRANSFER = "transfer",
}

-- ============================================================================
-- ARTIFACT INDEX (Replaces KeyRegistry for shared filesystem model)
-- ============================================================================

--- Create a new ArtifactIndex for simulation
-- Simulates the home-realm CRDT document for per-artifact access control
-- @return table ArtifactIndex object
function artifact.ArtifactIndex_new()
    local index = {
        -- artifacts[hash] = { name, size, mime_type, owner, status, created_at, grants = {}, provenance = nil }
        artifacts = {},
        -- revocations = array of { hash, revoked_by, tick }
        revocations = {},
        -- Statistics
        total_stored = 0,
        total_grants = 0,
        total_revocations = 0,
        total_transfers = 0,
    }

    --- Store an artifact in the index
    -- @param hash string Artifact hash
    -- @param meta table { name, size, mime_type }
    -- @param owner string Owner member ID
    -- @param tick number Current simulation tick
    -- @return boolean success
    function index:store(hash, meta, owner, tick)
        if self.artifacts[hash] then
            return false -- Already exists
        end
        self.artifacts[hash] = {
            name = meta.name,
            size = meta.size,
            mime_type = meta.mime_type,
            owner = owner,
            status = artifact.STATUS.ACTIVE,
            created_at = tick,
            grants = {},  -- grantee -> { mode, granted_at, granted_by, expires_at }
        }
        self.total_stored = self.total_stored + 1
        return true
    end

    --- Grant access to an artifact
    -- @param hash string Artifact hash
    -- @param grantee string Member to grant access
    -- @param mode string Access mode (revocable/permanent/timed/transfer)
    -- @param granted_by string Who granted
    -- @param tick number Current tick
    -- @param expires_at number|nil Expiry tick for timed grants
    -- @return boolean success, string|nil error
    function index:grant(hash, grantee, mode, granted_by, tick, expires_at)
        local art = self.artifacts[hash]
        if not art then
            return false, "Artifact not found"
        end
        if art.status ~= artifact.STATUS.ACTIVE then
            return false, "Artifact not active"
        end
        if art.grants[grantee] then
            return false, "Already granted"
        end
        art.grants[grantee] = {
            mode = mode,
            granted_at = tick,
            granted_by = granted_by,
            expires_at = expires_at,
        }
        self.total_grants = self.total_grants + 1
        return true
    end

    --- Revoke access from a grantee
    -- @param hash string Artifact hash
    -- @param grantee string Member to revoke
    -- @return boolean success, string|nil error
    function index:revoke_access(hash, grantee)
        local art = self.artifacts[hash]
        if not art then
            return false, "Artifact not found"
        end
        local grant = art.grants[grantee]
        if not grant then
            return false, "No grant found"
        end
        if grant.mode == artifact.ACCESS_MODES.PERMANENT then
            return false, "Cannot revoke permanent grant"
        end
        art.grants[grantee] = nil
        self.total_revocations = self.total_revocations + 1
        return true
    end

    --- Recall an artifact (remove all revocable/timed grants, keep permanent)
    -- @param hash string Artifact hash
    -- @param tick number Current tick
    -- @return boolean success
    function index:recall(hash, tick)
        local art = self.artifacts[hash]
        if not art then
            return false
        end
        art.status = artifact.STATUS.RECALLED
        -- Remove revocable and timed grants, keep permanent
        local to_remove = {}
        for grantee, grant in pairs(art.grants) do
            if grant.mode ~= artifact.ACCESS_MODES.PERMANENT then
                table.insert(to_remove, grantee)
            end
        end
        for _, grantee in ipairs(to_remove) do
            art.grants[grantee] = nil
        end
        table.insert(self.revocations, { hash = hash, tick = tick })
        return true
    end

    --- Transfer ownership of an artifact
    -- @param hash string Artifact hash
    -- @param to string New owner
    -- @param from string Current owner
    -- @param tick number Current tick
    -- @return table|nil transferred entry, string|nil error
    function index:transfer(hash, to, from, tick)
        local art = self.artifacts[hash]
        if not art then
            return nil, "Artifact not found"
        end
        if art.owner ~= from then
            return nil, "Not the owner"
        end
        if art.status ~= artifact.STATUS.ACTIVE then
            return nil, "Artifact not active"
        end
        -- Mark as transferred
        art.status = artifact.STATUS.TRANSFERRED
        -- Create entry for recipient
        local new_entry = {
            name = art.name,
            size = art.size,
            mime_type = art.mime_type,
            owner = to,
            status = artifact.STATUS.ACTIVE,
            created_at = tick,
            grants = {},
            provenance = { original_owner = from, received_at = tick },
        }
        -- Auto-grant revocable access back to sender
        new_entry.grants[from] = {
            mode = artifact.ACCESS_MODES.REVOCABLE,
            granted_at = tick,
            granted_by = to,
        }
        self.total_transfers = self.total_transfers + 1
        return new_entry
    end

    --- Get artifacts accessible by a specific member
    -- @param member string Member ID
    -- @param now number Current tick (for expiry check)
    -- @return table Array of { hash, artifact } pairs
    function index:accessible_by(member, now)
        local result = {}
        for hash, art in pairs(self.artifacts) do
            if art.status == artifact.STATUS.ACTIVE then
                -- Owner always has access
                if art.owner == member then
                    table.insert(result, { hash = hash, artifact = art })
                else
                    local grant = art.grants[member]
                    if grant then
                        -- Check timed expiry
                        if grant.mode == artifact.ACCESS_MODES.TIMED and grant.expires_at and now > grant.expires_at then
                            -- Expired, skip
                        else
                            table.insert(result, { hash = hash, artifact = art })
                        end
                    end
                end
            end
        end
        return result
    end

    --- Get artifacts accessible by ALL members in a list (realm view)
    -- @param members table Array of member IDs
    -- @param now number Current tick
    -- @return table Array of { hash, artifact } pairs
    function index:accessible_by_all(members, now)
        local result = {}
        for hash, art in pairs(self.artifacts) do
            if art.status == artifact.STATUS.ACTIVE then
                local all_have_access = true
                for _, member in ipairs(members) do
                    if art.owner ~= member then
                        local grant = art.grants[member]
                        if not grant then
                            all_have_access = false
                            break
                        end
                        if grant.mode == artifact.ACCESS_MODES.TIMED and grant.expires_at and now > grant.expires_at then
                            all_have_access = false
                            break
                        end
                    end
                end
                if all_have_access then
                    table.insert(result, { hash = hash, artifact = art })
                end
            end
        end
        return result
    end

    --- Remove expired timed grants
    -- @param now number Current tick
    -- @return number Count of expired grants removed
    function index:gc_expired(now)
        local removed = 0
        for _, art in pairs(self.artifacts) do
            local to_remove = {}
            for grantee, grant in pairs(art.grants) do
                if grant.mode == artifact.ACCESS_MODES.TIMED and grant.expires_at and now > grant.expires_at then
                    table.insert(to_remove, grantee)
                end
            end
            for _, grantee in ipairs(to_remove) do
                art.grants[grantee] = nil
                removed = removed + 1
            end
        end
        return removed
    end

    --- Check if a grant allows download
    -- @param mode string Access mode
    -- @return boolean
    function index:allows_download(mode)
        return mode == artifact.ACCESS_MODES.PERMANENT
    end

    --- Get index statistics
    -- @return table Statistics
    function index:stats()
        local active = 0
        local recalled = 0
        local transferred = 0
        local total_grants = 0
        for _, art in pairs(self.artifacts) do
            if art.status == artifact.STATUS.ACTIVE then
                active = active + 1
            elseif art.status == artifact.STATUS.RECALLED then
                recalled = recalled + 1
            elseif art.status == artifact.STATUS.TRANSFERRED then
                transferred = transferred + 1
            end
            for _ in pairs(art.grants) do
                total_grants = total_grants + 1
            end
        end
        return {
            total_stored = self.total_stored,
            active = active,
            recalled = recalled,
            transferred = transferred,
            total_grants = total_grants,
            total_revocations = self.total_revocations,
            total_transfers = self.total_transfers,
        }
    end

    return index
end

-- Alias for cleaner API
artifact.ArtifactIndex = { new = artifact.ArtifactIndex_new }

-- ============================================================================
-- ID AND KEY GENERATION HELPERS
-- ============================================================================

local id_counter = 0

--- Generate a unique artifact hash (simulates blake3)
-- @return string 64-character hex string
function artifact.generate_hash()
    id_counter = id_counter + 1
    local chars = "0123456789abcdef"
    local result = {}
    local seed = id_counter * 1000 + math.random(999)
    for i = 1, 64 do
        local idx = ((seed * (i + 7) + math.random(16)) % 16) + 1
        result[i] = chars:sub(idx, idx)
    end
    return table.concat(result)
end

--- Generate a simulated encrypted key
-- @return string 32-character hex string (simulates 128-bit key)
function artifact.generate_encrypted_key()
    local chars = "0123456789abcdef"
    local result = {}
    for i = 1, 32 do
        local idx = math.random(1, 16)
        result[i] = chars:sub(idx, idx)
    end
    return table.concat(result)
end

--- Generate mock artifact data
-- @param name string Optional artifact name
-- @param size number Optional size in bytes
-- @param mime_type string Optional MIME type
-- @return table Artifact metadata
function artifact.generate_mock_artifact(name, size, mime_type)
    return {
        name = name or string.format("artifact_%d.bin", math.random(10000, 99999)),
        size = size or math.random(1024, 1048576),  -- 1KB to 1MB
        mime_type = mime_type or "application/octet-stream",
    }
end

-- Predefined test artifacts for realistic scenarios
artifact.TEST_ARTIFACTS = {
    { name = "document.pdf", size = 102400, mime_type = "application/pdf" },
    { name = "image.png", size = 204800, mime_type = "image/png" },
    { name = "video.mp4", size = 5242880, mime_type = "video/mp4" },
    { name = "data.json", size = 4096, mime_type = "application/json" },
    { name = "archive.zip", size = 1048576, mime_type = "application/zip" },
    { name = "notes.md", size = 2048, mime_type = "text/markdown" },
}

--- Get a random test artifact template
-- @return table Artifact metadata
function artifact.random_test_artifact()
    local template = artifact.TEST_ARTIFACTS[math.random(#artifact.TEST_ARTIFACTS)]
    return {
        name = template.name,
        size = template.size + math.random(-500, 500),  -- Add some variance
        mime_type = template.mime_type,
    }
end

-- ============================================================================
-- LATENCY MODELS
-- ============================================================================

--- Simulate artifact share latency (500-2000 microseconds)
-- Higher due to encryption overhead
-- @return number Latency in microseconds
function artifact.share_latency()
    return 500 + math.random(1500)
end

--- Simulate artifact recall latency (100-300 microseconds)
-- @return number Latency in microseconds
function artifact.recall_latency()
    return 100 + math.random(200)
end

--- Simulate key registry lookup latency (50-150 microseconds)
-- @return number Latency in microseconds
function artifact.registry_lookup_latency()
    return 50 + math.random(100)
end

--- Simulate artifact download latency (based on size)
-- @param size number Artifact size in bytes
-- @return number Latency in microseconds
function artifact.download_latency(size)
    -- Base latency + size-dependent component
    -- Assume ~10MB/s effective transfer
    local base = 200
    local size_factor = math.floor(size / 10240)  -- 10KB units
    return base + size_factor + math.random(100)
end

--- Simulate CRDT sync latency (200-500 microseconds)
-- @return number Latency in microseconds
function artifact.sync_latency()
    return 200 + math.random(300)
end

--- Simulate grant latency (100-400 microseconds)
-- @return number Latency in microseconds
function artifact.grant_latency()
    return 100 + math.random(300)
end

--- Simulate transfer latency (300-800 microseconds)
-- @return number Latency in microseconds
function artifact.transfer_latency()
    return 300 + math.random(500)
end

--- Simulate recovery latency (1000-5000 microseconds)
-- @return number Latency in microseconds
function artifact.recovery_latency()
    return 1000 + math.random(4000)
end

-- ============================================================================
-- STATISTICS HELPERS
-- ============================================================================

--- Calculate percentile from array of values
-- @param values table Array of numeric values
-- @param p number Percentile (0-100)
-- @return number The percentile value
function artifact.percentile(values, p)
    if #values == 0 then return 0 end

    local sorted = {}
    for _, v in ipairs(values) do
        table.insert(sorted, v)
    end
    table.sort(sorted)

    local idx = math.ceil(#sorted * p / 100)
    return sorted[math.max(1, idx)]
end

--- Calculate multiple percentiles
-- @param values table Array of numeric values
-- @return table Table with p50, p95, p99 values
function artifact.percentiles(values)
    return {
        p50 = artifact.percentile(values, 50),
        p95 = artifact.percentile(values, 95),
        p99 = artifact.percentile(values, 99),
    }
end

--- Calculate average of values
-- @param values table Array of numeric values
-- @return number Average value
function artifact.average(values)
    if #values == 0 then return 0 end
    local sum = 0
    for _, v in ipairs(values) do
        sum = sum + v
    end
    return sum / #values
end

-- ============================================================================
-- RESULT BUILDER
-- ============================================================================

--- Create a result builder for artifact scenarios
-- @param scenario_name string Name of the scenario
-- @return table Result builder object
function artifact.result_builder(scenario_name)
    local builder = {
        scenario = scenario_name,
        level = artifact.get_level(),
        started_at = os.time(),
        metrics = {},
        assertions = {},
        passed = true,
        errors = {}
    }

    function builder:add_metric(name, value)
        self.metrics[name] = value
        return self
    end

    function builder:add_metrics(metrics_table)
        for k, v in pairs(metrics_table) do
            self.metrics[k] = v
        end
        return self
    end

    function builder:record_assertion(name, passed, expected, actual)
        table.insert(self.assertions, {
            name = name,
            passed = passed,
            expected = expected,
            actual = actual
        })
        if not passed then
            self.passed = false
            table.insert(self.errors, string.format(
                "Assertion '%s' failed: expected %s, got %s",
                name, tostring(expected), tostring(actual)
            ))
        end
        return self
    end

    function builder:add_error(msg)
        table.insert(self.errors, msg)
        self.passed = false
        return self
    end

    function builder:build()
        self.ended_at = os.time()
        self.duration_sec = self.ended_at - self.started_at

        return {
            scenario = self.scenario,
            level = self.level,
            passed = self.passed,
            duration_sec = self.duration_sec,
            metrics = self.metrics,
            assertions = self.assertions,
            errors = self.errors
        }
    end

    return builder
end

return artifact

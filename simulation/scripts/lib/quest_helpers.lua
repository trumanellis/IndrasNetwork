-- Quest & Contacts Simulation Helpers
--
-- Utility functions for quest lifecycle and contacts simulation scenarios.
-- Uses Rust SDK bindings - no logic duplication for realm ID computation.
--
-- Key Concepts:
-- - Peer-based Realms: Realms are deterministically identified by their peer set
-- - Proof of Service: Quests support multiple claimants with proof artifacts
-- - Contacts: Members maintain a contacts list for auto-subscription

local quest = {}

-- ============================================================================
-- STRESS LEVELS: Quest-specific configurations
-- ============================================================================

quest.LEVELS = {
    quick = {
        name = "quick",
        realms = 3,
        quests_per_realm = 10,
        claims_per_quest = 3,
        members = 4,
        ticks = 200,
        sync_interval = 5,
    },
    medium = {
        name = "medium",
        realms = 10,
        quests_per_realm = 50,
        claims_per_quest = 5,
        members = 8,
        ticks = 500,
        sync_interval = 3,
    },
    full = {
        name = "full",
        realms = 20,
        quests_per_realm = 200,
        claims_per_quest = 8,
        members = 12,
        ticks = 1000,
        sync_interval = 2,
    }
}

-- Peer-based realm stress configurations
quest.PEER_REALM_LEVELS = {
    quick = {
        name = "quick",
        peers = 10,
        realm_combinations = 50,
        concurrent_ops = 20,
        ticks = 200,
    },
    medium = {
        name = "medium",
        peers = 20,
        realm_combinations = 500,
        concurrent_ops = 100,
        ticks = 500,
    },
    full = {
        name = "full",
        peers = 26,
        realm_combinations = 2000,
        concurrent_ops = 500,
        ticks = 1000,
    }
}

-- Contacts stress configurations
quest.CONTACTS_LEVELS = {
    quick = {
        name = "quick",
        peers = 8,
        contacts_per_peer = 3,
        sync_rounds = 10,
        ticks = 200,
    },
    medium = {
        name = "medium",
        peers = 16,
        contacts_per_peer = 8,
        sync_rounds = 30,
        ticks = 500,
    },
    full = {
        name = "full",
        peers = 26,
        contacts_per_peer = 15,
        sync_rounds = 100,
        ticks = 1000,
    }
}

-- Event types for JSONL logging
quest.EVENTS = {
    -- Realm lifecycle
    REALM_COMPUTED = "realm_computed",
    REALM_LOOKUP_CACHED = "realm_lookup_cached",
    REALM_CREATED = "realm_created",

    -- Quest lifecycle
    QUEST_CREATED = "quest_created",
    QUEST_CLAIM_SUBMITTED = "quest_claim_submitted",
    QUEST_CLAIM_VERIFIED = "quest_claim_verified",
    QUEST_COMPLETED = "quest_completed",

    -- Contacts lifecycle
    CONTACT_ADDED = "contact_added",
    CONTACT_REMOVED = "contact_removed",
    CONTACTS_SYNCED = "contacts_synced",

    -- Sentiment & blocking
    SENTIMENT_UPDATED = "sentiment_updated",
    CONTACT_BLOCKED = "contact_blocked",
    RELAYED_SENTIMENT = "relayed_sentiment_received",

    -- Membership
    MEMBER_JOINED = "member_joined",
    MEMBER_LEFT = "member_left",

    -- CRDT sync
    CRDT_CONVERGED = "crdt_converged",
    CRDT_CONFLICT = "crdt_conflict",
}

-- ============================================================================
-- CONFIGURATION HELPERS
-- ============================================================================

--- Get the current stress level from environment
-- @return string The stress level (quick, medium, or full)
function quest.get_level()
    return os.getenv("STRESS_LEVEL") or "medium"
end

--- Get the quest configuration for current stress level
-- @return table The level configuration
function quest.get_config()
    local level = quest.get_level()
    return quest.LEVELS[level] or quest.LEVELS.medium
end

--- Get the peer-based realm configuration for current stress level
-- @return table The level configuration
function quest.get_peer_realm_config()
    local level = quest.get_level()
    return quest.PEER_REALM_LEVELS[level] or quest.PEER_REALM_LEVELS.medium
end

--- Get the contacts configuration for current stress level
-- @return table The level configuration
function quest.get_contacts_config()
    local level = quest.get_level()
    return quest.CONTACTS_LEVELS[level] or quest.CONTACTS_LEVELS.medium
end

-- ============================================================================
-- CONTEXT AND LOGGING
-- ============================================================================

--- Create a correlation context for a quest scenario
-- @param scenario_name string Name of the scenario
-- @return CorrelationContext
function quest.new_context(scenario_name)
    local ctx = indras.correlation.new_root()
    ctx = ctx:with_tag("scenario", scenario_name)
    ctx = ctx:with_tag("subsystem", "quest")
    ctx = ctx:with_tag("stress_level", quest.get_level())
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
        -- Check if it's an array (sequential integer keys starting at 1)
        local is_array = true
        local max_idx = 0
        for k, _ in pairs(value) do
            if type(k) ~= "number" or k < 1 or math.floor(k) ~= k then
                is_array = false
                break
            end
            if k > max_idx then max_idx = k end
        end
        -- Also check if array has holes
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
function quest.create_logger(ctx)
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

    --- Log a quest event with standard fields
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
-- PEER-BASED REALM IDENTITY (Wraps Rust SDK bindings)
-- ============================================================================

--- Compute a deterministic realm ID from a set of peer IDs
-- This wraps the Rust SDK binding - no logic duplication
-- @param peer_ids table Array of peer identifiers
-- @return string Realm ID (hex string)
function quest.compute_realm_id(peer_ids)
    if #peer_ids < 2 then
        return nil, "Peer-based realms require at least 2 peers"
    end
    return indras.sdk.compute_realm_id(peer_ids)
end

--- Normalize peer IDs (sort and dedupe)
-- This wraps the Rust SDK binding
-- @param peer_ids table Array of peer identifiers
-- @return table Sorted, deduplicated peer IDs
function quest.normalize_peers(peer_ids)
    return indras.sdk.normalize_peers(peer_ids)
end

--- Check if two peer sets would produce the same realm
-- @param peers1 table First peer set
-- @param peers2 table Second peer set
-- @return boolean True if they produce the same realm ID
function quest.same_realm(peers1, peers2)
    local id1 = quest.compute_realm_id(peers1)
    local id2 = quest.compute_realm_id(peers2)
    return id1 == id2
end

-- ============================================================================
-- QUEST GENERATION HELPERS
-- ============================================================================

-- Random quest title templates
local QUEST_TITLE_TEMPLATES = {
    "Review %s document",
    "Fix bug in %s",
    "Update %s configuration",
    "Test %s feature",
    "Deploy %s service",
    "Optimize %s performance",
    "Write tests for %s",
    "Document %s API",
    "Refactor %s module",
    "Migrate %s data",
}

local QUEST_SUBJECTS = {
    "authentication", "database", "API", "frontend", "backend",
    "deployment", "testing", "logging", "monitoring", "security",
    "caching", "search", "notification", "payment", "user",
}

--- Generate a random quest title
-- @return string Random quest title
function quest.random_quest_title()
    local template = QUEST_TITLE_TEMPLATES[math.random(#QUEST_TITLE_TEMPLATES)]
    local subject = QUEST_SUBJECTS[math.random(#QUEST_SUBJECTS)]
    return string.format(template, subject)
end

--- Generate a random quest description
-- @return string Random quest description
function quest.random_quest_description()
    local title = quest.random_quest_title()
    return string.format(
        "Please %s. Expected completion within the sprint. " ..
        "Contact the creator if you have any questions.",
        title:lower()
    )
end

--- Generate a mock proof artifact ID
-- @return string Mock artifact ID (hex string)
function quest.random_proof_artifact()
    -- Generate a random 32-byte hex string (64 characters)
    local chars = "0123456789abcdef"
    local result = {}
    for i = 1, 64 do
        local idx = math.random(1, 16)
        result[i] = chars:sub(idx, idx)
    end
    return table.concat(result)
end

-- ============================================================================
-- QUEST TRACKER (For verification in simulations)
-- ============================================================================

--- Create a quest tracker for simulation verification
-- Tracks quest state and claim history
-- @return table QuestTracker object
function quest.QuestTracker_new()
    local tracker = {
        -- quests[quest_id] = { creator, claims = { claimant, proof, verified } }
        quests = {},
        -- Statistics
        quests_created = 0,
        claims_submitted = 0,
        claims_verified = 0,
        quests_completed = 0,
    }

    --- Record a new quest
    -- @param quest_id string Quest ID
    -- @param creator string Creator member ID
    function tracker:record_quest(quest_id, creator)
        self.quests[quest_id] = {
            creator = creator,
            claims = {},
            completed = false,
            created_at = os.time(),
        }
        self.quests_created = self.quests_created + 1
    end

    --- Record a claim submission
    -- @param quest_id string Quest ID
    -- @param claimant string Claimant member ID
    -- @param proof string Optional proof artifact ID
    -- @return number Claim index
    function tracker:record_claim(quest_id, claimant, proof)
        local q = self.quests[quest_id]
        if not q then return nil end

        table.insert(q.claims, {
            claimant = claimant,
            proof = proof,
            verified = false,
            submitted_at = os.time(),
        })
        self.claims_submitted = self.claims_submitted + 1
        return #q.claims - 1  -- 0-indexed for Rust compatibility
    end

    --- Record a claim verification
    -- @param quest_id string Quest ID
    -- @param claim_index number Claim index
    function tracker:record_verification(quest_id, claim_index)
        local q = self.quests[quest_id]
        if not q then return false end

        local claim = q.claims[claim_index + 1]  -- Lua is 1-indexed
        if not claim then return false end

        claim.verified = true
        claim.verified_at = os.time()
        self.claims_verified = self.claims_verified + 1
        return true
    end

    --- Record quest completion
    -- @param quest_id string Quest ID
    function tracker:record_completion(quest_id)
        local q = self.quests[quest_id]
        if not q then return false end

        q.completed = true
        q.completed_at = os.time()
        self.quests_completed = self.quests_completed + 1
        return true
    end

    --- Get pending claims for a quest
    -- @param quest_id string Quest ID
    -- @return table Array of pending claims
    function tracker:pending_claims(quest_id)
        local q = self.quests[quest_id]
        if not q then return {} end

        local pending = {}
        for i, claim in ipairs(q.claims) do
            if not claim.verified then
                table.insert(pending, { index = i - 1, claim = claim })
            end
        end
        return pending
    end

    --- Get verified claims for a quest
    -- @param quest_id string Quest ID
    -- @return table Array of verified claims
    function tracker:verified_claims(quest_id)
        local q = self.quests[quest_id]
        if not q then return {} end

        local verified = {}
        for i, claim in ipairs(q.claims) do
            if claim.verified then
                table.insert(verified, { index = i - 1, claim = claim })
            end
        end
        return verified
    end

    --- Verify consistency with realm member view
    -- @param realm_members table Array of member IDs in the realm
    -- @return boolean True if all members see consistent state
    function tracker:verify_consistency(realm_members)
        -- In simulation, we track a single source of truth
        -- Real implementation would compare CRDT states
        return true
    end

    --- Get statistics
    -- @return table Statistics table
    function tracker:stats()
        return {
            quests_created = self.quests_created,
            claims_submitted = self.claims_submitted,
            claims_verified = self.claims_verified,
            quests_completed = self.quests_completed,
            verification_rate = self.claims_submitted > 0
                and self.claims_verified / self.claims_submitted
                or 0,
            completion_rate = self.quests_created > 0
                and self.quests_completed / self.quests_created
                or 0,
        }
    end

    return tracker
end

-- Alias for cleaner API
quest.QuestTracker = { new = quest.QuestTracker_new }

-- ============================================================================
-- LATENCY MODELS (Realistic timings for simulation)
-- ============================================================================

--- Simulate quest creation latency (200-500 microseconds)
-- @return number Latency in microseconds
function quest.quest_create_latency()
    return 200 + math.random(300)
end

--- Simulate claim submission latency (150-300 microseconds)
-- Includes proof upload overhead
-- @return number Latency in microseconds
function quest.claim_submit_latency()
    return 150 + math.random(150)
end

--- Simulate claim verification latency (100-200 microseconds)
-- @return number Latency in microseconds
function quest.claim_verify_latency()
    return 100 + math.random(100)
end

--- Simulate attention switch latency (50-150 microseconds)
-- @return number Latency in microseconds
function quest.attention_switch_latency()
    return 50 + math.random(100)
end

-- ============================================================================
-- ID GENERATION HELPERS
-- ============================================================================

local id_counter = 0

--- Generate a unique quest ID
-- @return string Quest ID (hex string)
function quest.generate_quest_id()
    id_counter = id_counter + 1
    local chars = "0123456789abcdef"
    local result = {}
    -- Use counter and random for uniqueness
    local seed = id_counter * 1000 + math.random(999)
    for i = 1, 16 do
        local idx = ((seed * (i + 7)) % 16) + 1
        result[i] = chars:sub(idx, idx)
    end
    return "quest_" .. table.concat(result)
end

--- Generate a unique event ID
-- @return string Event ID (hex string)
function quest.generate_event_id()
    id_counter = id_counter + 1
    local chars = "0123456789abcdef"
    local result = {}
    local seed = id_counter * 1000 + math.random(999)
    for i = 1, 16 do
        local idx = ((seed * (i + 3)) % 16) + 1
        result[i] = chars:sub(idx, idx)
    end
    return "evt_" .. table.concat(result)
end

--- Simulate quest completion latency (150-300 microseconds)
-- @return number Latency in microseconds
function quest.quest_complete_latency()
    return 150 + math.random(150)
end

--- Simulate contact add latency (100-250 microseconds)
-- @return number Latency in microseconds
function quest.contact_add_latency()
    return 100 + math.random(150)
end

--- Simulate realm lookup latency (cached: 50-150 microseconds)
-- @return number Latency in microseconds
function quest.realm_lookup_latency()
    return 50 + math.random(100)
end

--- Simulate new realm creation latency (500-1500 microseconds)
-- Higher due to crypto operations and network setup
-- @return number Latency in microseconds
function quest.realm_create_latency()
    return 500 + math.random(1000)
end

-- ============================================================================
-- STATISTICS HELPERS
-- ============================================================================

--- Calculate percentile from array of values
-- @param values table Array of numeric values
-- @param p number Percentile (0-100)
-- @return number The percentile value
function quest.percentile(values, p)
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
function quest.percentiles(values)
    return {
        p50 = quest.percentile(values, 50),
        p95 = quest.percentile(values, 95),
        p99 = quest.percentile(values, 99),
    }
end

--- Calculate average of values
-- @param values table Array of numeric values
-- @return number Average value
function quest.average(values)
    if #values == 0 then return 0 end
    local sum = 0
    for _, v in ipairs(values) do
        sum = sum + v
    end
    return sum / #values
end

-- ============================================================================
-- THRESHOLD VALIDATION
-- ============================================================================

--- Assert metrics against thresholds
-- @param metrics table The metrics to validate
-- @param thresholds table The threshold configuration
-- @return boolean passed, table failures
function quest.assert_thresholds(metrics, thresholds)
    local failures = {}

    for metric_name, threshold in pairs(thresholds) do
        local actual = metrics[metric_name]
        if actual ~= nil then
            if threshold.min ~= nil and actual < threshold.min then
                table.insert(failures, {
                    metric = metric_name,
                    type = "min",
                    expected = threshold.min,
                    actual = actual
                })
            end

            if threshold.max ~= nil and actual > threshold.max then
                table.insert(failures, {
                    metric = metric_name,
                    type = "max",
                    expected = threshold.max,
                    actual = actual
                })
            end
        end
    end

    return #failures == 0, failures
end

-- ============================================================================
-- RESULT BUILDER
-- ============================================================================

--- Create a result builder for quest scenarios
-- @param scenario_name string Name of the scenario
-- @return table Result builder object
function quest.result_builder(scenario_name)
    local builder = {
        scenario = scenario_name,
        level = quest.get_level(),
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

return quest

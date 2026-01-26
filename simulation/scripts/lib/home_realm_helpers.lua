-- Home Realm Simulation Helpers
--
-- Utility functions for home realm simulation scenarios.
-- Uses Rust SDK bindings for deterministic home realm ID computation.
--
-- Key Concepts:
-- - Home Realm: Personal realm unique to each member
-- - Deterministic ID: home_realm_id = blake3("home-realm-v1:" + member_id)
-- - Multi-device sync: Same user can access from multiple devices
-- - Notes: Markdown documents with tags and timestamps

local home = {}

-- ============================================================================
-- STRESS LEVELS: Home realm specific configurations
-- ============================================================================

home.LEVELS = {
    quick = {
        name = "quick",
        members = 5,
        notes_per_member = 10,
        quests_per_member = 5,
        artifacts_per_member = 3,
        ticks = 200,
        sync_interval = 5,
    },
    medium = {
        name = "medium",
        members = 12,
        notes_per_member = 50,
        quests_per_member = 20,
        artifacts_per_member = 10,
        ticks = 500,
        sync_interval = 3,
    },
    full = {
        name = "full",
        members = 26,
        notes_per_member = 200,
        quests_per_member = 50,
        artifacts_per_member = 25,
        ticks = 1000,
        sync_interval = 2,
    }
}

-- Event types for JSONL logging
home.EVENTS = {
    -- Home realm lifecycle
    HOME_REALM_ID_COMPUTED = "home_realm_id_computed",
    HOME_REALM_CREATED = "home_realm_created",
    HOME_REALM_ACCESSED = "home_realm_accessed",

    -- Note lifecycle
    NOTE_CREATED = "note_created",
    NOTE_UPDATED = "note_updated",
    NOTE_DELETED = "note_deleted",
    NOTE_TAG_ADDED = "note_tag_added",

    -- Quest lifecycle (in home realm)
    HOME_QUEST_CREATED = "home_quest_created",
    HOME_QUEST_COMPLETED = "home_quest_completed",

    -- Artifact lifecycle
    ARTIFACT_UPLOADED = "artifact_uploaded",
    ARTIFACT_RETRIEVED = "artifact_retrieved",

    -- Persistence
    SESSION_STARTED = "session_started",
    SESSION_ENDED = "session_ended",
    DATA_PERSISTED = "data_persisted",
    DATA_RECOVERED = "data_recovered",

    -- Sync
    CRDT_CONVERGED = "crdt_converged",
    MULTI_DEVICE_SYNC = "multi_device_sync",
}

-- ============================================================================
-- CONFIGURATION HELPERS
-- ============================================================================

--- Get the current stress level from environment
-- @return string The stress level (quick, medium, or full)
function home.get_level()
    return os.getenv("STRESS_LEVEL") or "medium"
end

--- Get the home realm configuration for current stress level
-- @return table The level configuration
function home.get_config()
    local level = home.get_level()
    return home.LEVELS[level] or home.LEVELS.medium
end

-- ============================================================================
-- CONTEXT AND LOGGING
-- ============================================================================

--- Create a correlation context for a home realm scenario
-- @param scenario_name string Name of the scenario
-- @return CorrelationContext
function home.new_context(scenario_name)
    local ctx = indras.correlation.new_root()
    ctx = ctx:with_tag("scenario", scenario_name)
    ctx = ctx:with_tag("subsystem", "home_realm")
    ctx = ctx:with_tag("stress_level", home.get_level())
    return ctx
end

--- Create a context logger with automatic trace_id
-- @param ctx CorrelationContext The correlation context
-- @return table Logger object
function home.create_logger(ctx)
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

    --- Log a home realm event with standard fields
    function logger.event(event_type, fields)
        fields = fields or {}
        fields.trace_id = ctx.trace_id
        fields.event_type = event_type
        indras.log.info(event_type, fields)
    end

    return logger
end

-- ============================================================================
-- HOME REALM IDENTITY
-- ============================================================================

--- Compute a deterministic home realm ID from a member ID
-- home_realm_id = blake3("home-realm-v1:" + member_id)
-- @param member_id string Member identifier
-- @return string Home realm ID (hex string)
function home.compute_home_realm_id(member_id)
    -- Use indras SDK binding if available (wraps Rust implementation)
    if indras and indras.sdk and indras.sdk.compute_home_realm_id then
        return indras.sdk.compute_home_realm_id(member_id)
    end

    -- Fallback: Simple deterministic hash using string operations
    -- This simulates blake3 behavior with a portable Lua implementation
    local data = "home-realm-v1:" .. member_id

    -- Simple hash function (djb2 variant)
    local hash = 5381
    for i = 1, #data do
        hash = ((hash * 33) + data:byte(i)) % (2^32)
    end

    -- Expand to 64-character hex string (32 bytes)
    -- Use multiple rounds with different seeds for each segment
    local result = {}
    for i = 1, 8 do
        local segment = (hash * (i * 31337) + (i * 12345)) % (2^32)
        result[i] = string.format("%08x", math.floor(segment))
    end
    return table.concat(result)
end

--- Check if two member IDs would produce the same home realm
-- (They shouldn't - each member has unique home realm)
-- @param member1 string First member ID
-- @param member2 string Second member ID
-- @return boolean True if they produce the same home realm ID (should be false)
function home.same_home_realm(member1, member2)
    local id1 = home.compute_home_realm_id(member1)
    local id2 = home.compute_home_realm_id(member2)
    return id1 == id2
end

-- ============================================================================
-- NOTE GENERATION HELPERS
-- ============================================================================

-- Random note title templates
local NOTE_TITLE_TEMPLATES = {
    "Meeting Notes: %s",
    "Ideas for %s",
    "TODO: %s",
    "Journal Entry: %s",
    "Research on %s",
    "Plan for %s",
    "Summary of %s",
    "Notes on %s",
    "Draft: %s",
    "Quick thoughts on %s",
}

local NOTE_SUBJECTS = {
    "project kickoff", "weekly standup", "architecture review",
    "feature planning", "bug triage", "sprint retrospective",
    "design discussion", "code review", "release planning",
    "performance optimization", "security audit", "documentation",
}

local NOTE_TAGS = {
    "work", "personal", "urgent", "later", "idea", "meeting",
    "project", "research", "draft", "important", "archive",
}

--- Generate a random note title
-- @return string Random note title
function home.random_note_title()
    local template = NOTE_TITLE_TEMPLATES[math.random(#NOTE_TITLE_TEMPLATES)]
    local subject = NOTE_SUBJECTS[math.random(#NOTE_SUBJECTS)]
    return string.format(template, subject)
end

--- Generate random markdown content
-- @return string Random markdown content
function home.random_note_content()
    local lines = {
        "# " .. home.random_note_title(),
        "",
        "## Overview",
        "",
        "This is a simulated note with markdown content.",
        "",
        "## Key Points",
        "",
        "- First important point",
        "- Second important point",
        "- Third important point",
        "",
        "## Next Steps",
        "",
        "1. Review this document",
        "2. Take action on items",
        "3. Follow up next week",
        "",
        string.format("Created: %s", os.date("%Y-%m-%d %H:%M:%S")),
    }
    return table.concat(lines, "\n")
end

--- Generate random tags for a note
-- @param count number Number of tags to generate (default 1-3)
-- @return table Array of tag strings
function home.random_tags(count)
    count = count or math.random(1, 3)
    local tags = {}
    local used = {}

    while #tags < count do
        local idx = math.random(#NOTE_TAGS)
        if not used[idx] then
            used[idx] = true
            table.insert(tags, NOTE_TAGS[idx])
        end
    end

    return tags
end

-- ============================================================================
-- QUEST GENERATION HELPERS (for home realm)
-- ============================================================================

local PERSONAL_QUEST_TEMPLATES = {
    "Complete %s task",
    "Review %s material",
    "Practice %s",
    "Read about %s",
    "Work on %s project",
    "Finish %s assignment",
}

local PERSONAL_SUBJECTS = {
    "programming", "writing", "exercise", "learning",
    "organizing", "planning", "research", "design",
}

--- Generate a random personal quest title
-- @return string Random quest title
function home.random_quest_title()
    local template = PERSONAL_QUEST_TEMPLATES[math.random(#PERSONAL_QUEST_TEMPLATES)]
    local subject = PERSONAL_SUBJECTS[math.random(#PERSONAL_SUBJECTS)]
    return string.format(template, subject)
end

--- Generate a random quest description
-- @return string Random quest description
function home.random_quest_description()
    return string.format(
        "Personal task: %s. Target completion by end of week.",
        home.random_quest_title():lower()
    )
end

-- ============================================================================
-- ARTIFACT HELPERS
-- ============================================================================

--- Generate mock artifact data (simulated PNG)
-- @param size number Approximate size in bytes (default 100-1000)
-- @return table { data = bytes, size = number, mime_type = string }
function home.generate_mock_artifact(size)
    size = size or math.random(100, 1000)

    -- PNG signature + minimal structure
    local data = {
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A,  -- PNG signature
    }

    -- Add random bytes to reach desired size
    for i = 1, size - 8 do
        table.insert(data, math.random(0, 255))
    end

    return {
        data = data,
        size = #data,
        mime_type = "image/png",
        name = string.format("artifact_%s.png", math.random(10000, 99999)),
    }
end

--- Generate a mock artifact ID (blake3 hash simulation)
-- @return string 64-character hex string
function home.generate_artifact_id()
    local chars = "0123456789abcdef"
    local result = {}
    for i = 1, 64 do
        local idx = math.random(1, 16)
        result[i] = chars:sub(idx, idx)
    end
    return table.concat(result)
end

-- ============================================================================
-- NOTE TRACKER (For verification in simulations)
-- ============================================================================

--- Create a note tracker for simulation verification
-- @return table NoteTracker object
function home.NoteTracker_new()
    local tracker = {
        -- notes[note_id] = { title, content, tags, created_at, updated_at }
        notes = {},
        -- Statistics
        notes_created = 0,
        notes_updated = 0,
        notes_deleted = 0,
    }

    --- Record a new note
    -- @param note_id string Note ID
    -- @param title string Note title
    -- @param content string Note content
    -- @param tags table Note tags
    function tracker:record_note(note_id, title, content, tags)
        local now = os.time()
        self.notes[note_id] = {
            title = title,
            content = content,
            tags = tags or {},
            created_at = now,
            updated_at = now,
        }
        self.notes_created = self.notes_created + 1
    end

    --- Record a note update
    -- @param note_id string Note ID
    -- @param content string New content (optional)
    -- @param tags table New tags (optional)
    function tracker:record_update(note_id, content, tags)
        local note = self.notes[note_id]
        if not note then return false end

        if content then note.content = content end
        if tags then note.tags = tags end
        note.updated_at = os.time()
        self.notes_updated = self.notes_updated + 1
        return true
    end

    --- Record note deletion
    -- @param note_id string Note ID
    function tracker:record_deletion(note_id)
        if self.notes[note_id] then
            self.notes[note_id] = nil
            self.notes_deleted = self.notes_deleted + 1
            return true
        end
        return false
    end

    --- Get notes by tag
    -- @param tag string Tag to filter by
    -- @return table Array of notes with the tag
    function tracker:notes_with_tag(tag)
        local result = {}
        for id, note in pairs(self.notes) do
            for _, t in ipairs(note.tags) do
                if t == tag then
                    table.insert(result, { id = id, note = note })
                    break
                end
            end
        end
        return result
    end

    --- Get statistics
    -- @return table Statistics table
    function tracker:stats()
        local count = 0
        for _ in pairs(self.notes) do count = count + 1 end

        return {
            notes_created = self.notes_created,
            notes_updated = self.notes_updated,
            notes_deleted = self.notes_deleted,
            current_count = count,
        }
    end

    return tracker
end

home.NoteTracker = { new = home.NoteTracker_new }

-- ============================================================================
-- LATENCY MODELS
-- ============================================================================

--- Simulate home realm ID computation latency (10-50 microseconds)
-- Very fast due to blake3 efficiency
-- @return number Latency in microseconds
function home.realm_id_latency()
    return 10 + math.random(40)
end

--- Simulate note creation latency (100-300 microseconds)
-- @return number Latency in microseconds
function home.note_create_latency()
    return 100 + math.random(200)
end

--- Simulate note update latency (80-200 microseconds)
-- @return number Latency in microseconds
function home.note_update_latency()
    return 80 + math.random(120)
end

--- Simulate artifact upload latency (500-2000 microseconds)
-- Higher due to blob storage overhead
-- @return number Latency in microseconds
function home.artifact_upload_latency()
    return 500 + math.random(1500)
end

--- Simulate artifact retrieval latency (200-800 microseconds)
-- @return number Latency in microseconds
function home.artifact_retrieve_latency()
    return 200 + math.random(600)
end

-- ============================================================================
-- STATISTICS HELPERS
-- ============================================================================

--- Calculate percentile from array of values
-- @param values table Array of numeric values
-- @param p number Percentile (0-100)
-- @return number The percentile value
function home.percentile(values, p)
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
function home.percentiles(values)
    return {
        p50 = home.percentile(values, 50),
        p95 = home.percentile(values, 95),
        p99 = home.percentile(values, 99),
    }
end

--- Calculate average of values
-- @param values table Array of numeric values
-- @return number Average value
function home.average(values)
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

--- Create a result builder for home realm scenarios
-- @param scenario_name string Name of the scenario
-- @return table Result builder object
function home.result_builder(scenario_name)
    local builder = {
        scenario = scenario_name,
        level = home.get_level(),
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

return home

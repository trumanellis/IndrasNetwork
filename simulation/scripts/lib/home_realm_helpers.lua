-- Home Realm Simulation Helpers
--
-- Utility functions for home realm simulation scenarios.
-- Uses Rust SyncEngine bindings for deterministic home realm ID computation.
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

    -- Chat and messaging
    CHAT_MESSAGE = "chat_message",

    -- Proof and blessings
    PROOF_SUBMITTED = "proof_submitted",
    BLESSING_GIVEN = "blessing_given",
    BLESSING_RECEIVED = "blessing_received",

    -- Token of Gratitude lifecycle
    TOKEN_MINTED = "token_minted",
    GRATITUDE_PLEDGED = "gratitude_pledged",
    GRATITUDE_RELEASED = "gratitude_released",
    GRATITUDE_WITHDRAWN = "gratitude_withdrawn",
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
-- HOME REALM IDENTITY
-- ============================================================================

--- Compute a deterministic home realm ID from a member ID
-- home_realm_id = blake3("home-realm-v1:" + member_id)
-- @param member_id string Member identifier
-- @return string Home realm ID (hex string)
function home.compute_home_realm_id(member_id)
    -- Use indras SyncEngine binding if available (wraps Rust implementation)
    if indras and indras.sync_engine and indras.sync_engine.compute_home_realm_id then
        return indras.sync_engine.compute_home_realm_id(member_id)
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
-- CHAT HELPERS
-- ============================================================================

--- Random chat message templates
local CHAT_MESSAGES = {
    "Hey everyone!",
    "Great work on this!",
    "I'll take a look",
    "Making progress here",
    "Almost done",
    "Need some help with this",
    "Thanks for the feedback",
    "Let me know when you're ready",
    "Good point!",
    "I agree with that approach",
}

--- Generate a random chat message
-- @return string Random chat message
function home.random_chat_message()
    return CHAT_MESSAGES[math.random(#CHAT_MESSAGES)]
end

--- Send a chat message (simulation)
-- @param logger table Logger object
-- @param member string Member ID
-- @param content string Message content
-- @param tick number Current simulation tick
-- @return table Event data
function home.send_chat_message(logger, member, content, tick)
    local event_data = {
        member = member,
        content = content,
        message_type = "text",
        tick = tick,
    }
    logger.event(home.EVENTS.CHAT_MESSAGE, event_data)
    return event_data
end

-- ============================================================================
-- PROOF AND BLESSING HELPERS
-- ============================================================================

--- Submit proof for a quest claim
-- Posts a ProofSubmitted event to the realm chat
-- @param logger table Logger object
-- @param member string Member ID (claimant)
-- @param quest_id string Quest ID
-- @param quest_title string Quest title
-- @param artifact_id string Artifact ID serving as proof
-- @param artifact_name string Artifact name
-- @param tick number Current simulation tick
-- @return table Event data
function home.submit_proof(logger, member, quest_id, quest_title, artifact_id, artifact_name, tick)
    local event_data = {
        member = member,
        quest_id = quest_id,
        quest_title = quest_title,
        artifact_id = artifact_id,
        artifact_name = artifact_name or "proof.png",
        tick = tick,
    }
    logger.event(home.EVENTS.PROOF_SUBMITTED, event_data)
    return event_data
end

--- Give a blessing to a quest proof
-- Releases accumulated attention as validation
-- @param logger table Logger object
-- @param blesser string Member ID giving the blessing
-- @param claimant string Member ID who submitted the proof
-- @param quest_id string Quest ID
-- @param quest_title string Quest title (optional)
-- @param event_count number Number of attention events being blessed
-- @param attention_millis number Total attention time in milliseconds
-- @param tick number Current simulation tick
-- @return table Event data
function home.bless_proof(logger, blesser, claimant, quest_id, quest_title, event_count, attention_millis, tick)
    local event_data = {
        blesser = blesser,
        claimant = claimant,
        quest_id = quest_id,
        quest_title = quest_title or "",
        event_count = event_count or 1,
        attention_millis = attention_millis or 0,
        tick = tick,
    }
    logger.event(home.EVENTS.BLESSING_GIVEN, event_data)
    return event_data
end

--- Record a blessing received event
-- From the perspective of the proof submitter
-- @param logger table Logger object
-- @param claimant string Member ID who received the blessing
-- @param blesser string Member ID who gave the blessing
-- @param quest_id string Quest ID
-- @param quest_title string Quest title (optional)
-- @param attention_millis number Attention time in this blessing
-- @param total_blessed_millis number Total blessed attention for this proof
-- @param tick number Current simulation tick
-- @return table Event data
function home.record_blessing_received(logger, claimant, blesser, quest_id, quest_title, attention_millis, total_blessed_millis, tick)
    local event_data = {
        claimant = claimant,
        blesser = blesser,
        quest_id = quest_id,
        quest_title = quest_title or "",
        attention_millis = attention_millis or 0,
        total_blessed_millis = total_blessed_millis or 0,
        tick = tick,
    }
    logger.event(home.EVENTS.BLESSING_RECEIVED, event_data)
    return event_data
end

-- ============================================================================
-- TOKEN OF GRATITUDE HELPERS
-- ============================================================================

--- Token counter for generating unique IDs
local _token_counter = 0

--- Generate a unique token ID
-- @param quest_id string Quest ID
-- @param steward string Steward member ID
-- @param tick number Current tick
-- @return string Unique token ID
function home.make_token_id(quest_id, steward, tick)
    _token_counter = _token_counter + 1
    return string.format("tok_%s_%s_%d_%d",
        quest_id:sub(1, 8),
        steward:sub(1, 8),
        tick,
        _token_counter)
end

--- Emit a token_minted event
-- @param logger table Logger object
-- @param tick number Current tick
-- @param realm_id string Realm ID
-- @param token_id string Token ID
-- @param steward string Steward (claimant) member ID
-- @param value_millis number Token value in milliseconds
-- @param blesser string Blesser member ID
-- @param source_quest_id string Source quest ID
function home.emit_token_minted(logger, tick, realm_id, token_id, steward, value_millis, blesser, source_quest_id)
    logger.event(home.EVENTS.TOKEN_MINTED, {
        tick = tick,
        realm_id = realm_id,
        token_id = token_id,
        steward = steward,
        value_millis = value_millis,
        blesser = blesser,
        source_quest_id = source_quest_id,
    })
end

--- Emit a gratitude_pledged event
-- @param logger table Logger object
-- @param tick number Current tick
-- @param realm_id string Realm ID
-- @param token_id string Token ID
-- @param pledger string Pledger member ID
-- @param target_quest_id string Target quest ID
-- @param amount_millis number Pledged amount in milliseconds
function home.emit_gratitude_pledged(logger, tick, realm_id, token_id, pledger, target_quest_id, amount_millis)
    logger.event(home.EVENTS.GRATITUDE_PLEDGED, {
        tick = tick,
        realm_id = realm_id,
        token_id = token_id,
        pledger = pledger,
        target_quest_id = target_quest_id,
        amount_millis = amount_millis,
    })
end

--- Emit a gratitude_released event
-- @param logger table Logger object
-- @param tick number Current tick
-- @param realm_id string Realm ID
-- @param token_id string Token ID
-- @param from_steward string Current steward member ID
-- @param to_steward string New steward member ID
-- @param target_quest_id string Target quest ID
-- @param amount_millis number Released amount in milliseconds
function home.emit_gratitude_released(logger, tick, realm_id, token_id, from_steward, to_steward, target_quest_id, amount_millis)
    logger.event(home.EVENTS.GRATITUDE_RELEASED, {
        tick = tick,
        realm_id = realm_id,
        token_id = token_id,
        from_steward = from_steward,
        to_steward = to_steward,
        target_quest_id = target_quest_id,
        amount_millis = amount_millis,
    })
end

--- Emit a gratitude_withdrawn event
-- @param logger table Logger object
-- @param tick number Current tick
-- @param realm_id string Realm ID
-- @param token_id string Token ID
-- @param steward string Steward member ID
-- @param target_quest_id string Target quest ID
-- @param amount_millis number Withdrawn amount in milliseconds
function home.emit_gratitude_withdrawn(logger, tick, realm_id, token_id, steward, target_quest_id, amount_millis)
    logger.event(home.EVENTS.GRATITUDE_WITHDRAWN, {
        tick = tick,
        realm_id = realm_id,
        token_id = token_id,
        steward = steward,
        target_quest_id = target_quest_id,
        amount_millis = amount_millis,
    })
end

-- ============================================================================
-- BLESSING TRACKER
-- ============================================================================

--- Create a blessing tracker for simulation verification
-- Tracks attention and blessings for quests
-- @return table BlessingTracker object
function home.BlessingTracker_new()
    local tracker = {
        -- attention[quest_id][member] = { events = {}, total_millis = 0 }
        attention = {},
        -- blessings[quest_id][claimant] = { blessings = {}, total_millis = 0 }
        blessings = {},
        -- blessed_events[quest_id][member] = set of event indices already blessed
        blessed_events = {},
    }

    --- Record attention focused on a quest
    -- @param quest_id string Quest ID
    -- @param member string Member ID
    -- @param event_index number Event index in attention document
    -- @param duration_millis number Duration of attention in milliseconds
    function tracker:record_attention(quest_id, member, event_index, duration_millis)
        if not self.attention[quest_id] then
            self.attention[quest_id] = {}
        end
        if not self.attention[quest_id][member] then
            self.attention[quest_id][member] = { events = {}, total_millis = 0 }
        end

        table.insert(self.attention[quest_id][member].events, {
            index = event_index,
            duration_millis = duration_millis,
        })
        self.attention[quest_id][member].total_millis =
            self.attention[quest_id][member].total_millis + duration_millis
    end

    --- Get unblessed attention event indices for a member on a quest
    -- @param quest_id string Quest ID
    -- @param member string Member ID
    -- @return table Array of { index, duration_millis } for unblessed events
    function tracker:get_unblessed_attention(quest_id, member)
        local result = {}

        -- Check if member has attention on this quest
        if not self.attention[quest_id] or not self.attention[quest_id][member] then
            return result
        end

        -- Get the set of already blessed events
        local blessed = {}
        if self.blessed_events[quest_id] and self.blessed_events[quest_id][member] then
            blessed = self.blessed_events[quest_id][member]
        end

        -- Filter to unblessed events
        for _, event in ipairs(self.attention[quest_id][member].events) do
            if not blessed[event.index] then
                table.insert(result, event)
            end
        end

        return result
    end

    --- Record a blessing
    -- @param quest_id string Quest ID
    -- @param claimant string Member who submitted proof
    -- @param blesser string Member giving blessing
    -- @param event_indices table Array of attention event indices being blessed
    -- @param attention_millis number Total attention time being blessed
    function tracker:record_blessing(quest_id, claimant, blesser, event_indices, attention_millis)
        -- Track the blessing
        if not self.blessings[quest_id] then
            self.blessings[quest_id] = {}
        end
        if not self.blessings[quest_id][claimant] then
            self.blessings[quest_id][claimant] = { blessings = {}, total_millis = 0 }
        end

        table.insert(self.blessings[quest_id][claimant].blessings, {
            blesser = blesser,
            event_indices = event_indices,
            attention_millis = attention_millis,
        })
        self.blessings[quest_id][claimant].total_millis =
            self.blessings[quest_id][claimant].total_millis + attention_millis

        -- Mark events as blessed
        if not self.blessed_events[quest_id] then
            self.blessed_events[quest_id] = {}
        end
        if not self.blessed_events[quest_id][blesser] then
            self.blessed_events[quest_id][blesser] = {}
        end

        for _, idx in ipairs(event_indices) do
            self.blessed_events[quest_id][blesser][idx] = true
        end
    end

    --- Get total blessed attention for a claim
    -- @param quest_id string Quest ID
    -- @param claimant string Member who submitted proof
    -- @return number Total blessed attention in milliseconds
    function tracker:get_total_blessed(quest_id, claimant)
        if self.blessings[quest_id] and self.blessings[quest_id][claimant] then
            return self.blessings[quest_id][claimant].total_millis
        end
        return 0
    end

    --- Get all blessers for a claim
    -- @param quest_id string Quest ID
    -- @param claimant string Member who submitted proof
    -- @return table Array of blesser member IDs
    function tracker:get_blessers(quest_id, claimant)
        local result = {}
        local seen = {}

        if self.blessings[quest_id] and self.blessings[quest_id][claimant] then
            for _, blessing in ipairs(self.blessings[quest_id][claimant].blessings) do
                if not seen[blessing.blesser] then
                    seen[blessing.blesser] = true
                    table.insert(result, blessing.blesser)
                end
            end
        end

        return result
    end

    return tracker
end

home.BlessingTracker = { new = home.BlessingTracker_new }

-- ============================================================================
-- BLESSING LATENCY MODELS
-- ============================================================================

--- Simulate proof submission latency (500-1500 microseconds)
-- Includes artifact reference and chat message posting
-- @return number Latency in microseconds
function home.proof_submit_latency()
    return 500 + math.random(1000)
end

--- Simulate blessing latency (200-600 microseconds)
-- Includes validation and chat message posting
-- @return number Latency in microseconds
function home.bless_latency()
    return 200 + math.random(400)
end

-- ============================================================================
-- DURATION FORMATTING
-- ============================================================================

--- Format milliseconds as human-readable duration
-- @param millis number Duration in milliseconds
-- @return string Human-readable duration (e.g., "2h 30m")
function home.format_duration(millis)
    local seconds = math.floor(millis / 1000)
    local minutes = math.floor(seconds / 60)
    local hours = math.floor(minutes / 60)

    if hours > 0 then
        local remaining_mins = minutes % 60
        if remaining_mins > 0 then
            return string.format("%dh %dm", hours, remaining_mins)
        else
            return string.format("%dh", hours)
        end
    elseif minutes > 0 then
        return string.format("%dm", minutes)
    else
        return string.format("%ds", seconds)
    end
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

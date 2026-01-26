-- Discovery Helpers Library
--
-- Utility functions for peer discovery simulation scenarios.
-- Handles mutual peer discovery, realm creation, rate limiting, and PQ key exchange.
--
-- Key Insight: Realms ARE peer sets. Every group of peers automatically creates
-- the potential for collaboration. Discovery is about peers finding each other,
-- which implicitly creates/expands the set of possible realms.

local discovery = {}

-- ============================================================================
-- STRESS LEVELS: Discovery-specific configurations
-- ============================================================================

discovery.LEVELS = {
    quick = {
        name = "quick",
        peers = 5,
        realms = 2,
        ticks = 200,
        rate_limit_window = 30,
        broadcast_interval = 5,
        catchup_delay = 20,
    },
    medium = {
        name = "medium",
        peers = 15,
        realms = 5,
        ticks = 500,
        rate_limit_window = 30,
        broadcast_interval = 3,
        catchup_delay = 15,
    },
    full = {
        name = "full",
        peers = 26,
        realms = 10,
        ticks = 1000,
        rate_limit_window = 30,
        broadcast_interval = 2,
        catchup_delay = 10,
    }
}

-- PQ key sizes from ML-KEM-768 and ML-DSA-65 specifications
discovery.PQ_KEYS = {
    kem_encap_key = 1184,      -- ML-KEM-768 encapsulation key
    kem_decap_key = 2400,      -- ML-KEM-768 decapsulation key
    dsa_verifying_key = 1952,  -- ML-DSA-65 verifying key
    dsa_signing_key = 4032,    -- ML-DSA-65 signing key
    dsa_signature = 2420,      -- ML-DSA-65 signature
}

-- Message types for discovery protocol
discovery.MSG_TYPES = {
    INTERFACE_JOIN = "interface_join",
    PEER_INTRODUCTION = "peer_introduction",
    INTRODUCTION_REQUEST = "introduction_request",
    INTRODUCTION_RESPONSE = "introduction_response",
    PRESENCE_BROADCAST = "presence_broadcast",
}

-- Event types for JSONL logging
discovery.EVENTS = {
    -- Peer lifecycle
    PEER_ONLINE = "peer_online",
    PEER_OFFLINE = "peer_offline",

    -- Discovery broadcasts
    PRESENCE_BROADCAST = "presence_broadcast",
    PRESENCE_RECEIVED = "presence_received",

    -- Peer introductions
    PEER_INTRODUCTION_SENT = "peer_introduction_sent",
    PEER_INTRODUCTION_RECEIVED = "peer_introduction_received",

    -- Catch-up mechanism
    INTRODUCTION_REQUEST_SENT = "introduction_request_sent",
    INTRODUCTION_RESPONSE_SENT = "introduction_response_sent",
    INTRODUCTION_RESPONSE_RATE_LIMITED = "introduction_response_rate_limited",

    -- Discovery outcomes
    PEER_DISCOVERED = "peer_discovered",
    PQ_KEYS_EXCHANGED = "pq_keys_exchanged",
    REALM_AVAILABLE = "realm_available",

    -- Convergence
    CONVERGENCE_ACHIEVED = "convergence_achieved",
}

-- ============================================================================
-- CONFIGURATION HELPERS
-- ============================================================================

--- Get the current stress level from environment
-- @return string The stress level (quick, medium, or full)
function discovery.get_level()
    return os.getenv("STRESS_LEVEL") or "medium"
end

--- Get the configuration for current stress level
-- @return table The level configuration
function discovery.get_config()
    local level = discovery.get_level()
    return discovery.LEVELS[level] or discovery.LEVELS.medium
end

-- ============================================================================
-- CONTEXT AND LOGGING
-- ============================================================================

--- Create a correlation context for a discovery scenario
-- @param scenario_name string Name of the scenario
-- @return CorrelationContext
function discovery.new_context(scenario_name)
    local ctx = indras.correlation.new_root()
    ctx = ctx:with_tag("scenario", scenario_name)
    ctx = ctx:with_tag("subsystem", "discovery")
    ctx = ctx:with_tag("stress_level", discovery.get_level())
    return ctx
end

--- Create a context logger with automatic trace_id
-- @param ctx CorrelationContext The correlation context
-- @return table Logger object
function discovery.create_logger(ctx)
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

    --- Log a discovery event with standard fields
    function logger.event(event_type, fields)
        fields = fields or {}
        fields.trace_id = ctx.trace_id
        fields.event_type = event_type
        indras.log.info(event_type, fields)
    end

    return logger
end

-- ============================================================================
-- PEER AND REALM IDENTITY
-- ============================================================================

--- Generate a simple hash for a peer set (realm identity)
-- In practice this would be hash(sorted_concatenated_public_keys)
-- @param peer_set table Array of peer identifiers
-- @return string Realm identifier
function discovery.realm_id(peer_set)
    if #peer_set < 2 then
        return nil  -- Realms require at least 2 peers
    end

    -- Sort peer IDs for deterministic ordering
    local sorted = {}
    for _, peer in ipairs(peer_set) do
        table.insert(sorted, tostring(peer))
    end
    table.sort(sorted)

    -- Simple hash: concatenate sorted IDs
    return "realm:" .. table.concat(sorted, "+")
end

--- Get all possible realm IDs for a given peer's known peers
-- Returns all subsets of size >= 2 that include the given peer
-- @param peer string The peer to include in all realms
-- @param known_peers table Array of peers this peer knows
-- @return table Array of realm IDs
function discovery.realms_for_peer(peer, known_peers)
    local realms = {}
    local all_peers = { peer }
    for _, p in ipairs(known_peers) do
        table.insert(all_peers, p)
    end

    -- Generate all subsets of size >= 2 containing the peer
    local n = #known_peers
    -- Iterate through all possible subsets of known_peers
    for mask = 1, (2^n) - 1 do
        local subset = { peer }
        for i = 1, n do
            if mask % 2 == 1 then
                table.insert(subset, known_peers[i])
            end
            mask = math.floor(mask / 2)
        end
        if #subset >= 2 then
            local realm = discovery.realm_id(subset)
            if realm then
                table.insert(realms, realm)
            end
        end
    end

    return realms
end

--- Count the number of possible realms for N peers
-- This is 2^N - N - 1 (all subsets of size >= 2)
-- @param n number Number of peers
-- @return number Number of possible realms
function discovery.count_possible_realms(n)
    if n < 2 then return 0 end
    return (2^n) - n - 1
end

-- ============================================================================
-- DISCOVERY STATE TRACKER
-- ============================================================================

--- Create a discovery state tracker
-- Tracks who knows whom and their PQ keys
-- @param peers table Array of peer identifiers
-- @return table Tracker object
function discovery.create_tracker(peers)
    local tracker = {
        peers = peers,
        -- discovery_matrix[from][to] = { discovered = bool, has_pq_keys = bool, tick = number }
        matrix = {},
        -- Total discovery events
        discoveries = 0,
        -- PQ key exchanges
        key_exchanges = 0,
    }

    -- Initialize matrix
    for _, from in ipairs(peers) do
        tracker.matrix[tostring(from)] = {}
        for _, to in ipairs(peers) do
            if from ~= to then
                tracker.matrix[tostring(from)][tostring(to)] = {
                    discovered = false,
                    has_pq_keys = false,
                    tick = nil,
                    kem_key_size = nil,
                    dsa_key_size = nil,
                }
            end
        end
    end

    --- Record a discovery event
    -- @param from string The discovering peer
    -- @param to string The discovered peer
    -- @param tick number The tick when discovery occurred
    -- @return boolean True if this is a new discovery
    function tracker:record_discovery(from, to, tick)
        local from_id = tostring(from)
        local to_id = tostring(to)

        if not self.matrix[from_id] or not self.matrix[from_id][to_id] then
            return false
        end

        local entry = self.matrix[from_id][to_id]
        if entry.discovered then
            return false  -- Already discovered
        end

        entry.discovered = true
        entry.tick = tick
        self.discoveries = self.discoveries + 1
        return true
    end

    --- Record PQ key exchange
    -- @param from string The peer receiving keys
    -- @param to string The peer whose keys were received
    -- @param kem_size number Size of KEM encapsulation key
    -- @param dsa_size number Size of DSA verifying key
    function tracker:record_pq_keys(from, to, kem_size, dsa_size)
        local from_id = tostring(from)
        local to_id = tostring(to)

        if not self.matrix[from_id] or not self.matrix[from_id][to_id] then
            return false
        end

        local entry = self.matrix[from_id][to_id]
        if entry.has_pq_keys then
            return false  -- Already have keys
        end

        entry.has_pq_keys = true
        entry.kem_key_size = kem_size
        entry.dsa_key_size = dsa_size
        self.key_exchanges = self.key_exchanges + 1
        return true
    end

    --- Check if discovery is complete (all peers know each other)
    -- @return boolean True if all pairs have discovered each other
    function tracker:is_complete()
        for from_id, targets in pairs(self.matrix) do
            for to_id, entry in pairs(targets) do
                if not entry.discovered then
                    return false
                end
            end
        end
        return true
    end

    --- Check if PQ key exchange is complete
    -- @return boolean True if all peers have each other's keys
    function tracker:is_pq_complete()
        for from_id, targets in pairs(self.matrix) do
            for to_id, entry in pairs(targets) do
                if not entry.has_pq_keys then
                    return false
                end
            end
        end
        return true
    end

    --- Check if a specific peer knows another
    -- @param from string The peer to check
    -- @param to string The peer they might know
    -- @return boolean True if from knows to
    function tracker:knows(from, to)
        local from_id = tostring(from)
        local to_id = tostring(to)
        if not self.matrix[from_id] or not self.matrix[from_id][to_id] then
            return false
        end
        return self.matrix[from_id][to_id].discovered
    end

    --- Check if a specific peer has another's PQ keys
    -- @param from string The peer to check
    -- @param to string The peer whose keys they might have
    -- @return boolean True if from has to's keys
    function tracker:has_keys(from, to)
        local from_id = tostring(from)
        local to_id = tostring(to)
        if not self.matrix[from_id] or not self.matrix[from_id][to_id] then
            return false
        end
        return self.matrix[from_id][to_id].has_pq_keys
    end

    --- Get peers known by a specific peer
    -- @param peer string The peer to check
    -- @return table Array of known peer IDs
    function tracker:known_peers(peer)
        local peer_id = tostring(peer)
        local known = {}
        if self.matrix[peer_id] then
            for to_id, entry in pairs(self.matrix[peer_id]) do
                if entry.discovered then
                    table.insert(known, to_id)
                end
            end
        end
        return known
    end

    --- Calculate discovery completeness percentage
    -- @return number Percentage (0-1) of peer pairs that have discovered each other
    function tracker:completeness()
        local total = 0
        local discovered = 0
        for _, targets in pairs(self.matrix) do
            for _, entry in pairs(targets) do
                total = total + 1
                if entry.discovered then
                    discovered = discovered + 1
                end
            end
        end
        if total == 0 then return 1.0 end
        return discovered / total
    end

    --- Calculate PQ key completeness percentage
    -- @return number Percentage (0-1) of pairs with complete key exchange
    function tracker:pq_completeness()
        local total = 0
        local with_keys = 0
        for _, targets in pairs(self.matrix) do
            for _, entry in pairs(targets) do
                total = total + 1
                if entry.has_pq_keys then
                    with_keys = with_keys + 1
                end
            end
        end
        if total == 0 then return 1.0 end
        return with_keys / total
    end

    --- Get discovery latencies for percentile calculation
    -- @return table Array of latency values (tick numbers)
    function tracker:get_latencies()
        local latencies = {}
        for _, targets in pairs(self.matrix) do
            for _, entry in pairs(targets) do
                if entry.tick then
                    table.insert(latencies, entry.tick)
                end
            end
        end
        return latencies
    end

    --- Get statistics summary
    -- @return table Statistics table
    function tracker:stats()
        return {
            total_peers = #self.peers,
            discoveries = self.discoveries,
            key_exchanges = self.key_exchanges,
            completeness = self:completeness(),
            pq_completeness = self:pq_completeness(),
            is_complete = self:is_complete(),
            is_pq_complete = self:is_pq_complete(),
        }
    end

    return tracker
end

-- ============================================================================
-- RATE LIMITER
-- ============================================================================

--- Create a rate limiter for introduction responses
-- Enforces 1 response per peer per window
-- @param window_ticks number Number of ticks in the rate limit window
-- @return table Rate limiter object
function discovery.create_rate_limiter(window_ticks)
    local limiter = {
        window = window_ticks,
        -- responses[responder][requester] = last_response_tick
        responses = {},
        -- Statistics
        allowed = 0,
        limited = 0,
    }

    --- Check if a response can be sent
    -- @param responder string The peer sending the response
    -- @param requester string The peer requesting introduction
    -- @param current_tick number The current simulation tick
    -- @return boolean True if response is allowed
    function limiter:can_respond(responder, requester, current_tick)
        local responder_id = tostring(responder)
        local requester_id = tostring(requester)

        if not self.responses[responder_id] then
            return true
        end

        local last_tick = self.responses[responder_id][requester_id]
        if not last_tick then
            return true
        end

        return (current_tick - last_tick) >= self.window
    end

    --- Record a response (updates rate limit tracking)
    -- @param responder string The peer sending the response
    -- @param requester string The peer requesting introduction
    -- @param current_tick number The current simulation tick
    -- @return boolean True if response was allowed, false if rate limited
    function limiter:record_response(responder, requester, current_tick)
        local responder_id = tostring(responder)
        local requester_id = tostring(requester)

        if not self:can_respond(responder, requester, current_tick) then
            self.limited = self.limited + 1
            return false
        end

        if not self.responses[responder_id] then
            self.responses[responder_id] = {}
        end
        self.responses[responder_id][requester_id] = current_tick
        self.allowed = self.allowed + 1
        return true
    end

    --- Get statistics
    -- @return table Statistics table
    function limiter:stats()
        return {
            window = self.window,
            allowed = self.allowed,
            limited = self.limited,
            total = self.allowed + self.limited,
        }
    end

    return limiter
end

-- ============================================================================
-- PQ KEY GENERATION
-- ============================================================================

--- Generate mock PQ keys for a peer
-- @param peer string The peer identifier
-- @return table Table with kem_encap_key, dsa_verifying_key, and sizes
function discovery.generate_pq_keys(peer)
    return {
        peer_id = tostring(peer),
        kem_encap_key = string.format("kem_%s_%d", tostring(peer), discovery.PQ_KEYS.kem_encap_key),
        kem_encap_key_size = discovery.PQ_KEYS.kem_encap_key,
        dsa_verifying_key = string.format("dsa_%s_%d", tostring(peer), discovery.PQ_KEYS.dsa_verifying_key),
        dsa_verifying_key_size = discovery.PQ_KEYS.dsa_verifying_key,
    }
end

--- Validate PQ key sizes
-- @param kem_size number The KEM encapsulation key size
-- @param dsa_size number The DSA verifying key size
-- @return boolean True if sizes are valid
function discovery.validate_pq_key_sizes(kem_size, dsa_size)
    return kem_size == discovery.PQ_KEYS.kem_encap_key and
           dsa_size == discovery.PQ_KEYS.dsa_verifying_key
end

-- ============================================================================
-- PEER STATE MANAGEMENT
-- ============================================================================

--- Create a peer state manager
-- Tracks online/offline state and PQ keys for each peer
-- @param peers table Array of peer identifiers
-- @return table Peer state manager
function discovery.create_peer_state(peers)
    local state = {
        peers = {},
    }

    -- Initialize peer state
    for _, peer in ipairs(peers) do
        local peer_id = tostring(peer)
        state.peers[peer_id] = {
            id = peer_id,
            online = false,
            online_since = nil,
            pq_keys = nil,
            known_peers = {},  -- peer_id -> pq_keys
        }
    end

    --- Bring a peer online
    -- @param peer string The peer identifier
    -- @param tick number The current tick
    function state:bring_online(peer, tick)
        local peer_id = tostring(peer)
        local p = self.peers[peer_id]
        if p and not p.online then
            p.online = true
            p.online_since = tick
            -- Generate PQ keys on first online
            if not p.pq_keys then
                p.pq_keys = discovery.generate_pq_keys(peer)
            end
            return true
        end
        return false
    end

    --- Take a peer offline
    -- @param peer string The peer identifier
    function state:bring_offline(peer)
        local peer_id = tostring(peer)
        local p = self.peers[peer_id]
        if p and p.online then
            p.online = false
            p.online_since = nil
            return true
        end
        return false
    end

    --- Check if a peer is online
    -- @param peer string The peer identifier
    -- @return boolean True if online
    function state:is_online(peer)
        local peer_id = tostring(peer)
        return self.peers[peer_id] and self.peers[peer_id].online
    end

    --- Get a peer's PQ keys
    -- @param peer string The peer identifier
    -- @return table PQ keys or nil
    function state:get_pq_keys(peer)
        local peer_id = tostring(peer)
        return self.peers[peer_id] and self.peers[peer_id].pq_keys
    end

    --- Record that a peer learned another peer's info
    -- @param learner string The peer learning
    -- @param learned string The peer being learned about
    -- @param pq_keys table The learned peer's PQ keys
    function state:learn_peer(learner, learned, pq_keys)
        local learner_id = tostring(learner)
        local learned_id = tostring(learned)
        local p = self.peers[learner_id]
        if p then
            p.known_peers[learned_id] = pq_keys
            return true
        end
        return false
    end

    --- Get online peers
    -- @return table Array of online peer IDs
    function state:online_peers()
        local online = {}
        for peer_id, p in pairs(self.peers) do
            if p.online then
                table.insert(online, peer_id)
            end
        end
        return online
    end

    --- Get offline peers
    -- @return table Array of offline peer IDs
    function state:offline_peers()
        local offline = {}
        for peer_id, p in pairs(self.peers) do
            if not p.online then
                table.insert(offline, peer_id)
            end
        end
        return offline
    end

    return state
end

-- ============================================================================
-- STATISTICS HELPERS
-- ============================================================================

--- Calculate percentile from array of values
-- @param values table Array of numeric values
-- @param p number Percentile (0-100)
-- @return number The percentile value
function discovery.percentile(values, p)
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
function discovery.percentiles(values)
    return {
        p50 = discovery.percentile(values, 50),
        p95 = discovery.percentile(values, 95),
        p99 = discovery.percentile(values, 99),
    }
end

--- Calculate average of values
-- @param values table Array of numeric values
-- @return number Average value
function discovery.average(values)
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
function discovery.assert_thresholds(metrics, thresholds)
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

--- Create a result builder for discovery scenarios
-- @param scenario_name string Name of the scenario
-- @return table Result builder object
function discovery.result_builder(scenario_name)
    local builder = {
        scenario = scenario_name,
        level = discovery.get_level(),
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

return discovery

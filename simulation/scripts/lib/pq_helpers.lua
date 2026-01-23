-- PQ Helpers Library
--
-- Utility functions for PQ crypto stress test scenarios.
-- Provides common operations for testing post-quantum cryptography.

local pq = {}

-- Default latency parameters (microseconds) based on actual PQ crypto benchmarks
pq.LATENCY = {
    -- ML-DSA-65 signing: ~150-300us typical
    SIGN_BASE = 200,
    SIGN_VARIANCE = 100,
    -- ML-DSA-65 verification: ~100-200us typical
    VERIFY_BASE = 150,
    VERIFY_VARIANCE = 50,
    -- ML-KEM-768 encapsulation: ~50-100us typical
    KEM_ENCAP_BASE = 75,
    KEM_ENCAP_VARIANCE = 25,
    -- ML-KEM-768 decapsulation: ~50-100us typical
    KEM_DECAP_BASE = 75,
    KEM_DECAP_VARIANCE = 25,
}

-- Key sizes in bytes (for reference)
pq.KEY_SIZES = {
    -- ML-DSA-65
    DSA_VERIFYING_KEY = 1952,
    DSA_SIGNING_KEY = 4032,
    DSA_SIGNATURE = 2420,
    -- ML-KEM-768
    KEM_ENCAPSULATION_KEY = 1184,
    KEM_DECAPSULATION_KEY = 2400,
    KEM_CIPHERTEXT = 1088,
    KEM_SHARED_SECRET = 32,
}

--- Generate random latency with variance
-- @param base number Base latency in microseconds
-- @param variance number Maximum deviation from base
-- @return number Random latency value
function pq.random_latency(base, variance)
    return math.floor(base + (math.random() - 0.5) * 2 * variance)
end

--- Generate signature creation latency
-- @return number Latency in microseconds
function pq.sign_latency()
    return pq.random_latency(pq.LATENCY.SIGN_BASE, pq.LATENCY.SIGN_VARIANCE)
end

--- Generate signature verification latency
-- @return number Latency in microseconds
function pq.verify_latency()
    return pq.random_latency(pq.LATENCY.VERIFY_BASE, pq.LATENCY.VERIFY_VARIANCE)
end

--- Generate KEM encapsulation latency
-- @return number Latency in microseconds
function pq.encap_latency()
    return pq.random_latency(pq.LATENCY.KEM_ENCAP_BASE, pq.LATENCY.KEM_ENCAP_VARIANCE)
end

--- Generate KEM decapsulation latency
-- @return number Latency in microseconds
function pq.decap_latency()
    return pq.random_latency(pq.LATENCY.KEM_DECAP_BASE, pq.LATENCY.KEM_DECAP_VARIANCE)
end

--- Create an interface with N members using PQ invites
-- @param sim Simulation The simulation instance
-- @param creator PeerId The interface creator
-- @param members table Array of PeerIds to add
-- @param interface_id string The interface identifier
-- @param ctx table Correlation context for logging
-- @return table Stats about the join process
function pq.create_populated_interface(sim, creator, members, interface_id, ctx)
    local stats = {
        created = 0,
        accepted = 0,
        failed = 0,
        total_encap_latency = 0,
        total_decap_latency = 0,
    }

    for _, member in ipairs(members) do
        if member ~= creator then
            -- Record invite creation
            sim:record_invite_created(creator, member, interface_id)
            stats.created = stats.created + 1

            -- KEM encapsulation by creator
            local encap_lat = pq.encap_latency()
            sim:record_kem_encapsulation(creator, member, encap_lat)
            stats.total_encap_latency = stats.total_encap_latency + encap_lat

            -- KEM decapsulation by new member
            local decap_lat = pq.decap_latency()
            local success = math.random() > 0.001  -- 0.1% failure rate
            sim:record_kem_decapsulation(member, creator, decap_lat, success)
            stats.total_decap_latency = stats.total_decap_latency + decap_lat

            if success then
                sim:record_invite_accepted(member, interface_id)
                stats.accepted = stats.accepted + 1
            else
                sim:record_invite_failed(member, interface_id, "KEM decapsulation failed")
                stats.failed = stats.failed + 1
            end
        end
    end

    return stats
end

--- Generate message load with PQ signatures
-- @param sim Simulation The simulation instance
-- @param peers table Array of online peers
-- @param msgs_per_tick number Number of messages to generate
-- @param failure_rate number Optional signature verification failure rate (default 0.001)
-- @return table Stats about the operation
function pq.generate_message_load(sim, peers, msgs_per_tick, failure_rate)
    failure_rate = failure_rate or 0.001
    local stats = {
        signatures = 0,
        verifications = 0,
        failures = 0,
    }

    for _ = 1, msgs_per_tick do
        if #peers >= 2 then
            -- Pick random sender and receiver
            local sender_idx = math.random(#peers)
            local recv_idx = math.random(#peers)
            while recv_idx == sender_idx do
                recv_idx = math.random(#peers)
            end

            local sender = peers[sender_idx]
            local receiver = peers[recv_idx]

            -- Sign message
            local sign_lat = pq.sign_latency()
            sim:record_pq_signature(sender, sign_lat, 256)
            stats.signatures = stats.signatures + 1

            -- Verify at receiver
            local verify_lat = pq.verify_latency()
            local success = math.random() > failure_rate
            sim:record_pq_verification(receiver, sender, verify_lat, success)

            if success then
                stats.verifications = stats.verifications + 1
            else
                stats.failures = stats.failures + 1
            end

            -- Also send network message
            sim:send_message(sender, receiver, "pq_msg")
        end
    end

    return stats
end

--- Measure latency of an operation by wrapping it
-- @param operation_fn function The function to measure
-- @return number duration_us, any result
function pq.measure_latency(operation_fn)
    local start = os.clock()
    local result = operation_fn()
    local duration = os.clock() - start
    return math.floor(duration * 1000000), result  -- Convert to microseconds
end

--- Calculate percentiles from an array of values
-- @param values table Array of numeric values
-- @param p number Percentile to calculate (0-100)
-- @return number The percentile value
function pq.percentile(values, p)
    if #values == 0 then return 0 end

    -- Copy and sort
    local sorted = {}
    for _, v in ipairs(values) do
        table.insert(sorted, v)
    end
    table.sort(sorted)

    local idx = math.ceil(#sorted * p / 100)
    return sorted[math.max(1, idx)]
end

--- Calculate multiple percentiles at once
-- @param values table Array of numeric values
-- @return table Table with p50, p95, p99 values
function pq.percentiles(values)
    return {
        p50 = pq.percentile(values, 50),
        p95 = pq.percentile(values, 95),
        p99 = pq.percentile(values, 99),
    }
end

--- Calculate average of values
-- @param values table Array of numeric values
-- @return number Average value
function pq.average(values)
    if #values == 0 then return 0 end
    local sum = 0
    for _, v in ipairs(values) do
        sum = sum + v
    end
    return sum / #values
end

--- Assert PQ metrics are within thresholds
-- @param stats SimStats The simulation stats
-- @param thresholds table Threshold configuration
-- @return boolean passed, table failures
function pq.assert_metrics(stats, thresholds)
    local failures = {}

    -- Signature latency
    if thresholds.signature_latency_p99_us then
        local actual = stats:avg_signature_latency_us()  -- Approximation
        if actual > thresholds.signature_latency_p99_us then
            table.insert(failures, {
                metric = "signature_latency",
                expected = thresholds.signature_latency_p99_us,
                actual = actual
            })
        end
    end

    -- Signature failure rate
    if thresholds.signature_failure_rate_max then
        local actual = stats:signature_failure_rate()
        if actual > thresholds.signature_failure_rate_max then
            table.insert(failures, {
                metric = "signature_failure_rate",
                expected = thresholds.signature_failure_rate_max,
                actual = actual
            })
        end
    end

    -- KEM latency
    if thresholds.kem_latency_p99_us then
        local actual = stats:avg_kem_encap_latency_us()
        if actual > thresholds.kem_latency_p99_us then
            table.insert(failures, {
                metric = "kem_latency",
                expected = thresholds.kem_latency_p99_us,
                actual = actual
            })
        end
    end

    -- KEM failure rate
    if thresholds.kem_failure_rate_max then
        local actual = stats:kem_failure_rate()
        if actual > thresholds.kem_failure_rate_max then
            table.insert(failures, {
                metric = "kem_failure_rate",
                expected = thresholds.kem_failure_rate_max,
                actual = actual
            })
        end
    end

    -- Invite success rate
    if thresholds.invite_success_rate_min then
        local actual = stats:invite_success_rate()
        if actual < thresholds.invite_success_rate_min then
            table.insert(failures, {
                metric = "invite_success_rate",
                expected = thresholds.invite_success_rate_min,
                actual = actual
            })
        end
    end

    return #failures == 0, failures
end

--- Format latency for logging (us -> human readable)
-- @param latency_us number Latency in microseconds
-- @return string Formatted string
function pq.format_latency(latency_us)
    if latency_us < 1000 then
        return string.format("%dus", latency_us)
    elseif latency_us < 1000000 then
        return string.format("%.2fms", latency_us / 1000)
    else
        return string.format("%.2fs", latency_us / 1000000)
    end
end

--- Create a correlation context with PQ scenario tag
-- @param scenario_name string Name of the scenario
-- @return CorrelationContext
function pq.new_context(scenario_name)
    local ctx = indras.correlation.new_root()
    ctx = ctx:with_tag("scenario", scenario_name)
    ctx = ctx:with_tag("crypto", "post-quantum")
    return ctx
end

return pq

-- Rate Limiting Verification Scenario
--
-- Tests that introduction responses are properly rate-limited
-- (1 response per peer per 30-second window).
--
-- Flow:
-- 1. Alice, Bob, Carol form a group
-- 2. Dave joins and sends multiple IntroductionRequests rapidly
-- 3. Only first request gets response, subsequent ones rate-limited
-- 4. After 30s window, new request succeeds
--
-- This scenario validates the rate limiting protection mechanism.

local discovery = require("lib.discovery_helpers")
local thresholds = require("config.discovery_thresholds")

-- ============================================================================
-- SETUP
-- ============================================================================

local ctx = discovery.new_context("discovery_rate_limit")
local logger = discovery.create_logger(ctx)
local config = discovery.get_config()

-- Configuration for this scenario
local SCENARIO_CONFIG = {
    quick = {
        group_size = 3,
        rate_limit_window = 30,
        rapid_requests = 5,
        max_ticks = 150,
    },
    medium = {
        group_size = 5,
        rate_limit_window = 30,
        rapid_requests = 10,
        max_ticks = 200,
    },
    full = {
        group_size = 8,
        rate_limit_window = 30,
        rapid_requests = 20,
        max_ticks = 300,
    },
}
local cfg = SCENARIO_CONFIG[discovery.get_level()] or SCENARIO_CONFIG.medium

logger.info("Starting rate limit verification scenario", {
    level = discovery.get_level(),
    group_size = cfg.group_size,
    rate_limit_window = cfg.rate_limit_window,
    rapid_requests = cfg.rapid_requests,
    max_ticks = cfg.max_ticks,
})

-- Create mesh with group + 1 requester
local total_peers = cfg.group_size + 1
local mesh = indras.MeshBuilder.new(total_peers):full_mesh()
local sim_config = indras.SimConfig.new({
    wake_probability = 0,
    sleep_probability = 0,
    initial_online_probability = 0,
    max_ticks = cfg.max_ticks,
})
local sim = indras.Simulation.new(mesh, sim_config)
sim:initialize()

local all_peers = mesh:peers()
local group_peers = {}
local requester = nil

for i, peer in ipairs(all_peers) do
    if i <= cfg.group_size then
        table.insert(group_peers, peer)
    else
        requester = peer
    end
end

local peer_state = discovery.create_peer_state(all_peers)
local rate_limiter = discovery.create_rate_limiter(cfg.rate_limit_window)
local result = discovery.result_builder("discovery_rate_limit")

-- Track metrics
local first_request_responses = 0
local rate_limited_count = 0
local post_window_responses = 0
local rate_limit_violations = 0

-- ============================================================================
-- PHASE 1: GROUP FORMATION (ticks 1-30)
-- Establish the initial peer group
-- ============================================================================

logger.info("Phase 1: Group formation", {
    phase = 1,
    group_size = cfg.group_size,
})

-- Bring group peers online
for tick = 1, 5 do
    sim:step()
end

for _, peer in ipairs(group_peers) do
    peer_state:bring_online(peer, sim.tick)
    sim:force_online(peer)
    logger.event(discovery.EVENTS.PEER_ONLINE, {
        tick = sim.tick,
        peer = tostring(peer),
        role = "group",
    })
    sim:step()
end

-- Group peers discover each other (quick discovery for this test)
for _, from in ipairs(group_peers) do
    for _, to in ipairs(group_peers) do
        if from ~= to then
            local to_keys = peer_state:get_pq_keys(to)
            peer_state:learn_peer(from, to, to_keys)
        end
    end
end

logger.info("Phase 1 complete: Group formed", {
    phase = 1,
    tick = sim.tick,
    group_size = #group_peers,
})

-- ============================================================================
-- PHASE 2: RAPID REQUESTS (ticks 31-60)
-- Requester sends multiple requests rapidly, only first should get response
-- ============================================================================

logger.info("Phase 2: Rapid request testing", {
    phase = 2,
    rapid_requests = cfg.rapid_requests,
})

-- Bring requester online
sim:step()
peer_state:bring_online(requester, sim.tick)
sim:force_online(requester)
logger.event(discovery.EVENTS.PEER_ONLINE, {
    tick = sim.tick,
    peer = tostring(requester),
    role = "requester",
})

local first_request_tick = sim.tick

-- Send multiple rapid requests
for request_num = 1, cfg.rapid_requests do
    sim:step()

    local requester_id = tostring(requester)

    logger.event(discovery.EVENTS.INTRODUCTION_REQUEST_SENT, {
        tick = sim.tick,
        requester = requester_id,
        request_number = request_num,
    })

    -- Each group peer evaluates whether to respond
    for _, group_peer in ipairs(group_peers) do
        local group_peer_id = tostring(group_peer)

        if peer_state:is_online(group_peer) then
            local can_respond = rate_limiter:can_respond(group_peer, requester, sim.tick)

            if can_respond then
                -- Record response (this also updates rate limit state)
                local allowed = rate_limiter:record_response(group_peer, requester, sim.tick)

                if allowed then
                    if request_num == 1 then
                        first_request_responses = first_request_responses + 1
                    else
                        -- This would be a violation - responding to subsequent request
                        rate_limit_violations = rate_limit_violations + 1
                        logger.warn("Rate limit violation detected", {
                            tick = sim.tick,
                            responder = group_peer_id,
                            request_number = request_num,
                        })
                    end

                    logger.event(discovery.EVENTS.INTRODUCTION_RESPONSE_SENT, {
                        tick = sim.tick,
                        responder = group_peer_id,
                        requester = requester_id,
                        request_number = request_num,
                    })
                end
            else
                -- Properly rate limited
                rate_limited_count = rate_limited_count + 1

                logger.event(discovery.EVENTS.INTRODUCTION_RESPONSE_RATE_LIMITED, {
                    tick = sim.tick,
                    responder = group_peer_id,
                    requester = requester_id,
                    request_number = request_num,
                })
            end
        end
    end
end

logger.info("Phase 2 complete: Rapid requests processed", {
    phase = 2,
    tick = sim.tick,
    first_request_responses = first_request_responses,
    rate_limited_count = rate_limited_count,
    rate_limit_violations = rate_limit_violations,
})

-- ============================================================================
-- PHASE 3: WAIT FOR WINDOW (ticks 61-100)
-- Wait for rate limit window to expire
-- ============================================================================

logger.info("Phase 3: Waiting for rate limit window", {
    phase = 3,
    window_ticks = cfg.rate_limit_window,
})

-- Calculate tick when window expires
local window_expiry_tick = first_request_tick + cfg.rate_limit_window

-- Run until window expires
for tick = sim.tick + 1, window_expiry_tick + 5 do
    sim:step()
end

logger.info("Phase 3 complete: Rate limit window expired", {
    phase = 3,
    tick = sim.tick,
    window_expiry_tick = window_expiry_tick,
})

-- ============================================================================
-- PHASE 4: POST-WINDOW REQUEST (ticks 101-150)
-- Send request after window, should succeed
-- ============================================================================

logger.info("Phase 4: Post-window request", {
    phase = 4,
    current_tick = sim.tick,
})

sim:step()

local requester_id = tostring(requester)

logger.event(discovery.EVENTS.INTRODUCTION_REQUEST_SENT, {
    tick = sim.tick,
    requester = requester_id,
    phase = "post_window",
})

-- Group peers should now be able to respond
for _, group_peer in ipairs(group_peers) do
    local group_peer_id = tostring(group_peer)

    if peer_state:is_online(group_peer) then
        local can_respond = rate_limiter:can_respond(group_peer, requester, sim.tick)

        if can_respond then
            local allowed = rate_limiter:record_response(group_peer, requester, sim.tick)

            if allowed then
                post_window_responses = post_window_responses + 1

                logger.event(discovery.EVENTS.INTRODUCTION_RESPONSE_SENT, {
                    tick = sim.tick,
                    responder = group_peer_id,
                    requester = requester_id,
                    phase = "post_window",
                })
            end
        else
            -- This would be unexpected - should be able to respond after window
            logger.warn("Unexpected rate limit after window", {
                tick = sim.tick,
                responder = group_peer_id,
            })
        end
    end
end

logger.info("Phase 4 complete: Post-window responses", {
    phase = 4,
    tick = sim.tick,
    post_window_responses = post_window_responses,
})

-- ============================================================================
-- VERIFICATION
-- ============================================================================

logger.info("Phase 5: Verification", {
    phase = 5,
})

-- Calculate metrics
local limiter_stats = rate_limiter:stats()

-- Rate limit was enforced if we got responses for first request and blocked subsequent ones
local rate_limit_enforced = (first_request_responses > 0) and (rate_limited_count > 0) and 1.0 or 0.0

-- Response after window
local response_after_window = post_window_responses > 0 and 1.0 or 0.0

result:add_metrics({
    group_size = cfg.group_size,
    rapid_requests = cfg.rapid_requests,
    rate_limit_window = cfg.rate_limit_window,
    first_request_responses = first_request_responses,
    rate_limited_count = rate_limited_count,
    rate_limit_violations = rate_limit_violations,
    post_window_responses = post_window_responses,
    rate_limit_enforced = rate_limit_enforced,
    response_after_window = response_after_window,
    total_allowed = limiter_stats.allowed,
    total_limited = limiter_stats.limited,
})

-- Assertions
result:record_assertion("first_request_got_responses",
    first_request_responses > 0, true, first_request_responses > 0)
result:record_assertion("subsequent_requests_rate_limited",
    rate_limited_count > 0, true, rate_limited_count > 0)
result:record_assertion("no_rate_limit_violations",
    rate_limit_violations == 0, true, rate_limit_violations == 0)
result:record_assertion("response_after_window",
    post_window_responses > 0, true, post_window_responses > 0)

-- Validate against thresholds
local scenario_thresholds = thresholds.get("rate_limit")
local passed, failures = discovery.assert_thresholds(result.metrics, scenario_thresholds)

for _, failure in ipairs(failures) do
    result:add_error(string.format(
        "Threshold '%s' failed: expected %s %s, got %s",
        failure.metric, failure.type, tostring(failure.expected), tostring(failure.actual)
    ))
end

-- ============================================================================
-- FINAL RESULTS
-- ============================================================================

local final_result = result:build()

logger.info("Rate limit verification scenario completed", {
    passed = final_result.passed,
    level = final_result.level,
    first_request_responses = first_request_responses,
    rate_limited_count = rate_limited_count,
    rate_limit_violations = rate_limit_violations,
    post_window_responses = post_window_responses,
    rate_limit_enforced = rate_limit_enforced,
    response_after_window = response_after_window,
})

-- Standard assertions
indras.assert.gt(first_request_responses, 0, "First request should get responses")
indras.assert.gt(rate_limited_count, 0, "Subsequent requests should be rate limited")
indras.assert.eq(rate_limit_violations, 0, "Should have no rate limit violations")
indras.assert.gt(post_window_responses, 0, "Should respond after window expires")

logger.info("Rate limit verification scenario passed", {
    rate_limit_enforced = true,
    violations = rate_limit_violations,
})

return final_result

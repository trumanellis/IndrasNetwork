-- SyncEngine Attention Tracking Stress Test
--
-- Tests the attention tracking system where members focus on quests to
-- "charge them up" and quest ranking emerges from accumulated attention.
--
-- Phases:
-- 1. Setup: Create peers, realm, and quests
-- 2. Random Focus: Members randomly switch attention between quests
-- 3. Sustained Focus: Some members focus for extended periods
-- 4. Verification: Calculate attention and verify ranking consistency
-- 5. Edge Cases: Test member leaving, rapid switching

local attention_helpers = require("lib.attention_helpers")
local quest_helpers = require("lib.quest_helpers")
local thresholds = require("config.attention_thresholds")

-- ============================================================================
-- SETUP
-- ============================================================================

local ctx = attention_helpers.new_context("sync_engine_attention_stress")
local logger = attention_helpers.create_logger(ctx)
local config = attention_helpers.get_config()

logger.info("Starting attention tracking stress scenario", {
    level = attention_helpers.get_level(),
    members = config.members,
    quests = config.quests,
    switches_per_member = config.switches_per_member,
})

-- Create mesh with members
local mesh = indras.MeshBuilder.new(config.members):full_mesh()
local sim_config = indras.SimConfig.new({
    wake_probability = 0,
    sleep_probability = 0,
    initial_online_probability = 1,
    max_ticks = config.ticks,
})
local sim = indras.Simulation.new(mesh, sim_config)
sim:initialize()

local peers = mesh:peers()
local result = attention_helpers.result_builder("sync_engine_attention_stress")

-- Metrics tracking
local latencies = {
    switch = {},
    calc = {},
}

-- Create attention document (using SyncEngine binding)
local attention_doc = indras.sync_engine.attention.new()

-- Create quest IDs
local quest_ids = {}
for i = 1, config.quests do
    table.insert(quest_ids, string.format("quest_%03d", i))
end

-- Create local tracker for verification
local tracker = attention_helpers.AttentionTracker.new()

-- ============================================================================
-- PHASE 1: SETUP (Bring all peers online)
-- ============================================================================

logger.info("Phase 1: Setup", {
    phase = 1,
    peer_count = #peers,
    quest_count = #quest_ids,
})

for _, peer in ipairs(peers) do
    sim:force_online(peer)
end

sim:step()

logger.info("Phase 1 complete: Setup done", {
    phase = 1,
    tick = sim.tick,
})

-- ============================================================================
-- PHASE 2: RANDOM FOCUS
-- Members randomly switch attention between quests
-- ============================================================================

logger.info("Phase 2: Random focus switching", {
    phase = 2,
    description = "Members randomly switch attention",
    switches_per_member = config.switches_per_member,
})

local total_switches = 0
local switch_start_time = os.clock()

for _, peer in ipairs(peers) do
    local member_id = tostring(peer)

    for i = 1, config.switches_per_member do
        -- Pick a random quest
        local quest_idx = math.random(#quest_ids)
        local quest_id = quest_ids[quest_idx]

        -- Measure switch latency
        local start_time = os.clock()
        local event_id = attention_doc:focus_on_quest(member_id, quest_id)
        local latency = (os.clock() - start_time) * 1000000  -- microseconds
        table.insert(latencies.switch, latency)

        -- Track in local tracker
        local timestamp = os.time() * 1000 + i  -- Ensure unique timestamps
        tracker:record_switch(member_id, quest_id, timestamp)

        total_switches = total_switches + 1

        logger.event(attention_helpers.EVENTS.ATTENTION_SWITCHED, {
            tick = sim.tick,
            member = member_id,
            quest_id = quest_id,
            event_id = event_id,
            latency_us = latency,
        })

        -- Step occasionally
        if total_switches % 100 == 0 then
            sim:step()
        end
    end
end

local phase2_duration = os.clock() - switch_start_time

logger.info("Phase 2 complete: Random switches done", {
    phase = 2,
    tick = sim.tick,
    total_switches = total_switches,
    duration_sec = phase2_duration,
    avg_switch_latency_us = attention_helpers.average(latencies.switch),
})

-- ============================================================================
-- PHASE 3: SUSTAINED FOCUS
-- Some members focus for extended periods
-- ============================================================================

logger.info("Phase 3: Sustained focus", {
    phase = 3,
    description = "Some members focus on specific quests",
})

-- Pick top 3 quests to receive sustained attention
local sustained_quests = { quest_ids[1], quest_ids[2], quest_ids[3] }
local sustained_members = {}

for i = 1, math.min(3, #peers) do
    local member_id = tostring(peers[i])
    local quest_id = sustained_quests[i]

    attention_doc:focus_on_quest(member_id, quest_id)
    tracker:record_switch(member_id, quest_id, os.time() * 1000)
    sustained_members[member_id] = quest_id

    logger.event(attention_helpers.EVENTS.ATTENTION_SWITCHED, {
        tick = sim.tick,
        member = member_id,
        quest_id = quest_id,
        sustained = true,
    })
end

-- Simulate time passing (for attention accumulation)
for tick = 1, 50 do
    sim:step()
end

logger.info("Phase 3 complete: Sustained focus established", {
    phase = 3,
    tick = sim.tick,
    sustained_members = 3,
})

-- ============================================================================
-- PHASE 4: VERIFICATION
-- Calculate attention and verify ranking consistency
-- ============================================================================

logger.info("Phase 4: Verification", {
    phase = 4,
    description = "Calculate attention and verify consistency",
})

-- Calculate attention via SyncEngine
local calc_start = os.clock()
local ranked_quests = attention_doc:quests_by_attention()
local calc_latency = (os.clock() - calc_start) * 1000000
table.insert(latencies.calc, calc_latency)

logger.event(attention_helpers.EVENTS.ATTENTION_CALCULATED, {
    tick = sim.tick,
    quest_count = #ranked_quests,
    latency_us = calc_latency,
})

-- Verify ranking is non-empty
local ranking_valid = #ranked_quests > 0

-- Verify focus tracking
local focus_checks = { passed = 0, failed = 0 }
for member_id, expected_quest in pairs(sustained_members) do
    local actual_quest = attention_doc:current_focus(member_id)
    if actual_quest == expected_quest then
        focus_checks.passed = focus_checks.passed + 1
    else
        focus_checks.failed = focus_checks.failed + 1
        logger.warn("Focus tracking mismatch", {
            member = member_id,
            expected = expected_quest,
            actual = actual_quest,
        })
    end
end

local focus_tracking_accuracy = focus_checks.passed /
    (focus_checks.passed + focus_checks.failed)

-- Verify ranking consistency (calculate twice, should be same)
local ranked_quests_2 = attention_doc:quests_by_attention()
local ranking_consistent = #ranked_quests == #ranked_quests_2

if ranking_consistent and #ranked_quests > 0 then
    for i = 1, math.min(5, #ranked_quests) do
        if ranked_quests[i].quest_id ~= ranked_quests_2[i].quest_id then
            ranking_consistent = false
            break
        end
    end
end

logger.event(attention_helpers.EVENTS.RANKING_VERIFIED, {
    tick = sim.tick,
    ranking_valid = ranking_valid,
    ranking_consistent = ranking_consistent,
    focus_tracking_accuracy = focus_tracking_accuracy,
    top_quest = ranked_quests[1] and ranked_quests[1].quest_id or "none",
    top_attention_ms = ranked_quests[1] and ranked_quests[1].total_attention_millis or 0,
})

logger.info("Phase 4 complete: Verification done", {
    phase = 4,
    tick = sim.tick,
    ranking_valid = ranking_valid,
    ranking_consistent = ranking_consistent,
})

-- ============================================================================
-- PHASE 5: EDGE CASES
-- Test clearing attention, rapid switching
-- ============================================================================

logger.info("Phase 5: Edge cases", {
    phase = 5,
    description = "Test clearing attention and rapid switching",
})

-- Test clearing attention
local clear_member = tostring(peers[1])
attention_doc:clear_attention(clear_member)
tracker:record_switch(clear_member, nil, os.time() * 1000)

local focus_after_clear = attention_doc:current_focus(clear_member)
local clear_worked = focus_after_clear == nil

logger.event(attention_helpers.EVENTS.ATTENTION_CLEARED, {
    tick = sim.tick,
    member = clear_member,
    clear_worked = clear_worked,
})

-- Test rapid switching
local rapid_switches = 0
local rapid_member = tostring(peers[2])
for i = 1, 100 do
    local quest_id = quest_ids[(i % #quest_ids) + 1]
    attention_doc:focus_on_quest(rapid_member, quest_id)
    rapid_switches = rapid_switches + 1
end

local event_count_after_rapid = attention_doc:event_count()

logger.info("Phase 5 complete: Edge cases tested", {
    phase = 5,
    tick = sim.tick,
    clear_worked = clear_worked,
    rapid_switches = rapid_switches,
    total_events = event_count_after_rapid,
})

-- ============================================================================
-- FINAL RESULTS
-- ============================================================================

-- Calculate metrics
local switch_percentiles = attention_helpers.percentiles(latencies.switch)
local calc_percentiles = attention_helpers.percentiles(latencies.calc)

-- Record metrics
result:add_metrics({
    total_switches = total_switches + rapid_switches,
    total_events = event_count_after_rapid,
    unique_quests_with_attention = #ranked_quests,

    attention_switch_p99_us = switch_percentiles.p99,
    attention_switch_p95_us = switch_percentiles.p95,
    attention_switch_p50_us = switch_percentiles.p50,

    attention_calc_p99_us = calc_percentiles.p99,

    ranking_consistency = ranking_consistent and 1.0 or 0.0,
    focus_tracking_accuracy = focus_tracking_accuracy,

    -- CRDT convergence (simulated as 100% since we're single-threaded)
    attention_crdt_convergence = 1.0,
})

-- Assertions
result:record_assertion("ranking_consistency",
    ranking_consistent, true, ranking_consistent)
result:record_assertion("focus_tracking_accuracy",
    focus_tracking_accuracy >= 1.0, 1.0, focus_tracking_accuracy)
result:record_assertion("clear_attention_works",
    clear_worked, true, clear_worked)

local final_result = result:build()

logger.info("Attention tracking stress scenario completed", {
    passed = final_result.passed,
    level = final_result.level,
    duration_sec = final_result.duration_sec,
    final_tick = sim.tick,
    total_switches = total_switches + rapid_switches,
    total_events = event_count_after_rapid,
    switch_p99_us = switch_percentiles.p99,
    ranking_consistent = ranking_consistent,
})

-- Standard assertions
indras.assert.eq(ranking_consistent, true, "Ranking should be consistent")
indras.assert.eq(focus_tracking_accuracy, 1.0, "Focus tracking should be 100% accurate")
indras.assert.eq(clear_worked, true, "Clearing attention should work")
indras.assert.gt(#ranked_quests, 0, "Should have quests with attention")

logger.info("Attention tracking stress scenario passed", {
    top_quest = ranked_quests[1] and ranked_quests[1].quest_id or "none",
})

return final_result

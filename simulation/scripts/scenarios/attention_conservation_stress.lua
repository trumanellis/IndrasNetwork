-- Attention Conservation Invariant Stress Test
--
-- Tests the conservation invariant: the count of members with a non-nil current
-- focus equals the count of members who have been assigned focus and have not
-- cleared it. Specifically:
--
--   count_total_focused(doc, member_ids) == expected_focused_count
--
-- at every observable point during the simulation.
--
-- Phases:
-- 1. Setup: Create peers, quest IDs, and attention doc
-- 2. Genesis: Each peer focuses on an initial quest; assert count == 5
-- 3. Concurrent switches: 10 rounds of all-peer switches; assert conservation each round
-- 4. Peer leave: Two peers clear attention; assert count drops correctly
-- 5. Rapid switching stress: One peer switches 100 times; assert conservation at end
-- 6. Edge cases: Same-quest re-focus and immediate switch-back

local attention_helpers = require("lib.attention_helpers")
local thresholds = require("config.attention_thresholds")

-- ============================================================================
-- CONSERVATION HELPER
-- ============================================================================

--- Count how many members currently have a non-nil focus.
-- This is the core conservation invariant checker.
-- @param doc  attention document
-- @param member_ids table  array of member ID strings
-- @return number  count of members with active focus
local function count_total_focused(doc, member_ids)
    local count = 0
    for _, member_id in ipairs(member_ids) do
        if doc:current_focus(member_id) ~= nil then
            count = count + 1
        end
    end
    return count
end

-- ============================================================================
-- SETUP
-- ============================================================================

local ctx    = attention_helpers.new_context("attention_conservation_stress")
local logger = attention_helpers.create_logger(ctx)
local config = attention_helpers.get_config()

logger.info("Starting attention conservation stress scenario", {
    level               = attention_helpers.get_level(),
    members             = config.members,
    quests              = config.quests,
    switches_per_member = config.switches_per_member,
})

-- We use exactly 5 peers for deterministic conservation assertions.
local PEER_COUNT  = 5
local QUEST_COUNT = 5

local mesh = indras.MeshBuilder.new(PEER_COUNT):full_mesh()
local sim_config = indras.SimConfig.new({
    wake_probability             = 0,
    sleep_probability            = 0,
    initial_online_probability   = 1,
    max_ticks                    = config.ticks,
})
local sim = indras.Simulation.new(mesh, sim_config)
sim:initialize()

local peers  = mesh:peers()
local result = attention_helpers.result_builder("attention_conservation_stress")

-- Bring all peers online
for _, peer in ipairs(peers) do
    sim:force_online(peer)
end
sim:step()

-- Build member ID strings and quest ID strings
local member_ids = {}
for _, peer in ipairs(peers) do
    table.insert(member_ids, tostring(peer))
end

local quest_ids = {}
for i = 1, QUEST_COUNT do
    table.insert(quest_ids, string.format("quest_%03d", i))
end

-- Attention document under test
local attention_doc = indras.sync_engine.attention.new()

-- Metrics
local total_switches        = 0
local conservation_violations = 0

-- ============================================================================
-- PHASE 1: SETUP COMPLETE
-- ============================================================================

logger.info("Phase 1: Setup complete", {
    phase        = 1,
    peer_count   = #peers,
    quest_count  = #quest_ids,
    member_count = #member_ids,
    tick         = sim.tick,
})

indras.narrative("The network assembles — five peers ready to focus")

-- ============================================================================
-- PHASE 2: GENESIS
-- Each peer focuses on its initial quest.
-- Conservation invariant: count_total_focused == PEER_COUNT
-- ============================================================================

logger.info("Phase 2: Genesis — initial focus assignment", {
    phase       = 2,
    description = "Each peer focuses on a distinct quest",
})

for i, member_id in ipairs(member_ids) do
    local quest_id = quest_ids[i]  -- peer i -> quest i (1-to-1)
    attention_doc:focus_on_quest(member_id, quest_id)
    total_switches = total_switches + 1

    logger.event(attention_helpers.EVENTS.ATTENTION_SWITCHED, {
        tick      = sim.tick,
        member    = member_id,
        quest_id  = quest_id,
        phase     = 2,
    })
end

sim:step()

local focused_after_genesis = count_total_focused(attention_doc, member_ids)
local genesis_ok = (focused_after_genesis == PEER_COUNT)

if not genesis_ok then
    conservation_violations = conservation_violations + 1
    logger.warn("Conservation violation after genesis", {
        expected = PEER_COUNT,
        actual   = focused_after_genesis,
    })
end

indras.narrative("Five peers, five quests — the conservation invariant holds at birth")
logger.info("Phase 2 complete: Genesis", {
    phase                 = 2,
    tick                  = sim.tick,
    focused_after_genesis = focused_after_genesis,
    expected              = PEER_COUNT,
    conservation_ok       = genesis_ok,
})

-- ============================================================================
-- PHASE 3: CONCURRENT SWITCHES
-- 10 rounds where every peer switches to a different quest.
-- After each round assert conservation still holds (count stays == PEER_COUNT).
-- Switching preserves the count because a peer leaves one quest and joins
-- another atomically — they remain focused throughout.
-- ============================================================================

logger.info("Phase 3: Concurrent switches", {
    phase       = 3,
    description = "10 rounds of all-peer quest switches; conservation checked each round",
    rounds      = 10,
})

local phase3_violations = 0

for round = 1, 10 do
    -- Each peer switches to a quest offset by round (wraps around)
    for i, member_id in ipairs(member_ids) do
        local quest_idx = ((i + round - 1) % QUEST_COUNT) + 1
        local quest_id  = quest_ids[quest_idx]

        attention_doc:focus_on_quest(member_id, quest_id)
        total_switches = total_switches + 1

        logger.event(attention_helpers.EVENTS.ATTENTION_SWITCHED, {
            tick     = sim.tick,
            member   = member_id,
            quest_id = quest_id,
            round    = round,
            phase    = 3,
        })
    end

    -- Assert conservation after every round
    local focused = count_total_focused(attention_doc, member_ids)
    local round_ok = (focused == PEER_COUNT)

    if not round_ok then
        phase3_violations = phase3_violations + 1
        conservation_violations = conservation_violations + 1
        logger.warn("Conservation violation in concurrent switches", {
            round    = round,
            expected = PEER_COUNT,
            actual   = focused,
        })
    end

    logger.info("Round complete", {
        phase           = 3,
        round           = round,
        total_focused   = focused,
        conservation_ok = round_ok,
    })

    -- Step sim periodically
    if round % 3 == 0 then
        sim:step()
    end
end

indras.narrative("Attention flows freely — switching never breaks the count")
logger.info("Phase 3 complete: Concurrent switches", {
    phase            = 3,
    tick             = sim.tick,
    total_switches   = total_switches,
    phase3_violations = phase3_violations,
})

-- ============================================================================
-- PHASE 4: PEER LEAVE
-- Two peers clear their attention one at a time.
-- Conservation count decreases by 1 for each clear.
-- ============================================================================

logger.info("Phase 4: Peer leave — clear attention", {
    phase       = 4,
    description = "Two peers clear attention; count drops from 5 to 4 to 3",
})

-- First peer clears
local leaver_a = member_ids[1]
attention_doc:clear_attention(leaver_a)

logger.event(attention_helpers.EVENTS.ATTENTION_CLEARED, {
    tick   = sim.tick,
    member = leaver_a,
    phase  = 4,
    step   = "first_clear",
})

sim:step()

local focused_after_first_clear = count_total_focused(attention_doc, member_ids)
local first_clear_ok = (focused_after_first_clear == PEER_COUNT - 1)

if not first_clear_ok then
    conservation_violations = conservation_violations + 1
    logger.warn("Conservation violation after first clear", {
        expected = PEER_COUNT - 1,
        actual   = focused_after_first_clear,
    })
end

logger.info("First peer cleared", {
    phase             = 4,
    total_focused     = focused_after_first_clear,
    expected          = PEER_COUNT - 1,
    conservation_ok   = first_clear_ok,
})

-- Second peer clears
local leaver_b = member_ids[2]
attention_doc:clear_attention(leaver_b)

logger.event(attention_helpers.EVENTS.ATTENTION_CLEARED, {
    tick   = sim.tick,
    member = leaver_b,
    phase  = 4,
    step   = "second_clear",
})

sim:step()

local focused_after_second_clear = count_total_focused(attention_doc, member_ids)
local second_clear_ok = (focused_after_second_clear == PEER_COUNT - 2)

if not second_clear_ok then
    conservation_violations = conservation_violations + 1
    logger.warn("Conservation violation after second clear", {
        expected = PEER_COUNT - 2,
        actual   = focused_after_second_clear,
    })
end

indras.narrative("Two peers depart — the count contracts gracefully")
logger.info("Phase 4 complete: Peer leave", {
    phase                     = 4,
    tick                      = sim.tick,
    focused_after_first_clear = focused_after_first_clear,
    focused_after_second_clear = focused_after_second_clear,
    first_clear_ok            = first_clear_ok,
    second_clear_ok           = second_clear_ok,
})

-- ============================================================================
-- PHASE 5: RAPID SWITCHING STRESS
-- One of the remaining focused peers switches 100 times between quests.
-- Conservation count must stay == 3 (PEER_COUNT - 2) throughout.
-- ============================================================================

logger.info("Phase 5: Rapid switching stress", {
    phase       = 5,
    description = "One peer switches 100 times; conservation checked at end",
    rapid_count = 100,
    expected_focused = PEER_COUNT - 2,
})

local rapid_member   = member_ids[3]   -- still focused after phase 4
local rapid_switches = 0

for i = 1, 100 do
    local quest_id = quest_ids[(i % QUEST_COUNT) + 1]
    attention_doc:focus_on_quest(rapid_member, quest_id)
    rapid_switches  = rapid_switches + 1
    total_switches  = total_switches + 1
end

local focused_after_rapid = count_total_focused(attention_doc, member_ids)
local rapid_ok = (focused_after_rapid == PEER_COUNT - 2)

if not rapid_ok then
    conservation_violations = conservation_violations + 1
    logger.warn("Conservation violation after rapid switching", {
        expected = PEER_COUNT - 2,
        actual   = focused_after_rapid,
    })
end

sim:step()

indras.narrative("A hundred switches in an instant — the invariant holds firm")
logger.info("Phase 5 complete: Rapid switching stress", {
    phase              = 5,
    tick               = sim.tick,
    rapid_switches     = rapid_switches,
    focused_after_rapid = focused_after_rapid,
    expected           = PEER_COUNT - 2,
    conservation_ok    = rapid_ok,
})

-- ============================================================================
-- PHASE 6: EDGE CASES
-- 6a. Re-focus on the same quest (no-op in terms of conservation count)
-- 6b. Immediate switch-back (focus A -> B -> A)
-- ============================================================================

logger.info("Phase 6: Edge cases", {
    phase       = 6,
    description = "Re-focus same quest, immediate switch-back",
})

-- 6a: Same-quest re-focus — count must not change
local edge_member = member_ids[3]
local current_quest = attention_doc:current_focus(edge_member)
local focused_before_refocus = count_total_focused(attention_doc, member_ids)

-- Focus on the exact same quest again
attention_doc:focus_on_quest(edge_member, current_quest)
total_switches = total_switches + 1

local focused_after_refocus = count_total_focused(attention_doc, member_ids)
local refocus_ok = (focused_after_refocus == focused_before_refocus)

if not refocus_ok then
    conservation_violations = conservation_violations + 1
    logger.warn("Conservation violation on same-quest re-focus", {
        expected = focused_before_refocus,
        actual   = focused_after_refocus,
    })
end

logger.event(attention_helpers.EVENTS.ATTENTION_SWITCHED, {
    tick           = sim.tick,
    member         = edge_member,
    quest_id       = current_quest,
    edge_case      = "same_quest_refocus",
    focused_before = focused_before_refocus,
    focused_after  = focused_after_refocus,
    conservation_ok = refocus_ok,
})

-- 6b: Immediate switch-back A -> B -> A — count must remain stable
local quest_a = quest_ids[1]
local quest_b = quest_ids[2]

attention_doc:focus_on_quest(edge_member, quest_a)
total_switches = total_switches + 1
local focused_mid_switchback = count_total_focused(attention_doc, member_ids)

attention_doc:focus_on_quest(edge_member, quest_b)
total_switches = total_switches + 1
local focused_after_b = count_total_focused(attention_doc, member_ids)

attention_doc:focus_on_quest(edge_member, quest_a)
total_switches = total_switches + 1
local focused_after_switchback = count_total_focused(attention_doc, member_ids)

-- All three states should have the same count (== PEER_COUNT - 2)
local expected_edge = PEER_COUNT - 2
local switchback_ok = (
    focused_mid_switchback  == expected_edge and
    focused_after_b         == expected_edge and
    focused_after_switchback == expected_edge
)

if not switchback_ok then
    conservation_violations = conservation_violations + 1
    logger.warn("Conservation violation during switch-back", {
        expected              = expected_edge,
        focused_mid           = focused_mid_switchback,
        focused_after_b       = focused_after_b,
        focused_after_return  = focused_after_switchback,
    })
end

logger.event(attention_helpers.EVENTS.ATTENTION_SWITCHED, {
    tick                   = sim.tick,
    member                 = edge_member,
    edge_case              = "switch_back",
    focused_mid_switchback  = focused_mid_switchback,
    focused_after_b         = focused_after_b,
    focused_after_switchback = focused_after_switchback,
    conservation_ok         = switchback_ok,
})

sim:step()

local total_events = attention_doc:event_count()

indras.narrative("Every edge path traced — the conservation law never bends")
logger.info("Phase 6 complete: Edge cases", {
    phase              = 6,
    tick               = sim.tick,
    refocus_ok         = refocus_ok,
    switchback_ok      = switchback_ok,
    total_events       = total_events,
})

-- ============================================================================
-- FINAL RESULTS
-- ============================================================================

-- Overall conservation check: zero violations throughout all phases
local conservation_perfect = (conservation_violations == 0)

result:add_metrics({
    total_switches            = total_switches,
    total_events              = total_events,
    conservation_violations   = conservation_violations,

    -- Phase outcome flags recorded as 0/1 for numeric metrics
    genesis_conservation       = genesis_ok        and 1.0 or 0.0,
    concurrent_conservation    = (phase3_violations == 0) and 1.0 or 0.0,
    first_clear_conservation   = first_clear_ok    and 1.0 or 0.0,
    second_clear_conservation  = second_clear_ok   and 1.0 or 0.0,
    rapid_switch_conservation  = rapid_ok          and 1.0 or 0.0,
    refocus_conservation       = refocus_ok        and 1.0 or 0.0,
    switchback_conservation    = switchback_ok     and 1.0 or 0.0,

    final_focused_count        = count_total_focused(attention_doc, member_ids),
})

result:record_assertion("genesis_conservation",
    genesis_ok, PEER_COUNT, focused_after_genesis)
result:record_assertion("concurrent_switches_conservation",
    phase3_violations == 0, 0, phase3_violations)
result:record_assertion("first_clear_conservation",
    first_clear_ok, PEER_COUNT - 1, focused_after_first_clear)
result:record_assertion("second_clear_conservation",
    second_clear_ok, PEER_COUNT - 2, focused_after_second_clear)
result:record_assertion("rapid_switch_conservation",
    rapid_ok, PEER_COUNT - 2, focused_after_rapid)
result:record_assertion("refocus_conservation",
    refocus_ok, focused_before_refocus, focused_after_refocus)
result:record_assertion("switchback_conservation",
    switchback_ok, expected_edge, focused_after_switchback)
result:record_assertion("zero_conservation_violations",
    conservation_perfect, 0, conservation_violations)

local final_result = result:build()

logger.info("Attention conservation stress scenario completed", {
    passed                  = final_result.passed,
    level                   = final_result.level,
    duration_sec            = final_result.duration_sec,
    final_tick              = sim.tick,
    total_switches          = total_switches,
    total_events            = total_events,
    conservation_violations = conservation_violations,
})

-- Standard assertions (will abort the runner on failure)
indras.assert.eq(genesis_ok,      true, "Conservation must hold after genesis")
indras.assert.eq(first_clear_ok,  true, "Conservation must hold after first peer clears")
indras.assert.eq(second_clear_ok, true, "Conservation must hold after second peer clears")
indras.assert.eq(rapid_ok,        true, "Conservation must hold after 100 rapid switches")
indras.assert.eq(refocus_ok,      true, "Re-focusing same quest must not change focused count")
indras.assert.eq(switchback_ok,   true, "Immediate switch-back must preserve focused count")
indras.assert.eq(conservation_violations, 0, "Zero conservation violations across all phases")
indras.assert.gt(total_switches,  0, "Should have recorded switch operations")

logger.info("Attention conservation stress scenario passed", {
    conservation_violations = conservation_violations,
    total_switches          = total_switches,
})

return final_result

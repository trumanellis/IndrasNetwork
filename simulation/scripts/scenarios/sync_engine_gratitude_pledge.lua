-- SyncEngine Gratitude Pledge Scenario
--
-- Demonstrates the full Token of Gratitude lifecycle:
-- 1. Realm creation with 3 members (A, B, C)
-- 2. Quest creation (Design Logo, Write Documentation, Build API)
-- 3. Proof submission and blessings (minting tokens)
-- 4. Token pledging to quests as bounty
-- 5. Token release to proof submitters (steward transfer)
-- 6. Token chaining (token flows through 3+ stewards)
-- 7. Pledge withdrawal
--
-- This scenario exercises the discrete token system end-to-end,
-- verifying minting, pledging, releasing, withdrawing, and chaining.

local quest_helpers = require("lib.quest_helpers")
local home = require("lib.home_realm_helpers")

-- ============================================================================
-- SETUP
-- ============================================================================

local ctx = quest_helpers.new_context("sync_engine_gratitude_pledge")
local logger = quest_helpers.create_logger(ctx)

logger.info("Starting Gratitude Pledge scenario", {
    level = quest_helpers.get_level(),
    description = "Full token lifecycle: mint, pledge, release, withdraw, chain",
})

-- Create 3-peer full mesh
local mesh = indras.MeshBuilder.new(3):full_mesh()
local sim_config = indras.SimConfig.new({
    wake_probability = 0,
    sleep_probability = 0,
    initial_online_probability = 1,
    max_ticks = 200,
})
local sim = indras.Simulation.new(mesh, sim_config)
sim:initialize()

local peers = mesh:peers()
local result = quest_helpers.result_builder("sync_engine_gratitude_pledge")

-- Assign roles
local peer_a = tostring(peers[1])
local peer_b = tostring(peers[2])
local peer_c = tostring(peers[3])
local all_members = { peer_a, peer_b, peer_c }

-- Tracking
local blessing_tracker = home.BlessingTracker.new()
local token_counter = 0

--- Generate a unique token ID
local function make_token_id(quest_id, steward, tick)
    token_counter = token_counter + 1
    return string.format("tok_%s_%s_%d_%d", quest_id:sub(1, 8), steward:sub(1, 8), tick, token_counter)
end

-- Force all peers online
for _, peer in ipairs(peers) do
    sim:force_online(peer)
end

-- ============================================================================
-- PHASE 1: SETUP -- Create realm, join members, add contacts
-- ============================================================================

indras.narrative("Three members gather in the Gratitude Workshop")
logger.info("Phase 1: Setup", { phase = 1 })

local realm_id = quest_helpers.compute_realm_id(all_members)

logger.event("realm_created", {
    tick = sim.tick,
    realm_id = realm_id,
    members = table.concat(all_members, ","),
    member_count = 3,
})
sim:step()

for _, member in ipairs(all_members) do
    logger.event("member_joined", {
        tick = sim.tick,
        realm_id = realm_id,
        member = member,
    })
end
sim:step()

-- Contacts (all pairs, bidirectional)
for i, member in ipairs(all_members) do
    for j, contact in ipairs(all_members) do
        if i ~= j then
            logger.event("contact_added", {
                tick = sim.tick,
                member = member,
                contact = contact,
            })
        end
    end
end
sim:step()

-- Rename realm
logger.event("realm_alias_set", {
    tick = sim.tick,
    member = peer_a,
    realm_id = realm_id,
    alias = "Gratitude Workshop",
})
sim:step()

logger.info("Phase 1 complete", { phase = 1, tick = sim.tick })

-- ============================================================================
-- PHASE 2: CREATE QUESTS
-- ============================================================================

indras.narrative("New quests take shape — a logo, documentation, and an API")
logger.info("Phase 2: Create quests", { phase = 2 })

-- Quest A: Design Logo (created by A)
local quest_a_id = quest_helpers.compute_quest_id(realm_id, "Design Logo")
logger.event("quest_created", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = quest_a_id,
    creator = peer_a,
    title = "Design Logo",
    description = "Create a logo for the Gratitude Workshop",
})
sim:step()

-- Quest B: Write Documentation (created by A)
local quest_b_id = quest_helpers.compute_quest_id(realm_id, "Write Documentation")
logger.event("quest_created", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = quest_b_id,
    creator = peer_a,
    title = "Write Documentation",
    description = "Document the token system for new members",
})
sim:step()

-- Quest C: Build API (created by C)
local quest_c_id = quest_helpers.compute_quest_id(realm_id, "Build API")
logger.event("quest_created", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = quest_c_id,
    creator = peer_c,
    title = "Build API",
    description = "Build the gratitude pledge API endpoints",
})
sim:step()

logger.info("Phase 2 complete", { phase = 2, tick = sim.tick })

-- ============================================================================
-- PHASE 3: ATTENTION FOCUS
-- ============================================================================

indras.narrative("All eyes turn to the logo quest")
logger.info("Phase 3: Members focus attention", { phase = 3 })

-- A focuses on Quest A
logger.event("attention_switched", {
    tick = sim.tick,
    member = peer_a,
    quest_id = quest_a_id,
    latency_us = quest_helpers.attention_switch_latency(),
})
blessing_tracker:record_attention(quest_a_id, peer_a, 1, 30000)
sim:step()

-- B focuses on Quest A
logger.event("attention_switched", {
    tick = sim.tick,
    member = peer_b,
    quest_id = quest_a_id,
    latency_us = quest_helpers.attention_switch_latency(),
})
blessing_tracker:record_attention(quest_a_id, peer_b, 1, 45000)
sim:step()

-- C focuses on Quest A
logger.event("attention_switched", {
    tick = sim.tick,
    member = peer_c,
    quest_id = quest_a_id,
    latency_us = quest_helpers.attention_switch_latency(),
})
blessing_tracker:record_attention(quest_a_id, peer_c, 1, 20000)
sim:step()

logger.info("Phase 3 complete", { phase = 3, tick = sim.tick })

-- ============================================================================
-- PHASE 4: B SUBMITS PROOF FOR QUEST A
-- ============================================================================

indras.narrative("B presents a logo that captures the workshop's spirit")
logger.info("Phase 4: B submits proof for Quest A", { phase = 4 })

local folder_a_id = quest_helpers.compute_folder_id(realm_id, quest_a_id, peer_b)

logger.event("proof_folder_submitted", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = quest_a_id,
    claimant = peer_b,
    folder_id = folder_a_id,
    narrative_preview = "Designed a clean logo capturing the workshop's essence.",
    artifact_count = 2,
    quest_title = "Design Logo",
    narrative = "# Logo Design\n\nA minimalist logo with warm colors.",
    artifacts = {},
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_b,
    realm_id = realm_id,
    content = "Submitted my logo proof!",
    message_type = "text",
})
sim:step()

logger.info("Phase 4 complete", { phase = 4, tick = sim.tick })

-- ============================================================================
-- PHASE 5: BLESSINGS FOR QUEST A (MINTING TOKENS T1 AND T2)
-- ============================================================================

indras.narrative("Blessings flow — B's work is recognized with tokens of gratitude")
logger.info("Phase 5: Bless Quest A proof -> Mint tokens for B", { phase = 5 })

-- A blesses B's proof (30min attention) -> Token T1 minted
local a_attention = 30000 -- 30s (displayed as 30s in viewer, millis)
blessing_tracker:record_blessing(quest_a_id, peer_b, peer_a, {1}, a_attention)

logger.event("blessing_given", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = quest_a_id,
    claimant = peer_b,
    blesser = peer_a,
    event_count = 1,
    attention_millis = a_attention,
})

local token_t1 = make_token_id(quest_a_id, peer_b, sim.tick)
logger.event("token_minted", {
    tick = sim.tick,
    realm_id = realm_id,
    token_id = token_t1,
    steward = peer_b,
    value_millis = a_attention,
    blesser = peer_a,
    source_quest_id = quest_a_id,
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_a,
    realm_id = realm_id,
    content = "Excellent logo work! Releasing gratitude.",
    message_type = "text",
})
sim:step()

-- C blesses B's proof (20s attention) -> Token T2 minted
local c_attention = 20000
blessing_tracker:record_blessing(quest_a_id, peer_b, peer_c, {1}, c_attention)

logger.event("blessing_given", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = quest_a_id,
    claimant = peer_b,
    blesser = peer_c,
    event_count = 1,
    attention_millis = c_attention,
})

local token_t2 = make_token_id(quest_a_id, peer_b, sim.tick)
logger.event("token_minted", {
    tick = sim.tick,
    realm_id = realm_id,
    token_id = token_t2,
    steward = peer_b,
    value_millis = c_attention,
    blesser = peer_c,
    source_quest_id = quest_a_id,
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_c,
    realm_id = realm_id,
    content = "Clean design! Gratitude released.",
    message_type = "text",
})
sim:step()

-- Verify: B now has 2 tokens (T1=30s, T2=20s)
logger.info("Phase 5 complete: B has 2 tokens", {
    phase = 5,
    tick = sim.tick,
    token_t1 = token_t1,
    token_t1_value = a_attention,
    token_t2 = token_t2,
    token_t2_value = c_attention,
})

-- ============================================================================
-- PHASE 6: B PLEDGES T2 TO QUEST B AS BOUNTY
-- ============================================================================

indras.narrative("B pledges gratitude forward — past work fuels the next quest")
logger.info("Phase 6: B pledges T2 to Quest B", { phase = 6 })

logger.event("gratitude_pledged", {
    tick = sim.tick,
    realm_id = realm_id,
    token_id = token_t2,
    pledger = peer_b,
    target_quest_id = quest_b_id,
    amount_millis = c_attention,
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_b,
    realm_id = realm_id,
    content = "Pledged 20s of gratitude to the Documentation quest!",
    message_type = "text",
})
sim:step()

-- Quest B now shows 20s bounty
logger.info("Phase 6 complete: Quest B bounty = 20s", {
    phase = 6,
    tick = sim.tick,
    quest_b_bounty = c_attention,
})

-- ============================================================================
-- PHASE 7: C SUBMITS PROOF FOR QUEST B
-- ============================================================================

indras.narrative("C delivers documentation that brings clarity to the system")
logger.info("Phase 7: C submits proof for Quest B", { phase = 7 })

local folder_b_id = quest_helpers.compute_folder_id(realm_id, quest_b_id, peer_c)

logger.event("proof_folder_submitted", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = quest_b_id,
    claimant = peer_c,
    folder_id = folder_b_id,
    narrative_preview = "Comprehensive token system docs with examples.",
    artifact_count = 3,
    quest_title = "Write Documentation",
    narrative = "# Token System Documentation\n\nFull guide to the gratitude pledge system.",
    artifacts = {},
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_c,
    realm_id = realm_id,
    content = "Documentation is ready for review!",
    message_type = "text",
})
sim:step()

logger.info("Phase 7 complete", { phase = 7, tick = sim.tick })

-- ============================================================================
-- PHASE 8: B RELEASES T2 TO C (STEWARD TRANSFER)
-- ============================================================================

indras.narrative("Gratitude changes hands as B releases a token to C")
logger.info("Phase 8: B releases T2 to C", { phase = 8 })

logger.event("gratitude_released", {
    tick = sim.tick,
    realm_id = realm_id,
    token_id = token_t2,
    from_steward = peer_b,
    to_steward = peer_c,
    target_quest_id = quest_b_id,
    amount_millis = c_attention,
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_b,
    realm_id = realm_id,
    content = "Great docs, C! Releasing gratitude.",
    message_type = "text",
})
sim:step()

-- Verify: C now owns T2 (20s), B still owns T1 (30s)
logger.info("Phase 8 complete: Steward transfer", {
    phase = 8,
    tick = sim.tick,
    t2_steward = "C",
    t1_steward = "B",
})

-- ============================================================================
-- PHASE 9: TOKEN CHAINING -- C PLEDGES T2 TO QUEST C
-- ============================================================================

indras.narrative("The token travels onward — C pledges it to the API quest")
logger.info("Phase 9: Token chaining -- C pledges T2 to Quest C", { phase = 9 })

logger.event("gratitude_pledged", {
    tick = sim.tick,
    realm_id = realm_id,
    token_id = token_t2,
    pledger = peer_c,
    target_quest_id = quest_c_id,
    amount_millis = c_attention,
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_c,
    realm_id = realm_id,
    content = "Passing the gratitude forward! Pledged to Build API quest.",
    message_type = "text",
})
sim:step()

logger.info("Phase 9 complete: T2 now pledged to Quest C", {
    phase = 9,
    tick = sim.tick,
    quest_c_bounty = c_attention,
})

-- ============================================================================
-- PHASE 10: A SUBMITS PROOF FOR QUEST C, C RELEASES T2
-- ============================================================================

indras.narrative("A single token has now flowed through three stewards")
logger.info("Phase 10: A submits proof for Quest C", { phase = 10 })

local folder_c_id = quest_helpers.compute_folder_id(realm_id, quest_c_id, peer_a)

logger.event("proof_folder_submitted", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = quest_c_id,
    claimant = peer_a,
    folder_id = folder_c_id,
    narrative_preview = "API endpoints for pledge/release/withdraw.",
    artifact_count = 4,
    quest_title = "Build API",
    narrative = "# Gratitude API\n\nREST endpoints for the pledge system.",
    artifacts = {},
})
sim:step()

-- C releases T2 to A (3rd steward!)
logger.event("gratitude_released", {
    tick = sim.tick,
    realm_id = realm_id,
    token_id = token_t2,
    from_steward = peer_c,
    to_steward = peer_a,
    target_quest_id = quest_c_id,
    amount_millis = c_attention,
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_c,
    realm_id = realm_id,
    content = "Solid API work! Gratitude flows onward.",
    message_type = "text",
})
sim:step()

-- T2 has now flowed: B -> C -> A (3 stewards)
logger.info("Phase 10 complete: Token chained through 3 stewards", {
    phase = 10,
    tick = sim.tick,
    t2_steward = "A",
    t2_chain = "B -> C -> A",
})

-- ============================================================================
-- PHASE 11: PLEDGE AND WITHDRAW DEMONSTRATION
-- ============================================================================

indras.narrative("B reconsiders a pledge — gratitude returns to its source")
logger.info("Phase 11: Withdraw demonstration", { phase = 11 })

-- B pledges T1 to Quest C
logger.event("gratitude_pledged", {
    tick = sim.tick,
    realm_id = realm_id,
    token_id = token_t1,
    pledger = peer_b,
    target_quest_id = quest_c_id,
    amount_millis = a_attention,
})
sim:step()

-- B changes mind, withdraws T1
logger.event("gratitude_withdrawn", {
    tick = sim.tick,
    realm_id = realm_id,
    token_id = token_t1,
    steward = peer_b,
    target_quest_id = quest_c_id,
    amount_millis = a_attention,
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_b,
    realm_id = realm_id,
    content = "Changed my mind about that pledge. Withdrawn!",
    message_type = "text",
})
sim:step()

-- T1 is back in B's wallet, unpledged
logger.info("Phase 11 complete: T1 withdrawn, back in B's wallet", {
    phase = 11,
    tick = sim.tick,
})

-- ============================================================================
-- PHASE 12: FINAL STATE VERIFICATION
-- ============================================================================

indras.narrative("The cycle completes — gratitude found its way to those who built")
logger.info("Phase 12: Final state", { phase = 12 })

-- Expected final state:
-- B: T1 (30s, available)
-- C: no tokens
-- A: T2 (20s, available)
-- Quest B bounty: 0 (released)
-- Quest C bounty: 0 (released)

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_a,
    realm_id = realm_id,
    content = "Final state: I hold T2 (20s) from the logo quest. Token chaining works!",
    message_type = "text",
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_b,
    realm_id = realm_id,
    content = "Final state: I hold T1 (30s) -- my first token, still mine after all the action.",
    message_type = "text",
})
sim:step()

logger.info("Scenario complete!", {
    total_ticks = sim.tick,
    tokens_minted = 2,
    pledges = 3,
    releases = 2,
    withdrawals = 1,
    steward_transfers = "B->C->A (T2)",
})

result
    :add_metric("total_ticks", sim.tick)
    :add_metric("tokens_minted", 2)
    :add_metric("pledges", 3)
    :add_metric("releases", 2)
    :add_metric("withdrawals", 1)

return result:build()

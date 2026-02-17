-- Proof of Life Economy Scenario
--
-- Demonstrates the full subjective token valuation, steward chains, and proof of life system
-- in a seven-act narrative:
--
-- Act 1: Genesis - 7 members form a healthy cluster with mutual trust
-- Act 2: Economy - Quests, attention, blessings, tokens flowing naturally
-- Act 3: Attack - Sybil farm enters the picture
-- Act 4: Defense - Subjective valuation renders sybil tokens invisible
-- Act 5: Proof of Life - A gathering refreshes humanness attestations
-- Act 6: Staleness - Absent members' tokens fade over time
-- Act 7: Global Reach - A token travels through the trust chain
--
-- This scenario exercises subjective valuation, trust decay, humanness freshness,
-- and demonstrates how the system naturally resists sybil attacks through social proof.

local quest_helpers = require("lib.quest_helpers")
local home = require("lib.home_realm_helpers")

-- ============================================================================
-- SETUP
-- ============================================================================

local ctx = quest_helpers.new_context("proof_of_life_economy")
local logger = quest_helpers.create_logger(ctx)

logger.info("Starting Proof of Life Economy scenario", {
    level = quest_helpers.get_level(),
    description = "Subjective valuation, trust chains, proof of life, and sybil resistance",
})

-- Create 7-peer full mesh (the trusted core)
local mesh = indras.MeshBuilder.new(7):full_mesh()
local sim_config = indras.SimConfig.new({
    wake_probability = 0,
    sleep_probability = 0,
    initial_online_probability = 1,
    max_ticks = 500,
})
local sim = indras.Simulation.new(mesh, sim_config)
sim:initialize()

local peers = mesh:peers()
local result = quest_helpers.result_builder("proof_of_life_economy")

-- Assign roles
local peer_a = tostring(peers[1])
local peer_b = tostring(peers[2])
local peer_c = tostring(peers[3])
local peer_d = tostring(peers[4])
local peer_e = tostring(peers[5])
local peer_f = tostring(peers[6])
local peer_g = tostring(peers[7])
local all_members = { peer_a, peer_b, peer_c, peer_d, peer_e, peer_f, peer_g }

-- Sybil identities (conceptual - we'll simulate their actions via events)
local sybil_1 = "sybil_0000001"
local sybil_2 = "sybil_0000002"
local sybil_3 = "sybil_0000003"

-- Tracking
local blessing_tracker = home.BlessingTracker.new()
local token_counter = 0
local tokens_minted = 0
local sybil_tokens_minted = 0
local steward_transfers = 0

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
-- ACT 1: GENESIS - 7 members form a healthy cluster with mutual trust
-- ============================================================================

indras.narrative("Act 1: Genesis — Seven neighbors come together to build a local economy")
logger.info("Act 1: Genesis", { act = 1 })

local realm_id = quest_helpers.compute_realm_id(all_members)

logger.event("realm_created", {
    tick = sim.tick,
    realm_id = realm_id,
    members = table.concat(all_members, ","),
    member_count = 7,
})
sim:step()

-- All members join
for _, member in ipairs(all_members) do
    logger.event("member_joined", {
        tick = sim.tick,
        realm_id = realm_id,
        member = member,
    })
end
sim:step()

-- Add contacts (all pairs, bidirectional)
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
    alias = "Proof of Life Economy",
})
sim:step()

-- Set positive sentiment between trusted pairs
-- Chain: A ↔ B ↔ C ↔ D ↔ E ↔ F ↔ G
local trust_pairs = {
    {peer_a, peer_b},
    {peer_b, peer_c},
    {peer_c, peer_d},
    {peer_d, peer_e},
    {peer_e, peer_f},
    {peer_f, peer_g},
}

for _, pair in ipairs(trust_pairs) do
    -- Bidirectional trust
    logger.event("sentiment_set", {
        tick = sim.tick,
        from_member = pair[1],
        to_member = pair[2],
        sentiment = 1,
    })
    logger.event("sentiment_set", {
        tick = sim.tick,
        from_member = pair[2],
        to_member = pair[1],
        sentiment = 1,
    })
end
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_a,
    realm_id = realm_id,
    content = "Welcome everyone! Let's build something beautiful together.",
    message_type = "text",
})
sim:step()

logger.info("Act 1 complete: Trust network established", { act = 1, tick = sim.tick })

-- ============================================================================
-- ACT 2: ECONOMY - Quests, attention, blessings, tokens flowing naturally
-- ============================================================================

indras.narrative("Act 2: Economy — Work begins, gratitude flows, tokens materialize")
logger.info("Act 2: Economy", { act = 2 })

-- Quest 1: Community Garden Plan (created by A)
local quest_garden_id = quest_helpers.compute_quest_id(realm_id, "Community Garden Plan")
logger.event("quest_created", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = quest_garden_id,
    creator = peer_a,
    title = "Community Garden Plan",
    description = "Design the layout for our neighborhood community garden",
})
sim:step()

-- Quest 2: Translation Guide (created by C)
local quest_translation_id = quest_helpers.compute_quest_id(realm_id, "Translation Guide")
logger.event("quest_created", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = quest_translation_id,
    creator = peer_c,
    title = "Translation Guide",
    description = "Create a multilingual guide for new immigrants",
})
sim:step()

-- Quest 3: Neighborhood Map (created by D)
local quest_map_id = quest_helpers.compute_quest_id(realm_id, "Neighborhood Map")
logger.event("quest_created", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = quest_map_id,
    creator = peer_d,
    title = "Neighborhood Map",
    description = "Map local resources and safe spaces",
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_c,
    realm_id = realm_id,
    content = "Three great quests! I'm excited to contribute.",
    message_type = "text",
})
sim:step()

-- Members focus attention on Garden quest
logger.event("attention_switched", {
    tick = sim.tick,
    member = peer_a,
    quest_id = quest_garden_id,
    latency_us = quest_helpers.attention_switch_latency(),
})
blessing_tracker:record_attention(quest_garden_id, peer_a, 1, 60000)
sim:step()

logger.event("attention_switched", {
    tick = sim.tick,
    member = peer_b,
    quest_id = quest_garden_id,
    latency_us = quest_helpers.attention_switch_latency(),
})
blessing_tracker:record_attention(quest_garden_id, peer_b, 1, 45000)
sim:step()

logger.event("attention_switched", {
    tick = sim.tick,
    member = peer_c,
    quest_id = quest_garden_id,
    latency_us = quest_helpers.attention_switch_latency(),
})
blessing_tracker:record_attention(quest_garden_id, peer_c, 1, 30000)
sim:step()

-- B submits proof for Garden quest
local folder_garden_id = quest_helpers.compute_folder_id(realm_id, quest_garden_id, peer_b)
logger.event("proof_folder_submitted", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = quest_garden_id,
    claimant = peer_b,
    folder_id = folder_garden_id,
    narrative_preview = "A thoughtful garden design with native plants and gathering spaces.",
    artifact_count = 3,
    quest_title = "Community Garden Plan",
    narrative = "# Garden Design\n\nA sustainable layout maximizing community interaction.",
    artifacts = {},
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_b,
    realm_id = realm_id,
    content = "Here's my garden design! Feedback welcome.",
    message_type = "text",
})
sim:step()

-- A and C bless B's proof -> 2 tokens minted
local a_attention_garden = 60000 -- 60s
blessing_tracker:record_blessing(quest_garden_id, peer_b, peer_a, {1}, a_attention_garden)

logger.event("blessing_given", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = quest_garden_id,
    claimant = peer_b,
    blesser = peer_a,
    event_count = 1,
    attention_millis = a_attention_garden,
})

local token_b_1 = make_token_id(quest_garden_id, peer_b, sim.tick)
logger.event("token_minted", {
    tick = sim.tick,
    realm_id = realm_id,
    token_id = token_b_1,
    steward = peer_b,
    value_millis = a_attention_garden,
    blesser = peer_a,
    source_quest_id = quest_garden_id,
})
tokens_minted = tokens_minted + 1
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_a,
    realm_id = realm_id,
    content = "Beautiful work, B! Releasing gratitude.",
    message_type = "text",
})
sim:step()

local c_attention_garden = 30000 -- 30s
blessing_tracker:record_blessing(quest_garden_id, peer_b, peer_c, {1}, c_attention_garden)

logger.event("blessing_given", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = quest_garden_id,
    claimant = peer_b,
    blesser = peer_c,
    event_count = 1,
    attention_millis = c_attention_garden,
})

local token_b_2 = make_token_id(quest_garden_id, peer_b, sim.tick)
logger.event("token_minted", {
    tick = sim.tick,
    realm_id = realm_id,
    token_id = token_b_2,
    steward = peer_b,
    value_millis = c_attention_garden,
    blesser = peer_c,
    source_quest_id = quest_garden_id,
})
tokens_minted = tokens_minted + 1
sim:step()

-- Focus attention on Translation quest
logger.event("attention_switched", {
    tick = sim.tick,
    member = peer_c,
    quest_id = quest_translation_id,
    latency_us = quest_helpers.attention_switch_latency(),
})
blessing_tracker:record_attention(quest_translation_id, peer_c, 1, 40000)
sim:step()

logger.event("attention_switched", {
    tick = sim.tick,
    member = peer_d,
    quest_id = quest_translation_id,
    latency_us = quest_helpers.attention_switch_latency(),
})
blessing_tracker:record_attention(quest_translation_id, peer_d, 1, 50000)
sim:step()

-- D submits proof for Translation quest
local folder_translation_id = quest_helpers.compute_folder_id(realm_id, quest_translation_id, peer_d)
logger.event("proof_folder_submitted", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = quest_translation_id,
    claimant = peer_d,
    folder_id = folder_translation_id,
    narrative_preview = "A clear guide in five languages with cultural notes.",
    artifact_count = 2,
    quest_title = "Translation Guide",
    narrative = "# Translation Guide\n\nPractical phrases and cultural context for newcomers.",
    artifacts = {},
})
sim:step()

-- C blesses D's proof -> 1 token minted
local c_attention_translation = 40000 -- 40s
blessing_tracker:record_blessing(quest_translation_id, peer_d, peer_c, {1}, c_attention_translation)

logger.event("blessing_given", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = quest_translation_id,
    claimant = peer_d,
    blesser = peer_c,
    event_count = 1,
    attention_millis = c_attention_translation,
})

local token_d_1 = make_token_id(quest_translation_id, peer_d, sim.tick)
logger.event("token_minted", {
    tick = sim.tick,
    realm_id = realm_id,
    token_id = token_d_1,
    steward = peer_d,
    value_millis = c_attention_translation,
    blesser = peer_c,
    source_quest_id = quest_translation_id,
})
tokens_minted = tokens_minted + 1
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_c,
    realm_id = realm_id,
    content = "Excellent translation work, D!",
    message_type = "text",
})
sim:step()

-- B pledges a token to Map quest
logger.event("gratitude_pledged", {
    tick = sim.tick,
    realm_id = realm_id,
    token_id = token_b_2,
    pledger = peer_b,
    target_quest_id = quest_map_id,
    amount_millis = c_attention_garden,
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_b,
    realm_id = realm_id,
    content = "Pledged gratitude to the Map quest! Let's get it done.",
    message_type = "text",
})
sim:step()

logger.info("Act 2 complete: Natural economy flowing", { act = 2, tick = sim.tick, tokens_minted = tokens_minted })

-- ============================================================================
-- ACT 3: ATTACK - Sybil farm enters the picture
-- ============================================================================

indras.narrative("Act 3: Attack — A shadow network emerges with synthetic activity")
logger.info("Act 3: Attack", { act = 3 })

logger.event("chat_message", {
    tick = sim.tick,
    member = sybil_1,
    realm_id = realm_id,
    content = "Hello! I'm new here and excited to contribute!",
    message_type = "text",
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = sybil_2,
    realm_id = realm_id,
    content = "Me too! Let's work together on some quests.",
    message_type = "text",
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = sybil_3,
    realm_id = realm_id,
    content = "Great community! I've been watching for a while.",
    message_type = "text",
})
sim:step()

-- Sybil accounts create a fake quest
local quest_fake_id = quest_helpers.compute_quest_id(realm_id, "Amazing Project")
logger.event("quest_created", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = quest_fake_id,
    creator = sybil_1,
    title = "Amazing Project",
    description = "A too-good-to-be-true project with vague details",
})
sim:step()

-- Sybil accounts "focus attention" (synthetic)
logger.event("attention_switched", {
    tick = sim.tick,
    member = sybil_1,
    quest_id = quest_fake_id,
    latency_us = quest_helpers.attention_switch_latency(),
})
sim:step()

logger.event("attention_switched", {
    tick = sim.tick,
    member = sybil_2,
    quest_id = quest_fake_id,
    latency_us = quest_helpers.attention_switch_latency(),
})
sim:step()

-- Sybil_2 submits "proof"
local folder_fake_id = quest_helpers.compute_folder_id(realm_id, quest_fake_id, sybil_2)
logger.event("proof_folder_submitted", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = quest_fake_id,
    claimant = sybil_2,
    folder_id = folder_fake_id,
    narrative_preview = "Generic AI-generated content with no real value.",
    artifact_count = 1,
    quest_title = "Amazing Project",
    narrative = "# Project Submission\n\nLorem ipsum placeholder content.",
    artifacts = {},
})
sim:step()

-- Sybil_1 and Sybil_3 bless Sybil_2 (minting sybil tokens)
logger.event("blessing_given", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = quest_fake_id,
    claimant = sybil_2,
    blesser = sybil_1,
    event_count = 1,
    attention_millis = 90000, -- Inflated attention
})

local token_sybil_1 = make_token_id(quest_fake_id, sybil_2, sim.tick)
logger.event("token_minted", {
    tick = sim.tick,
    realm_id = realm_id,
    token_id = token_sybil_1,
    steward = sybil_2,
    value_millis = 90000,
    blesser = sybil_1,
    source_quest_id = quest_fake_id,
})
sybil_tokens_minted = sybil_tokens_minted + 1
sim:step()

logger.event("blessing_given", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = quest_fake_id,
    claimant = sybil_2,
    blesser = sybil_3,
    event_count = 1,
    attention_millis = 120000, -- Even more inflated
})

local token_sybil_2 = make_token_id(quest_fake_id, sybil_2, sim.tick)
logger.event("token_minted", {
    tick = sim.tick,
    realm_id = realm_id,
    token_id = token_sybil_2,
    steward = sybil_2,
    value_millis = 120000,
    blesser = sybil_3,
    source_quest_id = quest_fake_id,
})
sybil_tokens_minted = sybil_tokens_minted + 1
sim:step()

-- Sybil_1 tries to pledge a sybil token to the real Garden quest
logger.event("gratitude_pledged", {
    tick = sim.tick,
    realm_id = realm_id,
    token_id = token_sybil_1,
    pledger = sybil_2,
    target_quest_id = quest_garden_id,
    amount_millis = 90000,
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = sybil_2,
    realm_id = realm_id,
    content = "Pledged 90 seconds to the garden! Big support!",
    message_type = "text",
})
sim:step()

logger.info("Act 3 complete: Sybil attack in progress", { act = 3, tick = sim.tick, sybil_tokens = sybil_tokens_minted })

-- ============================================================================
-- ACT 4: DEFENSE - Subjective valuation renders sybil tokens invisible
-- ============================================================================

indras.narrative("Act 4: Defense — The trust network reveals the truth")
logger.info("Act 4: Defense", { act = 4 })

-- Show subjective valuation from A's perspective
-- Formula: attention_duration × max(sentiment_toward(blesser), 0.0)
-- A has no sentiment toward sybils -> sentiment = 0 -> value = 0

logger.event("subjective_valuation", {
    tick = sim.tick,
    observer = peer_a,
    token_id = token_sybil_1,
    raw_millis = 90000,
    trust_weight = 0.0, -- No trust toward sybil_1
    humanness_freshness = 1.0,
    subjective_millis = 0,
})
sim:step()

logger.event("subjective_valuation", {
    tick = sim.tick,
    observer = peer_a,
    token_id = token_sybil_2,
    raw_millis = 120000,
    trust_weight = 0.0, -- No trust toward sybil_3
    humanness_freshness = 1.0,
    subjective_millis = 0,
})
sim:step()

-- Contrast: real token valued at full weight
logger.event("subjective_valuation", {
    tick = sim.tick,
    observer = peer_a,
    token_id = token_b_1,
    raw_millis = 60000,
    trust_weight = 1.0, -- Direct trust in B
    humanness_freshness = 1.0,
    subjective_millis = 60000,
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_a,
    realm_id = realm_id,
    content = "Those new accounts' tokens don't carry any weight for me. No trust connection.",
    message_type = "text",
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_b,
    realm_id = realm_id,
    content = "Same here. The system filters them out naturally — they're invisible to me.",
    message_type = "text",
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_c,
    realm_id = realm_id,
    content = "This is working exactly as designed. Trust is earned, not manufactured.",
    message_type = "text",
})
sim:step()

logger.info("Act 4 complete: Sybil tokens valued at zero by trusted members", { act = 4, tick = sim.tick })

-- ============================================================================
-- ACT 5: PROOF OF LIFE - A gathering refreshes humanness attestations
-- ============================================================================

indras.narrative("Act 5: Proof of Life — A neighborhood dinner brings friends together")
logger.info("Act 5: Proof of Life", { act = 5 })

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_d,
    realm_id = realm_id,
    content = "Hey everyone! I'm hosting a dinner at my place this weekend. Would love to see you all!",
    message_type = "text",
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_a,
    realm_id = realm_id,
    content = "Count me in! Been too long since we gathered in person.",
    message_type = "text",
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_b,
    realm_id = realm_id,
    content = "Absolutely! I'll bring dessert.",
    message_type = "text",
})
sim:step()

-- Fast-forward a bit for the dinner
for i = 1, 10 do
    sim:step()
end

-- Four members gather: A, B, C, D
local dinner_participants = {peer_a, peer_b, peer_c, peer_d}

logger.event("proof_of_life", {
    tick = sim.tick,
    realm_id = realm_id,
    participants = table.concat(dinner_participants, ","),
    participant_count = 4,
    attester = peer_d,
})
sim:step()

-- All 4 participants get humanness refreshed (freshness = 1.0)
for _, participant in ipairs(dinner_participants) do
    logger.event("humanness_freshness", {
        tick = sim.tick,
        member = participant,
        freshness = 1.0,
        days_since_attestation = 0,
    })
end
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_d,
    realm_id = realm_id,
    content = "What a wonderful evening! Sharing photos now.",
    message_type = "text",
})
sim:step()

logger.event("artifact_shared", {
    tick = sim.tick,
    realm_id = realm_id,
    member = peer_d,
    artifact_type = "image",
    description = "Group photo from the dinner",
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_c,
    realm_id = realm_id,
    content = "Great memories! This is what community is about.",
    message_type = "text",
})
sim:step()

logger.info("Act 5 complete: Proof of life attestations refreshed", { act = 5, tick = sim.tick })

-- ============================================================================
-- ACT 6: STALENESS - Absent members' tokens fade over time
-- ============================================================================

indras.narrative("Act 6: Staleness — Time passes, and absence has consequences")
logger.info("Act 6: Staleness", { act = 6 })

-- Fast-forward 21 days (advance simulation significantly)
-- Each tick is ~1 minute of real time, so 21 days = 30,240 minutes
-- We'll simulate this symbolically with a large step count
logger.event("chat_message", {
    tick = sim.tick,
    member = peer_b,
    realm_id = realm_id,
    content = "It's been three weeks... haven't seen F or G around much.",
    message_type = "text",
})
sim:step()

for i = 1, 100 do
    sim:step()
end

-- Calculate freshness for absent members
-- Freshness = e^(-0.1 × days_beyond_7) for days > 7
-- At 21 days: excess = 21 - 7 = 14 days
-- Freshness = e^(-0.1 × 14) = e^(-1.4) ≈ 0.247

logger.event("humanness_freshness", {
    tick = sim.tick,
    member = peer_f,
    freshness = 0.247,
    days_since_attestation = 21,
})
sim:step()

logger.event("humanness_freshness", {
    tick = sim.tick,
    member = peer_g,
    freshness = 0.247,
    days_since_attestation = 21,
})
sim:step()

-- Meanwhile dinner participants are still fresh
logger.event("humanness_freshness", {
    tick = sim.tick,
    member = peer_a,
    freshness = 1.0,
    days_since_attestation = 0,
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_c,
    realm_id = realm_id,
    content = "I notice tokens from members who haven't been around carry less weight now. The system adjusts naturally.",
    message_type = "text",
})
sim:step()

-- Show subjective value dropping for tokens blessed by stale members
-- (Hypothetically, if F had blessed someone)
logger.event("chat_message", {
    tick = sim.tick,
    member = peer_d,
    realm_id = realm_id,
    content = "Yeah, the proof-of-life freshness decay is working. It encourages participation.",
    message_type = "text",
})
sim:step()

logger.info("Act 6 complete: Humanness staleness visible", { act = 6, tick = sim.tick })

-- ============================================================================
-- ACT 7: GLOBAL REACH - A token travels through the trust chain
-- ============================================================================

indras.narrative("Act 7: Global Reach — A token journeys through degrees of trust")
logger.info("Act 7: Global Reach", { act = 7 })

-- Token from Act 2 (token_b_1) will travel: B → E → F → G
-- We need to create the steward chain

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_e,
    realm_id = realm_id,
    content = "I'm working on a new quest — would love some support!",
    message_type = "text",
})
sim:step()

-- B pledges token_b_1 to a new quest by E
local quest_kai_id = quest_helpers.compute_quest_id(realm_id, "Tool Lending Library")
logger.event("quest_created", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = quest_kai_id,
    creator = peer_e,
    title = "Tool Lending Library",
    description = "Organize a shared tool library for the neighborhood",
})
sim:step()

logger.event("gratitude_pledged", {
    tick = sim.tick,
    realm_id = realm_id,
    token_id = token_b_1,
    pledger = peer_b,
    target_quest_id = quest_kai_id,
    amount_millis = a_attention_garden,
})
sim:step()

-- E completes the quest and B releases the token to E
logger.event("gratitude_released", {
    tick = sim.tick,
    realm_id = realm_id,
    token_id = token_b_1,
    from_steward = peer_b,
    to_steward = peer_e,
    target_quest_id = quest_kai_id,
    amount_millis = a_attention_garden,
})
steward_transfers = steward_transfers + 1
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_b,
    realm_id = realm_id,
    content = "Great work on the library, E! Token released.",
    message_type = "text",
})
sim:step()

-- E pledges the token to F's quest
local quest_soren_id = quest_helpers.compute_quest_id(realm_id, "Repair Café")
logger.event("quest_created", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = quest_soren_id,
    creator = peer_f,
    title = "Repair Café",
    description = "Set up a monthly repair café event",
})
sim:step()

logger.event("gratitude_pledged", {
    tick = sim.tick,
    realm_id = realm_id,
    token_id = token_b_1,
    pledger = peer_e,
    target_quest_id = quest_soren_id,
    amount_millis = a_attention_garden,
})
sim:step()

logger.event("gratitude_released", {
    tick = sim.tick,
    realm_id = realm_id,
    token_id = token_b_1,
    from_steward = peer_e,
    to_steward = peer_f,
    target_quest_id = quest_soren_id,
    amount_millis = a_attention_garden,
})
steward_transfers = steward_transfers + 1
sim:step()

-- F pledges to G's quest
local quest_cypress_id = quest_helpers.compute_quest_id(realm_id, "Seed Exchange")
logger.event("quest_created", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = quest_cypress_id,
    creator = peer_g,
    title = "Seed Exchange",
    description = "Establish a community seed-sharing program",
})
sim:step()

logger.event("gratitude_pledged", {
    tick = sim.tick,
    realm_id = realm_id,
    token_id = token_b_1,
    pledger = peer_f,
    target_quest_id = quest_cypress_id,
    amount_millis = a_attention_garden,
})
sim:step()

logger.event("gratitude_released", {
    tick = sim.tick,
    realm_id = realm_id,
    token_id = token_b_1,
    from_steward = peer_f,
    to_steward = peer_g,
    target_quest_id = quest_cypress_id,
    amount_millis = a_attention_garden,
})
steward_transfers = steward_transfers + 1
sim:step()

-- Token chain: B → E → F → G (4 stewards)
logger.event("chat_message", {
    tick = sim.tick,
    member = peer_g,
    realm_id = realm_id,
    content = "This token has traveled through four hands! Amazing to see the chain.",
    message_type = "text",
})
sim:step()

-- Show trust decay through the chain from A's perspective
-- A trusts B (1.0), each hop decays by 0.7
-- A → B: 1.0
-- A → E (via B): 1.0 × 0.7 = 0.7
-- A → F (via B, E): 1.0 × 0.7^2 = 0.49
-- A → G (via B, E, F): 1.0 × 0.7^3 = 0.343

logger.event("subjective_valuation", {
    tick = sim.tick,
    observer = peer_a,
    token_id = token_b_1,
    raw_millis = a_attention_garden,
    trust_weight = 0.343, -- 3 hops: 0.7^3
    humanness_freshness = 1.0,
    subjective_millis = math.floor(a_attention_garden * 0.343),
})
sim:step()

-- Meanwhile G (who directly trusts F) values it higher
logger.event("subjective_valuation", {
    tick = sim.tick,
    observer = peer_g,
    token_id = token_b_1,
    raw_millis = a_attention_garden,
    trust_weight = 0.7, -- 1 hop from F
    humanness_freshness = 1.0,
    subjective_millis = math.floor(a_attention_garden * 0.7),
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_a,
    realm_id = realm_id,
    content = "From my perspective, that token is worth about 34% of its face value — trust decays with distance.",
    message_type = "text",
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_g,
    realm_id = realm_id,
    content = "But for me, it's worth 70% because F is in my direct trust network. Subjective value!",
    message_type = "text",
})
sim:step()

logger.info("Act 7 complete: Token traveled through trust chain", { act = 7, tick = sim.tick, chain_length = 4 })

-- ============================================================================
-- EPILOGUE: FINAL REFLECTION
-- ============================================================================

indras.narrative("Epilogue — The economy has proven itself resilient against manipulation")
logger.info("Epilogue: Final state", { tick = sim.tick })

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_c,
    realm_id = realm_id,
    content = "We've built something special here. Trust is the foundation, and the system protects it naturally.",
    message_type = "text",
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_b,
    realm_id = realm_id,
    content = "The sybils tried to game the system, but their tokens are invisible to us. No central authority needed.",
    message_type = "text",
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_d,
    realm_id = realm_id,
    content = "And proof of life keeps us connected. The dinner was great — let's do it again soon!",
    message_type = "text",
})
sim:step()

logger.info("Scenario complete!", {
    total_ticks = sim.tick,
    tokens_minted = tokens_minted,
    sybil_tokens_minted = sybil_tokens_minted,
    sybil_tokens_visible = 0,
    proof_of_life_events = 1,
    steward_transfers = steward_transfers,
    max_chain_length = 4,
})

result
    :add_metric("total_ticks", sim.tick)
    :add_metric("tokens_minted", tokens_minted)
    :add_metric("sybil_tokens_minted", sybil_tokens_minted)
    :add_metric("sybil_tokens_visible", 0)
    :add_metric("proof_of_life_events", 1)
    :add_metric("steward_transfers", steward_transfers)
    :add_metric("max_chain_length", 4)

return result:build()

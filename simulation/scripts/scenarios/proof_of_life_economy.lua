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

-- Assign roles (futuristic names per CLAUDE.md)
local peer_zephyr = tostring(peers[1])
local peer_nova = tostring(peers[2])
local peer_sage = tostring(peers[3])
local peer_ember = tostring(peers[4])
local peer_kai = tostring(peers[5])
local peer_soren = tostring(peers[6])
local peer_cypress = tostring(peers[7])
local all_members = { peer_zephyr, peer_nova, peer_sage, peer_ember, peer_kai, peer_soren, peer_cypress }

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
    member = peer_zephyr,
    realm_id = realm_id,
    alias = "Proof of Life Economy",
})
sim:step()

-- Set positive sentiment between trusted pairs
-- Chain: Zephyr ↔ Nova ↔ Sage ↔ Ember ↔ Kai ↔ Soren ↔ Cypress
local trust_pairs = {
    {peer_zephyr, peer_nova},
    {peer_nova, peer_sage},
    {peer_sage, peer_ember},
    {peer_ember, peer_kai},
    {peer_kai, peer_soren},
    {peer_soren, peer_cypress},
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
    member = peer_zephyr,
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

-- Quest 1: Community Garden Plan (created by Zephyr)
local quest_garden_id = quest_helpers.compute_quest_id(realm_id, "Community Garden Plan")
logger.event("quest_created", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = quest_garden_id,
    creator = peer_zephyr,
    title = "Community Garden Plan",
    description = "Design the layout for our neighborhood community garden",
})
sim:step()

-- Quest 2: Translation Guide (created by Sage)
local quest_translation_id = quest_helpers.compute_quest_id(realm_id, "Translation Guide")
logger.event("quest_created", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = quest_translation_id,
    creator = peer_sage,
    title = "Translation Guide",
    description = "Create a multilingual guide for new immigrants",
})
sim:step()

-- Quest 3: Neighborhood Map (created by Ember)
local quest_map_id = quest_helpers.compute_quest_id(realm_id, "Neighborhood Map")
logger.event("quest_created", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = quest_map_id,
    creator = peer_ember,
    title = "Neighborhood Map",
    description = "Map local resources and safe spaces",
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_sage,
    realm_id = realm_id,
    content = "Three great quests! I'm excited to contribute.",
    message_type = "text",
})
sim:step()

-- Members focus attention on Garden quest
logger.event("attention_switched", {
    tick = sim.tick,
    member = peer_zephyr,
    quest_id = quest_garden_id,
    latency_us = quest_helpers.attention_switch_latency(),
})
blessing_tracker:record_attention(quest_garden_id, peer_zephyr, 1, 60000)
sim:step()

logger.event("attention_switched", {
    tick = sim.tick,
    member = peer_nova,
    quest_id = quest_garden_id,
    latency_us = quest_helpers.attention_switch_latency(),
})
blessing_tracker:record_attention(quest_garden_id, peer_nova, 1, 45000)
sim:step()

logger.event("attention_switched", {
    tick = sim.tick,
    member = peer_sage,
    quest_id = quest_garden_id,
    latency_us = quest_helpers.attention_switch_latency(),
})
blessing_tracker:record_attention(quest_garden_id, peer_sage, 1, 30000)
sim:step()

-- Nova submits proof for Garden quest
local folder_garden_id = quest_helpers.compute_folder_id(realm_id, quest_garden_id, peer_nova)
logger.event("proof_folder_submitted", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = quest_garden_id,
    claimant = peer_nova,
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
    member = peer_nova,
    realm_id = realm_id,
    content = "Here's my garden design! Feedback welcome.",
    message_type = "text",
})
sim:step()

-- Zephyr and Sage bless Nova's proof -> 2 tokens minted
local zephyr_attention_garden = 60000 -- 60s
blessing_tracker:record_blessing(quest_garden_id, peer_nova, peer_zephyr, {1}, zephyr_attention_garden)

logger.event("blessing_given", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = quest_garden_id,
    claimant = peer_nova,
    blesser = peer_zephyr,
    event_count = 1,
    attention_millis = zephyr_attention_garden,
})

local token_nova_1 = make_token_id(quest_garden_id, peer_nova, sim.tick)
logger.event("token_minted", {
    tick = sim.tick,
    realm_id = realm_id,
    token_id = token_nova_1,
    steward = peer_nova,
    value_millis = zephyr_attention_garden,
    blesser = peer_zephyr,
    source_quest_id = quest_garden_id,
})
tokens_minted = tokens_minted + 1
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_zephyr,
    realm_id = realm_id,
    content = "Beautiful work, Nova! Releasing gratitude.",
    message_type = "text",
})
sim:step()

local sage_attention_garden = 30000 -- 30s
blessing_tracker:record_blessing(quest_garden_id, peer_nova, peer_sage, {1}, sage_attention_garden)

logger.event("blessing_given", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = quest_garden_id,
    claimant = peer_nova,
    blesser = peer_sage,
    event_count = 1,
    attention_millis = sage_attention_garden,
})

local token_nova_2 = make_token_id(quest_garden_id, peer_nova, sim.tick)
logger.event("token_minted", {
    tick = sim.tick,
    realm_id = realm_id,
    token_id = token_nova_2,
    steward = peer_nova,
    value_millis = sage_attention_garden,
    blesser = peer_sage,
    source_quest_id = quest_garden_id,
})
tokens_minted = tokens_minted + 1
sim:step()

-- Focus attention on Translation quest
logger.event("attention_switched", {
    tick = sim.tick,
    member = peer_sage,
    quest_id = quest_translation_id,
    latency_us = quest_helpers.attention_switch_latency(),
})
blessing_tracker:record_attention(quest_translation_id, peer_sage, 1, 40000)
sim:step()

logger.event("attention_switched", {
    tick = sim.tick,
    member = peer_ember,
    quest_id = quest_translation_id,
    latency_us = quest_helpers.attention_switch_latency(),
})
blessing_tracker:record_attention(quest_translation_id, peer_ember, 1, 50000)
sim:step()

-- Ember submits proof for Translation quest
local folder_translation_id = quest_helpers.compute_folder_id(realm_id, quest_translation_id, peer_ember)
logger.event("proof_folder_submitted", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = quest_translation_id,
    claimant = peer_ember,
    folder_id = folder_translation_id,
    narrative_preview = "A clear guide in five languages with cultural notes.",
    artifact_count = 2,
    quest_title = "Translation Guide",
    narrative = "# Translation Guide\n\nPractical phrases and cultural context for newcomers.",
    artifacts = {},
})
sim:step()

-- Sage blesses Ember's proof -> 1 token minted
local sage_attention_translation = 40000 -- 40s
blessing_tracker:record_blessing(quest_translation_id, peer_ember, peer_sage, {1}, sage_attention_translation)

logger.event("blessing_given", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = quest_translation_id,
    claimant = peer_ember,
    blesser = peer_sage,
    event_count = 1,
    attention_millis = sage_attention_translation,
})

local token_ember_1 = make_token_id(quest_translation_id, peer_ember, sim.tick)
logger.event("token_minted", {
    tick = sim.tick,
    realm_id = realm_id,
    token_id = token_ember_1,
    steward = peer_ember,
    value_millis = sage_attention_translation,
    blesser = peer_sage,
    source_quest_id = quest_translation_id,
})
tokens_minted = tokens_minted + 1
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_sage,
    realm_id = realm_id,
    content = "Excellent translation work, Ember!",
    message_type = "text",
})
sim:step()

-- Nova pledges a token to Map quest
logger.event("gratitude_pledged", {
    tick = sim.tick,
    realm_id = realm_id,
    token_id = token_nova_2,
    pledger = peer_nova,
    target_quest_id = quest_map_id,
    amount_millis = sage_attention_garden,
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_nova,
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

-- Show subjective valuation from Zephyr's perspective
-- Formula: attention_duration × max(sentiment_toward(blesser), 0.0)
-- Zephyr has no sentiment toward sybils -> sentiment = 0 -> value = 0

logger.event("subjective_valuation", {
    tick = sim.tick,
    observer = peer_zephyr,
    token_id = token_sybil_1,
    raw_millis = 90000,
    trust_weight = 0.0, -- No trust toward sybil_1
    humanness_freshness = 1.0,
    subjective_millis = 0,
})
sim:step()

logger.event("subjective_valuation", {
    tick = sim.tick,
    observer = peer_zephyr,
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
    observer = peer_zephyr,
    token_id = token_nova_1,
    raw_millis = 60000,
    trust_weight = 1.0, -- Direct trust in Nova
    humanness_freshness = 1.0,
    subjective_millis = 60000,
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_zephyr,
    realm_id = realm_id,
    content = "Those new accounts' tokens don't carry any weight for me. No trust connection.",
    message_type = "text",
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_nova,
    realm_id = realm_id,
    content = "Same here. The system filters them out naturally — they're invisible to me.",
    message_type = "text",
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_sage,
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
    member = peer_ember,
    realm_id = realm_id,
    content = "Hey everyone! I'm hosting a dinner at my place this weekend. Would love to see you all!",
    message_type = "text",
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_zephyr,
    realm_id = realm_id,
    content = "Count me in! Been too long since we gathered in person.",
    message_type = "text",
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_nova,
    realm_id = realm_id,
    content = "Absolutely! I'll bring dessert.",
    message_type = "text",
})
sim:step()

-- Fast-forward a bit for the dinner
for i = 1, 10 do
    sim:step()
end

-- Four members gather: Zephyr, Nova, Sage, Ember
local dinner_participants = {peer_zephyr, peer_nova, peer_sage, peer_ember}

logger.event("proof_of_life", {
    tick = sim.tick,
    realm_id = realm_id,
    participants = table.concat(dinner_participants, ","),
    participant_count = 4,
    attester = peer_ember,
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
    member = peer_ember,
    realm_id = realm_id,
    content = "What a wonderful evening! Sharing photos now.",
    message_type = "text",
})
sim:step()

logger.event("artifact_shared", {
    tick = sim.tick,
    realm_id = realm_id,
    member = peer_ember,
    artifact_type = "image",
    description = "Group photo from the dinner",
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_sage,
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
    member = peer_nova,
    realm_id = realm_id,
    content = "It's been three weeks... haven't seen Soren or Cypress around much.",
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
    member = peer_soren,
    freshness = 0.247,
    days_since_attestation = 21,
})
sim:step()

logger.event("humanness_freshness", {
    tick = sim.tick,
    member = peer_cypress,
    freshness = 0.247,
    days_since_attestation = 21,
})
sim:step()

-- Meanwhile dinner participants are still fresh
logger.event("humanness_freshness", {
    tick = sim.tick,
    member = peer_zephyr,
    freshness = 1.0,
    days_since_attestation = 0,
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_sage,
    realm_id = realm_id,
    content = "I notice tokens from members who haven't been around carry less weight now. The system adjusts naturally.",
    message_type = "text",
})
sim:step()

-- Show subjective value dropping for tokens blessed by stale members
-- (Hypothetically, if Soren had blessed someone)
logger.event("chat_message", {
    tick = sim.tick,
    member = peer_ember,
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

-- Token from Act 2 (token_nova_1) will travel: Nova → Kai → Soren → Cypress
-- We need to create the steward chain

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_kai,
    realm_id = realm_id,
    content = "I'm working on a new quest — would love some support!",
    message_type = "text",
})
sim:step()

-- Nova pledges token_nova_1 to a new quest by Kai
local quest_kai_id = quest_helpers.compute_quest_id(realm_id, "Tool Lending Library")
logger.event("quest_created", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = quest_kai_id,
    creator = peer_kai,
    title = "Tool Lending Library",
    description = "Organize a shared tool library for the neighborhood",
})
sim:step()

logger.event("gratitude_pledged", {
    tick = sim.tick,
    realm_id = realm_id,
    token_id = token_nova_1,
    pledger = peer_nova,
    target_quest_id = quest_kai_id,
    amount_millis = zephyr_attention_garden,
})
sim:step()

-- Kai completes the quest and Nova releases the token to Kai
logger.event("gratitude_released", {
    tick = sim.tick,
    realm_id = realm_id,
    token_id = token_nova_1,
    from_steward = peer_nova,
    to_steward = peer_kai,
    target_quest_id = quest_kai_id,
    amount_millis = zephyr_attention_garden,
})
steward_transfers = steward_transfers + 1
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_nova,
    realm_id = realm_id,
    content = "Great work on the library, Kai! Token released.",
    message_type = "text",
})
sim:step()

-- Kai pledges the token to Soren's quest
local quest_soren_id = quest_helpers.compute_quest_id(realm_id, "Repair Café")
logger.event("quest_created", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = quest_soren_id,
    creator = peer_soren,
    title = "Repair Café",
    description = "Set up a monthly repair café event",
})
sim:step()

logger.event("gratitude_pledged", {
    tick = sim.tick,
    realm_id = realm_id,
    token_id = token_nova_1,
    pledger = peer_kai,
    target_quest_id = quest_soren_id,
    amount_millis = zephyr_attention_garden,
})
sim:step()

logger.event("gratitude_released", {
    tick = sim.tick,
    realm_id = realm_id,
    token_id = token_nova_1,
    from_steward = peer_kai,
    to_steward = peer_soren,
    target_quest_id = quest_soren_id,
    amount_millis = zephyr_attention_garden,
})
steward_transfers = steward_transfers + 1
sim:step()

-- Soren pledges to Cypress's quest
local quest_cypress_id = quest_helpers.compute_quest_id(realm_id, "Seed Exchange")
logger.event("quest_created", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = quest_cypress_id,
    creator = peer_cypress,
    title = "Seed Exchange",
    description = "Establish a community seed-sharing program",
})
sim:step()

logger.event("gratitude_pledged", {
    tick = sim.tick,
    realm_id = realm_id,
    token_id = token_nova_1,
    pledger = peer_soren,
    target_quest_id = quest_cypress_id,
    amount_millis = zephyr_attention_garden,
})
sim:step()

logger.event("gratitude_released", {
    tick = sim.tick,
    realm_id = realm_id,
    token_id = token_nova_1,
    from_steward = peer_soren,
    to_steward = peer_cypress,
    target_quest_id = quest_cypress_id,
    amount_millis = zephyr_attention_garden,
})
steward_transfers = steward_transfers + 1
sim:step()

-- Token chain: Nova → Kai → Soren → Cypress (4 stewards)
logger.event("chat_message", {
    tick = sim.tick,
    member = peer_cypress,
    realm_id = realm_id,
    content = "This token has traveled through four hands! Amazing to see the chain.",
    message_type = "text",
})
sim:step()

-- Show trust decay through the chain from Zephyr's perspective
-- Zephyr trusts Nova (1.0), each hop decays by 0.7
-- Zephyr → Nova: 1.0
-- Zephyr → Kai (via Nova): 1.0 × 0.7 = 0.7
-- Zephyr → Soren (via Nova, Kai): 1.0 × 0.7^2 = 0.49
-- Zephyr → Cypress (via Nova, Kai, Soren): 1.0 × 0.7^3 = 0.343

logger.event("subjective_valuation", {
    tick = sim.tick,
    observer = peer_zephyr,
    token_id = token_nova_1,
    raw_millis = zephyr_attention_garden,
    trust_weight = 0.343, -- 3 hops: 0.7^3
    humanness_freshness = 1.0,
    subjective_millis = math.floor(zephyr_attention_garden * 0.343),
})
sim:step()

-- Meanwhile Cypress (who directly trusts Soren) values it higher
logger.event("subjective_valuation", {
    tick = sim.tick,
    observer = peer_cypress,
    token_id = token_nova_1,
    raw_millis = zephyr_attention_garden,
    trust_weight = 0.7, -- 1 hop from Soren
    humanness_freshness = 1.0,
    subjective_millis = math.floor(zephyr_attention_garden * 0.7),
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_zephyr,
    realm_id = realm_id,
    content = "From my perspective, that token is worth about 34% of its face value — trust decays with distance.",
    message_type = "text",
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_cypress,
    realm_id = realm_id,
    content = "But for me, it's worth 70% because Soren is in my direct trust network. Subjective value!",
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
    member = peer_sage,
    realm_id = realm_id,
    content = "We've built something special here. Trust is the foundation, and the system protects it naturally.",
    message_type = "text",
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_nova,
    realm_id = realm_id,
    content = "The sybils tried to game the system, but their tokens are invisible to us. No central authority needed.",
    message_type = "text",
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_ember,
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

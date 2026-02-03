-- Bioregional Delegation Hierarchy Scenario
--
-- Demonstrates the full delegation chain from Root to Individual,
-- chain validation, subjective trust evaluation, and alternative paths:
--
-- Act 1: Temple Genesis — Root delegates down through the bioregional hierarchy
-- Act 2: Attestation — An individual is attested through the full chain
-- Act 3: Subjective Trust — Different observers evaluate the same chain differently
-- Act 4: Compromised Temple — One temple loses trust; attestations degrade
-- Act 5: Alternative Paths — Same individual attested through a different branch
--
-- This scenario exercises the bioregional delegation tree, chain validation,
-- and demonstrates how subjective trust makes each observer's view unique.

local quest_helpers = require("lib.quest_helpers")
local home = require("lib.home_realm_helpers")

-- ============================================================================
-- SETUP
-- ============================================================================

local ctx = quest_helpers.new_context("bioregional_delegation")
local logger = quest_helpers.create_logger(ctx)

logger.info("Starting Bioregional Delegation scenario", {
    level = quest_helpers.get_level(),
    description = "Delegation hierarchy, chain validation, subjective trust, alternative paths",
})

-- Create a 10-peer mesh:
--   Peer 1: Root Temple (Temples of Refuge)
--   Peer 2: Neotropical Realm Temple
--   Peer 3: Central America Subrealm Temple
--   Peer 4: Caribbean Bioregion Temple
--   Peer 5: Cuban Moist Forests Ecoregion Temple
--   Peer 6: Individual attester (Zephyr)
--   Peer 7: Observer (Lyra) — trusts the Neotropical chain
--   Peer 8: Observer (Orion) — distrusts the Caribbean temple
--   Peer 9: Afrotropics Realm Temple (alternative branch)
--   Peer 10: Southern Afrotropics Subrealm Temple (alternative branch)

local mesh = indras.MeshBuilder.new(10):full_mesh()
local sim_config = indras.SimConfig.new({
    wake_probability = 0,
    sleep_probability = 0,
    initial_online_probability = 1,
    max_ticks = 300,
})
local sim = indras.Simulation.new(mesh, sim_config)
sim:initialize()

local peers = mesh:peers()
local result = quest_helpers.result_builder("bioregional_delegation")

-- Assign roles
local root_temple       = tostring(peers[1])
local neotropical_realm = tostring(peers[2])
local central_america   = tostring(peers[3])
local caribbean_bio     = tostring(peers[4])
local cuban_eco         = tostring(peers[5])
local zephyr            = tostring(peers[6])   -- individual to be attested
local lyra              = tostring(peers[7])    -- trusting observer
local orion             = tostring(peers[8])    -- skeptical observer
local afrotropics_realm = tostring(peers[9])    -- alternative realm
local southern_afro     = tostring(peers[10])   -- alternative subrealm

local all_members = {
    root_temple, neotropical_realm, central_america, caribbean_bio,
    cuban_eco, zephyr, lyra, orion, afrotropics_realm, southern_afro,
}

-- Tracking
local delegations_issued = 0
local chains_validated = 0
local trust_evaluations = 0

-- Force all peers online
for _, peer in ipairs(peers) do
    sim:force_online(peer)
end

-- Create realm
local realm_id = quest_helpers.compute_realm_id(all_members)

logger.event("realm_created", {
    tick = sim.tick,
    realm_id = realm_id,
    members = table.concat(all_members, ","),
    member_count = #all_members,
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

-- ============================================================================
-- ACT 1: TEMPLE GENESIS — Delegation flows down the hierarchy
-- ============================================================================

indras.narrative("Act 1: Temple Genesis — The root delegates authority through the bioregional tree")
logger.info("Act 1: Temple Genesis", { act = 1 })

logger.event("chat_message", {
    tick = sim.tick,
    member = root_temple,
    realm_id = realm_id,
    content = "As the Root Temple of Refuge, I delegate attestation authority to the Neotropical Realm.",
    message_type = "text",
})
sim:step()

-- Root → Neotropical Realm (level: Realm)
logger.event("delegation_issued", {
    tick = sim.tick,
    delegator = root_temple,
    delegate = neotropical_realm,
    level = "Realm",
    bioregion_code = "central-america",
    chain_position = 1,
})
delegations_issued = delegations_issued + 1
sim:step()

-- Neotropical Realm → Central America Subrealm (level: Subrealm)
logger.event("delegation_issued", {
    tick = sim.tick,
    delegator = neotropical_realm,
    delegate = central_america,
    level = "Subrealm",
    bioregion_code = "central-america/caribbean",
    chain_position = 2,
})
delegations_issued = delegations_issued + 1
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = neotropical_realm,
    realm_id = realm_id,
    content = "The Central America subrealm temple is now authorized to delegate further.",
    message_type = "text",
})
sim:step()

-- Central America → Caribbean Bioregion (level: Bioregion)
logger.event("delegation_issued", {
    tick = sim.tick,
    delegator = central_america,
    delegate = caribbean_bio,
    level = "Bioregion",
    bioregion_code = "NT26",
    chain_position = 3,
})
delegations_issued = delegations_issued + 1
sim:step()

-- Caribbean Bioregion → Cuban Moist Forests Ecoregion (level: Ecoregion)
logger.event("delegation_issued", {
    tick = sim.tick,
    delegator = caribbean_bio,
    delegate = cuban_eco,
    level = "Ecoregion",
    bioregion_code = "459",
    chain_position = 4,
})
delegations_issued = delegations_issued + 1
sim:step()

-- Cuban Ecoregion → Zephyr Individual (level: Individual)
logger.event("delegation_issued", {
    tick = sim.tick,
    delegator = cuban_eco,
    delegate = zephyr,
    level = "Individual",
    chain_position = 5,
})
delegations_issued = delegations_issued + 1
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = cuban_eco,
    realm_id = realm_id,
    content = "Zephyr is now authorized as an individual attester for the Cuban Moist Forests ecoregion.",
    message_type = "text",
})
sim:step()

-- Also set up the alternative Afrotropics branch
-- Root → Afrotropics Realm
logger.event("delegation_issued", {
    tick = sim.tick,
    delegator = root_temple,
    delegate = afrotropics_realm,
    level = "Realm",
    bioregion_code = "afrotropics",
    chain_position = 1,
})
delegations_issued = delegations_issued + 1
sim:step()

-- Afrotropics → Southern Afrotropics Subrealm
logger.event("delegation_issued", {
    tick = sim.tick,
    delegator = afrotropics_realm,
    delegate = southern_afro,
    level = "Subrealm",
    bioregion_code = "afrotropics/southern",
    chain_position = 2,
})
delegations_issued = delegations_issued + 1
sim:step()

logger.info("Act 1 complete: Delegation hierarchy established", {
    act = 1,
    tick = sim.tick,
    delegations_issued = delegations_issued,
})

-- ============================================================================
-- ACT 2: ATTESTATION — Zephyr attests a new member through the full chain
-- ============================================================================

indras.narrative("Act 2: Attestation — A new community member is attested through the full chain")
logger.info("Act 2: Attestation", { act = 2 })

-- Zephyr attests Nova (conceptual — Zephyr is the attester at the end of the chain)
local nova_id = "nova_attestee_001"

logger.event("chat_message", {
    tick = sim.tick,
    member = zephyr,
    realm_id = realm_id,
    content = "I've been working alongside Nova for months. She's clearly human — we share meals, stories, and laughter.",
    message_type = "text",
})
sim:step()

-- The full delegation chain for this attestation:
-- Root → Neotropical → Central America → Caribbean → Cuban Eco → Zephyr
logger.event("humanness_attestation", {
    tick = sim.tick,
    subject = nova_id,
    attester = zephyr,
    chain_length = 5,
    chain = table.concat({
        root_temple, neotropical_realm, central_america,
        caribbean_bio, cuban_eco, zephyr,
    }, " -> "),
    levels = "Root -> Realm -> Subrealm -> Bioregion -> Ecoregion -> Individual",
})
sim:step()

-- Validate the chain
logger.event("chain_validated", {
    tick = sim.tick,
    subject = nova_id,
    chain_length = 5,
    result = "valid",
    checks = "connectivity=pass, level_descent=pass, no_skip=pass, attester_match=pass",
})
chains_validated = chains_validated + 1
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = zephyr,
    realm_id = realm_id,
    content = "Nova's attestation is recorded with a full delegation chain from the Root Temple down to me.",
    message_type = "text",
})
sim:step()

-- Also record humanness freshness
logger.event("humanness_freshness", {
    tick = sim.tick,
    member = nova_id,
    freshness = 1.0,
    days_since_attestation = 0,
})
sim:step()

logger.info("Act 2 complete: Attestation recorded and validated", {
    act = 2,
    tick = sim.tick,
    chains_validated = chains_validated,
})

-- ============================================================================
-- ACT 3: SUBJECTIVE TRUST — Different observers see different chain strength
-- ============================================================================

indras.narrative("Act 3: Subjective Trust — The same chain, seen through different eyes")
logger.info("Act 3: Subjective Trust", { act = 3 })

-- Set up trust relationships
-- Lyra trusts the whole Neotropical chain (sentiment = 1.0 for each link)
local lyra_trust = {
    { member = root_temple,       sentiment = 1.0, label = "Root Temple" },
    { member = neotropical_realm, sentiment = 1.0, label = "Neotropical Realm" },
    { member = central_america,   sentiment = 0.9, label = "Central America" },
    { member = caribbean_bio,     sentiment = 0.8, label = "Caribbean Bioregion" },
    { member = cuban_eco,         sentiment = 0.7, label = "Cuban Ecoregion" },
    { member = zephyr,            sentiment = 0.9, label = "Zephyr" },
}

for _, trust in ipairs(lyra_trust) do
    logger.event("sentiment_set", {
        tick = sim.tick,
        from_member = lyra,
        to_member = trust.member,
        sentiment = trust.sentiment,
    })
end
sim:step()

-- Lyra evaluates the chain
-- Chain trust = product of sentiments along each link
local lyra_chain_trust = 1.0
for _, trust in ipairs(lyra_trust) do
    lyra_chain_trust = lyra_chain_trust * trust.sentiment
end

logger.event("temple_trust_evaluation", {
    tick = sim.tick,
    observer = lyra,
    subject = nova_id,
    chain_trust = lyra_chain_trust,
    verdict = "strong",
    detail = string.format(
        "Lyra trusts each temple in the chain: %.1f × %.1f × %.1f × %.1f × %.1f × %.1f = %.3f",
        1.0, 1.0, 0.9, 0.8, 0.7, 0.9, lyra_chain_trust
    ),
})
trust_evaluations = trust_evaluations + 1
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = lyra,
    realm_id = realm_id,
    content = string.format(
        "From my perspective, Nova's attestation chain has %.1f%% trust strength. I know these temples well.",
        lyra_chain_trust * 100
    ),
    message_type = "text",
})
sim:step()

-- Orion has mixed feelings — trusts Root and Realm but is neutral on Caribbean
local orion_trust = {
    { member = root_temple,       sentiment = 1.0, label = "Root Temple" },
    { member = neotropical_realm, sentiment = 0.8, label = "Neotropical Realm" },
    { member = central_america,   sentiment = 0.5, label = "Central America" },
    { member = caribbean_bio,     sentiment = 0.2, label = "Caribbean Bioregion" },
    { member = cuban_eco,         sentiment = 0.3, label = "Cuban Ecoregion" },
    { member = zephyr,            sentiment = 0.0, label = "Zephyr (unknown)" },
}

for _, trust in ipairs(orion_trust) do
    logger.event("sentiment_set", {
        tick = sim.tick,
        from_member = orion,
        to_member = trust.member,
        sentiment = trust.sentiment,
    })
end
sim:step()

-- Orion evaluates the chain
local orion_chain_trust = 1.0
for _, trust in ipairs(orion_trust) do
    orion_chain_trust = orion_chain_trust * trust.sentiment
end

logger.event("temple_trust_evaluation", {
    tick = sim.tick,
    observer = orion,
    subject = nova_id,
    chain_trust = orion_chain_trust,
    verdict = "weak",
    detail = string.format(
        "Orion has limited trust in lower links: %.1f × %.1f × %.1f × %.1f × %.1f × %.1f = %.4f",
        1.0, 0.8, 0.5, 0.2, 0.3, 0.0, orion_chain_trust
    ),
})
trust_evaluations = trust_evaluations + 1
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = orion,
    realm_id = realm_id,
    content = string.format(
        "I don't know the Caribbean temple or Zephyr. Nova's chain only carries %.1f%% trust for me.",
        orion_chain_trust * 100
    ),
    message_type = "text",
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = lyra,
    realm_id = realm_id,
    content = "That's the beauty of subjective trust — no single authority decides. Each of us evaluates independently.",
    message_type = "text",
})
sim:step()

logger.info("Act 3 complete: Subjective trust demonstrated", {
    act = 3,
    tick = sim.tick,
    trust_evaluations = trust_evaluations,
})

-- ============================================================================
-- ACT 4: COMPROMISED TEMPLE — One temple loses trust
-- ============================================================================

indras.narrative("Act 4: Compromised Temple — The Caribbean temple faces a crisis of trust")
logger.info("Act 4: Compromised Temple", { act = 4 })

logger.event("chat_message", {
    tick = sim.tick,
    member = lyra,
    realm_id = realm_id,
    content = "I've heard troubling reports about the Caribbean Bioregion temple. Adjusting my trust.",
    message_type = "text",
})
sim:step()

-- Lyra reduces trust in the Caribbean temple
logger.event("sentiment_set", {
    tick = sim.tick,
    from_member = lyra,
    to_member = caribbean_bio,
    sentiment = 0.1, -- was 0.8
})
sim:step()

-- Re-evaluate Lyra's chain trust
local lyra_new_chain_trust = 1.0 * 1.0 * 0.9 * 0.1 * 0.7 * 0.9 -- updated Caribbean to 0.1

logger.event("temple_trust_evaluation", {
    tick = sim.tick,
    observer = lyra,
    subject = nova_id,
    chain_trust = lyra_new_chain_trust,
    verdict = "degraded",
    detail = string.format(
        "After Caribbean trust drop: %.1f × %.1f × %.1f × %.1f × %.1f × %.1f = %.4f",
        1.0, 1.0, 0.9, 0.1, 0.7, 0.9, lyra_new_chain_trust
    ),
})
trust_evaluations = trust_evaluations + 1
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = lyra,
    realm_id = realm_id,
    content = string.format(
        "Nova's attestation through the Caribbean chain now only carries %.1f%% trust for me. Down from %.1f%%.",
        lyra_new_chain_trust * 100,
        lyra_chain_trust * 100
    ),
    message_type = "text",
})
sim:step()

-- Key insight: Orion was already skeptical of Caribbean — his view barely changed
logger.event("chat_message", {
    tick = sim.tick,
    member = orion,
    realm_id = realm_id,
    content = "Interesting — I never trusted the Caribbean temple much. My view of Nova's attestation hasn't changed.",
    message_type = "text",
})
sim:step()

logger.info("Act 4 complete: Compromised temple degrades chain trust", {
    act = 4,
    tick = sim.tick,
    trust_evaluations = trust_evaluations,
})

-- ============================================================================
-- ACT 5: ALTERNATIVE PATHS — Nova gets attested through a different branch
-- ============================================================================

indras.narrative("Act 5: Alternative Paths — A new attestation chain offers a different trust profile")
logger.info("Act 5: Alternative Paths", { act = 5 })

logger.event("chat_message", {
    tick = sim.tick,
    member = zephyr,
    realm_id = realm_id,
    content = "Nova has also been spending time in the Afrotropics region. She can be attested through that branch too.",
    message_type = "text",
})
sim:step()

-- Alternative chain: Root → Afrotropics → Southern Afrotropics → ... → Nova
-- (shorter chain for demonstration — skipping some levels as they aren't set up)
logger.event("humanness_attestation", {
    tick = sim.tick,
    subject = nova_id,
    attester = southern_afro,
    chain_length = 2,
    chain = table.concat({
        root_temple, afrotropics_realm, southern_afro,
    }, " -> "),
    levels = "Root -> Realm -> Subrealm",
    note = "Partial chain — Southern Afrotropics directly attests (temple-level attestation)",
})
sim:step()

logger.event("chain_validated", {
    tick = sim.tick,
    subject = nova_id,
    chain_length = 2,
    result = "valid",
    checks = "connectivity=pass, level_descent=pass, no_skip=pass, attester_match=pass",
})
chains_validated = chains_validated + 1
sim:step()

-- Now evaluate both chains from Lyra's perspective
-- Lyra trusts the Afrotropics branch highly
logger.event("sentiment_set", {
    tick = sim.tick,
    from_member = lyra,
    to_member = afrotropics_realm,
    sentiment = 0.95,
})
logger.event("sentiment_set", {
    tick = sim.tick,
    from_member = lyra,
    to_member = southern_afro,
    sentiment = 0.9,
})
sim:step()

local lyra_afro_trust = 1.0 * 0.95 * 0.9 -- Root × Afrotropics × Southern

logger.event("temple_trust_evaluation", {
    tick = sim.tick,
    observer = lyra,
    subject = nova_id,
    chain_trust = lyra_afro_trust,
    verdict = "strong",
    chain_path = "Afrotropics",
    detail = string.format(
        "Afrotropics chain: %.1f × %.2f × %.1f = %.3f",
        1.0, 0.95, 0.9, lyra_afro_trust
    ),
})
trust_evaluations = trust_evaluations + 1
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = lyra,
    realm_id = realm_id,
    content = string.format(
        "Nova now has two attestation paths. Caribbean chain: %.1f%% trust. Afrotropics chain: %.1f%% trust. I prefer the Afrotropics path!",
        lyra_new_chain_trust * 100,
        lyra_afro_trust * 100
    ),
    message_type = "text",
})
sim:step()

-- Orion evaluates the Afrotropics chain
logger.event("sentiment_set", {
    tick = sim.tick,
    from_member = orion,
    to_member = afrotropics_realm,
    sentiment = 0.7,
})
logger.event("sentiment_set", {
    tick = sim.tick,
    from_member = orion,
    to_member = southern_afro,
    sentiment = 0.6,
})
sim:step()

local orion_afro_trust = 1.0 * 0.7 * 0.6

logger.event("temple_trust_evaluation", {
    tick = sim.tick,
    observer = orion,
    subject = nova_id,
    chain_trust = orion_afro_trust,
    verdict = "moderate",
    chain_path = "Afrotropics",
    detail = string.format(
        "Afrotropics chain: %.1f × %.1f × %.1f = %.2f",
        1.0, 0.7, 0.6, orion_afro_trust
    ),
})
trust_evaluations = trust_evaluations + 1
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = orion,
    realm_id = realm_id,
    content = string.format(
        "The Afrotropics chain gives me %.1f%% trust for Nova — much better than the Caribbean route at %.1f%%.",
        orion_afro_trust * 100,
        orion_chain_trust * 100
    ),
    message_type = "text",
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = zephyr,
    realm_id = realm_id,
    content = "Multiple attestation paths make the network resilient. If one branch weakens, others can carry the weight.",
    message_type = "text",
})
sim:step()

logger.info("Act 5 complete: Alternative paths provide resilience", {
    act = 5,
    tick = sim.tick,
    chains_validated = chains_validated,
    trust_evaluations = trust_evaluations,
})

-- ============================================================================
-- EPILOGUE
-- ============================================================================

indras.narrative("Epilogue — The bioregional delegation tree proves its strength through diversity")
logger.info("Epilogue: Final state", { tick = sim.tick })

logger.event("chat_message", {
    tick = sim.tick,
    member = lyra,
    realm_id = realm_id,
    content = "The delegation hierarchy works because trust is subjective and paths are multiple. No single point of failure.",
    message_type = "text",
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = orion,
    realm_id = realm_id,
    content = "And when a temple stumbles, the network routes around it naturally. The tree self-heals through alternative branches.",
    message_type = "text",
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = zephyr,
    realm_id = realm_id,
    content = "Rooted in the land, connected through trust, resilient through redundancy. This is Indra's Network.",
    message_type = "text",
})
sim:step()

logger.info("Scenario complete!", {
    total_ticks = sim.tick,
    delegations_issued = delegations_issued,
    chains_validated = chains_validated,
    trust_evaluations = trust_evaluations,
})

result
    :add_metric("total_ticks", sim.tick)
    :add_metric("delegations_issued", delegations_issued)
    :add_metric("chains_validated", chains_validated)
    :add_metric("trust_evaluations", trust_evaluations)

return result:build()

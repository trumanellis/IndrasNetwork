-- SyncEngine Immune Response Simulation
--
-- Demonstrates the full sentiment/trust/blocking lifecycle — the "immune
-- system" of Indra's Network — through a five-character story.
--
-- Cast:
--   A — first to detect the threat
--   B — quickly corroborates A's warning
--   C — receives relayed sentiment signals and acts on them
--   D — the bad actor who gets progressively isolated
--   E — an innocent bystander connected to D
--
-- Phases map to the immune system analogy:
--   1. Genesis          (healthy body)
--   2. Infection         (pathogen appears)
--   3. Detection         (innate immunity)
--   4. Signal Propagation (cytokine cascade)
--   5. Graduated Response (adaptive immunity)
--   6. Cascade           (inflammation)
--   7. Recovery          (homeostasis)
--
-- Usage:
--   STRESS_LEVEL=quick cargo run --bin lua_runner -- scenarios/sync_engine_immune_response.lua
--   ... | cargo run -p indras-realm-viewer --bin omni-viewer

local quest_helpers = require("lib.quest_helpers")
local immune = require("lib.immune_helpers")

-- ============================================================================
-- SETUP
-- ============================================================================

local ctx = quest_helpers.new_context("sync_engine_immune_response")
local logger = quest_helpers.create_logger(ctx)
local config = immune.get_config()

logger.info("Starting immune response simulation", {
    level = quest_helpers.get_level(),
    members = #immune.ALL_MEMBERS,
    ticks = config.ticks,
    spam_count = config.spam_count,
})

-- Create mesh and simulation
local mesh = indras.MeshBuilder.new(#immune.ALL_MEMBERS):full_mesh()
local sim_config = indras.SimConfig.new({
    wake_probability = 0,
    sleep_probability = 0,
    initial_online_probability = 1,
    max_ticks = config.ticks,
})
local sim = indras.Simulation.new(mesh, sim_config)
sim:initialize()

local result = quest_helpers.result_builder("sync_engine_immune_response")

-- Bring all peers online
local peers = mesh:peers()
for _, peer in ipairs(peers) do
    sim:force_online(peer)
end

-- Map peer indices to character names
local Z = immune.A
local N = immune.B
local S = immune.C
local O = immune.D
local L = immune.E

-- Track state locally for assertions
local contacts = {}   -- member -> set of contacts
local realms = {}     -- realm_id -> { members }
local blocked = {}    -- member -> set of blocked
local sentiments = {} -- member -> { contact -> sentiment }

for _, m in ipairs(immune.ALL_MEMBERS) do
    contacts[m] = {}
    blocked[m] = {}
    sentiments[m] = {}
end

--- Add a contact relationship (one-way) and emit event
local function add_contact(member, contact)
    contacts[member][contact] = true
    immune.add_contact(logger, sim.tick, member, contact)
end

--- Create a peer-set realm and emit events
local function create_realm(members_list)
    local peers_sorted = quest_helpers.normalize_peers(members_list)
    local realm_id = quest_helpers.compute_realm_id(peers_sorted)
    realms[realm_id] = {}
    for _, m in ipairs(peers_sorted) do
        realms[realm_id][m] = true
    end
    immune.create_realm(logger, sim.tick, realm_id, peers_sorted, #peers_sorted)
    for _, m in ipairs(peers_sorted) do
        immune.join_realm(logger, sim.tick, realm_id, m)
    end
    return realm_id
end

--- Find all realms that contain a specific member
local function realms_containing(member)
    local result_realms = {}
    for realm_id, members_set in pairs(realms) do
        if members_set[member] then
            table.insert(result_realms, realm_id)
        end
    end
    return result_realms
end

--- Find all realms shared between two members
local function shared_realms(a, b)
    local result_realms = {}
    for realm_id, members_set in pairs(realms) do
        if members_set[a] and members_set[b] then
            table.insert(result_realms, realm_id)
        end
    end
    return result_realms
end

--- Block a contact: remove from contacts, leave shared realms, emit events
local function block_contact(member, contact)
    -- Find realms to leave (realms where BOTH are members)
    local realms_to_leave = shared_realms(member, contact)
    local realm_ids_left = {}

    for _, realm_id in ipairs(realms_to_leave) do
        table.insert(realm_ids_left, realm_id)
    end

    -- Emit the block event
    immune.block_contact(logger, sim.tick, member, contact, realm_ids_left)

    -- Update local state: remove contact (bidirectional)
    contacts[member][contact] = nil
    contacts[contact][member] = nil
    blocked[member][contact] = true
    sentiments[member][contact] = nil

    -- Leave each shared realm
    for _, realm_id in ipairs(realms_to_leave) do
        -- The blocker leaves the realm (since it's a peer-set realm,
        -- the realm dissolves for everyone once a member leaves)
        immune.leave_realm(logger, sim.tick, realm_id, member)
        realms[realm_id][member] = nil
        -- Other members also effectively leave since the peer-set
        -- no longer matches
        for other_member, _ in pairs(realms[realm_id]) do
            immune.leave_realm(logger, sim.tick, realm_id, other_member)
        end
        realms[realm_id] = nil
    end
end

-- ============================================================================
-- PHASE 1: GENESIS — Healthy body
-- All five peers join, form mutual contacts, create peer-set realms
-- ============================================================================

immune.phase(logger, sim.tick, 1, "Genesis: Forming healthy network")

-- Everyone adds everyone as a contact (mutual)
for _, a in ipairs(immune.ALL_MEMBERS) do
    for _, b in ipairs(immune.ALL_MEMBERS) do
        if a ~= b then
            add_contact(a, b)
        end
    end
    sim:step()
end

-- Create peer-set realms for natural groupings:
--   {Z,N,S}  — the "inner circle"
--   {Z,N,O}  — Z+N know D
--   {Z,S,O}  — Z+C know D
--   {O,L}    — D+E pair
--   {Z,N,S,L} — E in the main group
local realm_zns  = create_realm({Z, N, S})
local realm_zno  = create_realm({Z, N, O})
local realm_zso  = create_realm({Z, S, O})
local realm_ol   = create_realm({O, L})
local realm_znsl = create_realm({Z, N, S, L})

sim:step()
sim:step()

-- Everyone starts with neutral sentiment (0)
for _, a in ipairs(immune.ALL_MEMBERS) do
    for _, b in ipairs(immune.ALL_MEMBERS) do
        if a ~= b then
            sentiments[a][b] = 0
            immune.set_sentiment(logger, sim.tick, a, b, 0)
        end
    end
    sim:step()
end

-- Positive sentiments within the inner circle
immune.set_sentiment(logger, sim.tick, Z, N, 1)
immune.set_sentiment(logger, sim.tick, N, Z, 1)
immune.set_sentiment(logger, sim.tick, Z, S, 1)
immune.set_sentiment(logger, sim.tick, S, Z, 1)
immune.set_sentiment(logger, sim.tick, N, S, 1)
immune.set_sentiment(logger, sim.tick, S, N, 1)
sentiments[Z][N] = 1; sentiments[N][Z] = 1
sentiments[Z][S] = 1; sentiments[S][Z] = 1
sentiments[N][S] = 1; sentiments[S][N] = 1

sim:step()
sim:step()

local initial_realm_count = 0
for _ in pairs(realms) do initial_realm_count = initial_realm_count + 1 end

indras.narrative("A community forms, unaware of the storm ahead")
logger.info("Phase 1 complete: Healthy network formed", {
    phase = 1,
    tick = sim.tick,
    realms = initial_realm_count,
    contacts_per_member = #immune.ALL_MEMBERS - 1,
})

-- ============================================================================
-- PHASE 2: INFECTION — Pathogen appears
-- D starts misbehaving: sends spam to shared realms
-- ============================================================================

immune.phase(logger, sim.tick, 2, "Infection: D begins misbehaving")

for i = 1, config.spam_count do
    -- D spams in {Z,N,O} realm
    if realms[realm_zno] then
        immune.chat(logger, sim.tick, O,
            string.format("BUY NOW!!! Amazing deal #%d - click here!!!", i),
            realm_zno)
    end

    -- D spams in {Z,S,O} realm
    if realms[realm_zso] then
        immune.chat(logger, sim.tick, O,
            string.format("URGENT: You've been selected!!! Offer #%d", i),
            realm_zso)
    end

    if i % 3 == 0 then
        sim:step()
    end
end

indras.narrative("Members build trust through personal connections")
logger.info("Phase 2 complete: D sent spam", {
    phase = 2,
    tick = sim.tick,
    spam_messages = config.spam_count * 2,
})

-- ============================================================================
-- PHASE 3: DETECTION — Innate immunity
-- A is the first to notice and rates D negatively
-- ============================================================================

immune.phase(logger, sim.tick, 3, "Detection: A detects the threat")

sim:step()
sim:step()

-- A rates D as "don't recommend"
immune.set_sentiment(logger, sim.tick, Z, O, -1)
sentiments[Z][O] = -1

-- A sends a warning to the inner circle
immune.chat(logger, sim.tick, Z,
    "Heads up — D is spamming our shared realms with scam links.",
    realm_zns)

sim:step()

indras.narrative("Cracks appear — not everyone sees eye to eye")
logger.info("Phase 3 complete: A flagged D", {
    phase = 3,
    tick = sim.tick,
    a_sentiment_d = sentiments[Z][O],
})

-- ============================================================================
-- PHASE 4: SIGNAL PROPAGATION — Cytokine cascade
-- B corroborates; relay signals reach C and E
-- ============================================================================

immune.phase(logger, sim.tick, 4, "Signal Propagation: Warnings spread through the network")

sim:step()

-- B independently rates D as "don't recommend"
immune.set_sentiment(logger, sim.tick, N, O, -1)
sentiments[N][O] = -1

immune.chat(logger, sim.tick, N,
    "Confirmed. I'm seeing the same spam from D.",
    realm_zns)

sim:step()

-- Relay: C receives relayed negative sentiment about D
-- via A (a trusted contact with sentiment +1)
for tick_delay = 1, config.relay_delay_ticks do
    sim:step()
end

immune.relay_sentiment(logger, sim.tick, S, O, -1, Z)
immune.relay_sentiment(logger, sim.tick, S, O, -1, N)

sim:step()

-- Relay: E also receives a relayed warning via A
immune.relay_sentiment(logger, sim.tick, L, O, -1, Z)

sim:step()

indras.narrative("Word spreads through the community's trust graph")
logger.info("Phase 4 complete: Sentiment signals relayed", {
    phase = 4,
    tick = sim.tick,
    b_sentiment_d = sentiments[N][O],
    relays_sent = 3,
})

-- ============================================================================
-- PHASE 5: GRADUATED RESPONSE — Adaptive immunity
-- C acts on relayed signals; A blocks D
-- ============================================================================

immune.phase(logger, sim.tick, 5, "Graduated Response: Network begins isolating the threat")

sim:step()
sim:step()

-- C, having received two independent relay signals, sets -1
immune.set_sentiment(logger, sim.tick, S, O, -1)
sentiments[S][O] = -1

immune.chat(logger, sim.tick, S,
    "Got warnings about D from both A and B. Setting to don't-recommend.",
    realm_zns)

sim:step()
sim:step()

-- A takes the strongest action: blocks D entirely
-- This triggers cascade: leave all shared realms
block_contact(Z, O)

sim:step()

indras.narrative("A member is blocked — the network's immune system activates")
logger.info("Phase 5 complete: A blocked D", {
    phase = 5,
    tick = sim.tick,
    c_sentiment_d = sentiments[S][O],
    a_blocked_d = blocked[Z][O] ~= nil,
})

-- ============================================================================
-- PHASE 6: CASCADE — Inflammation
-- B also blocks D. Remaining shared realms dissolve.
-- ============================================================================

immune.phase(logger, sim.tick, 6, "Cascade: Blocking propagates, realms dissolve")

sim:step()
sim:step()

-- B blocks D
block_contact(N, O)

sim:step()

-- C blocks D
block_contact(S, O)

sim:step()
sim:step()

-- Count remaining realms
local final_realm_count = 0
for _ in pairs(realms) do final_realm_count = final_realm_count + 1 end

-- Count D's remaining contacts
local orion_contacts = 0
for _, is_contact in pairs(contacts[O]) do
    if is_contact then orion_contacts = orion_contacts + 1 end
end

-- Count D's remaining realms
local orion_realms = #realms_containing(O)

logger.info("Phase 6 complete: D isolated", {
    phase = 6,
    tick = sim.tick,
    realms_before = initial_realm_count,
    realms_after = final_realm_count,
    d_contacts_remaining = orion_contacts,
    d_realms_remaining = orion_realms,
})

-- ============================================================================
-- PHASE 7: RECOVERY — Homeostasis
-- Remaining peers are healthy. E decides about D.
-- ============================================================================

immune.phase(logger, sim.tick, 7, "Recovery: Network stabilizes")

sim:step()

-- E received the relay warnings but makes her own decision.
-- E sets D to "don't recommend" but doesn't block — E still
-- has a personal connection via {O,L} realm.
immune.set_sentiment(logger, sim.tick, L, O, -1)
sentiments[L][O] = -1

immune.chat(logger, sim.tick, L,
    "I've seen the warnings about D. Setting to don't-recommend, but keeping contact for now.",
    realm_znsl)

sim:step()

-- The inner circle + E remain healthy
-- Positive sentiment reinforcement
immune.set_sentiment(logger, sim.tick, Z, L, 1)
immune.set_sentiment(logger, sim.tick, L, Z, 1)
immune.set_sentiment(logger, sim.tick, N, L, 1)
immune.set_sentiment(logger, sim.tick, L, N, 1)
sentiments[Z][L] = 1; sentiments[L][Z] = 1
sentiments[N][L] = 1; sentiments[L][N] = 1

sim:step()
sim:step()

-- Verify final state
local healthy_members = { Z, N, S, L }
local healthy_mutual_contacts = 0
for _, a in ipairs(healthy_members) do
    for _, b in ipairs(healthy_members) do
        if a ~= b and contacts[a][b] then
            healthy_mutual_contacts = healthy_mutual_contacts + 1
        end
    end
end

-- D's final contact count (should only have E, if anyone)
local orion_final_contacts = 0
for contact, is_contact in pairs(contacts[O]) do
    if is_contact then
        orion_final_contacts = orion_final_contacts + 1
    end
end

-- Who still has D as a contact?
local who_has_orion = 0
for _, m in ipairs(immune.ALL_MEMBERS) do
    if contacts[m][O] then
        who_has_orion = who_has_orion + 1
    end
end

indras.narrative("The community finds its balance — boundaries protect the whole")
logger.info("Phase 7 complete: Network recovered", {
    phase = 7,
    tick = sim.tick,
    healthy_mutual_contacts = healthy_mutual_contacts,
    orion_final_contacts = orion_final_contacts,
    who_still_has_orion_as_contact = who_has_orion,
})

-- ============================================================================
-- FINAL RESULTS
-- ============================================================================

result:add_metrics({
    total_members = #immune.ALL_MEMBERS,
    initial_realms = initial_realm_count,
    final_realms = final_realm_count,
    realms_dissolved = initial_realm_count - final_realm_count,
    spam_messages = config.spam_count * 2,
    negative_sentiments_set = 4,  -- Z, N, S, L all set -1 on D
    blocks_issued = 3,           -- Z, N, S blocked D
    relay_signals_sent = 3,      -- 2 to C, 1 to E
    orion_final_contacts = orion_final_contacts,
    healthy_mutual_contacts = healthy_mutual_contacts,
    who_still_has_orion = who_has_orion,
})

-- Assertions
result:record_assertion("orion_isolated",
    orion_final_contacts <= 1, "<=1", orion_final_contacts)

-- E is the only one who should still have D as contact
result:record_assertion("only_e_has_d",
    who_has_orion <= 1, "<=1", who_has_orion)

-- Healthy members should maintain mutual contacts
-- 4 healthy members, each has 3 contacts = 12 directed edges
result:record_assertion("healthy_contacts_intact",
    healthy_mutual_contacts >= 12, ">=12", healthy_mutual_contacts)

-- At least some realms should survive (the ones not involving D)
result:record_assertion("some_realms_survive",
    final_realm_count >= 1, ">=1", final_realm_count)

local final_result = result:build()

logger.info("Immune response simulation completed", {
    passed = final_result.passed,
    level = final_result.level,
    duration_sec = final_result.duration_sec,
    final_tick = sim.tick,
    d_isolated = orion_final_contacts <= 1,
    healthy_network_intact = healthy_mutual_contacts >= 12,
})

-- Standard assertions
indras.assert.le(orion_final_contacts, 1,
    "D should have at most 1 contact remaining")
indras.assert.le(who_has_orion, 1,
    "At most 1 member should still have D as contact")
indras.assert.ge(healthy_mutual_contacts, 12,
    "Healthy members should maintain all mutual contacts")
indras.assert.ge(final_realm_count, 1,
    "At least some realms should survive the cascade")

logger.info("Immune response simulation passed", {
    summary = "D isolated, healthy network intact, immune system worked",
})

return final_result

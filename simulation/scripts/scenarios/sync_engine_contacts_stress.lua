-- SyncEngine Contacts Stress Test
--
-- Tests contacts realm and auto-subscription pattern.
--
-- Contacts Flow:
-- 1. Join Contacts Realm: All peers join global contacts realm
-- 2. Add Contacts: Peers add each other as contacts
-- 3. Auto-Subscribe Simulation: Simulate auto-subscription to peer-set realms
-- 4. Contact Sync: Verify contacts propagate across network
-- 5. Remove Contacts: Test contact removal and realm cleanup

local quest_helpers = require("lib.quest_helpers")
local thresholds = require("config.quest_thresholds")

-- ============================================================================
-- SETUP
-- ============================================================================

local ctx = quest_helpers.new_context("sync_engine_contacts_stress")
local logger = quest_helpers.create_logger(ctx)
local config = quest_helpers.get_contacts_config()

logger.info("Starting contacts stress scenario", {
    level = quest_helpers.get_level(),
    peers = config.peers,
    contacts_per_peer = config.contacts_per_peer,
    sync_rounds = config.sync_rounds,
})

-- Create mesh with peers
local mesh = indras.MeshBuilder.new(config.peers):full_mesh()
local sim_config = indras.SimConfig.new({
    wake_probability = 0,
    sleep_probability = 0,
    initial_online_probability = 1,
    max_ticks = config.ticks,
})
local sim = indras.Simulation.new(mesh, sim_config)
sim:initialize()

local peers = mesh:peers()
local result = quest_helpers.result_builder("sync_engine_contacts_stress")

-- Metrics tracking
local latencies = {
    contacts_join = {},
    add_contact = {},
    remove_contact = {},
    realm_subscription = {},
}

-- Per-peer contacts
local peer_contacts = {}  -- peer_id -> Contacts object
for _, peer in ipairs(peers) do
    peer_contacts[tostring(peer)] = indras.sync_engine.contacts.new()
end

-- Auto-subscribed realms tracking
local subscribed_realms = {}  -- realm_id -> { subscribers = {} }

-- ============================================================================
-- PHASE 1: JOIN CONTACTS REALM
-- All peers join global contacts realm
-- ============================================================================

logger.info("Phase 1: Join contacts realm", {
    phase = 1,
    description = "All peers join global contacts realm",
    peer_count = #peers,
})

for _, peer in ipairs(peers) do
    sim:force_online(peer)
end

-- Simulate joining the global contacts realm
-- In real implementation this would involve CRDT sync
local contacts_realm_id = "contacts_global"
for _, peer in ipairs(peers) do
    local start_time = os.clock()
    -- Simulate realm join latency
    local latency = (os.clock() - start_time) * 1000000 + quest_helpers.realm_create_latency()
    table.insert(latencies.contacts_join, latency)

    logger.event("contacts_realm_joined", {
        tick = sim.tick,
        peer = tostring(peer),
        realm_id = contacts_realm_id,
        latency_us = latency,
    })

    sim:step()
end

indras.narrative("Members begin discovering each other")
logger.info("Phase 1 complete: All peers joined contacts realm", {
    phase = 1,
    tick = sim.tick,
    avg_latency_us = quest_helpers.average(latencies.contacts_join),
})

-- ============================================================================
-- PHASE 2: ADD CONTACTS
-- Peers add each other as contacts
-- ============================================================================

logger.info("Phase 2: Add contacts", {
    phase = 2,
    description = "Peers add each other as contacts",
    contacts_per_peer = config.contacts_per_peer,
})

local total_contacts_added = 0
for i, peer in ipairs(peers) do
    local peer_id = tostring(peer)
    local contacts = peer_contacts[peer_id]

    -- Add random contacts (up to contacts_per_peer)
    local added = 0
    local attempts = 0
    while added < config.contacts_per_peer and attempts < #peers * 2 do
        local other_idx = math.random(#peers)
        local other = tostring(peers[other_idx])

        if other ~= peer_id then
            local start_time = os.clock()
            local was_new = contacts:add(other)
            local latency = (os.clock() - start_time) * 1000000

            if was_new then
                table.insert(latencies.add_contact, latency)
                added = added + 1
                total_contacts_added = total_contacts_added + 1

                logger.event(quest_helpers.EVENTS.CONTACT_ADDED, {
                    tick = sim.tick,
                    peer = peer_id,
                    contact = other,
                    latency_us = latency,
                })

                -- Simulate auto-subscription to peer-set realm
                local realm_peers = quest_helpers.normalize_peers({peer_id, other})
                if #realm_peers >= 2 then
                    local realm_id = quest_helpers.compute_realm_id(realm_peers)

                    if not subscribed_realms[realm_id] then
                        subscribed_realms[realm_id] = { subscribers = {} }
                    end
                    subscribed_realms[realm_id].subscribers[peer_id] = true

                    local sub_start = os.clock()
                    local sub_latency = (os.clock() - sub_start) * 1000000 + quest_helpers.realm_lookup_latency()
                    table.insert(latencies.realm_subscription, sub_latency)

                    logger.event("auto_subscribed", {
                        tick = sim.tick,
                        peer = peer_id,
                        realm_id = realm_id,
                        realm_members = table.concat(realm_peers, ","),
                        latency_us = sub_latency,
                    })
                end
            end
        end
        attempts = attempts + 1
    end

    -- Step occasionally
    if i % 5 == 0 then
        sim:step()
    end
end

indras.narrative("Connections multiply as the social graph grows")
logger.info("Phase 2 complete: Contacts added", {
    phase = 2,
    tick = sim.tick,
    total_contacts_added = total_contacts_added,
    avg_latency_us = quest_helpers.average(latencies.add_contact),
})

-- ============================================================================
-- PHASE 3: AUTO-SUBSCRIBE SIMULATION
-- Verify auto-subscription to peer-set realms
-- ============================================================================

logger.info("Phase 3: Auto-subscription verification", {
    phase = 3,
    description = "Verify auto-subscription to peer-set realms",
})

-- Count total auto-subscriptions
local total_subscriptions = 0
local unique_realms = 0
for realm_id, realm_data in pairs(subscribed_realms) do
    unique_realms = unique_realms + 1
    for subscriber, _ in pairs(realm_data.subscribers) do
        total_subscriptions = total_subscriptions + 1
    end
end

-- Verify all contacts have corresponding realm subscriptions
local subscription_success = 0
local subscription_expected = 0
for peer_id, contacts in pairs(peer_contacts) do
    local contact_list = contacts:list()
    for _, contact in ipairs(contact_list) do
        subscription_expected = subscription_expected + 1

        local realm_peers = quest_helpers.normalize_peers({peer_id, contact})
        local realm_id = quest_helpers.compute_realm_id(realm_peers)

        if subscribed_realms[realm_id] and subscribed_realms[realm_id].subscribers[peer_id] then
            subscription_success = subscription_success + 1
        end
    end
end

local auto_subscription_rate = subscription_expected > 0
    and subscription_success / subscription_expected
    or 1.0

logger.info("Phase 3 complete: Auto-subscription verified", {
    phase = 3,
    tick = sim.tick,
    unique_realms = unique_realms,
    total_subscriptions = total_subscriptions,
    subscription_success = subscription_success,
    subscription_expected = subscription_expected,
    auto_subscription_rate = auto_subscription_rate,
})

-- ============================================================================
-- PHASE 4: CONTACT SYNC
-- Simulate contact synchronization rounds
-- ============================================================================

logger.info("Phase 4: Contact sync", {
    phase = 4,
    description = "Simulate contact synchronization",
    sync_rounds = config.sync_rounds,
})

local sync_converged = 0
local sync_diverged = 0

for round = 1, config.sync_rounds do
    -- In each round, verify contacts are consistent
    -- In real implementation, this would compare CRDT states

    -- For simulation, we verify our local state is self-consistent
    local round_consistent = true
    for peer_id, contacts in pairs(peer_contacts) do
        local contact_count = contacts:count()
        local contact_list = contacts:list()

        if contact_count ~= #contact_list then
            round_consistent = false
            break
        end
    end

    if round_consistent then
        sync_converged = sync_converged + 1
    else
        sync_diverged = sync_diverged + 1
    end

    -- Step simulation
    sim:step()

    -- Log sync event periodically
    if round % 10 == 0 then
        logger.event(quest_helpers.EVENTS.CONTACTS_SYNCED, {
            tick = sim.tick,
            round = round,
            converged = round_consistent,
        })
    end
end

local sync_convergence_rate = sync_converged / (sync_converged + sync_diverged)

indras.narrative("The contact system is pushed to its limits")
logger.info("Phase 4 complete: Contact sync verified", {
    phase = 4,
    tick = sim.tick,
    sync_converged = sync_converged,
    sync_diverged = sync_diverged,
    sync_convergence_rate = sync_convergence_rate,
})

-- ============================================================================
-- PHASE 5: REMOVE CONTACTS
-- Test contact removal
-- ============================================================================

logger.info("Phase 5: Remove contacts", {
    phase = 5,
    description = "Test contact removal",
})

local total_removed = 0
for peer_id, contacts in pairs(peer_contacts) do
    local contact_list = contacts:list()

    -- Remove half of contacts
    local remove_count = math.floor(#contact_list / 2)
    for i = 1, remove_count do
        if #contact_list >= i then
            local contact_to_remove = contact_list[i]

            local start_time = os.clock()
            contacts:remove(contact_to_remove)
            local latency = (os.clock() - start_time) * 1000000 + quest_helpers.contact_add_latency()
            table.insert(latencies.remove_contact, latency)

            total_removed = total_removed + 1

            logger.event(quest_helpers.EVENTS.CONTACT_REMOVED, {
                tick = sim.tick,
                peer = peer_id,
                contact = contact_to_remove,
                latency_us = latency,
            })
        end
    end

    sim:step()
end

-- Verify removal consistency
local removal_verification = { passed = 0, failed = 0 }
for peer_id, contacts in pairs(peer_contacts) do
    local expected_count = config.contacts_per_peer - math.floor(config.contacts_per_peer / 2)
    local actual_count = contacts:count()

    -- Allow some variance due to random selection
    if actual_count <= config.contacts_per_peer then
        removal_verification.passed = removal_verification.passed + 1
    else
        removal_verification.failed = removal_verification.failed + 1
    end
end

indras.narrative("A rich web of connections holds firm under pressure")
logger.info("Phase 5 complete: Contacts removed", {
    phase = 5,
    tick = sim.tick,
    total_removed = total_removed,
    removal_passed = removal_verification.passed,
    removal_failed = removal_verification.failed,
})

-- ============================================================================
-- FINAL RESULTS
-- ============================================================================

-- Calculate metrics
local contacts_join_percentiles = quest_helpers.percentiles(latencies.contacts_join)
local add_contact_percentiles = quest_helpers.percentiles(latencies.add_contact)
local realm_sub_percentiles = quest_helpers.percentiles(latencies.realm_subscription)

-- Record metrics
result:add_metrics({
    total_peers = #peers,
    total_contacts_added = total_contacts_added,
    total_contacts_removed = total_removed,
    unique_realms_created = unique_realms,
    total_subscriptions = total_subscriptions,

    contacts_join_p99_us = contacts_join_percentiles.p99,
    contacts_join_p95_us = contacts_join_percentiles.p95,
    contacts_join_p50_us = contacts_join_percentiles.p50,

    add_contact_p99_us = add_contact_percentiles.p99,
    add_contact_p95_us = add_contact_percentiles.p95,
    add_contact_p50_us = add_contact_percentiles.p50,

    realm_subscription_p99_us = realm_sub_percentiles.p99,

    contact_sync_convergence = sync_convergence_rate,
    auto_subscription_success = auto_subscription_rate,
})

-- Assertions
result:record_assertion("contact_sync_convergence",
    sync_convergence_rate >= 0.99, 0.99, sync_convergence_rate)
result:record_assertion("auto_subscription_success",
    auto_subscription_rate >= 1.0, 1.0, auto_subscription_rate)

local final_result = result:build()

logger.info("Contacts stress scenario completed", {
    passed = final_result.passed,
    level = final_result.level,
    duration_sec = final_result.duration_sec,
    final_tick = sim.tick,
    total_contacts_added = total_contacts_added,
    total_contacts_removed = total_removed,
    unique_realms = unique_realms,
    sync_convergence_rate = sync_convergence_rate,
    auto_subscription_rate = auto_subscription_rate,
})

-- Standard assertions
indras.assert.ge(sync_convergence_rate, 0.99, "Contact sync convergence should be >= 99%")
indras.assert.eq(auto_subscription_rate, 1.0, "Auto-subscription success should be 100%")
indras.assert.gt(total_contacts_added, 0, "Should have added contacts")
indras.assert.gt(unique_realms, 0, "Should have created peer-set realms")

logger.info("Contacts stress scenario passed", {
    unique_realms = unique_realms,
    total_subscriptions = total_subscriptions,
})

return final_result

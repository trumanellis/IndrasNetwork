-- SDK Quest Proof & Blessing Stress Test
--
-- Tests the full quest proof and blessing flow:
-- 1. Members create quests
-- 2. Members focus attention on quests
-- 3. One member submits proof (appears in chat)
-- 4. Other members bless the proof with their accumulated attention
-- 5. Chat messages flow throughout
--
-- This scenario exercises the realm chat with proof submission and
-- attention-based blessing verification.

local quest_helpers = require("lib.quest_helpers")
local home = require("lib.home_realm_helpers")
local thresholds = require("config.quest_thresholds")

-- ============================================================================
-- SETUP
-- ============================================================================

local ctx = quest_helpers.new_context("sdk_quest_proof_blessing")
local logger = quest_helpers.create_logger(ctx)
local config = quest_helpers.get_config()

logger.info("Starting quest proof & blessing scenario", {
    level = quest_helpers.get_level(),
    realms = config.realms,
    quests_per_realm = config.quests_per_realm,
    members = config.members,
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
local result = quest_helpers.result_builder("sdk_quest_proof_blessing")

-- Tracking
local blessing_tracker = home.BlessingTracker.new()
local realms = {}  -- realm_id -> { members = {}, quests = {} }
local total_chat_messages = 0
local total_proofs_submitted = 0
local total_blessings = 0

-- ============================================================================
-- PHASE 1: SETUP - Create realms and quests
-- ============================================================================

logger.info("Phase 1: Setup - Creating realms and quests", { phase = 1 })

for _, peer in ipairs(peers) do
    sim:force_online(peer)
end

-- Create realms with different peer combinations
for i = 1, math.min(config.realms, 3) do
    local num_peers = math.min(4 + math.random(2), #peers)
    local selected = {}
    local used = {}

    while #selected < num_peers do
        local idx = math.random(#peers)
        if not used[idx] then
            used[idx] = true
            table.insert(selected, tostring(peers[idx]))
        end
    end

    local realm_id = quest_helpers.compute_realm_id(selected)
    realms[realm_id] = {
        members = selected,
        quests = {},
    }

    logger.event(quest_helpers.EVENTS.REALM_CREATED, {
        tick = sim.tick,
        realm_id = realm_id,
        members = table.concat(selected, ","),
        member_count = #selected,
    })

    -- Members join
    for _, member in ipairs(selected) do
        logger.event("member_joined", {
            tick = sim.tick,
            realm_id = realm_id,
            member = member,
        })
    end

    -- Members add each other as contacts (creates network connections)
    for i, member in ipairs(selected) do
        for j, other in ipairs(selected) do
            if i ~= j then
                logger.event("contact_added", {
                    tick = sim.tick,
                    member = member,
                    contact = other,
                })
            end
        end
    end

    sim:step()
end

-- Create quests in each realm
for realm_id, realm_data in pairs(realms) do
    for i = 1, math.min(config.quests_per_realm, 3) do
        local creator = realm_data.members[math.random(#realm_data.members)]
        local quest_id = quest_helpers.generate_quest_id()
        local title = quest_helpers.random_quest_title()

        table.insert(realm_data.quests, {
            id = quest_id,
            title = title,
            creator = creator,
            proof = nil,
            proof_submitter = nil,
        })

        logger.event(quest_helpers.EVENTS.QUEST_CREATED, {
            tick = sim.tick,
            realm_id = realm_id,
            quest_id = quest_id,
            creator = creator,
            title = title,
            latency_us = quest_helpers.quest_create_latency(),
        })

        sim:step()
    end
end

local total_quests = 0
for _, realm_data in pairs(realms) do
    total_quests = total_quests + #realm_data.quests
end

logger.info("Phase 1 complete", {
    phase = 1,
    tick = sim.tick,
    realms = config.realms,
    total_quests = total_quests,
})

-- ============================================================================
-- PHASE 2: ATTENTION ACCUMULATION
-- Members focus on quests and accumulate attention
-- ============================================================================

logger.info("Phase 2: Attention accumulation", {
    phase = 2,
    description = "Members focus on quests to accumulate attention",
})

local attention_events = 0
for realm_id, realm_data in pairs(realms) do
    for _, quest in ipairs(realm_data.quests) do
        -- Each member (except creator) focuses on the quest
        for _, member in ipairs(realm_data.members) do
            -- Focus on quest
            local event_id = quest_helpers.generate_event_id()
            local duration_ms = 30000 + math.random(90000)  -- 30-120 seconds

            logger.event("attention_switched", {
                tick = sim.tick,
                member = member,
                quest_id = quest.id,
                event_id = event_id,
                latency_us = quest_helpers.attention_switch_latency(),
            })

            -- Track attention for blessing
            blessing_tracker:record_attention(quest.id, member, attention_events, duration_ms)
            attention_events = attention_events + 1

            -- Occasional chat message while working
            if math.random() < 0.3 then
                total_chat_messages = total_chat_messages + 1
                logger.event("chat_message", {
                    tick = sim.tick,
                    member = member,
                    content = home.random_chat_message(),
                    message_type = "text",
                })
            end

            sim:step()
        end
    end
end

logger.info("Phase 2 complete", {
    phase = 2,
    tick = sim.tick,
    attention_events = attention_events,
    chat_messages = total_chat_messages,
})

-- ============================================================================
-- PHASE 3: PROOF SUBMISSION
-- One member per quest submits proof (posted to chat)
-- ============================================================================

logger.info("Phase 3: Proof submission", {
    phase = 3,
    description = "Members submit proof for completed quests",
})

for realm_id, realm_data in pairs(realms) do
    for _, quest in ipairs(realm_data.quests) do
        -- Pick a non-creator member to submit proof
        local candidates = {}
        for _, member in ipairs(realm_data.members) do
            if member ~= quest.creator then
                table.insert(candidates, member)
            end
        end

        if #candidates > 0 then
            local submitter = candidates[math.random(#candidates)]
            local artifact_id = home.generate_artifact_id()
            local artifact_name = string.format("proof_%s.png", quest.id:sub(1, 8))

            quest.proof = artifact_id
            quest.proof_submitter = submitter

            -- Submit claim
            logger.event("quest_claim_submitted", {
                tick = sim.tick,
                realm_id = realm_id,
                quest_id = quest.id,
                claimant = submitter,
                claim_index = 0,
                proof_artifact = artifact_id,
            })

            -- Post proof to chat
            logger.event("proof_submitted", {
                tick = sim.tick,
                realm_id = realm_id,
                quest_id = quest.id,
                claimant = submitter,
                quest_title = quest.title,
                artifact_id = artifact_id,
                artifact_name = artifact_name,
            })

            total_proofs_submitted = total_proofs_submitted + 1
            sim:step()
        end
    end
end

logger.info("Phase 3 complete", {
    phase = 3,
    tick = sim.tick,
    proofs_submitted = total_proofs_submitted,
})

-- ============================================================================
-- PHASE 4: BLESSING FLOW
-- Members bless proofs by releasing their accumulated attention
-- ============================================================================

logger.info("Phase 4: Blessing flow", {
    phase = 4,
    description = "Members bless proofs with accumulated attention",
})

for realm_id, realm_data in pairs(realms) do
    for _, quest in ipairs(realm_data.quests) do
        if quest.proof_submitter then
            -- Each other member blesses the proof
            for _, member in ipairs(realm_data.members) do
                if member ~= quest.proof_submitter then
                    -- Get unblessed attention events for this member
                    local unblessed = blessing_tracker:get_unblessed_attention(quest.id, member)

                    if #unblessed > 0 then
                        -- Calculate total attention to bless
                        local total_millis = 0
                        local event_indices = {}
                        for _, evt in ipairs(unblessed) do
                            table.insert(event_indices, evt.index)
                            total_millis = total_millis + evt.duration_millis
                        end

                        -- Record blessing
                        blessing_tracker:record_blessing(
                            quest.id,
                            quest.proof_submitter,
                            member,
                            event_indices,
                            total_millis
                        )

                        -- Post blessing to chat
                        logger.event("blessing_given", {
                            tick = sim.tick,
                            realm_id = realm_id,
                            quest_id = quest.id,
                            claimant = quest.proof_submitter,
                            blesser = member,
                            event_count = #event_indices,
                            attention_millis = total_millis,
                        })

                        total_blessings = total_blessings + 1

                        -- Celebratory chat message
                        if math.random() < 0.5 then
                            total_chat_messages = total_chat_messages + 1
                            logger.event("chat_message", {
                                tick = sim.tick,
                                member = member,
                                content = "Great work!",
                                message_type = "text",
                            })
                        end

                        sim:step()
                    end
                end
            end

            -- Log total blessed for this proof
            local total_blessed = blessing_tracker:get_total_blessed(quest.id, quest.proof_submitter)
            local blessers = blessing_tracker:get_blessers(quest.id, quest.proof_submitter)

            logger.info("Proof blessed", {
                quest_id = quest.id,
                claimant = quest.proof_submitter,
                total_blessed_millis = total_blessed,
                blesser_count = #blessers,
                formatted_duration = home.format_duration(total_blessed),
            })
        end
    end
end

logger.info("Phase 4 complete", {
    phase = 4,
    tick = sim.tick,
    total_blessings = total_blessings,
})

-- ============================================================================
-- PHASE 5: VERIFICATION AND COMPLETION
-- Creators verify claims and complete quests
-- ============================================================================

logger.info("Phase 5: Verification and completion", {
    phase = 5,
    description = "Creators verify proofs and complete quests",
})

local total_verified = 0
local total_completed = 0

for realm_id, realm_data in pairs(realms) do
    for _, quest in ipairs(realm_data.quests) do
        if quest.proof_submitter then
            -- Verify claim
            logger.event("quest_claim_verified", {
                tick = sim.tick,
                realm_id = realm_id,
                quest_id = quest.id,
                claim_index = 0,
            })
            total_verified = total_verified + 1

            -- Complete quest
            local blessers = blessing_tracker:get_blessers(quest.id, quest.proof_submitter)
            logger.event("quest_completed", {
                tick = sim.tick,
                realm_id = realm_id,
                quest_id = quest.id,
                verified_claims = 1,
                pending_claims = 0,
            })
            total_completed = total_completed + 1

            -- Celebratory chat
            total_chat_messages = total_chat_messages + 1
            logger.event("chat_message", {
                tick = sim.tick,
                member = quest.creator,
                content = "Quest completed! Thanks everyone!",
                message_type = "text",
            })

            sim:step()
        end
    end
end

logger.info("Phase 5 complete", {
    phase = 5,
    tick = sim.tick,
    total_verified = total_verified,
    total_completed = total_completed,
})

-- ============================================================================
-- FINAL RESULTS
-- ============================================================================

result:add_metrics({
    total_realms = config.realms,
    total_quests = total_quests,
    total_chat_messages = total_chat_messages,
    total_proofs_submitted = total_proofs_submitted,
    total_blessings = total_blessings,
    total_verified = total_verified,
    total_completed = total_completed,
    attention_events = attention_events,
})

result:record_assertion("proofs_submitted",
    total_proofs_submitted > 0, true, total_proofs_submitted > 0)
result:record_assertion("blessings_given",
    total_blessings > 0, true, total_blessings > 0)
result:record_assertion("quests_completed",
    total_completed > 0, true, total_completed > 0)

local final_result = result:build()

logger.info("Quest proof & blessing scenario completed", {
    passed = final_result.passed,
    level = final_result.level,
    duration_sec = final_result.duration_sec,
    final_tick = sim.tick,
    total_chat_messages = total_chat_messages,
    total_proofs_submitted = total_proofs_submitted,
    total_blessings = total_blessings,
})

-- Assertions
indras.assert.gt(total_proofs_submitted, 0, "Should have submitted proofs")
indras.assert.gt(total_blessings, 0, "Should have given blessings")
indras.assert.gt(total_chat_messages, 0, "Should have chat messages")

logger.info("Quest proof & blessing scenario passed")

return final_result

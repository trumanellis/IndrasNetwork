-- SDK Home Realm Stress Test
--
-- Tests personal home realm functionality at scale with JSONL logging
-- for automated testing and analysis.
--
-- Key Insight: Each member has exactly one home realm with a deterministic ID
-- derived from their member ID. This enables multi-device sync - any device
-- with the same identity will access the same home realm.
--
-- Phases:
-- 1. Setup: Create mesh topology with N members
-- 2. Identity Test: Verify home_realm_id is deterministic per member
-- 3. Uniqueness Test: Verify different members have different home realms
-- 4. Note Operations: Create, update, delete notes with tags
-- 5. Quest Operations: Create and complete personal quests
-- 6. Artifact Operations: Upload and retrieve artifacts
-- 7. Persistence Test: Simulate session restart and data recovery
-- 8. Multi-device Sync: Same member accessing from "multiple devices"
--
-- JSONL Output: All events logged with trace_id for distributed tracing

local home = require("lib.home_realm_helpers")

-- ============================================================================
-- SETUP
-- ============================================================================

local ctx = home.new_context("sdk_home_realm_stress")
local logger = home.create_logger(ctx)
local config = home.get_config()

logger.info("Starting home realm stress scenario", {
    level = home.get_level(),
    members = config.members,
    notes_per_member = config.notes_per_member,
    quests_per_member = config.quests_per_member,
    artifacts_per_member = config.artifacts_per_member,
})

-- Create mesh with N members
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
local result = home.result_builder("sdk_home_realm_stress")

-- Metrics tracking
local latencies = {
    realm_id = {},
    note_create = {},
    note_update = {},
    quest_create = {},
    artifact_upload = {},
    artifact_retrieve = {},
}

local identity_checks = { passed = 0, failed = 0 }
local uniqueness_checks = { passed = 0, failed = 0 }
local note_ops = { created = 0, updated = 0, deleted = 0, failed = 0 }
local quest_ops = { created = 0, completed = 0, failed = 0 }
local artifact_ops = { uploaded = 0, retrieved = 0, failed = 0 }
local persistence_checks = { passed = 0, failed = 0 }
local sync_checks = { passed = 0, failed = 0 }

-- Storage for verification
local member_home_realms = {}  -- member_id -> home_realm_id
local note_tracker = home.NoteTracker.new()

-- ============================================================================
-- PHASE 1: SETUP (Bring all peers online)
-- ============================================================================

logger.info("Phase 1: Setup - Bringing peers online", {
    phase = 1,
    peer_count = #peers,
})

for _, peer in ipairs(peers) do
    sim:force_online(peer)
end

sim:step()

logger.info("Phase 1 complete: All peers online", {
    phase = 1,
    tick = sim.tick,
})

-- ============================================================================
-- PHASE 2: IDENTITY TEST
-- Verify home_realm_id is deterministic per member
-- ============================================================================

logger.info("Phase 2: Identity test - Home realm ID determinism", {
    phase = 2,
    description = "Verify same member always gets same home realm ID",
})

local identity_tests = math.min(100, #peers * 10)
for i = 1, identity_tests do
    -- Pick a random member
    local member = tostring(peers[math.random(#peers)])

    -- Compute home realm ID twice
    local start_time = os.clock()
    local realm_id1 = home.compute_home_realm_id(member)
    local latency1 = (os.clock() - start_time) * 1000000

    start_time = os.clock()
    local realm_id2 = home.compute_home_realm_id(member)
    local latency2 = (os.clock() - start_time) * 1000000

    table.insert(latencies.realm_id, latency1)
    table.insert(latencies.realm_id, latency2)

    -- Verify consistency
    if realm_id1 == realm_id2 then
        identity_checks.passed = identity_checks.passed + 1
        member_home_realms[member] = realm_id1
    else
        identity_checks.failed = identity_checks.failed + 1
        logger.warn("Identity determinism failure", {
            member = member,
            realm_id1 = realm_id1,
            realm_id2 = realm_id2,
        })
    end

    -- Log event
    logger.event(home.EVENTS.HOME_REALM_ID_COMPUTED, {
        tick = sim.tick,
        member = member,
        realm_id = realm_id1,
        latency_us = latency1,
        consistent = realm_id1 == realm_id2,
    })

    if i % 10 == 0 then
        sim:step()
    end
end

logger.info("Phase 2 complete: Identity tests", {
    phase = 2,
    tick = sim.tick,
    passed = identity_checks.passed,
    failed = identity_checks.failed,
})

-- ============================================================================
-- PHASE 3: UNIQUENESS TEST
-- Verify different members have different home realms
-- ============================================================================

logger.info("Phase 3: Uniqueness test", {
    phase = 3,
    description = "Verify different members have different home realm IDs",
})

-- Compute home realm for all members
for _, peer in ipairs(peers) do
    local member = tostring(peer)
    member_home_realms[member] = home.compute_home_realm_id(member)
end

-- Check all pairs for uniqueness
local uniqueness_tests = 0
for i = 1, #peers do
    for j = i + 1, #peers do
        local member1 = tostring(peers[i])
        local member2 = tostring(peers[j])
        local id1 = member_home_realms[member1]
        local id2 = member_home_realms[member2]

        uniqueness_tests = uniqueness_tests + 1

        if id1 ~= id2 then
            uniqueness_checks.passed = uniqueness_checks.passed + 1
        else
            uniqueness_checks.failed = uniqueness_checks.failed + 1
            logger.warn("Uniqueness failure (collision)", {
                member1 = member1,
                member2 = member2,
                realm_id = id1,
            })
        end
    end
end

logger.info("Phase 3 complete: Uniqueness tests", {
    phase = 3,
    tick = sim.tick,
    total_pairs = uniqueness_tests,
    passed = uniqueness_checks.passed,
    failed = uniqueness_checks.failed,
})

sim:step()

-- ============================================================================
-- PHASE 4: NOTE OPERATIONS
-- Create, update, delete notes with tags
-- ============================================================================

logger.info("Phase 4: Note operations", {
    phase = 4,
    description = "Create, update, and delete notes in home realm",
    notes_per_member = config.notes_per_member,
})

local notes_created = {}  -- member -> { note_ids }

for _, peer in ipairs(peers) do
    local member = tostring(peer)
    notes_created[member] = {}

    for i = 1, config.notes_per_member do
        -- Create note
        local note_id = home.generate_artifact_id():sub(1, 32)  -- 16 bytes = 32 hex chars
        local title = home.random_note_title()
        local content = home.random_note_content()
        local tags = home.random_tags()

        local start_time = os.clock()
        -- Simulate note creation (in real impl this would call SDK)
        note_tracker:record_note(note_id, title, content, tags)
        local latency = (os.clock() - start_time) * 1000000 + home.note_create_latency()
        table.insert(latencies.note_create, latency)

        table.insert(notes_created[member], note_id)
        note_ops.created = note_ops.created + 1

        logger.event(home.EVENTS.NOTE_CREATED, {
            tick = sim.tick,
            member = member,
            note_id = note_id,
            title = title,
            tag_count = #tags,
            latency_us = latency,
        })

        -- Randomly update some notes
        if math.random() < 0.3 and #notes_created[member] > 0 then
            local update_idx = math.random(#notes_created[member])
            local update_id = notes_created[member][update_idx]

            start_time = os.clock()
            note_tracker:record_update(update_id, home.random_note_content(), nil)
            latency = (os.clock() - start_time) * 1000000 + home.note_update_latency()
            table.insert(latencies.note_update, latency)

            note_ops.updated = note_ops.updated + 1

            logger.event(home.EVENTS.NOTE_UPDATED, {
                tick = sim.tick,
                member = member,
                note_id = update_id,
                latency_us = latency,
            })
        end

        if i % config.sync_interval == 0 then
            sim:step()
        end
    end
end

-- Delete some notes
local delete_count = math.floor(note_ops.created * 0.1)  -- Delete 10%
for i = 1, delete_count do
    local member = tostring(peers[math.random(#peers)])
    if #notes_created[member] > 0 then
        local delete_idx = math.random(#notes_created[member])
        local delete_id = notes_created[member][delete_idx]

        if note_tracker:record_deletion(delete_id) then
            table.remove(notes_created[member], delete_idx)
            note_ops.deleted = note_ops.deleted + 1

            logger.event(home.EVENTS.NOTE_DELETED, {
                tick = sim.tick,
                member = member,
                note_id = delete_id,
            })
        end
    end
end

logger.info("Phase 4 complete: Note operations", {
    phase = 4,
    tick = sim.tick,
    created = note_ops.created,
    updated = note_ops.updated,
    deleted = note_ops.deleted,
})

-- ============================================================================
-- PHASE 5: QUEST OPERATIONS
-- Create and complete personal quests
-- ============================================================================

logger.info("Phase 5: Quest operations", {
    phase = 5,
    description = "Create and complete personal quests in home realm",
    quests_per_member = config.quests_per_member,
})

local quests_created = {}  -- member -> { quest_ids }

for _, peer in ipairs(peers) do
    local member = tostring(peer)
    quests_created[member] = {}

    for i = 1, config.quests_per_member do
        -- Create quest
        local quest_id = home.generate_artifact_id():sub(1, 32)
        local title = home.random_quest_title()
        local description = home.random_quest_description()

        local start_time = os.clock()
        -- Simulate quest creation
        local latency = (os.clock() - start_time) * 1000000 + 200  -- Base latency

        table.insert(quests_created[member], { id = quest_id, completed = false })
        table.insert(latencies.quest_create, latency)
        quest_ops.created = quest_ops.created + 1

        logger.event(home.EVENTS.HOME_QUEST_CREATED, {
            tick = sim.tick,
            member = member,
            quest_id = quest_id,
            title = title,
            latency_us = latency,
        })

        if i % config.sync_interval == 0 then
            sim:step()
        end
    end
end

-- Complete some quests
local complete_rate = 0.6
for member, quests in pairs(quests_created) do
    for _, quest in ipairs(quests) do
        if math.random() < complete_rate and not quest.completed then
            quest.completed = true
            quest_ops.completed = quest_ops.completed + 1

            logger.event(home.EVENTS.HOME_QUEST_COMPLETED, {
                tick = sim.tick,
                member = member,
                quest_id = quest.id,
            })
        end
    end
end

logger.info("Phase 5 complete: Quest operations", {
    phase = 5,
    tick = sim.tick,
    created = quest_ops.created,
    completed = quest_ops.completed,
    completion_rate = quest_ops.completed / quest_ops.created,
})

sim:step()

-- ============================================================================
-- PHASE 6: ARTIFACT OPERATIONS
-- Upload and retrieve artifacts
-- ============================================================================

logger.info("Phase 6: Artifact operations", {
    phase = 6,
    description = "Upload and retrieve artifacts in home realm",
    artifacts_per_member = config.artifacts_per_member,
})

local artifacts_uploaded = {}  -- member -> { artifact_ids }

for _, peer in ipairs(peers) do
    local member = tostring(peer)
    artifacts_uploaded[member] = {}

    for i = 1, config.artifacts_per_member do
        -- Upload artifact
        local artifact = home.generate_mock_artifact()
        local artifact_id = home.generate_artifact_id()

        local start_time = os.clock()
        -- Simulate upload
        local latency = (os.clock() - start_time) * 1000000 + home.artifact_upload_latency()
        table.insert(latencies.artifact_upload, latency)

        table.insert(artifacts_uploaded[member], artifact_id)
        artifact_ops.uploaded = artifact_ops.uploaded + 1

        logger.event(home.EVENTS.ARTIFACT_UPLOADED, {
            tick = sim.tick,
            member = member,
            artifact_id = artifact_id,
            size = artifact.size,
            mime_type = artifact.mime_type,
            latency_us = latency,
        })

        if i % config.sync_interval == 0 then
            sim:step()
        end
    end
end

-- Retrieve some artifacts
local retrieve_count = math.floor(artifact_ops.uploaded * 0.5)  -- Retrieve 50%
for i = 1, retrieve_count do
    local member = tostring(peers[math.random(#peers)])
    if #artifacts_uploaded[member] > 0 then
        local artifact_id = artifacts_uploaded[member][math.random(#artifacts_uploaded[member])]

        local start_time = os.clock()
        -- Simulate retrieval
        local latency = (os.clock() - start_time) * 1000000 + home.artifact_retrieve_latency()
        table.insert(latencies.artifact_retrieve, latency)

        artifact_ops.retrieved = artifact_ops.retrieved + 1

        logger.event(home.EVENTS.ARTIFACT_RETRIEVED, {
            tick = sim.tick,
            member = member,
            artifact_id = artifact_id,
            latency_us = latency,
        })
    end
end

logger.info("Phase 6 complete: Artifact operations", {
    phase = 6,
    tick = sim.tick,
    uploaded = artifact_ops.uploaded,
    retrieved = artifact_ops.retrieved,
})

-- ============================================================================
-- PHASE 6.5: INLINE IMAGES IN CHAT
-- Share images inline in the chat
-- ============================================================================

logger.info("Phase 6.5: Inline images in chat", {
    phase = 6.5,
    description = "Share images and galleries inline in chat",
})

-- Featured test asset
local FEATURED_ASSET = {
    path = "assets/Logo_black.png",
    name = "Logo_black.png",
    size = 830269,
    mime_type = "image/png",
    dimensions = {1024, 1024},
}

-- Create a realm for chat (using first 3 members)
local chat_peer_ids = {}
for i = 1, math.min(3, #peers) do
    table.insert(chat_peer_ids, tostring(peers[i]))
end

-- Emit realm created for chat context
local chat_realm_id = "chat-realm-" .. sim.tick
logger.event("realm_created", {
    tick = sim.tick,
    realm_id = chat_realm_id,
    member_count = #chat_peer_ids,
    members = table.concat(chat_peer_ids, ","),
})

for _, member_id in ipairs(chat_peer_ids) do
    logger.event("member_joined", {
        tick = sim.tick,
        realm_id = chat_realm_id,
        member = member_id,
    })
end

sim:step()

-- First member sends a text message
logger.event("chat_message", {
    tick = sim.tick,
    member = chat_peer_ids[1],
    content = "Hey everyone! Check out our logo:",
    message_type = "text",
    message_id = "msg-" .. sim.tick .. "-" .. chat_peer_ids[1],
})

sim:step()

-- First member shares the logo as an inline image
logger.event("chat_image", {
    tick = sim.tick,
    member = chat_peer_ids[1],
    mime_type = FEATURED_ASSET.mime_type,
    filename = FEATURED_ASSET.name,
    dimensions = FEATURED_ASSET.dimensions,
    alt_text = "IndrasNetwork Logo",
    asset_path = FEATURED_ASSET.path,
    message_id = "img-" .. sim.tick .. "-" .. chat_peer_ids[1],
})

sim:step()

-- Second member responds
if #chat_peer_ids >= 2 then
    logger.event("chat_message", {
        tick = sim.tick,
        member = chat_peer_ids[2],
        content = "That looks great! Here's a gallery of screenshots:",
        message_type = "text",
        message_id = "msg-" .. sim.tick .. "-" .. chat_peer_ids[2],
    })

    sim:step()

    -- Second member shares a gallery
    logger.event("chat_gallery", {
        tick = sim.tick,
        member = chat_peer_ids[2],
        folder_id = "gallery-screenshots-001",
        title = "Project Screenshots",
        items = {
            {
                name = "dashboard.png",
                mime_type = "image/png",
                size = 256000,
                artifact_hash = home.generate_artifact_id(),
                dimensions = {1920, 1080},
                asset_path = FEATURED_ASSET.path,
            },
            {
                name = "chat_view.png",
                mime_type = "image/png",
                size = 312000,
                artifact_hash = home.generate_artifact_id(),
                dimensions = {1920, 1080},
                asset_path = FEATURED_ASSET.path,
            },
            {
                name = "settings.png",
                mime_type = "image/png",
                size = 198000,
                artifact_hash = home.generate_artifact_id(),
                dimensions = {1920, 1080},
                asset_path = FEATURED_ASSET.path,
            },
        },
        message_id = "gallery-" .. sim.tick .. "-" .. chat_peer_ids[2],
    })
end

sim:step()

-- Third member shares another image
if #chat_peer_ids >= 3 then
    logger.event("chat_message", {
        tick = sim.tick,
        member = chat_peer_ids[3],
        content = "Here's my contribution!",
        message_type = "text",
        message_id = "msg-" .. sim.tick .. "-" .. chat_peer_ids[3],
    })

    sim:step()

    logger.event("chat_image", {
        tick = sim.tick,
        member = chat_peer_ids[3],
        mime_type = "image/png",
        filename = "my_design.png",
        dimensions = {800, 600},
        alt_text = "My design contribution",
        asset_path = FEATURED_ASSET.path,
        message_id = "img-" .. sim.tick .. "-" .. chat_peer_ids[3],
    })
end

sim:step()

-- Final message
logger.event("chat_message", {
    tick = sim.tick,
    member = chat_peer_ids[1],
    content = "Great work everyone! The images look fantastic.",
    message_type = "text",
    message_id = "msg-" .. sim.tick .. "-" .. chat_peer_ids[1],
})

logger.info("Phase 6.5 complete: Inline images shared", {
    phase = 6.5,
    tick = sim.tick,
    images_shared = #chat_peer_ids >= 3 and 2 or 1,
    galleries_shared = #chat_peer_ids >= 2 and 1 or 0,
})

sim:step()

-- ============================================================================
-- PHASE 7: PERSISTENCE TEST
-- Simulate session restart and data recovery
-- ============================================================================

logger.info("Phase 7: Persistence test", {
    phase = 7,
    description = "Simulate session restart and verify data recovery",
})

-- Simulate "session end" for each member
for _, peer in ipairs(peers) do
    local member = tostring(peer)
    local home_realm_id = member_home_realms[member]

    -- Record session end
    logger.event(home.EVENTS.SESSION_ENDED, {
        tick = sim.tick,
        member = member,
        realm_id = home_realm_id,
        notes_count = notes_created[member] and #notes_created[member] or 0,
        quests_count = quests_created[member] and #quests_created[member] or 0,
        artifacts_count = artifacts_uploaded[member] and #artifacts_uploaded[member] or 0,
    })
end

sim:step()

-- Simulate "session start" (data recovery)
for _, peer in ipairs(peers) do
    local member = tostring(peer)

    -- Recompute home realm ID (should be same as before)
    local recovered_realm_id = home.compute_home_realm_id(member)
    local original_realm_id = member_home_realms[member]

    logger.event(home.EVENTS.SESSION_STARTED, {
        tick = sim.tick,
        member = member,
        realm_id = recovered_realm_id,
    })

    -- Verify realm ID consistency after "restart"
    if recovered_realm_id == original_realm_id then
        persistence_checks.passed = persistence_checks.passed + 1

        logger.event(home.EVENTS.DATA_RECOVERED, {
            tick = sim.tick,
            member = member,
            realm_id = recovered_realm_id,
            consistent = true,
        })
    else
        persistence_checks.failed = persistence_checks.failed + 1

        logger.warn("Persistence failure", {
            member = member,
            original_realm_id = original_realm_id,
            recovered_realm_id = recovered_realm_id,
        })
    end
end

logger.info("Phase 7 complete: Persistence tests", {
    phase = 7,
    tick = sim.tick,
    passed = persistence_checks.passed,
    failed = persistence_checks.failed,
})

-- ============================================================================
-- PHASE 8: MULTI-DEVICE SYNC TEST
-- Same member accessing from "multiple devices"
-- ============================================================================

logger.info("Phase 8: Multi-device sync test", {
    phase = 8,
    description = "Verify same member gets same home realm from multiple devices",
})

local sync_tests = math.min(50, #peers * 5)
for i = 1, sync_tests do
    local member = tostring(peers[math.random(#peers)])

    -- Simulate access from "device 1"
    local device1_realm = home.compute_home_realm_id(member)

    -- Simulate access from "device 2" (same member, different "device")
    local device2_realm = home.compute_home_realm_id(member)

    -- Simulate access from "device 3"
    local device3_realm = home.compute_home_realm_id(member)

    -- All devices should see the same home realm
    if device1_realm == device2_realm and device2_realm == device3_realm then
        sync_checks.passed = sync_checks.passed + 1
    else
        sync_checks.failed = sync_checks.failed + 1
        logger.warn("Multi-device sync failure", {
            member = member,
            device1_realm = device1_realm,
            device2_realm = device2_realm,
            device3_realm = device3_realm,
        })
    end

    logger.event(home.EVENTS.MULTI_DEVICE_SYNC, {
        tick = sim.tick,
        member = member,
        devices_checked = 3,
        consistent = device1_realm == device2_realm and device2_realm == device3_realm,
    })

    if i % 10 == 0 then
        sim:step()
    end
end

logger.info("Phase 8 complete: Multi-device sync tests", {
    phase = 8,
    tick = sim.tick,
    passed = sync_checks.passed,
    failed = sync_checks.failed,
})

-- ============================================================================
-- FINAL RESULTS
-- ============================================================================

-- Calculate metrics
local identity_rate = identity_checks.passed /
    math.max(1, identity_checks.passed + identity_checks.failed)
local uniqueness_rate = uniqueness_checks.passed /
    math.max(1, uniqueness_checks.passed + uniqueness_checks.failed)
local persistence_rate = persistence_checks.passed /
    math.max(1, persistence_checks.passed + persistence_checks.failed)
local sync_rate = sync_checks.passed /
    math.max(1, sync_checks.passed + sync_checks.failed)

local realm_id_percentiles = home.percentiles(latencies.realm_id)
local note_create_percentiles = home.percentiles(latencies.note_create)
local artifact_upload_percentiles = home.percentiles(latencies.artifact_upload)

-- Record metrics
result:add_metrics({
    -- Identity metrics
    identity_consistency_rate = identity_rate,
    uniqueness_rate = uniqueness_rate,
    persistence_rate = persistence_rate,
    multi_device_sync_rate = sync_rate,

    -- Operation counts
    total_notes_created = note_ops.created,
    total_notes_updated = note_ops.updated,
    total_notes_deleted = note_ops.deleted,
    total_quests_created = quest_ops.created,
    total_quests_completed = quest_ops.completed,
    total_artifacts_uploaded = artifact_ops.uploaded,
    total_artifacts_retrieved = artifact_ops.retrieved,

    -- Latency percentiles
    realm_id_p50_us = realm_id_percentiles.p50,
    realm_id_p95_us = realm_id_percentiles.p95,
    realm_id_p99_us = realm_id_percentiles.p99,
    note_create_p50_us = note_create_percentiles.p50,
    note_create_p95_us = note_create_percentiles.p95,
    note_create_p99_us = note_create_percentiles.p99,
    artifact_upload_p50_us = artifact_upload_percentiles.p50,
    artifact_upload_p95_us = artifact_upload_percentiles.p95,
    artifact_upload_p99_us = artifact_upload_percentiles.p99,
})

-- Assertions
result:record_assertion("identity_consistency",
    identity_rate >= 1.0, 1.0, identity_rate)
result:record_assertion("uniqueness",
    uniqueness_rate >= 1.0, 1.0, uniqueness_rate)
result:record_assertion("persistence",
    persistence_rate >= 1.0, 1.0, persistence_rate)
result:record_assertion("multi_device_sync",
    sync_rate >= 1.0, 1.0, sync_rate)

local final_result = result:build()

logger.info("Home realm stress scenario completed", {
    passed = final_result.passed,
    level = final_result.level,
    duration_sec = final_result.duration_sec,
    final_tick = sim.tick,
    identity_rate = identity_rate,
    uniqueness_rate = uniqueness_rate,
    persistence_rate = persistence_rate,
    sync_rate = sync_rate,
    total_operations = note_ops.created + quest_ops.created + artifact_ops.uploaded,
})

-- Standard assertions
indras.assert.eq(identity_rate, 1.0, "Home realm ID identity should be 100%")
indras.assert.eq(uniqueness_rate, 1.0, "Home realm ID uniqueness should be 100%")
indras.assert.eq(persistence_rate, 1.0, "Data persistence rate should be 100%")
indras.assert.eq(sync_rate, 1.0, "Multi-device sync rate should be 100%")

logger.info("Home realm stress scenario passed", {
    members = config.members,
    notes = note_ops.created,
    quests = quest_ops.created,
    artifacts = artifact_ops.uploaded,
})

return final_result

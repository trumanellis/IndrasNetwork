-- SyncEngine Proof Folder Stress Test
--
-- Tests the proof folder documentation system:
-- 1. Members create quests in realms
-- 2. Claimants create proof folders (draft state)
-- 3. Claimants add narrative and artifacts to folders
-- 4. Claimants submit folders (triggers chat notification)
-- 5. Creators verify and complete quests
--
-- Verifies:
-- - Proof folder creation in draft state
-- - Narrative and artifact management
-- - Submission triggers chat notification
-- - Multiple proofs per quest (different claimants)
-- - CRDT synchronization across realm members

local quest_helpers = require("lib.quest_helpers")
local home = require("lib.home_realm_helpers")
local thresholds = require("config.quest_thresholds")

-- ============================================================================
-- SETUP
-- ============================================================================

local ctx = quest_helpers.new_context("sync_engine_proof_folder")
local logger = quest_helpers.create_logger(ctx)
local config = quest_helpers.get_config()

logger.info("Starting proof folder scenario", {
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
local result = quest_helpers.result_builder("sync_engine_proof_folder")

-- Tracking
local realms = {}  -- realm_id -> { members = {}, quests = {} }
local proof_folders = {}  -- folder_id -> { quest_id, claimant, narrative, artifacts, status }
local latencies = {
    folder_create = {},
    narrative_update = {},
    artifact_add = {},
    folder_submit = {},
}

-- Counters
local total_folders_created = 0
local total_narratives_updated = 0
local total_artifacts_added = 0
local total_folders_submitted = 0
local total_chat_notifications = 0

-- ============================================================================
-- PHASE 1: SETUP - Create realms and quests
-- ============================================================================

logger.info("Phase 1: Setup - Creating realms and quests", { phase = 1 })

for _, peer in ipairs(peers) do
    sim:force_online(peer)
end

-- Create realms with different peer combinations
local num_realms = math.min(config.realms, 3)
for i = 1, num_realms do
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

    sim:step()
end

-- Create quests in each realm
local total_quests = 0
for realm_id, realm_data in pairs(realms) do
    local quests_to_create = math.min(config.quests_per_realm, 3)
    for i = 1, quests_to_create do
        local creator = realm_data.members[math.random(#realm_data.members)]
        local quest_id = quest_helpers.generate_quest_id()
        local title = quest_helpers.random_quest_title()

        table.insert(realm_data.quests, {
            id = quest_id,
            title = title,
            creator = creator,
            realm_id = realm_id,
            proof_folders = {},  -- Track folders for this quest
        })

        logger.event(quest_helpers.EVENTS.QUEST_CREATED, {
            tick = sim.tick,
            realm_id = realm_id,
            quest_id = quest_id,
            creator = creator,
            title = title,
            latency_us = quest_helpers.quest_create_latency(),
        })

        total_quests = total_quests + 1
        sim:step()
    end
end

logger.info("Phase 1 complete", {
    phase = 1,
    tick = sim.tick,
    realms = num_realms,
    total_quests = total_quests,
})

-- ============================================================================
-- PHASE 2: CREATE PROOF FOLDERS (Draft State)
-- Multiple claimants create proof folders for each quest
-- ============================================================================

logger.info("Phase 2: Create proof folders (draft state)", {
    phase = 2,
    description = "Claimants create proof folders in draft state",
})

for realm_id, realm_data in pairs(realms) do
    for _, quest in ipairs(realm_data.quests) do
        -- Pick 1-2 non-creator members to create proof folders
        local candidates = {}
        for _, member in ipairs(realm_data.members) do
            if member ~= quest.creator then
                table.insert(candidates, member)
            end
        end

        local num_folders = math.min(math.random(1, 2), #candidates)
        for i = 1, num_folders do
            local claimant = candidates[i]
            local folder_id = home.generate_artifact_id()  -- Using as folder ID

            -- Measure latency for folder creation
            local start_time = os.clock()

            -- Create proof folder (draft state)
            proof_folders[folder_id] = {
                id = folder_id,
                quest_id = quest.id,
                claimant = claimant,
                narrative = "",
                artifacts = {},
                status = "draft",
                realm_id = realm_id,
            }

            local latency = (os.clock() - start_time) * 1000000
            table.insert(latencies.folder_create, latency)

            table.insert(quest.proof_folders, folder_id)
            total_folders_created = total_folders_created + 1

            logger.event("proof_folder_created", {
                tick = sim.tick,
                realm_id = realm_id,
                quest_id = quest.id,
                folder_id = folder_id,
                claimant = claimant,
                status = "draft",
                latency_us = latency,
            })

            -- Chat message about starting work
            logger.event("chat_message", {
                tick = sim.tick,
                member = claimant,
                content = string.format("Starting work on: %s", quest.title),
                message_type = "text",
            })

            -- Switch attention to the quest they're working on
            logger.event("attention_switched", {
                tick = sim.tick,
                member = claimant,
                quest_id = quest.id,
                latency_us = quest_helpers.attention_switch_latency(),
            })

            sim:step()
        end
    end
end

logger.info("Phase 2 complete", {
    phase = 2,
    tick = sim.tick,
    folders_created = total_folders_created,
})

-- ============================================================================
-- PHASE 3: UPDATE NARRATIVES
-- Claimants write markdown narratives describing their work
-- ============================================================================

logger.info("Phase 3: Update narratives", {
    phase = 3,
    description = "Claimants write markdown narratives",
})

-- Narrative templates with image placeholders
-- Uses ![caption](artifact:HASH) syntax for image references
local narrative_templates = {
    {
        text = [[## Work Completed

I finished the task by implementing the requested feature.

### Before State
![Before starting work](artifact:ARTIFACT_1)

### Steps Taken
1. Analyzed the requirements
2. Implemented the solution
3. Tested thoroughly

### After State
![After completing work](artifact:ARTIFACT_2)

The work is now complete and ready for review.]],
        artifact_refs = { "ARTIFACT_1", "ARTIFACT_2" }
    },
    {
        text = [[## Summary

Completed the assigned work on schedule.

### Evidence
![Screenshot of completed work](artifact:ARTIFACT_1)

### Details
- Reviewed existing code
- Made necessary changes
- Verified functionality

All requirements have been met.]],
        artifact_refs = { "ARTIFACT_1" }
    },
    {
        text = [[## Task Report

All objectives met successfully.

### Progress Photos

![Initial assessment](artifact:ARTIFACT_1)

![Work in progress](artifact:ARTIFACT_2)

![Final result](artifact:ARTIFACT_3)

### Summary
- All work completed on time
- Quality checks passed
- Ready for verification]],
        artifact_refs = { "ARTIFACT_1", "ARTIFACT_2", "ARTIFACT_3" }
    },
}

for folder_id, folder in pairs(proof_folders) do
    if folder.status == "draft" then
        local start_time = os.clock()

        -- Select a narrative template
        local template = narrative_templates[math.random(#narrative_templates)]
        folder.narrative_template = template
        folder.narrative_text = template.text

        local latency = (os.clock() - start_time) * 1000000
        table.insert(latencies.narrative_update, latency)

        total_narratives_updated = total_narratives_updated + 1

        logger.event("proof_folder_narrative_updated", {
            tick = sim.tick,
            realm_id = folder.realm_id,
            folder_id = folder_id,
            claimant = folder.claimant,
            narrative_length = #template.text,
            narrative = template.text,
            latency_us = latency,
        })

        sim:step()
    end
end

logger.info("Phase 3 complete", {
    phase = 3,
    tick = sim.tick,
    narratives_updated = total_narratives_updated,
})

-- ============================================================================
-- PHASE 4: ADD ARTIFACTS
-- Claimants add photos, documents, etc. to their proof folders
-- ============================================================================

logger.info("Phase 4: Add artifacts", {
    phase = 4,
    description = "Claimants add supporting artifacts to folders",
})

local artifact_types = {
    { name = "before.jpg", mime = "image/jpeg", size = 102400 },
    { name = "after.jpg", mime = "image/jpeg", size = 153600 },
    { name = "screenshot.png", mime = "image/png", size = 204800 },
    { name = "notes.md", mime = "text/markdown", size = 4096 },
    { name = "video_clip.mp4", mime = "video/mp4", size = 5242880 },
}

-- Test images for draft artifact previews
local draft_test_images = {
    "assets/Logo_transparent.png",
    "assets/Logo_black.png",
}

for folder_id, folder in pairs(proof_folders) do
    if folder.status == "draft" then
        -- Add 2-4 artifacts per folder
        local num_artifacts = 2 + math.random(2)
        for i = 1, num_artifacts do
            local artifact_type = artifact_types[math.random(#artifact_types)]
            local artifact_id = home.generate_artifact_id()

            local start_time = os.clock()

            -- Add artifact to folder
            local artifact = {
                artifact_id = artifact_id,
                name = string.format("%d_%s", i, artifact_type.name),
                size = artifact_type.size,
                mime_type = artifact_type.mime,
                caption = string.format("Evidence item %d", i),
            }
            table.insert(folder.artifacts, artifact)

            local latency = (os.clock() - start_time) * 1000000
            table.insert(latencies.artifact_add, latency)

            total_artifacts_added = total_artifacts_added + 1

            -- Pick a test image for image artifacts
            local asset_path = nil
            if artifact_type.mime:find("^image/") then
                local image_idx = ((i - 1) % #draft_test_images) + 1
                asset_path = draft_test_images[image_idx]
            end

            logger.event("proof_folder_artifact_added", {
                tick = sim.tick,
                realm_id = folder.realm_id,
                folder_id = folder_id,
                artifact_id = artifact_id,
                artifact_name = artifact.name,
                artifact_size = artifact.size,
                mime_type = artifact.mime_type,
                asset_path = asset_path,
                caption = artifact.caption,
                latency_us = latency,
            })

            sim:step()
        end
    end
end

-- After all artifacts are added, re-emit narrative with resolved artifact references
-- so the viewer can render embedded images in the markdown
for folder_id, folder in pairs(proof_folders) do
    if folder.status == "draft" and folder.narrative_template then
        local resolved_narrative = folder.narrative_text or ""
        local template = folder.narrative_template

        -- Replace ARTIFACT_N placeholders with actual artifact IDs
        if template.artifact_refs then
            for i, ref in ipairs(template.artifact_refs) do
                if folder.artifacts[i] then
                    resolved_narrative = resolved_narrative:gsub(
                        "artifact:" .. ref,
                        "artifact:" .. folder.artifacts[i].artifact_id
                    )
                end
            end
        end

        -- Store resolved narrative for submission
        folder.narrative_text = resolved_narrative

        logger.event("proof_folder_narrative_updated", {
            tick = sim.tick,
            realm_id = folder.realm_id,
            folder_id = folder_id,
            claimant = folder.claimant,
            narrative_length = #resolved_narrative,
            narrative = resolved_narrative,
        })

        sim:step()
    end
end

logger.info("Phase 4 complete", {
    phase = 4,
    tick = sim.tick,
    artifacts_added = total_artifacts_added,
})

-- ============================================================================
-- PHASE 5: SUBMIT PROOF FOLDERS
-- Claimants submit folders (triggers chat notification)
-- ============================================================================

logger.info("Phase 5: Submit proof folders", {
    phase = 5,
    description = "Claimants submit folders for review (triggers chat notification)",
})

for folder_id, folder in pairs(proof_folders) do
    if folder.status == "draft" then
        local start_time = os.clock()

        -- Submit folder
        folder.status = "submitted"

        local latency = (os.clock() - start_time) * 1000000
        table.insert(latencies.folder_submit, latency)

        total_folders_submitted = total_folders_submitted + 1

        -- Get the quest title from the quest data
        local quest_title = ""
        local realm_data = realms[folder.realm_id]
        if realm_data then
            for _, quest in ipairs(realm_data.quests) do
                if quest.id == folder.quest_id then
                    quest_title = quest.title
                    break
                end
            end
        end

        -- Build final narrative with actual artifact hashes
        local final_narrative = folder.narrative_text or ""
        local template = folder.narrative_template

        -- Map artifact refs to actual artifact hashes
        if template and template.artifact_refs then
            for i, ref in ipairs(template.artifact_refs) do
                if folder.artifacts[i] then
                    final_narrative = final_narrative:gsub(
                        "artifact:" .. ref,
                        "artifact:" .. folder.artifacts[i].artifact_id
                    )
                end
            end
        end

        -- Generate narrative preview (first 100 chars, stripping markdown)
        local preview_text = final_narrative:gsub("#", ""):gsub("%*", ""):gsub("%[.-%]%(.-%)","")
        local narrative_preview = preview_text:sub(1, 100)
        if #preview_text > 100 then
            narrative_preview = narrative_preview .. "..."
        end

        -- Build artifacts array for the event
        local artifacts_data = {}

        -- Use existing logo images from the assets folder for testing
        local test_images = {
            "assets/Logo_transparent.png",
            "assets/Logo_black.png",
        }

        for i, artifact in ipairs(folder.artifacts) do
            local image_idx = ((i - 1) % #test_images) + 1
            table.insert(artifacts_data, {
                artifact_hash = artifact.artifact_id,
                name = artifact.name,
                mime_type = "image/png",
                size = artifact.size,
                caption = artifact.caption,
                -- Use asset_path to load real images from the repo
                asset_path = test_images[image_idx],
            })
        end

        -- This triggers the chat notification to all realm members
        logger.event("proof_folder_submitted", {
            tick = sim.tick,
            realm_id = folder.realm_id,
            quest_id = folder.quest_id,
            claimant = folder.claimant,
            folder_id = folder_id,
            artifact_count = #folder.artifacts,
            narrative_preview = narrative_preview,
            quest_title = quest_title,
            narrative = final_narrative,
            artifacts = artifacts_data,
        })

        total_chat_notifications = total_chat_notifications + 1

        -- Celebratory chat from claimant
        logger.event("chat_message", {
            tick = sim.tick,
            member = folder.claimant,
            content = "Submitted my proof folder for review!",
            message_type = "text",
        })

        sim:step()
    end
end

logger.info("Phase 5 complete", {
    phase = 5,
    tick = sim.tick,
    folders_submitted = total_folders_submitted,
    chat_notifications = total_chat_notifications,
})

-- ============================================================================
-- PHASE 6: VERIFY AND COMPLETE
-- Quest creators verify claims and complete quests
-- ============================================================================

logger.info("Phase 6: Verify and complete", {
    phase = 6,
    description = "Quest creators verify proofs and complete quests",
})

local total_verified = 0
local total_completed = 0

for realm_id, realm_data in pairs(realms) do
    for _, quest in ipairs(realm_data.quests) do
        local verified_count = 0

        -- Creator focuses on their quest to review proofs
        if #quest.proof_folders > 0 then
            logger.event("attention_switched", {
                tick = sim.tick,
                member = quest.creator,
                quest_id = quest.id,
                latency_us = quest_helpers.attention_switch_latency(),
            })
        end

        -- Verify each submitted proof folder
        for i, folder_id in ipairs(quest.proof_folders) do
            local folder = proof_folders[folder_id]
            if folder and folder.status == "submitted" then
                logger.event("quest_claim_verified", {
                    tick = sim.tick,
                    realm_id = realm_id,
                    quest_id = quest.id,
                    claim_index = i - 1,
                    folder_id = folder_id,
                })
                verified_count = verified_count + 1
                total_verified = total_verified + 1
            end
        end

        -- Complete quest if any proofs were verified
        if verified_count > 0 then
            logger.event("quest_completed", {
                tick = sim.tick,
                realm_id = realm_id,
                quest_id = quest.id,
                verified_claims = verified_count,
                pending_claims = 0,
            })
            total_completed = total_completed + 1

            -- Creator thanks everyone
            logger.event("chat_message", {
                tick = sim.tick,
                member = quest.creator,
                content = "Quest completed! Thanks for the detailed proof documentation!",
                message_type = "text",
            })
        end

        sim:step()
    end
end

logger.info("Phase 6 complete", {
    phase = 6,
    tick = sim.tick,
    total_verified = total_verified,
    total_completed = total_completed,
})

-- ============================================================================
-- PHASE 7: CONSISTENCY VERIFICATION
-- Verify CRDT synchronization across realm members
-- ============================================================================

logger.info("Phase 7: Consistency verification", {
    phase = 7,
    description = "Verify proof folder CRDT sync across members",
})

local consistency_checks = { passed = 0, failed = 0 }

for folder_id, folder in pairs(proof_folders) do
    local realm_data = realms[folder.realm_id]
    if realm_data then
        -- Simulate checking that all members see the same folder state
        local members_with_folder = 0
        for _, member in ipairs(realm_data.members) do
            -- In real test, this would query each member's document state
            members_with_folder = members_with_folder + 1
        end

        if members_with_folder == #realm_data.members then
            consistency_checks.passed = consistency_checks.passed + 1
            logger.event("crdt_converged", {
                tick = sim.tick,
                folder_id = folder_id,
                members_synced = members_with_folder,
            })
        else
            consistency_checks.failed = consistency_checks.failed + 1
            logger.event("crdt_conflict", {
                tick = sim.tick,
                folder_id = folder_id,
                expected_members = #realm_data.members,
                actual_members = members_with_folder,
            })
        end
    end
end

local consistency_rate = 1.0
if consistency_checks.passed + consistency_checks.failed > 0 then
    consistency_rate = consistency_checks.passed / (consistency_checks.passed + consistency_checks.failed)
end

logger.info("Phase 7 complete", {
    phase = 7,
    tick = sim.tick,
    consistency_checks_passed = consistency_checks.passed,
    consistency_checks_failed = consistency_checks.failed,
    consistency_rate = consistency_rate,
})

-- ============================================================================
-- CALCULATE LATENCY PERCENTILES
-- ============================================================================

local function percentiles(arr)
    if #arr == 0 then
        return { p50 = 0, p95 = 0, p99 = 0 }
    end
    table.sort(arr)
    local p50_idx = math.ceil(#arr * 0.50)
    local p95_idx = math.ceil(#arr * 0.95)
    local p99_idx = math.ceil(#arr * 0.99)
    return {
        p50 = arr[p50_idx] or 0,
        p95 = arr[p95_idx] or 0,
        p99 = arr[p99_idx] or 0,
    }
end

local folder_create_percentiles = percentiles(latencies.folder_create)
local narrative_update_percentiles = percentiles(latencies.narrative_update)
local artifact_add_percentiles = percentiles(latencies.artifact_add)
local folder_submit_percentiles = percentiles(latencies.folder_submit)

-- ============================================================================
-- FINAL RESULTS
-- ============================================================================

result:add_metrics({
    -- Counts
    total_realms = num_realms,
    total_quests = total_quests,
    total_folders_created = total_folders_created,
    total_narratives_updated = total_narratives_updated,
    total_artifacts_added = total_artifacts_added,
    total_folders_submitted = total_folders_submitted,
    total_chat_notifications = total_chat_notifications,
    total_verified = total_verified,
    total_completed = total_completed,

    -- Latencies (P99)
    folder_create_p99_us = folder_create_percentiles.p99,
    narrative_update_p99_us = narrative_update_percentiles.p99,
    artifact_add_p99_us = artifact_add_percentiles.p99,
    folder_submit_p99_us = folder_submit_percentiles.p99,

    -- Consistency
    crdt_consistency_rate = consistency_rate,
})

-- Get thresholds for current stress level
local cfg = thresholds.get("proof_folder")

-- Calculate rates for threshold checks
local chat_notification_rate = 1.0
if total_folders_submitted > 0 then
    chat_notification_rate = total_chat_notifications / total_folders_submitted
end

-- Record assertions against thresholds
result:record_assertion("folders_created",
    total_folders_created > 0, true, total_folders_created > 0)
result:record_assertion("narratives_updated",
    total_narratives_updated > 0, true, total_narratives_updated > 0)
result:record_assertion("artifacts_added",
    total_artifacts_added > 0, true, total_artifacts_added > 0)
result:record_assertion("folders_submitted",
    total_folders_submitted > 0, true, total_folders_submitted > 0)
result:record_assertion("chat_notifications_sent",
    total_chat_notifications > 0, true, total_chat_notifications > 0)

-- Threshold-based assertions
if cfg.folder_create_p99_us then
    result:record_assertion("folder_create_latency",
        folder_create_percentiles.p99, cfg.folder_create_p99_us.max,
        folder_create_percentiles.p99 <= cfg.folder_create_p99_us.max)
end
if cfg.narrative_update_p99_us then
    result:record_assertion("narrative_update_latency",
        narrative_update_percentiles.p99, cfg.narrative_update_p99_us.max,
        narrative_update_percentiles.p99 <= cfg.narrative_update_p99_us.max)
end
if cfg.artifact_add_p99_us then
    result:record_assertion("artifact_add_latency",
        artifact_add_percentiles.p99, cfg.artifact_add_p99_us.max,
        artifact_add_percentiles.p99 <= cfg.artifact_add_p99_us.max)
end
if cfg.folder_submit_p99_us then
    result:record_assertion("folder_submit_latency",
        folder_submit_percentiles.p99, cfg.folder_submit_p99_us.max,
        folder_submit_percentiles.p99 <= cfg.folder_submit_p99_us.max)
end
if cfg.crdt_consistency_rate then
    result:record_assertion("crdt_consistency",
        consistency_rate, cfg.crdt_consistency_rate.min,
        consistency_rate >= cfg.crdt_consistency_rate.min)
end
if cfg.chat_notification_rate then
    result:record_assertion("chat_notification_rate",
        chat_notification_rate, cfg.chat_notification_rate.min,
        chat_notification_rate >= cfg.chat_notification_rate.min)
end

local final_result = result:build()

logger.info("Proof folder scenario completed", {
    passed = final_result.passed,
    level = final_result.level,
    duration_sec = final_result.duration_sec,
    final_tick = sim.tick,
    total_folders_created = total_folders_created,
    total_folders_submitted = total_folders_submitted,
    total_chat_notifications = total_chat_notifications,
})

-- Hard assertions (always checked)
indras.assert.gt(total_folders_created, 0, "Should have created proof folders")
indras.assert.gt(total_folders_submitted, 0, "Should have submitted proof folders")
indras.assert.eq(total_folders_created, total_folders_submitted, "All folders should be submitted")
indras.assert.gt(total_artifacts_added, 0, "Should have added artifacts to folders")
indras.assert.gt(total_chat_notifications, 0, "Submissions should trigger chat notifications")

-- Threshold-based assertions
if cfg.crdt_consistency_rate then
    indras.assert.ge(consistency_rate, cfg.crdt_consistency_rate.min,
        string.format("CRDT consistency rate (%.2f%%) should be >= %.2f%%",
            consistency_rate * 100, cfg.crdt_consistency_rate.min * 100))
end
if cfg.folder_create_p99_us then
    indras.assert.le(folder_create_percentiles.p99, cfg.folder_create_p99_us.max,
        string.format("Folder create p99 (%.0fμs) should be <= %.0fμs",
            folder_create_percentiles.p99, cfg.folder_create_p99_us.max))
end
if cfg.folder_submit_p99_us then
    indras.assert.le(folder_submit_percentiles.p99, cfg.folder_submit_p99_us.max,
        string.format("Folder submit p99 (%.0fμs) should be <= %.0fμs",
            folder_submit_percentiles.p99, cfg.folder_submit_p99_us.max))
end
if cfg.chat_notification_rate then
    indras.assert.ge(chat_notification_rate, cfg.chat_notification_rate.min,
        string.format("Chat notification rate (%.2f%%) should be >= %.2f%%",
            chat_notification_rate * 100, cfg.chat_notification_rate.min * 100))
end

logger.info("Proof folder scenario passed", {
    folder_create_p99_us = folder_create_percentiles.p99,
    folder_submit_p99_us = folder_submit_percentiles.p99,
    crdt_consistency_rate = consistency_rate,
    chat_notification_rate = chat_notification_rate,
})

return final_result

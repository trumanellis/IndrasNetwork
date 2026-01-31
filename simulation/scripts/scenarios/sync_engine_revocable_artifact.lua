-- SyncEngine Revocable Artifact Sharing Stress Test
--
-- Validates the revocable artifact sharing feature through automated
-- verification and logging. Tests the full lifecycle: share, verify
-- registry state, recall, and verify revocation.
--
-- Phases:
-- 1. Setup: Create mesh with N members, initialize simulation
-- 2. Share Artifacts: Members share artifacts with revocation support
-- 3. Verify Access: All members can access shared artifacts
-- 4. Recall Artifact: Original sharers recall their artifacts
-- 5. Verify Revocation: Confirm keys removed, status updated
-- 6. Permission Tests: Verify non-sharers cannot recall
-- 7. Multi-Artifact Test: Multiple sharers with independent revocation
-- 8. Assertions & Results: Validate all metrics against thresholds
--
-- JSONL Output: All events logged with trace_id for distributed tracing

local artifact = require("lib.artifact_helpers")
local quest_helpers = require("lib.quest_helpers")
local thresholds = require("config.artifact_thresholds")

-- ============================================================================
-- FEATURED TEST ASSET
-- ============================================================================
-- Use the real Logo_black.png asset for realistic testing and viewer display

local FEATURED_ASSET = {
    path = "assets/Logo_black.png",
    name = "Logo_black.png",
    size = 830269,  -- Actual file size in bytes
    mime_type = "image/png",
    description = "IndrasNetwork logo - 1024x1024 PNG",
}

-- ============================================================================
-- SETUP
-- ============================================================================

local ctx = artifact.new_context("sync_engine_revocable_artifact")
local logger = artifact.create_logger(ctx)
local config = artifact.get_config()

logger.info("Starting revocable artifact sharing scenario", {
    level = artifact.get_level(),
    members = config.members,
    artifacts_per_member = config.artifacts_per_member,
    recall_ratio = config.recall_ratio,
    ticks = config.ticks,
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
local result = artifact.result_builder("sync_engine_revocable_artifact")

-- Create realm from all peers
local peer_ids = {}
for _, peer in ipairs(peers) do
    table.insert(peer_ids, tostring(peer))
end
local realm_id = quest_helpers.compute_realm_id(peer_ids)

-- Shared state: Key registry (simulates CRDT document)
local registry = artifact.KeyRegistry.new()

-- Latency tracking
local latencies = {
    share = {},
    recall = {},
    registry_lookup = {},
    download = {},
    sync = {},
}

-- Counters
local counters = {
    artifacts_shared = 0,
    artifacts_recalled = 0,
    permission_denials = 0,
    convergence_successes = 0,
    convergence_failures = 0,
    tombstones_posted = 0,
    pre_recall_downloads = 0,
    post_recall_access_denied = 0,
}

-- Track artifacts by sharer for later recall
local artifacts_by_sharer = {}  -- member_id -> { hash = ..., ... }
local all_artifact_hashes = {}  -- For iteration

-- ============================================================================
-- PHASE 1: SETUP - Bring all peers online, create realm
-- ============================================================================

logger.info("Phase 1: Setup - Bringing peers online and creating realm", {
    phase = 1,
    peer_count = #peers,
})

for _, peer in ipairs(peers) do
    sim:force_online(peer)
end

sim:step()

logger.event("realm_created", {
    tick = sim.tick,
    realm_id = realm_id,
    member_count = #peers,
    members = table.concat(peer_ids, ","),
})

logger.info("Phase 1 complete: All peers online, realm created", {
    phase = 1,
    tick = sim.tick,
    realm_id = realm_id,
})

-- ============================================================================
-- PHASE 2: SHARE ARTIFACTS
-- Each member shares artifacts with revocation support
-- First member shares the featured Logo_black.png asset
-- ============================================================================

logger.info("Phase 2: Share artifacts with revocation support", {
    phase = 2,
    artifacts_per_member = config.artifacts_per_member,
    featured_asset = FEATURED_ASSET.name,
})

-- Track the featured asset hash for special handling
local featured_asset_hash = nil

for idx, peer in ipairs(peers) do
    local member = tostring(peer)
    artifacts_by_sharer[member] = {}

    for i = 1, config.artifacts_per_member do
        local artifact_meta
        local is_featured = false

        -- First member's first artifact is the featured Logo_black.png
        if idx == 1 and i == 1 then
            artifact_meta = {
                name = FEATURED_ASSET.name,
                size = FEATURED_ASSET.size,
                mime_type = FEATURED_ASSET.mime_type,
            }
            is_featured = true
        else
            -- Generate random artifact metadata
            artifact_meta = artifact.random_test_artifact()
        end

        local hash = artifact.generate_hash()
        local encrypted_key = artifact.generate_encrypted_key()

        -- Track featured asset hash
        if is_featured then
            featured_asset_hash = hash
        end

        local start_time = os.clock()

        -- Store in registry
        local success, err = registry:store(hash, artifact_meta, encrypted_key, member, sim.tick)

        local latency = (os.clock() - start_time) * 1000000 + artifact.share_latency()
        table.insert(latencies.share, latency)

        if success then
            counters.artifacts_shared = counters.artifacts_shared + 1
            table.insert(artifacts_by_sharer[member], {
                hash = hash,
                meta = artifact_meta,
                is_featured = is_featured,
            })
            table.insert(all_artifact_hashes, hash)

            logger.event(artifact.EVENTS.ARTIFACT_SHARED_REVOCABLE, {
                tick = sim.tick,
                realm_id = realm_id,
                artifact_hash = hash,
                name = artifact_meta.name,
                size = artifact_meta.size,
                mime_type = artifact_meta.mime_type,
                sharer = member,
                latency_us = latency,
                is_featured = is_featured,
                asset_path = is_featured and FEATURED_ASSET.path or nil,
            })

            logger.event(artifact.EVENTS.KEY_STORED, {
                tick = sim.tick,
                realm_id = realm_id,
                artifact_hash = hash,
                sharer = member,
            })

            -- Extra logging for featured asset
            if is_featured then
                logger.info("Featured asset shared", {
                    asset_path = FEATURED_ASSET.path,
                    artifact_hash = hash,
                    sharer = member,
                    description = FEATURED_ASSET.description,
                })
            end
        else
            logger.warn("Failed to share artifact", {
                member = member,
                error = err,
            })
        end

        if i % config.sync_interval == 0 then
            sim:step()
        end
    end
end

logger.info("Phase 2 complete: Artifacts shared", {
    phase = 2,
    tick = sim.tick,
    artifacts_shared = counters.artifacts_shared,
    featured_asset_hash = featured_asset_hash,
})

sim:step()

-- ============================================================================
-- PHASE 3: VERIFY ACCESS
-- All members can see artifacts in registry and retrieve keys
-- ============================================================================

logger.info("Phase 3: Verify access to shared artifacts", {
    phase = 3,
    total_artifacts = counters.artifacts_shared,
})

local access_granted = 0
local access_denied = 0

for _, hash in ipairs(all_artifact_hashes) do
    local start_time = os.clock()

    -- Check key is retrievable
    local key = registry:get_key(hash)
    local art = registry:get_artifact(hash)

    local latency = (os.clock() - start_time) * 1000000 + artifact.registry_lookup_latency()
    table.insert(latencies.registry_lookup, latency)

    if key and art and art.status == artifact.STATUS.SHARED then
        access_granted = access_granted + 1
        counters.pre_recall_downloads = counters.pre_recall_downloads + 1

        local is_featured = (hash == featured_asset_hash)
        logger.event(artifact.EVENTS.ACCESS_GRANTED, {
            tick = sim.tick,
            realm_id = realm_id,
            artifact_hash = hash,
            artifact_name = art.name,
            status = art.status,
            latency_us = latency,
            is_featured = is_featured,
            asset_path = is_featured and FEATURED_ASSET.path or nil,
        })

        -- Simulate download
        local download_latency = artifact.download_latency(art.size)
        table.insert(latencies.download, download_latency)

        -- Extra logging for featured asset download
        if is_featured then
            logger.event("featured_asset_downloaded", {
                tick = sim.tick,
                realm_id = realm_id,
                artifact_hash = hash,
                asset_path = FEATURED_ASSET.path,
                name = FEATURED_ASSET.name,
                size = FEATURED_ASSET.size,
                download_latency_us = download_latency,
            })
        end
    else
        access_denied = access_denied + 1
        logger.event(artifact.EVENTS.ACCESS_DENIED, {
            tick = sim.tick,
            realm_id = realm_id,
            artifact_hash = hash,
            reason = "key_not_found",
        })
    end
end

logger.info("Phase 3 complete: Access verified", {
    phase = 3,
    tick = sim.tick,
    access_granted = access_granted,
    access_denied = access_denied,
})

sim:step()

-- ============================================================================
-- PHASE 4: RECALL ARTIFACTS
-- Original sharers recall a portion of their artifacts
-- ============================================================================

logger.info("Phase 4: Recall artifacts", {
    phase = 4,
    recall_ratio = config.recall_ratio,
})

local recalled_hashes = {}  -- Track which were recalled

for member, artifacts in pairs(artifacts_by_sharer) do
    local recall_count = math.ceil(#artifacts * config.recall_ratio)

    for i = 1, recall_count do
        local art_data = artifacts[i]
        if art_data then
            local hash = art_data.hash

            local start_time = os.clock()

            local success, err = registry:revoke(hash, member, sim.tick)

            local latency = (os.clock() - start_time) * 1000000 + artifact.recall_latency()
            table.insert(latencies.recall, latency)

            if success then
                counters.artifacts_recalled = counters.artifacts_recalled + 1
                recalled_hashes[hash] = member
                counters.tombstones_posted = counters.tombstones_posted + 1

                local is_featured = (hash == featured_asset_hash)
                logger.event(artifact.EVENTS.ARTIFACT_RECALLED, {
                    tick = sim.tick,
                    realm_id = realm_id,
                    artifact_hash = hash,
                    artifact_name = art_data.meta.name,
                    revoked_by = member,
                    latency_us = latency,
                    is_featured = is_featured,
                    asset_path = is_featured and FEATURED_ASSET.path or nil,
                })

                logger.event(artifact.EVENTS.KEY_REMOVED, {
                    tick = sim.tick,
                    realm_id = realm_id,
                    artifact_hash = hash,
                    revoked_by = member,
                })

                -- Extra logging for featured asset recall
                if is_featured then
                    logger.event("featured_asset_recalled", {
                        tick = sim.tick,
                        realm_id = realm_id,
                        artifact_hash = hash,
                        asset_path = FEATURED_ASSET.path,
                        name = FEATURED_ASSET.name,
                        revoked_by = member,
                        description = "IndrasNetwork logo access revoked - key destroyed",
                    })
                end

                -- Simulate acknowledgment from other members
                for _, peer in ipairs(peers) do
                    local other_member = tostring(peer)
                    if other_member ~= member then
                        logger.event(artifact.EVENTS.RECALL_ACKNOWLEDGED, {
                            tick = sim.tick,
                            realm_id = realm_id,
                            artifact_hash = hash,
                            artifact_name = art_data.meta.name,
                            acknowledged_by = other_member,
                            blob_deleted = true,
                            key_removed = true,
                            is_featured = is_featured,
                        })
                    end
                end
            else
                logger.warn("Failed to recall artifact", {
                    member = member,
                    hash = hash,
                    error = err,
                })
            end
        end
    end
end

logger.info("Phase 4 complete: Artifacts recalled", {
    phase = 4,
    tick = sim.tick,
    artifacts_recalled = counters.artifacts_recalled,
    tombstones_posted = counters.tombstones_posted,
})

sim:step()

-- ============================================================================
-- PHASE 5: VERIFY REVOCATION
-- Confirm keys removed, is_revoked returns true, status updated
-- ============================================================================

logger.info("Phase 5: Verify revocation state", {
    phase = 5,
    recalled_count = counters.artifacts_recalled,
})

local revocation_verified = 0
local revocation_failed = 0

for hash, revoker in pairs(recalled_hashes) do
    -- Verify is_revoked returns true
    local is_revoked = registry:is_revoked(hash)

    -- Verify key is nil
    local key = registry:get_key(hash)

    -- Verify status
    local art = registry:get_artifact(hash)
    local status_correct = art and art.status == artifact.STATUS.RECALLED

    if is_revoked and key == nil and status_correct then
        revocation_verified = revocation_verified + 1
        counters.post_recall_access_denied = counters.post_recall_access_denied + 1

        logger.event(artifact.EVENTS.ACCESS_DENIED, {
            tick = sim.tick,
            realm_id = realm_id,
            artifact_hash = hash,
            reason = "artifact_recalled",
            is_revoked = true,
        })
    else
        revocation_failed = revocation_failed + 1
        logger.warn("Revocation verification failed", {
            hash = hash,
            is_revoked = is_revoked,
            key_nil = key == nil,
            status_correct = status_correct,
        })
    end
end

logger.info("Phase 5 complete: Revocation verified", {
    phase = 5,
    tick = sim.tick,
    verified = revocation_verified,
    failed = revocation_failed,
})

sim:step()

-- ============================================================================
-- PHASE 6: PERMISSION TESTS
-- Verify non-sharers cannot recall artifacts they didn't share
-- ============================================================================

logger.info("Phase 6: Permission tests - non-sharer recall attempts", {
    phase = 6,
})

local permission_tests_run = 0
local permission_denials_correct = 0

-- Try to have each member recall someone else's artifact
for _, peer in ipairs(peers) do
    local attacker = tostring(peer)

    for other_member, artifacts in pairs(artifacts_by_sharer) do
        if other_member ~= attacker and #artifacts > 0 then
            -- Pick an artifact that hasn't been recalled yet
            for _, art_data in ipairs(artifacts) do
                local hash = art_data.hash

                if not recalled_hashes[hash] then
                    permission_tests_run = permission_tests_run + 1

                    -- Check can_revoke returns false
                    local can_revoke = registry:can_revoke(hash, attacker)

                    if not can_revoke then
                        permission_denials_correct = permission_denials_correct + 1
                        counters.permission_denials = counters.permission_denials + 1

                        logger.event(artifact.EVENTS.PERMISSION_DENIED, {
                            tick = sim.tick,
                            realm_id = realm_id,
                            artifact_hash = hash,
                            attempted_by = attacker,
                            actual_sharer = other_member,
                            reason = "not_sharer",
                        })
                    else
                        logger.warn("Permission check failed - non-sharer could revoke", {
                            attacker = attacker,
                            actual_sharer = other_member,
                            hash = hash,
                        })
                    end

                    -- Also try actual revocation (should fail)
                    local success, err = registry:revoke(hash, attacker, sim.tick)
                    if not success then
                        -- Expected - permission denied
                    else
                        logger.error("Non-sharer was able to revoke artifact!", {
                            attacker = attacker,
                            hash = hash,
                        })
                    end

                    break  -- Only test one artifact per pair
                end
            end
        end
    end
end

logger.info("Phase 6 complete: Permission tests", {
    phase = 6,
    tick = sim.tick,
    tests_run = permission_tests_run,
    denials_correct = permission_denials_correct,
})

sim:step()

-- ============================================================================
-- PHASE 7: CRDT CONVERGENCE VERIFICATION
-- Verify all members see consistent registry state
-- ============================================================================

logger.info("Phase 7: CRDT convergence verification", {
    phase = 7,
})

-- Simulate each member's view of the registry
-- In a real test, each member would have their own CRDT replica
-- Here we verify the single registry is consistent

local convergence_tests = 0
local convergence_passed = 0

for _, hash in ipairs(all_artifact_hashes) do
    convergence_tests = convergence_tests + 1

    -- Check all members would see the same state
    local expected_revoked = recalled_hashes[hash] ~= nil
    local actual_revoked = registry:is_revoked(hash)

    if expected_revoked == actual_revoked then
        convergence_passed = convergence_passed + 1
        counters.convergence_successes = counters.convergence_successes + 1

        logger.event(artifact.EVENTS.CRDT_CONVERGED, {
            tick = sim.tick,
            realm_id = realm_id,
            artifact_hash = hash,
            expected_revoked = expected_revoked,
            actual_revoked = actual_revoked,
        })
    else
        counters.convergence_failures = counters.convergence_failures + 1

        logger.event(artifact.EVENTS.CRDT_CONFLICT, {
            tick = sim.tick,
            realm_id = realm_id,
            artifact_hash = hash,
            expected_revoked = expected_revoked,
            actual_revoked = actual_revoked,
        })
    end

    -- Simulate sync latency
    local sync_latency = artifact.sync_latency()
    table.insert(latencies.sync, sync_latency)
end

logger.info("Phase 7 complete: CRDT convergence", {
    phase = 7,
    tick = sim.tick,
    tests = convergence_tests,
    passed = convergence_passed,
})

sim:step()

-- ============================================================================
-- PHASE 8: ASSERTIONS & RESULTS
-- Validate all metrics against thresholds
-- ============================================================================

logger.info("Phase 8: Final assertions and results", {
    phase = 8,
})

-- Calculate metrics
local share_percentiles = artifact.percentiles(latencies.share)
local recall_percentiles = artifact.percentiles(latencies.recall)
local lookup_percentiles = artifact.percentiles(latencies.registry_lookup)

local key_storage_rate = counters.artifacts_shared > 0
    and counters.artifacts_shared / (config.members * config.artifacts_per_member)
    or 0

local revocation_rate = counters.artifacts_recalled > 0
    and revocation_verified / counters.artifacts_recalled
    or 1.0

local permission_denial_rate = permission_tests_run > 0
    and permission_denials_correct / permission_tests_run
    or 1.0

local post_recall_inaccessible_rate = counters.artifacts_recalled > 0
    and counters.post_recall_access_denied / counters.artifacts_recalled
    or 1.0

local convergence_rate = convergence_tests > 0
    and convergence_passed / convergence_tests
    or 1.0

local tombstone_rate = counters.artifacts_recalled > 0
    and counters.tombstones_posted / counters.artifacts_recalled
    or 1.0

-- Get thresholds
local cfg = thresholds.get("revocable_artifact")

-- Record metrics
result:add_metrics({
    -- Counts
    artifacts_shared = counters.artifacts_shared,
    artifacts_recalled = counters.artifacts_recalled,
    permission_denials = counters.permission_denials,
    tombstones_posted = counters.tombstones_posted,
    convergence_successes = counters.convergence_successes,
    convergence_failures = counters.convergence_failures,

    -- Latencies (P99)
    share_p99_us = share_percentiles.p99,
    recall_p99_us = recall_percentiles.p99,
    registry_lookup_p99_us = lookup_percentiles.p99,

    -- Latencies (P50)
    share_p50_us = share_percentiles.p50,
    recall_p50_us = recall_percentiles.p50,
    registry_lookup_p50_us = lookup_percentiles.p50,

    -- Rates
    key_storage_success_rate = key_storage_rate,
    revocation_success_rate = revocation_rate,
    permission_denial_rate = permission_denial_rate,
    post_recall_inaccessible_rate = post_recall_inaccessible_rate,
    crdt_convergence_rate = convergence_rate,
    tombstone_rate = tombstone_rate,
})

-- Record assertions
result:record_assertion("artifacts_shared",
    counters.artifacts_shared > 0, true, counters.artifacts_shared > 0)
result:record_assertion("artifacts_recalled",
    counters.artifacts_recalled > 0, true, counters.artifacts_recalled > 0)
result:record_assertion("key_storage_success_rate",
    key_storage_rate >= (cfg.key_storage_success_rate and cfg.key_storage_success_rate.min or 1.0),
    cfg.key_storage_success_rate and cfg.key_storage_success_rate.min or 1.0, key_storage_rate)
result:record_assertion("revocation_success_rate",
    revocation_rate >= (cfg.revocation_success_rate and cfg.revocation_success_rate.min or 1.0),
    cfg.revocation_success_rate and cfg.revocation_success_rate.min or 1.0, revocation_rate)
result:record_assertion("permission_denial_rate",
    permission_denial_rate >= (cfg.permission_denial_rate and cfg.permission_denial_rate.min or 1.0),
    cfg.permission_denial_rate and cfg.permission_denial_rate.min or 1.0, permission_denial_rate)
result:record_assertion("post_recall_inaccessible_rate",
    post_recall_inaccessible_rate >= (cfg.post_recall_inaccessible_rate and cfg.post_recall_inaccessible_rate.min or 1.0),
    cfg.post_recall_inaccessible_rate and cfg.post_recall_inaccessible_rate.min or 1.0, post_recall_inaccessible_rate)
result:record_assertion("crdt_convergence_rate",
    convergence_rate >= (cfg.crdt_convergence_rate and cfg.crdt_convergence_rate.min or 0.99),
    cfg.crdt_convergence_rate and cfg.crdt_convergence_rate.min or 0.99, convergence_rate)
result:record_assertion("tombstone_rate",
    tombstone_rate >= (cfg.tombstone_rate and cfg.tombstone_rate.min or 1.0),
    cfg.tombstone_rate and cfg.tombstone_rate.min or 1.0, tombstone_rate)

-- Latency assertions
if cfg.share_p99_us then
    result:record_assertion("share_latency_p99",
        share_percentiles.p99 <= cfg.share_p99_us.max,
        cfg.share_p99_us.max, share_percentiles.p99)
end
if cfg.recall_p99_us then
    result:record_assertion("recall_latency_p99",
        recall_percentiles.p99 <= cfg.recall_p99_us.max,
        cfg.recall_p99_us.max, recall_percentiles.p99)
end
if cfg.registry_lookup_p99_us then
    result:record_assertion("registry_lookup_latency_p99",
        lookup_percentiles.p99 <= cfg.registry_lookup_p99_us.max,
        cfg.registry_lookup_p99_us.max, lookup_percentiles.p99)
end

local final_result = result:build()

-- Get registry stats
local registry_stats = registry:stats()

logger.info("Revocable artifact scenario completed", {
    passed = final_result.passed,
    level = final_result.level,
    duration_sec = final_result.duration_sec,
    final_tick = sim.tick,
    artifacts_shared = counters.artifacts_shared,
    artifacts_recalled = counters.artifacts_recalled,
    permission_denials = counters.permission_denials,
    convergence_rate = convergence_rate,
    currently_shared = registry_stats.currently_shared,
    currently_recalled = registry_stats.currently_recalled,
})

-- Hard assertions (always checked)
indras.assert.gt(counters.artifacts_shared, 0, "Should have shared artifacts")
indras.assert.gt(counters.artifacts_recalled, 0, "Should have recalled artifacts")
indras.assert.eq(revocation_rate, 1.0, "All revocations should succeed")
indras.assert.eq(permission_denial_rate, 1.0, "All non-sharer revocations should be denied")
indras.assert.eq(post_recall_inaccessible_rate, 1.0, "All recalled artifacts should be inaccessible")

-- Threshold-based assertions
if cfg.crdt_convergence_rate then
    indras.assert.ge(convergence_rate, cfg.crdt_convergence_rate.min,
        string.format("CRDT convergence rate (%.2f%%) should be >= %.2f%%",
            convergence_rate * 100, cfg.crdt_convergence_rate.min * 100))
end
if cfg.share_p99_us then
    indras.assert.le(share_percentiles.p99, cfg.share_p99_us.max,
        string.format("Share p99 latency (%.0fus) should be <= %.0fus",
            share_percentiles.p99, cfg.share_p99_us.max))
end
if cfg.recall_p99_us then
    indras.assert.le(recall_percentiles.p99, cfg.recall_p99_us.max,
        string.format("Recall p99 latency (%.0fus) should be <= %.0fus",
            recall_percentiles.p99, cfg.recall_p99_us.max))
end

logger.info("Revocable artifact scenario passed", {
    share_p99_us = share_percentiles.p99,
    recall_p99_us = recall_percentiles.p99,
    key_storage_rate = key_storage_rate,
    revocation_rate = revocation_rate,
    permission_denial_rate = permission_denial_rate,
    crdt_convergence_rate = convergence_rate,
})

return final_result

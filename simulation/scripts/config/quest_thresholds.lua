-- Quest Thresholds Configuration
--
-- Defines pass/fail thresholds for quest and peer-based realm simulation scenarios.
-- Organized by stress level (quick/medium/full) and scenario type.

local thresholds = {}

-- ============================================================================
-- PEER-BASED REALM THRESHOLDS
-- ============================================================================

thresholds.peer_realm = {
    quick = {
        -- Realm ID consistency: 100% same peers = same ID
        realm_id_consistency = { min = 1.0 },
        -- Realm ID uniqueness: 100% different peers = different ID
        realm_id_uniqueness = { min = 1.0 },
        -- Cached lookup latency: p99 < 200 microseconds
        cached_lookup_p99_us = { max = 200 },
        -- New realm creation latency: p99 < 2000 microseconds (2ms)
        realm_create_p99_us = { max = 2000 },
        -- Concurrent access success rate: > 99%
        concurrent_success_rate = { min = 0.99 },
    },
    medium = {
        realm_id_consistency = { min = 1.0 },
        realm_id_uniqueness = { min = 1.0 },
        cached_lookup_p99_us = { max = 200 },
        realm_create_p99_us = { max = 2000 },
        concurrent_success_rate = { min = 0.995 },
    },
    full = {
        realm_id_consistency = { min = 1.0 },
        realm_id_uniqueness = { min = 1.0 },
        cached_lookup_p99_us = { max = 200 },
        realm_create_p99_us = { max = 2000 },
        concurrent_success_rate = { min = 0.999 },
    }
}

-- ============================================================================
-- QUEST LIFECYCLE THRESHOLDS
-- ============================================================================

thresholds.quest_lifecycle = {
    quick = {
        -- Quest create latency: p99 < 1000 microseconds (1ms)
        quest_create_p99_us = { max = 1000 },
        -- Proof submit latency: p99 < 500 microseconds
        proof_submit_p99_us = { max = 500 },
        -- Claim verify latency: p99 < 300 microseconds
        claim_verify_p99_us = { max = 300 },
        -- CRDT convergence rate: > 99%
        crdt_convergence_rate = { min = 0.99 },
        -- Multi-claimant consistency: All members see same claims
        multi_claimant_consistency = { min = 1.0 },
        -- Proof artifact references: Valid ArtifactIds
        valid_artifact_refs = { min = 1.0 },
    },
    medium = {
        quest_create_p99_us = { max = 1000 },
        proof_submit_p99_us = { max = 500 },
        claim_verify_p99_us = { max = 300 },
        crdt_convergence_rate = { min = 0.995 },
        multi_claimant_consistency = { min = 1.0 },
        valid_artifact_refs = { min = 1.0 },
    },
    full = {
        quest_create_p99_us = { max = 1000 },
        proof_submit_p99_us = { max = 500 },
        claim_verify_p99_us = { max = 300 },
        crdt_convergence_rate = { min = 0.999 },
        multi_claimant_consistency = { min = 1.0 },
        valid_artifact_refs = { min = 1.0 },
    }
}

-- ============================================================================
-- HOME REALM THRESHOLDS
-- ============================================================================

thresholds.home_realm = {
    quick = {
        -- Home realm ID consistency: 100% same member = same ID
        identity_consistency_rate = { min = 1.0 },
        -- Home realm ID uniqueness: 100% different members = different ID
        uniqueness_rate = { min = 1.0 },
        -- Persistence rate: Data survives restarts
        persistence_rate = { min = 1.0 },
        -- Multi-device sync: Same member from different devices
        multi_device_sync_rate = { min = 1.0 },
        -- Realm ID computation latency: p99 < 100 microseconds (very fast)
        realm_id_p99_us = { max = 100 },
        -- Note creation latency: p99 < 500 microseconds
        note_create_p99_us = { max = 500 },
        -- Artifact upload latency: p99 < 3000 microseconds (3ms)
        artifact_upload_p99_us = { max = 3000 },
    },
    medium = {
        identity_consistency_rate = { min = 1.0 },
        uniqueness_rate = { min = 1.0 },
        persistence_rate = { min = 1.0 },
        multi_device_sync_rate = { min = 1.0 },
        realm_id_p99_us = { max = 100 },
        note_create_p99_us = { max = 500 },
        artifact_upload_p99_us = { max = 3000 },
    },
    full = {
        identity_consistency_rate = { min = 1.0 },
        uniqueness_rate = { min = 1.0 },
        persistence_rate = { min = 1.0 },
        multi_device_sync_rate = { min = 1.0 },
        realm_id_p99_us = { max = 100 },
        note_create_p99_us = { max = 500 },
        artifact_upload_p99_us = { max = 3000 },
    }
}

-- ============================================================================
-- CONTACTS STRESS THRESHOLDS
-- ============================================================================

thresholds.contacts_stress = {
    quick = {
        -- Contacts realm join latency: p99 < 2000 microseconds (2ms)
        contacts_join_p99_us = { max = 2000 },
        -- Add contact latency: p99 < 500 microseconds
        add_contact_p99_us = { max = 500 },
        -- Contact sync convergence: > 99%
        contact_sync_convergence = { min = 0.99 },
        -- Auto-subscription success rate: 100%
        auto_subscription_success = { min = 1.0 },
    },
    medium = {
        contacts_join_p99_us = { max = 2000 },
        add_contact_p99_us = { max = 500 },
        contact_sync_convergence = { min = 0.995 },
        auto_subscription_success = { min = 1.0 },
    },
    full = {
        contacts_join_p99_us = { max = 2000 },
        add_contact_p99_us = { max = 500 },
        contact_sync_convergence = { min = 0.999 },
        auto_subscription_success = { min = 1.0 },
    }
}

-- ============================================================================
-- PROOF FOLDER THRESHOLDS
-- ============================================================================

thresholds.proof_folder = {
    quick = {
        -- Folder creation latency: p99 < 500 microseconds
        folder_create_p99_us = { max = 500 },
        -- Narrative update latency: p99 < 300 microseconds
        narrative_update_p99_us = { max = 300 },
        -- Artifact add latency: p99 < 400 microseconds
        artifact_add_p99_us = { max = 400 },
        -- Folder submit latency: p99 < 1000 microseconds (1ms)
        folder_submit_p99_us = { max = 1000 },
        -- CRDT consistency rate: All members see same folder state
        crdt_consistency_rate = { min = 0.99 },
        -- Chat notification rate: 100% of submissions trigger notifications
        chat_notification_rate = { min = 1.0 },
    },
    medium = {
        folder_create_p99_us = { max = 500 },
        narrative_update_p99_us = { max = 300 },
        artifact_add_p99_us = { max = 400 },
        folder_submit_p99_us = { max = 1000 },
        crdt_consistency_rate = { min = 0.995 },
        chat_notification_rate = { min = 1.0 },
    },
    full = {
        folder_create_p99_us = { max = 500 },
        narrative_update_p99_us = { max = 300 },
        artifact_add_p99_us = { max = 400 },
        folder_submit_p99_us = { max = 1000 },
        crdt_consistency_rate = { min = 0.999 },
        chat_notification_rate = { min = 1.0 },
    }
}

-- ============================================================================
-- UTILITY FUNCTIONS
-- ============================================================================

--- Get thresholds for a scenario at current stress level
-- @param scenario_name string The scenario name
-- @return table Thresholds for the scenario
function thresholds.get(scenario_name)
    local level = os.getenv("STRESS_LEVEL") or "medium"
    local scenario_thresholds = thresholds[scenario_name]

    if not scenario_thresholds then
        return {}
    end

    return scenario_thresholds[level] or scenario_thresholds.medium or {}
end

--- Get all scenario names
-- @return table Array of scenario names
function thresholds.scenarios()
    local scenarios = {}
    for k, v in pairs(thresholds) do
        if type(v) == "table" and v.quick ~= nil then
            table.insert(scenarios, k)
        end
    end
    return scenarios
end

return thresholds

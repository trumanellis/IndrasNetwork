-- Artifact Sharing Thresholds Configuration
--
-- Defines pass/fail thresholds for revocable artifact sharing simulation scenarios.
-- Organized by stress level (quick/medium/full) and metric type.

local thresholds = {}

-- ============================================================================
-- REVOCABLE ARTIFACT SHARING THRESHOLDS
-- ============================================================================

thresholds.revocable_artifact = {
    quick = {
        -- Share operation latency: p99 < 3000 microseconds (3ms)
        -- Includes encryption overhead
        share_p99_us = { max = 3000 },

        -- Recall operation latency: p99 < 500 microseconds
        recall_p99_us = { max = 500 },

        -- Registry lookup latency: p99 < 200 microseconds
        registry_lookup_p99_us = { max = 200 },

        -- Key storage success rate: 100%
        key_storage_success_rate = { min = 1.0 },

        -- Revocation success rate: 100% (when sharer initiates)
        revocation_success_rate = { min = 1.0 },

        -- Permission denial rate: 100% (non-sharer attempts should fail)
        permission_denial_rate = { min = 1.0 },

        -- Post-recall inaccessibility: 100%
        post_recall_inaccessible_rate = { min = 1.0 },

        -- CRDT convergence rate: > 99%
        crdt_convergence_rate = { min = 0.99 },

        -- Tombstone posting rate: 100% of recalls post tombstones
        tombstone_rate = { min = 1.0 },
    },
    medium = {
        share_p99_us = { max = 3000 },
        recall_p99_us = { max = 500 },
        registry_lookup_p99_us = { max = 200 },
        key_storage_success_rate = { min = 1.0 },
        revocation_success_rate = { min = 1.0 },
        permission_denial_rate = { min = 1.0 },
        post_recall_inaccessible_rate = { min = 1.0 },
        crdt_convergence_rate = { min = 0.995 },
        tombstone_rate = { min = 1.0 },
    },
    full = {
        share_p99_us = { max = 3000 },
        recall_p99_us = { max = 500 },
        registry_lookup_p99_us = { max = 200 },
        key_storage_success_rate = { min = 1.0 },
        revocation_success_rate = { min = 1.0 },
        permission_denial_rate = { min = 1.0 },
        post_recall_inaccessible_rate = { min = 1.0 },
        crdt_convergence_rate = { min = 0.999 },
        tombstone_rate = { min = 1.0 },
    }
}

-- ============================================================================
-- MULTI-ARTIFACT STRESS THRESHOLDS
-- ============================================================================

thresholds.multi_artifact = {
    quick = {
        -- Independent revocation rate: Each sharer can only revoke their own
        independent_revocation_rate = { min = 1.0 },

        -- Concurrent share success rate
        concurrent_share_success_rate = { min = 0.99 },

        -- Registry consistency after concurrent ops
        registry_consistency_rate = { min = 0.99 },
    },
    medium = {
        independent_revocation_rate = { min = 1.0 },
        concurrent_share_success_rate = { min = 0.995 },
        registry_consistency_rate = { min = 0.995 },
    },
    full = {
        independent_revocation_rate = { min = 1.0 },
        concurrent_share_success_rate = { min = 0.999 },
        registry_consistency_rate = { min = 0.999 },
    }
}

-- ============================================================================
-- DOWNLOAD ACCESS THRESHOLDS
-- ============================================================================

thresholds.download_access = {
    quick = {
        -- Pre-recall download success rate: 100%
        pre_recall_download_success_rate = { min = 1.0 },

        -- Post-recall download failure rate: 100% (should all fail)
        post_recall_download_failure_rate = { min = 1.0 },

        -- Download latency for small artifacts (< 100KB): p99 < 1000us
        small_download_p99_us = { max = 1000 },

        -- Download latency for large artifacts (> 1MB): p99 < 5000us
        large_download_p99_us = { max = 5000 },
    },
    medium = {
        pre_recall_download_success_rate = { min = 1.0 },
        post_recall_download_failure_rate = { min = 1.0 },
        small_download_p99_us = { max = 1000 },
        large_download_p99_us = { max = 5000 },
    },
    full = {
        pre_recall_download_success_rate = { min = 1.0 },
        post_recall_download_failure_rate = { min = 1.0 },
        small_download_p99_us = { max = 1000 },
        large_download_p99_us = { max = 5000 },
    }
}

-- ============================================================================
-- GRANT/REVOKE ACCESS THRESHOLDS
-- ============================================================================

thresholds.grant_access = {
    quick = {
        -- Grant operation latency: p99 < 500 microseconds
        grant_p99_us = { max = 500 },

        -- Revoke operation latency: p99 < 300 microseconds
        revoke_p99_us = { max = 300 },

        -- Grant success rate (valid grants succeed)
        grant_success_rate = { min = 1.0 },

        -- Permanent grants survive recall: 100%
        permanent_survives_recall_rate = { min = 1.0 },

        -- Revocable grants removed on recall: 100%
        revocable_removed_on_recall_rate = { min = 1.0 },

        -- Cannot revoke permanent: 100% denial rate
        permanent_revoke_denial_rate = { min = 1.0 },
    },
    medium = {
        grant_p99_us = { max = 500 },
        revoke_p99_us = { max = 300 },
        grant_success_rate = { min = 1.0 },
        permanent_survives_recall_rate = { min = 1.0 },
        revocable_removed_on_recall_rate = { min = 1.0 },
        permanent_revoke_denial_rate = { min = 1.0 },
    },
    full = {
        grant_p99_us = { max = 500 },
        revoke_p99_us = { max = 300 },
        grant_success_rate = { min = 1.0 },
        permanent_survives_recall_rate = { min = 1.0 },
        revocable_removed_on_recall_rate = { min = 1.0 },
        permanent_revoke_denial_rate = { min = 1.0 },
    }
}

-- ============================================================================
-- TRANSFER THRESHOLDS
-- ============================================================================

thresholds.transfer = {
    quick = {
        -- Transfer operation latency: p99 < 1000 microseconds
        transfer_p99_us = { max = 1000 },

        -- Transfer success rate
        transfer_success_rate = { min = 1.0 },

        -- Sender gets revocable access back: 100%
        sender_revocable_back_rate = { min = 1.0 },

        -- Original marked as transferred: 100%
        original_transferred_rate = { min = 1.0 },
    },
    medium = {
        transfer_p99_us = { max = 1000 },
        transfer_success_rate = { min = 1.0 },
        sender_revocable_back_rate = { min = 1.0 },
        original_transferred_rate = { min = 1.0 },
    },
    full = {
        transfer_p99_us = { max = 1000 },
        transfer_success_rate = { min = 1.0 },
        sender_revocable_back_rate = { min = 1.0 },
        original_transferred_rate = { min = 1.0 },
    }
}

-- ============================================================================
-- TIMED ACCESS THRESHOLDS
-- ============================================================================

thresholds.timed_access = {
    quick = {
        -- Timed grant expiry accuracy: access denied after expiry tick
        expiry_enforcement_rate = { min = 1.0 },

        -- GC removes expired grants
        gc_removal_rate = { min = 1.0 },

        -- Access valid before expiry
        pre_expiry_access_rate = { min = 1.0 },
    },
    medium = {
        expiry_enforcement_rate = { min = 1.0 },
        gc_removal_rate = { min = 1.0 },
        pre_expiry_access_rate = { min = 1.0 },
    },
    full = {
        expiry_enforcement_rate = { min = 1.0 },
        gc_removal_rate = { min = 1.0 },
        pre_expiry_access_rate = { min = 1.0 },
    }
}

-- ============================================================================
-- RECOVERY THRESHOLDS
-- ============================================================================

thresholds.recovery = {
    quick = {
        -- Recovery latency: p99 < 10000 microseconds (10ms)
        recovery_p99_us = { max = 10000 },

        -- Permanent grant recovery success rate
        permanent_recovery_rate = { min = 1.0 },

        -- Overall recovery success rate (best-effort for non-permanent)
        overall_recovery_rate = { min = 0.8 },
    },
    medium = {
        recovery_p99_us = { max = 10000 },
        permanent_recovery_rate = { min = 1.0 },
        overall_recovery_rate = { min = 0.85 },
    },
    full = {
        recovery_p99_us = { max = 10000 },
        permanent_recovery_rate = { min = 1.0 },
        overall_recovery_rate = { min = 0.9 },
    }
}

-- ============================================================================
-- UTILITY FUNCTIONS
-- ============================================================================

--- Get thresholds for a scenario at current stress level
-- @param scenario_name string The scenario name
-- @return table Thresholds for the scenario
function thresholds.get(scenario_name)
    local level = os.getenv("STRESS_LEVEL") or "quick"
    local scenario_thresholds = thresholds[scenario_name]

    if not scenario_thresholds then
        return {}
    end

    return scenario_thresholds[level] or scenario_thresholds.quick or {}
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

--- Assert a metric against a threshold
-- @param metric_name string Name of the metric
-- @param actual number Actual value
-- @param threshold table Threshold with min and/or max
-- @return boolean passed, string|nil error_msg
function thresholds.check(metric_name, actual, threshold)
    if not threshold then
        return true, nil
    end

    if threshold.min ~= nil and actual < threshold.min then
        return false, string.format(
            "%s (%.4f) is below minimum (%.4f)",
            metric_name, actual, threshold.min
        )
    end

    if threshold.max ~= nil and actual > threshold.max then
        return false, string.format(
            "%s (%.4f) exceeds maximum (%.4f)",
            metric_name, actual, threshold.max
        )
    end

    return true, nil
end

--- Assert multiple metrics against thresholds
-- @param metrics table Map of metric_name -> actual_value
-- @param scenario_thresholds table Map of metric_name -> threshold
-- @return boolean all_passed, table failures
function thresholds.check_all(metrics, scenario_thresholds)
    local failures = {}

    for metric_name, threshold in pairs(scenario_thresholds) do
        local actual = metrics[metric_name]
        if actual ~= nil then
            local passed, err = thresholds.check(metric_name, actual, threshold)
            if not passed then
                table.insert(failures, {
                    metric = metric_name,
                    actual = actual,
                    threshold = threshold,
                    error = err,
                })
            end
        end
    end

    return #failures == 0, failures
end

return thresholds

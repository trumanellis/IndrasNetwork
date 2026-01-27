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

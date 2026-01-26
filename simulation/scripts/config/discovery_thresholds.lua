-- Discovery Thresholds Configuration
--
-- Defines pass/fail thresholds for peer discovery simulation scenarios.
-- Organized by stress level (quick/medium/full) and scenario type.

local thresholds = {}

-- ============================================================================
-- DISCOVERY SCENARIO THRESHOLDS
-- ============================================================================

thresholds.discovery = {
    quick = {
        -- Discovery performance
        discovery_latency_p99_ticks = { max = 30 },
        discovery_success_rate = { min = 0.99 },
        -- PQ key exchange
        pq_key_completeness = { min = 1.0 },
        pq_kem_key_size = { min = 1184, max = 1184 },  -- ML-KEM-768
        pq_dsa_key_size = { min = 1952, max = 1952 },  -- ML-DSA-65
        -- Rate limiting
        rate_limit_violations = { max = 0 },
        -- Convergence
        convergence_ticks = { max = 100 },
    },
    medium = {
        discovery_latency_p99_ticks = { max = 20 },
        discovery_success_rate = { min = 0.995 },
        pq_key_completeness = { min = 1.0 },
        pq_kem_key_size = { min = 1184, max = 1184 },
        pq_dsa_key_size = { min = 1952, max = 1952 },
        rate_limit_violations = { max = 0 },
        convergence_ticks = { max = 75 },
    },
    full = {
        discovery_latency_p99_ticks = { max = 15 },
        discovery_success_rate = { min = 0.999 },
        pq_key_completeness = { min = 1.0 },
        pq_kem_key_size = { min = 1184, max = 1184 },
        pq_dsa_key_size = { min = 1952, max = 1952 },
        rate_limit_violations = { max = 0 },
        convergence_ticks = { max = 50 },
    }
}

-- ============================================================================
-- TWO-PEER DISCOVERY THRESHOLDS
-- ============================================================================

thresholds.two_peer = {
    quick = {
        mutual_discovery_ticks = { max = 50 },
        pq_key_exchange_complete = { min = 1.0 },
    },
    medium = {
        mutual_discovery_ticks = { max = 40 },
        pq_key_exchange_complete = { min = 1.0 },
    },
    full = {
        mutual_discovery_ticks = { max = 30 },
        pq_key_exchange_complete = { min = 1.0 },
    }
}

-- ============================================================================
-- MULTI-PEER DISCOVERY THRESHOLDS
-- ============================================================================

thresholds.multi_peer = {
    quick = {
        discovery_completeness = { min = 1.0 },
        realms_formed = { min = 1 },
        avg_discovery_latency_ticks = { max = 30 },
    },
    medium = {
        discovery_completeness = { min = 1.0 },
        realms_formed = { min = 3 },
        avg_discovery_latency_ticks = { max = 25 },
    },
    full = {
        discovery_completeness = { min = 1.0 },
        realms_formed = { min = 5 },
        avg_discovery_latency_ticks = { max = 20 },
    }
}

-- ============================================================================
-- LATE JOINER THRESHOLDS
-- ============================================================================

thresholds.late_joiner = {
    quick = {
        catchup_success_rate = { min = 0.95 },
        catchup_latency_ticks = { max = 50 },
        new_realms_available = { min = 1 },
    },
    medium = {
        catchup_success_rate = { min = 0.98 },
        catchup_latency_ticks = { max = 40 },
        new_realms_available = { min = 3 },
    },
    full = {
        catchup_success_rate = { min = 0.99 },
        catchup_latency_ticks = { max = 30 },
        new_realms_available = { min = 5 },
    }
}

-- ============================================================================
-- RATE LIMIT THRESHOLDS
-- ============================================================================

thresholds.rate_limit = {
    quick = {
        rate_limit_enforced = { min = 1.0 },       -- Must enforce limits
        rate_limit_violations = { max = 0 },       -- No violations
        response_after_window = { min = 1.0 },     -- Must respond after window
    },
    medium = {
        rate_limit_enforced = { min = 1.0 },
        rate_limit_violations = { max = 0 },
        response_after_window = { min = 1.0 },
    },
    full = {
        rate_limit_enforced = { min = 1.0 },
        rate_limit_violations = { max = 0 },
        response_after_window = { min = 1.0 },
    }
}

-- ============================================================================
-- RECONNECT THRESHOLDS
-- ============================================================================

thresholds.reconnect = {
    quick = {
        reconnect_success_rate = { min = 0.95 },
        rediscovery_latency_ticks = { max = 50 },
        awareness_restored = { min = 1.0 },
    },
    medium = {
        reconnect_success_rate = { min = 0.98 },
        rediscovery_latency_ticks = { max = 40 },
        awareness_restored = { min = 1.0 },
    },
    full = {
        reconnect_success_rate = { min = 0.99 },
        rediscovery_latency_ticks = { max = 30 },
        awareness_restored = { min = 1.0 },
    }
}

-- ============================================================================
-- PQ KEYS THRESHOLDS
-- ============================================================================

thresholds.pq_keys = {
    quick = {
        kem_key_size_correct = { min = 1.0 },
        dsa_key_size_correct = { min = 1.0 },
        key_propagation_complete = { min = 1.0 },
    },
    medium = {
        kem_key_size_correct = { min = 1.0 },
        dsa_key_size_correct = { min = 1.0 },
        key_propagation_complete = { min = 1.0 },
    },
    full = {
        kem_key_size_correct = { min = 1.0 },
        dsa_key_size_correct = { min = 1.0 },
        key_propagation_complete = { min = 1.0 },
    }
}

-- ============================================================================
-- STRESS TEST THRESHOLDS
-- ============================================================================

thresholds.stress = {
    quick = {
        member_consistency = { min = 0.95 },
        convergence_ticks = { max = 100 },
        discovery_failures = { max = 5 },
        churn_recovery_rate = { min = 0.9 },
    },
    medium = {
        member_consistency = { min = 0.98 },
        convergence_ticks = { max = 150 },
        discovery_failures = { max = 10 },
        churn_recovery_rate = { min = 0.95 },
    },
    full = {
        member_consistency = { min = 0.99 },
        convergence_ticks = { max = 200 },
        discovery_failures = { max = 20 },
        churn_recovery_rate = { min = 0.98 },
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

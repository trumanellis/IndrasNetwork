-- Attention Thresholds Configuration
--
-- Defines pass/fail thresholds for attention tracking simulation scenarios.
-- Organized by stress level (quick/medium/full).

local thresholds = {}

-- ============================================================================
-- ATTENTION STRESS THRESHOLDS
-- ============================================================================

thresholds.attention_stress = {
    quick = {
        -- Attention switch latency: p99 < 1000 microseconds (1ms)
        attention_switch_p99_us = { max = 1000 },
        -- CRDT convergence rate for attention document
        attention_crdt_convergence = { min = 0.99 },
        -- Attention calculation latency: p99 < 5000 microseconds (5ms)
        attention_calc_p99_us = { max = 5000 },
        -- Ranking consistency (same input = same ranking)
        ranking_consistency = { min = 1.0 },
        -- Focus tracking accuracy
        focus_tracking_accuracy = { min = 1.0 },
    },
    medium = {
        attention_switch_p99_us = { max = 1000 },
        attention_crdt_convergence = { min = 0.995 },
        attention_calc_p99_us = { max = 5000 },
        ranking_consistency = { min = 1.0 },
        focus_tracking_accuracy = { min = 1.0 },
    },
    full = {
        attention_switch_p99_us = { max = 1000 },
        attention_crdt_convergence = { min = 0.999 },
        attention_calc_p99_us = { max = 5000 },
        ranking_consistency = { min = 1.0 },
        focus_tracking_accuracy = { min = 1.0 },
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

return thresholds

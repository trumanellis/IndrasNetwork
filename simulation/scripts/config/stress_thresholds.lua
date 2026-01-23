-- Stress Test Thresholds Configuration
--
-- Defines pass/fail thresholds for all stress test scenarios.
-- Organized by stress level (quick/medium/full) and module.

local thresholds = {}

-- ============================================================================
-- CORE MODULE THRESHOLDS
-- ============================================================================
thresholds.core = {
    quick = {
        peerid_generation_rate = { min = 100 },       -- ops/sec
        serialization_fidelity = { min = 1.0 },       -- 100%
        packet_error_rate = { max = 0 },              -- 0%
        event_throughput = { min = 500 },             -- events/tick
    },
    medium = {
        peerid_generation_rate = { min = 500 },
        serialization_fidelity = { min = 1.0 },
        packet_error_rate = { max = 0 },
        event_throughput = { min = 1000 },
    },
    full = {
        peerid_generation_rate = { min = 1000 },
        serialization_fidelity = { min = 1.0 },
        packet_error_rate = { max = 0 },
        event_throughput = { min = 5000 },
    }
}

-- ============================================================================
-- CRYPTO MODULE THRESHOLDS (PQ signatures and KEM)
-- ============================================================================
thresholds.crypto = {
    quick = {
        signature_latency_p99_us = { max = 1000 },    -- < 1ms
        verification_latency_p99_us = { max = 500 },  -- < 0.5ms
        kem_encap_latency_p99_us = { max = 200 },     -- < 0.2ms
        kem_decap_latency_p99_us = { max = 200 },
        signature_failure_rate = { max = 0.001 },     -- < 0.1%
        kem_failure_rate = { max = 0.001 },
    },
    medium = {
        signature_latency_p99_us = { max = 800 },
        verification_latency_p99_us = { max = 400 },
        kem_encap_latency_p99_us = { max = 150 },
        kem_decap_latency_p99_us = { max = 150 },
        signature_failure_rate = { max = 0.001 },
        kem_failure_rate = { max = 0.001 },
    },
    full = {
        signature_latency_p99_us = { max = 600 },
        verification_latency_p99_us = { max = 300 },
        kem_encap_latency_p99_us = { max = 120 },
        kem_decap_latency_p99_us = { max = 120 },
        signature_failure_rate = { max = 0.001 },
        kem_failure_rate = { max = 0.001 },
    }
}

-- ============================================================================
-- TRANSPORT MODULE THRESHOLDS
-- ============================================================================
thresholds.transport = {
    quick = {
        connection_success_rate = { min = 0.9 },      -- > 90%
        connection_latency_p95 = { max = 10 },        -- < 10 ticks
        discovery_latency = { max = 5 },              -- < 5 ticks
    },
    medium = {
        connection_success_rate = { min = 0.95 },
        connection_latency_p95 = { max = 8 },
        discovery_latency = { max = 4 },
    },
    full = {
        connection_success_rate = { min = 0.95 },
        connection_latency_p95 = { max = 5 },
        discovery_latency = { max = 3 },
    }
}

-- ============================================================================
-- ROUTING MODULE THRESHOLDS
-- ============================================================================
thresholds.routing = {
    quick = {
        delivery_rate = { min = 0.6 },                -- > 60% under chaos
        avg_hops = { max = 10 },                      -- < 10 hops
        backprop_success_rate = { min = 0.3 },        -- > 30%
    },
    medium = {
        delivery_rate = { min = 0.5 },                -- > 50% (more chaos)
        avg_hops = { max = 8 },
        backprop_success_rate = { min = 0.4 },
    },
    full = {
        delivery_rate = { min = 0.4 },                -- > 40% (heavy chaos)
        avg_hops = { max = 6 },
        backprop_success_rate = { min = 0.5 },
    }
}

-- ============================================================================
-- STORAGE MODULE THRESHOLDS
-- ============================================================================
thresholds.storage = {
    quick = {
        append_throughput = { min = 500 },            -- events/tick
        pending_queue_max = { max = 1000 },           -- max queue depth
        write_error_rate = { max = 0 },               -- 0% errors
    },
    medium = {
        append_throughput = { min = 1000 },
        pending_queue_max = { max = 5000 },
        write_error_rate = { max = 0 },
    },
    full = {
        append_throughput = { min = 2000 },
        pending_queue_max = { max = 10000 },
        write_error_rate = { max = 0 },
    }
}

-- ============================================================================
-- SYNC MODULE THRESHOLDS
-- ============================================================================
thresholds.sync = {
    quick = {
        convergence_rate = { min = 0.95 },            -- > 95% converge
        convergence_ticks_per_peer = { max = 5 },     -- < 5 ticks/peer
        merge_conflict_rate = { max = 0.1 },          -- < 10% conflicts
    },
    medium = {
        convergence_rate = { min = 0.98 },
        convergence_ticks_per_peer = { max = 3 },
        merge_conflict_rate = { max = 0.05 },
    },
    full = {
        convergence_rate = { min = 0.99 },
        convergence_ticks_per_peer = { max = 2 },
        merge_conflict_rate = { max = 0.02 },
    }
}

-- ============================================================================
-- GOSSIP MODULE THRESHOLDS
-- ============================================================================
thresholds.gossip = {
    quick = {
        delivery_rate = { min = 0.95 },               -- > 95%
        dissemination_latency = { max = 10 },         -- < 10 ticks
        duplication_rate = { max = 0.1 },             -- < 10% dupes
    },
    medium = {
        delivery_rate = { min = 0.98 },
        dissemination_latency = { max = 7 },
        duplication_rate = { max = 0.05 },
    },
    full = {
        delivery_rate = { min = 0.99 },
        dissemination_latency = { max = 5 },
        duplication_rate = { max = 0.03 },
    }
}

-- ============================================================================
-- MESSAGING MODULE THRESHOLDS
-- ============================================================================
thresholds.messaging = {
    quick = {
        e2e_delivery_rate = { min = 0.5 },            -- > 50%
        confirmation_latency_avg = { max = 500 },     -- < 500 ticks
        interface_isolation = { min = 1.0 },          -- 100% isolation
    },
    medium = {
        e2e_delivery_rate = { min = 0.7 },
        confirmation_latency_avg = { max = 300 },
        interface_isolation = { min = 1.0 },
    },
    full = {
        e2e_delivery_rate = { min = 0.9 },
        confirmation_latency_avg = { max = 200 },
        interface_isolation = { min = 1.0 },
    }
}

-- ============================================================================
-- LOGGING MODULE THRESHOLDS
-- ============================================================================
thresholds.logging = {
    quick = {
        log_throughput_per_tick = { min = 5 },        -- > 5 events/tick
        correlation_completeness = { min = 1.0 },     -- 100%
    },
    medium = {
        log_throughput_per_tick = { min = 20 },
        correlation_completeness = { min = 1.0 },
    },
    full = {
        log_throughput_per_tick = { min = 100 },
        correlation_completeness = { min = 1.0 },
    }
}

-- ============================================================================
-- DTN MODULE THRESHOLDS
-- ============================================================================
thresholds.dtn = {
    quick = {
        bundle_delivery_rate = { min = 0.5 },         -- > 50% (high offline)
        custody_success_rate = { min = 0.8 },         -- > 80%
        spray_efficiency = { min = 0.3 },             -- > 30%
    },
    medium = {
        bundle_delivery_rate = { min = 0.6 },
        custody_success_rate = { min = 0.85 },
        spray_efficiency = { min = 0.4 },
    },
    full = {
        bundle_delivery_rate = { min = 0.7 },
        custody_success_rate = { min = 0.9 },
        spray_efficiency = { min = 0.5 },
    }
}

-- ============================================================================
-- NODE MODULE THRESHOLDS
-- ============================================================================
thresholds.node = {
    quick = {
        interface_creation_rate = { min = 1.0 },      -- 100%
        join_success_rate = { min = 0.95 },           -- > 95%
        isolation_violations = { max = 0 },           -- 0 leaks
    },
    medium = {
        interface_creation_rate = { min = 1.0 },
        join_success_rate = { min = 0.98 },
        isolation_violations = { max = 0 },
    },
    full = {
        interface_creation_rate = { min = 1.0 },
        join_success_rate = { min = 0.99 },
        isolation_violations = { max = 0 },
    }
}

-- ============================================================================
-- ENGINE MODULE THRESHOLDS
-- ============================================================================
thresholds.engine = {
    quick = {
        ticks_per_second = { min = 500 },             -- > 500 ticks/sec
        memory_mb = { max = 100 },                    -- < 100 MB
        event_processing_rate = { min = 1000 },       -- > 1000 events/sec
    },
    medium = {
        ticks_per_second = { min = 1000 },
        memory_mb = { max = 250 },
        event_processing_rate = { min = 5000 },
    },
    full = {
        ticks_per_second = { min = 500 },             -- Lower due to scale
        memory_mb = { max = 500 },
        event_processing_rate = { min = 10000 },
    }
}

-- ============================================================================
-- INTEGRATION SCENARIO THRESHOLDS
-- ============================================================================
thresholds.integration = {
    quick = {
        end_to_end_success_rate = { min = 0.8 },
        total_errors = { max = 5 },
    },
    medium = {
        end_to_end_success_rate = { min = 0.9 },
        total_errors = { max = 3 },
    },
    full = {
        end_to_end_success_rate = { min = 0.95 },
        total_errors = { max = 1 },
    }
}

-- ============================================================================
-- UTILITY FUNCTIONS
-- ============================================================================

--- Get thresholds for a module at current stress level
-- @param module_name string The module name
-- @return table Thresholds for the module
function thresholds.get(module_name)
    local level = os.getenv("STRESS_LEVEL") or "medium"
    local module_thresholds = thresholds[module_name]

    if not module_thresholds then
        return {}
    end

    return module_thresholds[level] or module_thresholds.medium or {}
end

--- Get all module names
-- @return table Array of module names
function thresholds.modules()
    local modules = {}
    for k, v in pairs(thresholds) do
        if type(v) == "table" and v.quick ~= nil then
            table.insert(modules, k)
        end
    end
    return modules
end

return thresholds

-- Stress Test Thresholds Configuration
-- Defines pass/fail criteria for all stress test modules at different levels
-- Thresholds become progressively stricter: quick -> medium (~20% stricter) -> full (~20% stricter)

return {
    -- Quick stress test thresholds (baseline)
    quick = {
        crypto = {
            signature_p99_us = 2000,
            kem_p99_us = 500,
            failure_rate_max = 0.01
        },
        transport = {
            connection_rate_min = 0.90,
            latency_max_ticks = 10
        },
        routing = {
            delivery_rate_min = 0.85,
            avg_latency_max_ticks = 20,
            backprop_rate_min = 0.90
        },
        storage = {
            throughput_min = 500
        },
        sync = {
            convergence_max_ticks = 50
        },
        gossip = {
            delivery_rate_min = 0.95,
            duplication_max = 0.10
        },
        messaging = {
            delivery_rate_min = 0.90
        },
        logging = {
            throughput_min = 5000
        },
        dtn = {
            delivery_rate_min = 0.70,
            custody_rate_min = 0.85
        },
        node = {
            creation_rate_min = 0.95,
            join_rate_min = 0.90
        },
        core = {
            error_rate_max = 0.0
        },
        engine = {
            ticks_per_sec_min = 500
        }
    },

    -- Medium stress test thresholds (~20% stricter than quick)
    medium = {
        crypto = {
            signature_p99_us = 1600,
            kem_p99_us = 400,
            failure_rate_max = 0.008
        },
        transport = {
            connection_rate_min = 0.92,
            latency_max_ticks = 8
        },
        routing = {
            delivery_rate_min = 0.88,
            avg_latency_max_ticks = 16,
            backprop_rate_min = 0.92
        },
        storage = {
            throughput_min = 600
        },
        sync = {
            convergence_max_ticks = 40
        },
        gossip = {
            delivery_rate_min = 0.96,
            duplication_max = 0.08
        },
        messaging = {
            delivery_rate_min = 0.92
        },
        logging = {
            throughput_min = 6000
        },
        dtn = {
            delivery_rate_min = 0.76,
            custody_rate_min = 0.88
        },
        node = {
            creation_rate_min = 0.96,
            join_rate_min = 0.92
        },
        core = {
            error_rate_max = 0.0
        },
        engine = {
            ticks_per_sec_min = 600
        }
    },

    -- Full stress test thresholds (~20% stricter than medium)
    full = {
        crypto = {
            signature_p99_us = 1280,
            kem_p99_us = 320,
            failure_rate_max = 0.0064
        },
        transport = {
            connection_rate_min = 0.936,
            latency_max_ticks = 6
        },
        routing = {
            delivery_rate_min = 0.904,
            avg_latency_max_ticks = 13,
            backprop_rate_min = 0.936
        },
        storage = {
            throughput_min = 720
        },
        sync = {
            convergence_max_ticks = 32
        },
        gossip = {
            delivery_rate_min = 0.968,
            duplication_max = 0.064
        },
        messaging = {
            delivery_rate_min = 0.936
        },
        logging = {
            throughput_min = 7200
        },
        dtn = {
            delivery_rate_min = 0.808,
            custody_rate_min = 0.904
        },
        node = {
            creation_rate_min = 0.968,
            join_rate_min = 0.936
        },
        core = {
            error_rate_max = 0.0
        },
        engine = {
            ticks_per_sec_min = 720
        }
    }
}

-- PQ Thresholds Configuration
--
-- Defines pass/fail thresholds for PQ crypto stress tests.
-- Based on ML-DSA-65 and ML-KEM-768 performance characteristics.

return {
    -- ML-DSA-65 signature thresholds
    signature = {
        -- Maximum acceptable p99 latency in microseconds
        -- ML-DSA-65 sign typically takes 150-300us
        latency_p99_us = 1000,

        -- Minimum throughput (operations per second)
        -- Conservative estimate: ~3000-5000 ops/sec on modern hardware
        throughput_min = 1000,

        -- Maximum acceptable failure rate (0.0 - 1.0)
        -- Cryptographic operations should rarely fail
        failure_rate_max = 0.001,
    },

    -- ML-DSA-65 verification thresholds
    verification = {
        -- Verification is typically faster than signing
        latency_p99_us = 500,

        -- Higher throughput expected for verification
        throughput_min = 2000,

        -- Same strict failure rate
        failure_rate_max = 0.001,
    },

    -- ML-KEM-768 encapsulation thresholds
    kem_encap = {
        -- KEM operations are typically faster than signatures
        -- ML-KEM-768 encap typically takes 50-100us
        latency_p99_us = 200,

        -- Higher throughput expected
        throughput_min = 5000,
    },

    -- ML-KEM-768 decapsulation thresholds
    kem_decap = {
        -- Similar to encapsulation
        latency_p99_us = 200,

        throughput_min = 5000,

        -- Decapsulation can fail with invalid ciphertext
        failure_rate_max = 0.01,
    },

    -- Invite flow thresholds
    invite = {
        -- Minimum success rate for invite acceptance
        -- Allow for some KEM failures and network issues
        success_rate_min = 0.95,

        -- Maximum time for invite round-trip (ticks)
        round_trip_max_ticks = 10,
    },

    -- Sync cycle thresholds
    sync = {
        -- Maximum time for a complete sync cycle
        cycle_time_max_ms = 100,

        -- Maximum signatures per sync cycle (scales with members)
        -- Prevent unbounded signature storms
        signatures_per_cycle_max = 10000,

        -- Maximum bandwidth per cycle (bytes)
        bandwidth_per_cycle_max = 10 * 1024 * 1024,  -- 10MB
    },

    -- Network/message delivery thresholds (during PQ operations)
    network = {
        -- Message delivery should not be severely impacted by PQ overhead
        delivery_rate_min = 0.90,

        -- Average delivery latency should stay reasonable
        avg_latency_max_ticks = 10,
    },

    -- Chaos/stress test thresholds (relaxed for adversarial conditions)
    chaos = {
        -- Lower expectations under chaos conditions
        signature_failure_rate_max = 0.05,  -- 5%
        kem_failure_rate_max = 0.05,
        invite_success_rate_min = 0.85,
        delivery_rate_min = 0.50,
    },
}

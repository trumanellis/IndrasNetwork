-- Pass Story Attack Simulation
--
-- Tests resistance to attack vectors against pass story authentication:
-- 1. Brute force cost analysis
-- 2. Dictionary attack simulation
-- 3. Partial knowledge (non-decomposability)
-- 4. Entropy gate bypass attempts
-- 5. Slot position sensitivity
-- 6. Normalization confusion
-- 7. Timing side-channel check

package.path = package.path .. ";lib/?.lua;scripts/lib/?.lua;simulation/scripts/lib/?.lua"
local sec = require("security_helpers")
local ctx = sec.new_context("pass_story_attack")

-- Configuration from environment
local stress = os.getenv("STRESS_LEVEL") or "quick"
local ITERATIONS = ({ quick = 10, medium = 50, full = 200 })[stress] or 10

indras.log.info("Starting pass story attack simulation", {
    trace_id = ctx.trace_id,
    stress_level = stress,
    iterations = ITERATIONS
})

-- Track results
local tests_passed = 0
local tests_failed = 0

local function test(name, fn)
    local ok, err = pcall(fn)
    if ok then
        tests_passed = tests_passed + 1
        indras.log.info("PASS: " .. name, { trace_id = ctx.trace_id })
    else
        tests_failed = tests_failed + 1
        indras.log.error("FAIL: " .. name, { trace_id = ctx.trace_id, error = tostring(err) })
    end
end

-- Dictionary of common English words
local COMMON_WORDS = {
    "the", "a", "an", "and", "or", "but", "of", "to", "in", "for",
    "on", "with", "at", "by", "from", "as", "is", "was", "are", "were",
    "be", "been", "have", "has", "had", "do", "does", "did", "will",
    "would", "could", "should", "may", "might", "can", "must"
}

local function dictionary_story()
    local slots = {}
    for i = 1, 23 do
        slots[i] = COMMON_WORDS[math.random(#COMMON_WORDS)]
    end
    return slots
end

-- =========================================================================
-- 1. Brute Force Cost Analysis
-- =========================================================================

test("Brute force: single verification takes measurable time", function()
    local story = sec.STORIES.EMBER
    local stats = sec.benchmark(function()
        indras.pass_story.verification_token(story)
    end, ITERATIONS)

    indras.log.info("Brute force cost analysis", {
        trace_id = ctx.trace_id,
        avg_us = stats.avg,
        p95_us = stats.p95,
        samples = stats.samples
    })

    indras.assert.gt(stats.avg, 0, "Verification must take measurable time")
end)

test("Brute force: entropy makes exhaustive search infeasible", function()
    local story = sec.STORIES.EMBER
    local gate = indras.pass_story.entropy_gate(story)

    -- With >200 bits, 2^200 attempts is infeasible
    indras.assert.gt(gate.total_bits, 200,
        "Story entropy should exceed 200 bits for brute force resistance")

    indras.log.info("Brute force entropy analysis", {
        trace_id = ctx.trace_id,
        total_bits = string.format("%.1f", gate.total_bits),
        search_space = string.format("2^%.0f", gate.total_bits)
    })
end)

-- =========================================================================
-- 2. Dictionary Attack Simulation
-- =========================================================================

test("Dictionary attack: pure common-word stories fail entropy gate", function()
    local failures = 0
    for i = 1, ITERATIONS do
        local dict_story = dictionary_story()
        local gate = indras.pass_story.entropy_gate(dict_story)
        if not gate.passed then
            failures = failures + 1
        end
    end

    indras.log.info("Dictionary attack results", {
        trace_id = ctx.trace_id,
        rejected = failures,
        total = ITERATIONS,
        reject_rate = string.format("%.0f%%", failures / ITERATIONS * 100)
    })

    -- Most dictionary-only stories should fail
    indras.assert.gt(failures, ITERATIONS * 0.8,
        "At least 80% of pure dictionary stories should fail entropy gate")
end)

test("Dictionary attack: mixed common/rare words analyzed", function()
    -- Half common, half rare
    local mixed = {
        "the", "chrysanthemum", "a", "bioluminescent", "for",
        "kaleidoscope", "is", "metamorphosis", "but", "serendipity",
        "of", "constellation", "and", "ephemeral", "or",
        "quintessence", "to", "iridescent", "in", "luminiferous",
        "at", "crepuscular", "petrichor"
    }
    local gate = indras.pass_story.entropy_gate(mixed)

    indras.log.info("Mixed dictionary analysis", {
        trace_id = ctx.trace_id,
        total_bits = string.format("%.1f", gate.total_bits),
        passed = gate.passed,
        weak_slots = gate.weak_slots and #gate.weak_slots or 0
    })
end)

-- =========================================================================
-- 3. Partial Knowledge Attack (Non-decomposability)
-- =========================================================================

test("Partial knowledge: changing ANY single slot changes token", function()
    local base = sec.STORIES.EMBER
    local base_token = indras.pass_story.verification_token(base)
    local mutations_detected = 0

    for slot_idx = 1, 23 do
        local mutated = {}
        for i = 1, 23 do mutated[i] = base[i] end
        mutated[slot_idx] = "xyzzy_replaced"

        local new_token = indras.pass_story.verification_token(mutated)
        if new_token ~= base_token then
            mutations_detected = mutations_detected + 1
        end
    end

    indras.log.info("Single-slot mutation sensitivity", {
        trace_id = ctx.trace_id,
        mutations_detected = mutations_detected,
        total_slots = 23
    })

    indras.assert.eq(mutations_detected, 23,
        "All 23 slot mutations must change the token")
end)

test("Partial knowledge: knowing 22 slots gives zero partial progress", function()
    local base = sec.STORIES.EMBER
    local base_token = indras.pass_story.verification_token(base)
    local partial_matches = 0

    for i = 1, ITERATIONS do
        -- Copy 22 slots from base, randomize 1
        local attempt = {}
        for j = 1, 23 do attempt[j] = base[j] end
        local target_slot = math.random(23)
        attempt[target_slot] = "wrong_guess_" .. tostring(i)

        local attempt_token = indras.pass_story.verification_token(attempt)
        if attempt_token == base_token then
            partial_matches = partial_matches + 1
        end
    end

    indras.assert.eq(partial_matches, 0,
        "Zero partial matches (non-decomposable: no oracle for individual slots)")

    indras.log.info("Partial knowledge attack results", {
        trace_id = ctx.trace_id,
        attempts = ITERATIONS,
        partial_matches = partial_matches
    })
end)

-- =========================================================================
-- 4. Entropy Gate Bypass Attempts
-- =========================================================================

test("Bypass attempt: repeated rare words (duplicate detection)", function()
    -- 3 rare words rotated across all 23 slots
    local rare_words = {"sesquipedalian", "antidisestablishmentarianism", "floccinaucinihilipilification"}
    local repeated = {}
    for i = 1, 23 do
        repeated[i] = rare_words[(i % 3) + 1]
    end

    local gate = indras.pass_story.entropy_gate(repeated)
    indras.log.info("Bypass: repeated rare words", {
        trace_id = ctx.trace_id,
        total_bits = string.format("%.1f", gate.total_bits),
        passed = gate.passed
    })
    -- Duplicate penalty should reduce entropy significantly
end)

test("Bypass attempt: semantic cluster (all space-themed)", function()
    local space = {
        "nebula", "galaxy", "cosmos", "stellar", "orbit",
        "asteroid", "comet", "planet", "moon", "star",
        "constellation", "supernova", "quasar", "pulsar", "void",
        "celestial", "astronomical", "interstellar", "cosmic", "spacetime",
        "gravity", "eclipse", "meteoric"
    }

    local gate = indras.pass_story.entropy_gate(space)
    indras.log.info("Bypass: semantic cluster", {
        trace_id = ctx.trace_id,
        total_bits = string.format("%.1f", gate.total_bits),
        passed = gate.passed
    })
end)

test("Bypass attempt: few rare words padding common ones", function()
    local padded = {
        "the", "the", "the", "the", "chrysanthemum",
        "the", "the", "the", "bioluminescent", "the",
        "the", "the", "the", "the", "the",
        "kaleidoscope", "the", "the", "the", "the",
        "the", "the", "the"
    }

    local gate = indras.pass_story.entropy_gate(padded)
    indras.log.info("Bypass: sparse rare words", {
        trace_id = ctx.trace_id,
        total_bits = string.format("%.1f", gate.total_bits),
        passed = gate.passed,
        weak_slots = gate.weak_slots and #gate.weak_slots or 0
    })

    indras.assert.false_(gate.passed,
        "Sparse rare words in sea of common should fail entropy gate")
end)

-- =========================================================================
-- 5. Slot Position Sensitivity
-- =========================================================================

test("Position sensitivity: adjacent slot swaps change token", function()
    local base = sec.STORIES.EMBER
    local base_token = indras.pass_story.verification_token(base)
    local swaps_detected = 0

    for i = 1, 22 do
        local swapped = {}
        for j = 1, 23 do swapped[j] = base[j] end
        swapped[i], swapped[i+1] = swapped[i+1], swapped[i]

        local new_token = indras.pass_story.verification_token(swapped)
        if new_token ~= base_token then
            swaps_detected = swaps_detected + 1
        end
    end

    indras.log.info("Adjacent swap sensitivity", {
        trace_id = ctx.trace_id,
        detected = swaps_detected,
        total = 22
    })

    indras.assert.eq(swaps_detected, 22,
        "All adjacent swaps must produce different tokens")
end)

test("Position sensitivity: cross-stage swaps change token", function()
    local base = sec.STORIES.EMBER
    local base_token = indras.pass_story.verification_token(base)
    local pairs = {{1, 12}, {3, 18}, {7, 23}, {2, 20}, {5, 15}}
    local detected = 0

    for _, pair in ipairs(pairs) do
        local swapped = {}
        for j = 1, 23 do swapped[j] = base[j] end
        swapped[pair[1]], swapped[pair[2]] = swapped[pair[2]], swapped[pair[1]]

        if indras.pass_story.verification_token(swapped) ~= base_token then
            detected = detected + 1
        end
    end

    indras.assert.eq(detected, #pairs,
        "All cross-stage swaps must produce different tokens")
end)

-- =========================================================================
-- 6. Normalization Confusion Attack
-- =========================================================================

test("Normalization: whitespace variants produce same token", function()
    local base = sec.STORIES.ZEPHYR
    local base_token = indras.pass_story.verification_token(base)

    local ws = {}
    for i = 1, 23 do
        ws[i] = "  " .. base[i] .. "  "
    end

    local ws_token = indras.pass_story.verification_token(ws)
    indras.assert.eq(ws_token, base_token,
        "Whitespace variants must normalize to same token")
end)

test("Normalization: case variants produce same token", function()
    local base = sec.STORIES.ZEPHYR
    local base_token = indras.pass_story.verification_token(base)

    local upper = {}
    for i = 1, 23 do
        upper[i] = string.upper(base[i])
    end

    local upper_token = indras.pass_story.verification_token(upper)
    indras.assert.eq(upper_token, base_token,
        "Case variants must normalize to same token")
end)

test("Normalization: truly different words produce different tokens", function()
    sec.assert_different_tokens(sec.STORIES.EMBER, sec.STORIES.ZEPHYR,
        "Different stories must produce different tokens")
end)

-- =========================================================================
-- 7. Timing Side-Channel Check
-- =========================================================================

test("Timing: no significant difference between different stories", function()
    local stats1 = sec.benchmark(function()
        indras.pass_story.verification_token(sec.STORIES.EMBER)
    end, ITERATIONS)

    local stats2 = sec.benchmark(function()
        indras.pass_story.verification_token(sec.STORIES.ZEPHYR)
    end, ITERATIONS)

    local mean_diff_pct = math.abs(stats1.avg - stats2.avg) / math.max(stats1.avg, 1) * 100

    indras.log.info("Timing side-channel analysis", {
        trace_id = ctx.trace_id,
        story1_avg_us = stats1.avg,
        story2_avg_us = stats2.avg,
        difference_pct = string.format("%.1f%%", mean_diff_pct)
    })

    -- Allow generous tolerance (50%) due to system noise
    indras.assert.lt(mean_diff_pct, 50,
        "Timing difference should be < 50% (no significant side channel)")
end)

test("Timing: valid vs invalid stories have similar timing", function()
    local stats_valid = sec.benchmark(function()
        indras.pass_story.verification_token(sec.STORIES.EMBER)
    end, ITERATIONS)

    local stats_weak = sec.benchmark(function()
        indras.pass_story.verification_token(sec.STORIES.WEAK)
    end, ITERATIONS)

    local diff_pct = math.abs(stats_valid.avg - stats_weak.avg) / math.max(stats_valid.avg, 1) * 100

    indras.log.info("Valid vs weak timing", {
        trace_id = ctx.trace_id,
        valid_avg_us = stats_valid.avg,
        weak_avg_us = stats_weak.avg,
        difference_pct = string.format("%.1f%%", diff_pct)
    })
end)

-- =========================================================================
-- Summary
-- =========================================================================

indras.log.info("Pass story attack simulation complete", {
    trace_id = ctx.trace_id,
    tests_passed = tests_passed,
    tests_failed = tests_failed,
    total_tests = tests_passed + tests_failed,
    stress_level = stress
})

indras.assert.eq(tests_failed, 0,
    string.format("%d attack test(s) failed out of %d", tests_failed, tests_passed + tests_failed))

return {
    tests_passed = tests_passed,
    tests_failed = tests_failed,
    total = tests_passed + tests_failed,
    stress_level = stress
}

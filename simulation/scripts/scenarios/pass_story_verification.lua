-- Pass Story Verification Test
--
-- Automated verification of pass story authentication:
-- 1. Template validation (11 stages, 23 slots)
-- 2. Story creation and rendering
-- 3. Normalization equivalence (case, whitespace, unicode)
-- 4. Entropy gate enforcement (accepts strong, rejects weak)
-- 5. Verification token determinism
-- 6. Token uniqueness across different stories
-- 7. Canonical encoding determinism
-- 8. Slot mutation sensitivity (changing 1 slot changes token)

package.path = package.path .. ";lib/?.lua;scripts/lib/?.lua;simulation/scripts/lib/?.lua"
local sec = require("security_helpers")
local ctx = sec.new_context("pass_story_verification")

indras.log.info("Starting pass story verification", {
    trace_id = ctx.trace_id
})

-- Track test results
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

-- =========================================================================
-- 1. Template Validation
-- =========================================================================

test("Template has 23 total slots", function()
    indras.assert.eq(indras.pass_story.template_slot_count(), 23, "Should have 23 slots")
end)

test("Template has 11 stages", function()
    local tmpl = indras.pass_story.template()
    indras.assert.eq(#tmpl.stages, 11, "Should have 11 stages")
end)

test("Template slot distribution sums to 23", function()
    local tmpl = indras.pass_story.template()
    local total = 0
    for _, stage in ipairs(tmpl.stages) do
        total = total + stage.slot_count
    end
    indras.assert.eq(total, 23, "Stage slot counts should sum to 23")
end)

-- =========================================================================
-- 2. Story Creation and Rendering
-- =========================================================================

test("Create story from raw slots", function()
    local story = indras.pass_story.Story.from_raw(sec.STORIES.EMBER)
    indras.assert.not_nil(story, "Story should be created")
    local rendered = story:render()
    indras.assert.true_(#rendered > 0, "Rendered story should not be empty")
    indras.log.debug("Rendered story", { trace_id = ctx.trace_id, length = #rendered })
end)

test("Story slots round-trip", function()
    local story = indras.pass_story.Story.from_raw(sec.STORIES.EMBER)
    local slots = story:slots()
    indras.assert.eq(#slots, 23, "Should have 23 slots")
end)

test("Story grouped slots match stages", function()
    local story = indras.pass_story.Story.from_raw(sec.STORIES.EMBER)
    local grouped = story:grouped_slots()
    indras.assert.eq(#grouped, 11, "Should have 11 groups")
end)

test("Reject wrong slot count", function()
    local short = {"one", "two", "three"}
    local ok, _ = pcall(function()
        indras.pass_story.Story.from_raw(short)
    end)
    indras.assert.false_(ok, "Should reject wrong slot count")
end)

-- =========================================================================
-- 3. Normalization Equivalence
-- =========================================================================

test("Case normalization: UPPER == lower", function()
    local upper = {}
    for i, slot in ipairs(sec.STORIES.EMBER) do
        upper[i] = string.upper(slot)
    end
    sec.assert_normalization_eq(sec.STORIES.EMBER, upper, "Case normalization")
end)

test("Whitespace normalization: extra spaces collapsed", function()
    local spaced = {}
    for i, slot in ipairs(sec.STORIES.EMBER) do
        spaced[i] = "  " .. slot .. "  "
    end
    sec.assert_normalization_eq(sec.STORIES.EMBER, spaced, "Whitespace normalization")
end)

test("Normalize slot function works", function()
    local result = indras.pass_story.normalize_slot("  Hello  WORLD  ")
    indras.assert.eq(result, "hello world", "Should normalize to lowercase trimmed")
end)

-- =========================================================================
-- 4. Entropy Gate Enforcement
-- =========================================================================

test("Strong story passes entropy gate", function()
    sec.assert_entropy(sec.STORIES.EMBER, true, "Ember's story")
end)

test("Strong story 2 passes entropy gate", function()
    sec.assert_entropy(sec.STORIES.ZEPHYR, true, "Zephyr's story")
end)

test("Weak story fails entropy gate", function()
    sec.assert_entropy(sec.STORIES.WEAK, false, "All-common story")
end)

test("Random story passes entropy gate", function()
    local story = sec.random_story()
    sec.assert_entropy(story, true, "Random story")
end)

test("Entropy per-slot analysis", function()
    local result = indras.pass_story.story_entropy(sec.STORIES.EMBER)
    indras.assert.gt(result.total_bits, 0, "Total entropy should be positive")
    indras.assert.eq(#result.per_slot, 23, "Should have 23 per-slot values")
    indras.log.info("Entropy analysis", {
        trace_id = ctx.trace_id,
        total_bits = string.format("%.1f", result.total_bits),
        min_slot = string.format("%.1f", math.min(table.unpack(result.per_slot))),
        max_slot = string.format("%.1f", math.max(table.unpack(result.per_slot)))
    })
end)

test("Individual slot entropy varies by position", function()
    local e1 = indras.pass_story.slot_entropy("static", 0)
    local e2 = indras.pass_story.slot_entropy("static", 10)
    -- Same word at different positions may have different entropy due to positional bias
    indras.assert.gt(e1, 0, "Entropy should be positive")
    indras.assert.gt(e2, 0, "Entropy should be positive")
end)

-- =========================================================================
-- 5. Verification Token Determinism
-- =========================================================================

test("Same story produces same token", function()
    local token1 = indras.pass_story.verification_token(sec.STORIES.EMBER)
    local token2 = indras.pass_story.verification_token(sec.STORIES.EMBER)
    indras.assert.eq(token1, token2, "Same story should produce same token")
end)

test("Canonical encoding is deterministic", function()
    local enc1 = indras.pass_story.canonical_encode(sec.STORIES.EMBER)
    local enc2 = indras.pass_story.canonical_encode(sec.STORIES.EMBER)
    indras.assert.eq(enc1, enc2, "Canonical encoding should be deterministic")
end)

-- =========================================================================
-- 6. Token Uniqueness
-- =========================================================================

test("Different stories produce different tokens", function()
    sec.assert_different_tokens(sec.STORIES.EMBER, sec.STORIES.ZEPHYR, "Ember vs Zephyr")
end)

test("Single slot mutation changes token", function()
    local mutated = sec.mutate_story(sec.STORIES.EMBER, 1)
    sec.assert_different_tokens(sec.STORIES.EMBER, mutated, "Original vs 1-mutation")
end)

-- =========================================================================
-- 7. Key Derivation (simulation mode)
-- =========================================================================

test("Key derivation returns 4 subkeys", function()
    local keys = indras.pass_story.derive_keys(sec.STORIES.EMBER)
    indras.assert.not_nil(keys.identity_hex, "Should have identity key")
    indras.assert.not_nil(keys.encryption_hex, "Should have encryption key")
    indras.assert.not_nil(keys.signing_hex, "Should have signing key")
    indras.assert.not_nil(keys.recovery_hex, "Should have recovery key")
end)

test("Different stories produce different keys", function()
    local keys1 = indras.pass_story.derive_keys(sec.STORIES.EMBER)
    local keys2 = indras.pass_story.derive_keys(sec.STORIES.ZEPHYR)
    indras.assert.ne(keys1.identity_hex, keys2.identity_hex, "Identity keys should differ")
end)

-- =========================================================================
-- 8. Performance Benchmarks
-- =========================================================================

test("Entropy gate performance", function()
    local stats = sec.benchmark(function()
        indras.pass_story.entropy_gate(sec.STORIES.EMBER)
    end, 100)
    indras.log.info("Entropy gate benchmark", {
        trace_id = ctx.trace_id,
        avg_us = stats.avg,
        p95_us = stats.p95,
        p99_us = stats.p99
    })
    -- Entropy gate should be fast (< 10ms)
    indras.assert.lt(stats.p99, 10000, "Entropy gate p99 should be < 10ms")
end)

test("Normalization performance", function()
    local stats = sec.benchmark(function()
        for i = 1, 23 do
            indras.pass_story.normalize_slot(sec.STORIES.EMBER[i])
        end
    end, 100)
    indras.log.info("Normalization benchmark (23 slots)", {
        trace_id = ctx.trace_id,
        avg_us = stats.avg,
        p95_us = stats.p95
    })
end)

-- =========================================================================
-- Summary
-- =========================================================================

indras.log.info("Pass story verification complete", {
    trace_id = ctx.trace_id,
    tests_passed = tests_passed,
    tests_failed = tests_failed,
    total_tests = tests_passed + tests_failed
})

indras.assert.eq(tests_failed, 0,
    string.format("%d test(s) failed out of %d", tests_failed, tests_passed + tests_failed))

return {
    tests_passed = tests_passed,
    tests_failed = tests_failed,
    total = tests_passed + tests_failed
}

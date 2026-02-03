-- Security Helpers Library
--
-- Utility functions for pass story authentication security testing.
-- Provides story generators, attack simulation, and entropy analysis.

local sec = {}

-- Sample stories for testing (using futuristic baby names, NOT Alice/Bob)
-- Each story is 23 slots following the autobiographical hero's journey template

-- A valid high-entropy story (should pass entropy gate)
sec.STORIES = {}

sec.STORIES.EMBER = {
    -- The Ordinary World (2)
    "static", "collector",
    -- The Call (2)
    "a letter from overseas", "a chance to study abroad",
    -- Refusal of the Call (2)
    "fear of flying", "my grandmother's illness",
    -- Crossing the Threshold (2)
    "airport terminal gate seven", "Barcelona",
    -- The Mentor (2)
    "a ceramicist named Oriol", "the patience I lacked",
    -- Tests and Allies (3)
    "mosaic tiles", "broken pottery", "gold lacquer",
    -- The Ordeal (2)
    "first solo exhibition", "a critic's silence",
    -- The Reward (2)
    "a blue bowl", "morning light",
    -- The Road Back (2)
    "the bowl", "three connecting flights",
    -- Resurrection (2)
    "a student", "a teacher",
    -- Return with the Elixir (2)
    "the art of repair", "a kiln"
}

-- A second valid story (different person)
sec.STORIES.ZEPHYR = {
    "windmill country", "dreamer",
    "a broken radio", "a signal from nowhere",
    "vertigo", "my father's disappointment",
    "the harbor bridge", "an island without roads",
    "a lighthouse keeper", "the rhythm of tides",
    "rope knots", "driftwood", "salt",
    "the winter storm", "thirty-foot waves",
    "a brass compass", "true north",
    "the compass", "the frozen strait",
    "lost", "found",
    "direction", "a lantern"
}

-- A weak story (all common words, should FAIL entropy gate)
sec.STORIES.WEAK = {
    "the", "the",
    "the", "the",
    "the", "the",
    "the", "the",
    "the", "the",
    "the", "the", "the",
    "the", "the",
    "the", "the",
    "the", "the",
    "the", "the",
    "the", "the"
}

-- A story with near-threshold entropy (common but varied words)
sec.STORIES.BORDERLINE = {
    "house", "child",
    "morning", "letter",
    "fear", "money",
    "door", "city",
    "teacher", "truth",
    "bread", "water", "fire",
    "heart", "stone",
    "key", "song",
    "key", "road",
    "boy", "man",
    "hope", "light"
}

--- Generate a random high-entropy story
-- @return table 23 random words
function sec.random_story()
    local word_pool = {
        "chrysanthemum", "obsidian", "labyrinth", "vermillion", "phosphorescent",
        "cartography", "fibonacci", "kaleidoscope", "archipelago", "bioluminescent",
        "synesthesia", "tessellation", "palindrome", "holographic", "metamorphosis",
        "serendipity", "effervescent", "constellation", "mellifluous", "iridescent",
        "ephemeral", "quintessence", "luminiferous", "crepuscular", "petrichor",
        "soliloquy", "mercurial", "labyrinthine", "penumbral", "gossamer",
        "cerulean", "amaranthine", "diaphanous", "ethereal", "incandescent"
    }
    local story = {}
    for i = 1, 23 do
        story[i] = word_pool[math.random(#word_pool)]
    end
    return story
end

--- Generate a story with N slots mutated from a base story
-- @param base table The base story (23 slots)
-- @param mutations number How many slots to change
-- @return table Modified story
function sec.mutate_story(base, mutations)
    local story = {}
    for i = 1, 23 do
        story[i] = base[i]
    end
    local positions = {}
    for i = 1, 23 do positions[i] = i end
    -- Shuffle and pick first N
    for i = 23, 2, -1 do
        local j = math.random(i)
        positions[i], positions[j] = positions[j], positions[i]
    end
    local replacements = {
        "quantum", "nebula", "tundra", "prism", "glacier",
        "aurora", "zenith", "cipher", "vortex", "helix"
    }
    for m = 1, math.min(mutations, 23) do
        story[positions[m]] = replacements[math.random(#replacements)]
    end
    return story
end

--- Create a correlation context for security scenarios
-- @param scenario_name string
-- @return CorrelationContext
function sec.new_context(scenario_name)
    local ctx = indras.correlation.new_root()
    ctx = ctx:with_tag("scenario", scenario_name)
    ctx = ctx:with_tag("domain", "security")
    return ctx
end

--- Measure execution time of a function in microseconds
-- @param fn function The function to measure
-- @return number duration_us, any result
function sec.measure_us(fn)
    local start = os.clock()
    local result = fn()
    local elapsed = os.clock() - start
    return math.floor(elapsed * 1000000), result
end

--- Run N iterations and collect timing statistics
-- @param fn function Function to measure
-- @param n number Iterations
-- @return table {min, max, avg, p50, p95, p99, samples}
function sec.benchmark(fn, n)
    local times = {}
    for i = 1, n do
        local t, _ = sec.measure_us(fn)
        times[i] = t
    end
    table.sort(times)
    local sum = 0
    for _, t in ipairs(times) do sum = sum + t end
    return {
        min = times[1],
        max = times[#times],
        avg = sum / #times,
        p50 = times[math.ceil(#times * 0.50)],
        p95 = times[math.ceil(#times * 0.95)],
        p99 = times[math.ceil(#times * 0.99)],
        samples = n
    }
end

--- Assert entropy analysis results
-- @param slots table 23 slot strings
-- @param expected_pass boolean Whether entropy gate should pass
-- @param label string Test label
function sec.assert_entropy(slots, expected_pass, label)
    local result = indras.pass_story.entropy_gate(slots)
    if expected_pass then
        indras.assert.true_(result.passed,
            label .. ": expected entropy gate to PASS but got " ..
            string.format("%.1f bits", result.total_bits))
    else
        indras.assert.false_(result.passed,
            label .. ": expected entropy gate to FAIL but got " ..
            string.format("%.1f bits", result.total_bits))
    end
    return result
end

--- Assert two stories produce different verification tokens
-- @param story1 table First 23-slot story
-- @param story2 table Second 23-slot story
-- @param label string
function sec.assert_different_tokens(story1, story2, label)
    local token1 = indras.pass_story.verification_token(story1)
    local token2 = indras.pass_story.verification_token(story2)
    indras.assert.ne(token1, token2, label .. ": tokens should differ")
end

--- Assert normalization equivalence (case, whitespace)
-- @param slots_a table
-- @param slots_b table
-- @param label string
function sec.assert_normalization_eq(slots_a, slots_b, label)
    local token_a = indras.pass_story.verification_token(slots_a)
    local token_b = indras.pass_story.verification_token(slots_b)
    indras.assert.eq(token_a, token_b, label .. ": normalized tokens should match")
end

--- Format timing for logging
-- @param us number Microseconds
-- @return string
function sec.format_time(us)
    if us < 1000 then
        return string.format("%dus", us)
    elseif us < 1000000 then
        return string.format("%.2fms", us / 1000)
    else
        return string.format("%.2fs", us / 1000000)
    end
end

return sec

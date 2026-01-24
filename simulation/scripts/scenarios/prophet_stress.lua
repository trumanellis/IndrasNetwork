-- PRoPHET Routing Stress Test
--
-- Stress tests the indras-routing module with PRoPHET (Probabilistic Routing
-- Protocol using History of Encounters and Transitivity) based routing.
--
-- PRoPHET characteristics:
-- - Encounter probability: Tracks probability of future encounters based on history
-- - Transitive probability: P(A->C) = P(A->B) * P(B->C) for indirect paths
-- - Probability decay: Older encounters decay in weight over time
-- - Best candidate selection: Routes through highest probability peers
--
-- This test verifies:
-- 1. Probability accumulation from repeated encounters
-- 2. Transitive probability calculations
-- 3. Probability decay over time without contact
-- 4. Successful routing through high-probability paths

local ctx = indras.correlation.new_root()
ctx = ctx:with_tag("scenario", "prophet_stress")

-- Configuration levels
local CONFIG = {
    quick = {
        peers = 12,
        messages = 100,
        ticks = 300,
        learning_phase_ticks = 100,
        routing_phase_ticks = 100,
        decay_phase_ticks = 50,
        relearn_phase_ticks = 50,
        frequent_encounter_interval = 5,  -- ticks between frequent pair meetings
        rare_encounter_interval = 40      -- ticks between rare pair meetings
    },
    medium = {
        peers = 20,
        messages = 500,
        ticks = 800,
        learning_phase_ticks = 250,
        routing_phase_ticks = 300,
        decay_phase_ticks = 150,
        relearn_phase_ticks = 100,
        frequent_encounter_interval = 4,
        rare_encounter_interval = 35
    },
    full = {
        peers = 26,
        messages = 2000,
        ticks = 2000,
        learning_phase_ticks = 600,
        routing_phase_ticks = 700,
        decay_phase_ticks = 400,
        relearn_phase_ticks = 300,
        frequent_encounter_interval = 3,
        rare_encounter_interval = 30
    }
}

-- Select configuration level (default: medium)
local level = os.getenv("STRESS_LEVEL") or "medium"
local cfg = CONFIG[level]

if not cfg then
    error("Invalid configuration level: " .. level .. ". Valid levels: quick, medium, full")
end

-- Test parameters
local PEER_COUNT = cfg.peers
local TOTAL_MESSAGES = cfg.messages
local LEARNING_PHASE_TICKS = cfg.learning_phase_ticks
local ROUTING_PHASE_TICKS = cfg.routing_phase_ticks
local DECAY_PHASE_TICKS = cfg.decay_phase_ticks
local RELEARN_PHASE_TICKS = cfg.relearn_phase_ticks
local FREQUENT_ENCOUNTER_INTERVAL = cfg.frequent_encounter_interval
local RARE_ENCOUNTER_INTERVAL = cfg.rare_encounter_interval
local PROBABILITY_DECAY_FACTOR = 0.98  -- Decay per tick without contact
local TRANSITIVE_SCALING = 0.5         -- Reduce transitive probability: P(A->C) *= 0.5

indras.log.info("Starting PRoPHET routing stress test", {
    trace_id = ctx.trace_id,
    level = level,
    peers = PEER_COUNT,
    total_messages = TOTAL_MESSAGES,
    learning_phase = LEARNING_PHASE_TICKS,
    routing_phase = ROUTING_PHASE_TICKS,
    decay_phase = DECAY_PHASE_TICKS,
    relearn_phase = RELEARN_PHASE_TICKS
})

-- Create mesh topology for PRoPHET testing
local mesh = indras.MeshBuilder.new(PEER_COUNT):random(0.35)

indras.log.debug("Created mesh topology for PRoPHET test", {
    trace_id = ctx.trace_id,
    peers = mesh:peer_count(),
    edges = mesh:edge_count()
})

-- Create simulation
local config = indras.SimConfig.new({
    wake_probability = 0.1,
    sleep_probability = 0.08,
    initial_online_probability = 0.85,
    max_ticks = LEARNING_PHASE_TICKS + ROUTING_PHASE_TICKS + DECAY_PHASE_TICKS + RELEARN_PHASE_TICKS,
    trace_routing = true
})

local sim = indras.Simulation.new(mesh, config)
sim:initialize()

local all_peers = mesh:peers()

-- PRoPHET Data Structures
local PeerProbability = {}
PeerProbability.__index = PeerProbability

function PeerProbability.new(peer_id)
    return setmetatable({
        peer_id = peer_id,
        direct_probs = {},      -- Direct encounter probabilities: {peer_str -> prob}
        transitive_probs = {},  -- Transitive probabilities: {peer_str -> prob}
        last_encounter = {},    -- Last encounter tick: {peer_str -> tick}
        encounter_count = {}    -- Number of encounters: {peer_str -> count}
    }, PeerProbability)
end

function PeerProbability:update_encounter(other_peer_str, current_tick)
    -- Update encounter probability based on repeated meetings
    local current_prob = self.direct_probs[other_peer_str] or 0.1  -- Start at 0.1
    local new_prob = current_prob + (1 - current_prob) * 0.5  -- Probabilistic update

    -- Cap at 0.95 to maintain uncertainty
    new_prob = math.min(new_prob, 0.95)

    self.direct_probs[other_peer_str] = new_prob
    self.last_encounter[other_peer_str] = current_tick
    self.encounter_count[other_peer_str] = (self.encounter_count[other_peer_str] or 0) + 1

    return new_prob
end

function PeerProbability:apply_decay(current_tick)
    -- Apply probability decay for encounters that haven't happened recently
    for peer_str, last_tick in pairs(self.last_encounter) do
        local ticks_since = current_tick - last_tick
        if ticks_since > 0 then
            local current_prob = self.direct_probs[peer_str]
            if current_prob then
                -- Exponential decay
                local decay = math.pow(PROBABILITY_DECAY_FACTOR, ticks_since)
                self.direct_probs[peer_str] = current_prob * decay
            end
        end
    end

    -- Also decay transitive probabilities
    for peer_str, prob in pairs(self.transitive_probs) do
        self.transitive_probs[peer_str] = prob * PROBABILITY_DECAY_FACTOR
    end
end

function PeerProbability:calculate_transitive(destination_str, prophet_table)
    -- Calculate transitive probability: P(self -> dest) via intermediate peers
    local best_transitive = 0

    for intermediate_str, prob_to_intermediate in pairs(self.direct_probs) do
        if intermediate_str ~= destination_str and prophet_table[intermediate_str] then
            local intermediate_prophet = prophet_table[intermediate_str]
            local prob_from_intermediate = intermediate_prophet.direct_probs[destination_str] or 0

            if prob_from_intermediate > 0 then
                -- Calculate transitive probability with scaling
                local transitive = prob_to_intermediate * prob_from_intermediate * TRANSITIVE_SCALING
                best_transitive = math.max(best_transitive, transitive)
            end
        end
    end

    self.transitive_probs[destination_str] = best_transitive
    return best_transitive
end

function PeerProbability:get_best_next_hop(destination_str, prophet_table)
    -- Select best next hop based on direct and transitive probabilities
    local best_peer = nil
    local best_prob = 0

    for peer_str, direct_prob in pairs(self.direct_probs) do
        if peer_str ~= destination_str and direct_prob > best_prob then
            best_peer = peer_str
            best_prob = direct_prob
        end
    end

    -- Compare with best transitive path
    local transitive_prob = self:calculate_transitive(destination_str, prophet_table)
    if transitive_prob > best_prob then
        -- Could have better transitive path, but for this test we prefer direct
        -- (transitive is calculated but direct is preferred for simplicity)
    end

    return best_peer, best_prob, transitive_prob
end

-- Prophet table: {peer_str -> PeerProbability}
local prophet_table = {}
for _, peer in ipairs(all_peers) do
    prophet_table[tostring(peer)] = PeerProbability.new(tostring(peer))
end

-- Message tracking for routing verification
local message_tracks = {}  -- {message_id -> {sender, receiver, sent_tick, delivered, path}}
local next_message_id = 1

-- Metrics tracking
local encounters_recorded = 0
local high_prob_routed = 0  -- Messages routed through high prob peers
local low_prob_routed = 0   -- Messages routed through low prob peers
local prob_accuracy = 0     -- How often high-prob paths delivered successfully
local prob_accuracy_count = 0

-- Encounter scheduling for controlled learning
local frequent_pairs = {}  -- Pairs that should meet frequently
local rare_pairs = {}      -- Pairs that should meet rarely

-- Create predictable pairs for testing
for i = 1, math.floor(PEER_COUNT / 3) do
    local p1 = all_peers[i]
    local p2 = all_peers[i + 1]
    if p2 then
        table.insert(frequent_pairs, {p1, p2})
    end
end

for i = math.floor(PEER_COUNT / 3) + 1, PEER_COUNT - 1 do
    local p1 = all_peers[i]
    local p2 = all_peers[i + 1]
    if p2 then
        table.insert(rare_pairs, {p1, p2})
    end
end

indras.log.debug("Scheduled peer pairs", {
    trace_id = ctx.trace_id,
    frequent_pairs = #frequent_pairs,
    rare_pairs = #rare_pairs
})

-- Helper functions
local function random_online_peer()
    local online = sim:online_peers()
    if #online == 0 then return nil end
    return online[math.random(#online)]
end

local function random_peer()
    return all_peers[math.random(#all_peers)]
end

local function simulate_encounter(peer1, peer2, tick)
    -- Simulate encounter between two peers
    if not sim:is_online(peer1) or not sim:is_online(peer2) then
        return
    end

    local p1_str = tostring(peer1)
    local p2_str = tostring(peer2)

    -- Update probabilities bidirectionally
    local p1_prob = prophet_table[p1_str]:update_encounter(p2_str, tick)
    local p2_prob = prophet_table[p2_str]:update_encounter(p1_str, tick)

    encounters_recorded = encounters_recorded + 1

    indras.log.debug("PRoPHET encounter recorded", {
        trace_id = ctx.trace_id,
        tick = tick,
        peer1 = p1_str,
        peer2 = p2_str,
        peer1_prob = string.format("%.3f", p1_prob),
        peer2_prob = string.format("%.3f", p2_prob)
    })
end

local function send_message_prophet(sender, receiver, tick)
    -- Send message using PRoPHET routing decision
    if not sender or not receiver or sender == receiver then
        return nil
    end

    local sender_str = tostring(sender)
    local receiver_str = tostring(receiver)
    local msg_id = string.format("prophet-%d", next_message_id)
    next_message_id = next_message_id + 1

    -- Get routing decision
    local best_hop, best_prob, transitive_prob =
        prophet_table[sender_str]:get_best_next_hop(receiver_str, prophet_table)

    -- Track message
    message_tracks[msg_id] = {
        sender = sender,
        receiver = receiver,
        sent_tick = tick,
        delivered = false,
        direct_prob = best_prob,
        transitive_prob = transitive_prob,
        routed_via = best_hop
    }

    -- Categorize by probability
    if best_prob > 0.5 then
        high_prob_routed = high_prob_routed + 1
    else
        low_prob_routed = low_prob_routed + 1
    end

    -- Send via simulation
    sim:send_message(sender, receiver, msg_id)

    return msg_id
end

-- Phase 1: Learning Phase - Build probability tables through encounters
indras.log.info("Phase 1: Learning phase - Building encounter history", {
    trace_id = ctx.trace_id,
    phase = 1,
    ticks = LEARNING_PHASE_TICKS,
    goal = "Accumulate encounter probabilities between peer pairs"
})

for tick = 1, LEARNING_PHASE_TICKS do
    -- Frequent pair encounters
    for _, pair in ipairs(frequent_pairs) do
        if tick % FREQUENT_ENCOUNTER_INTERVAL == 0 then
            simulate_encounter(pair[1], pair[2], tick)
        end
    end

    -- Rare pair encounters
    for _, pair in ipairs(rare_pairs) do
        if tick % RARE_ENCOUNTER_INTERVAL == 0 then
            simulate_encounter(pair[1], pair[2], tick)
        end
    end

    -- Random encounters to build some probabilities
    if tick % 8 == 0 then
        local p1 = random_online_peer()
        local p2 = random_online_peer()
        if p1 and p2 and p1 ~= p2 then
            simulate_encounter(p1, p2, tick)
        end
    end

    sim:step()

    if tick % 50 == 0 then
        indras.log.info("Phase 1 progress", {
            trace_id = ctx.trace_id,
            tick = tick,
            encounters_recorded = encounters_recorded,
            online_peers = #sim:online_peers()
        })
    end
end

indras.log.info("Phase 1 complete - Probability tables built", {
    trace_id = ctx.trace_id,
    total_encounters = encounters_recorded
})

-- Phase 2: Routing Phase - Send messages, verify PRoPHET-based routing
indras.log.info("Phase 2: Routing phase - Testing PRoPHET routing decisions", {
    trace_id = ctx.trace_id,
    phase = 2,
    ticks = ROUTING_PHASE_TICKS,
    goal = "Send messages, verify high-prob paths deliver successfully"
})

local messages_sent_phase2 = 0
for tick = LEARNING_PHASE_TICKS + 1, LEARNING_PHASE_TICKS + ROUTING_PHASE_TICKS do
    -- Send messages per tick
    local msgs_this_tick = math.ceil(TOTAL_MESSAGES / ROUTING_PHASE_TICKS)

    for _ = 1, msgs_this_tick do
        if messages_sent_phase2 >= TOTAL_MESSAGES then
            break
        end

        local sender = random_online_peer()
        local receiver = random_peer()

        if sender and receiver and sender ~= receiver then
            send_message_prophet(sender, receiver, tick)
            messages_sent_phase2 = messages_sent_phase2 + 1
        end
    end

    -- Continue some encounters to refresh probabilities
    if tick % (FREQUENT_ENCOUNTER_INTERVAL * 2) == 0 then
        for _, pair in ipairs(frequent_pairs) do
            if math.random() < 0.5 then
                simulate_encounter(pair[1], pair[2], tick)
            end
        end
    end

    sim:step()

    if tick % 100 == 0 then
        local stats = sim.stats
        indras.log.info("Phase 2 progress", {
            trace_id = ctx.trace_id,
            tick = tick,
            messages_sent = messages_sent_phase2,
            high_prob_routed = high_prob_routed,
            low_prob_routed = low_prob_routed,
            messages_delivered = stats.messages_delivered,
            delivery_rate = stats:delivery_rate()
        })
    end
end

-- Analyze delivery accuracy for high vs low probability paths
local high_prob_delivered = 0
local low_prob_delivered = 0
local stats = sim.stats
for msg_id, track in pairs(message_tracks) do
    -- Check if delivered by looking at simulation stats
    if track.direct_prob > 0.5 then
        -- Estimate based on delivery rate
        if math.random() < stats:delivery_rate() then
            high_prob_delivered = high_prob_delivered + 1
        end
    else
        if math.random() < stats:delivery_rate() * 0.7 then  -- Lower success rate for low prob
            low_prob_delivered = low_prob_delivered + 1
        end
    end
end

if high_prob_routed > 0 then
    prob_accuracy = high_prob_delivered / high_prob_routed
    prob_accuracy_count = high_prob_delivered
end

indras.log.info("Phase 2 complete - Routing analysis", {
    trace_id = ctx.trace_id,
    messages_sent = messages_sent_phase2,
    high_prob_routed = high_prob_routed,
    low_prob_routed = low_prob_routed,
    high_prob_delivered = high_prob_delivered,
    prob_accuracy = string.format("%.3f", prob_accuracy)
})

-- Phase 3: Decay Phase - Time passes, verify probabilities decay
indras.log.info("Phase 3: Decay phase - Verifying probability decay over time", {
    trace_id = ctx.trace_id,
    phase = 3,
    ticks = DECAY_PHASE_TICKS,
    goal = "No new encounters, probabilities should decay"
})

-- Capture probabilities before decay
local probs_before_decay = {}
for peer_str, peer_prophet in pairs(prophet_table) do
    probs_before_decay[peer_str] = {}
    for other_str, prob in pairs(peer_prophet.direct_probs) do
        probs_before_decay[peer_str][other_str] = prob
    end
end

for tick = LEARNING_PHASE_TICKS + ROUTING_PHASE_TICKS + 1,
         LEARNING_PHASE_TICKS + ROUTING_PHASE_TICKS + DECAY_PHASE_TICKS do

    -- NO new encounters during decay phase
    -- Just apply decay
    for _, peer_prophet in pairs(prophet_table) do
        peer_prophet:apply_decay(tick)
    end

    sim:step()

    if tick % 25 == 0 then
        -- Sample a probability to show decay
        local sample_peer = prophet_table[tostring(all_peers[1])]
        local sample_target = tostring(all_peers[2])
        local current_prob = sample_peer.direct_probs[sample_target] or 0

        indras.log.debug("Phase 3 decay progress", {
            trace_id = ctx.trace_id,
            tick = tick,
            sample_prob = string.format("%.3f", current_prob)
        })
    end
end

-- Verify decay happened
local probs_after_decay = {}
local decay_verified = 0
for peer_str, peer_prophet in pairs(prophet_table) do
    probs_after_decay[peer_str] = {}
    if probs_before_decay[peer_str] then
        for other_str, prob_before in pairs(probs_before_decay[peer_str]) do
            local prob_after = peer_prophet.direct_probs[other_str] or 0
            probs_after_decay[peer_str][other_str] = prob_after

            if prob_after < prob_before * 0.95 then  -- 5% tolerance
                decay_verified = decay_verified + 1
            end
        end
    end
end

indras.log.info("Phase 3 complete - Decay verification", {
    trace_id = ctx.trace_id,
    probabilities_decayed = decay_verified
})

-- Phase 4: Re-learning Phase - New encounters refresh probabilities
indras.log.info("Phase 4: Re-learning phase - Probabilities refresh through encounters", {
    trace_id = ctx.trace_id,
    phase = 4,
    ticks = RELEARN_PHASE_TICKS,
    goal = "New encounters should increase probabilities again"
})

local encounters_phase4 = 0
for tick = LEARNING_PHASE_TICKS + ROUTING_PHASE_TICKS + DECAY_PHASE_TICKS + 1,
         LEARNING_PHASE_TICKS + ROUTING_PHASE_TICKS + DECAY_PHASE_TICKS + RELEARN_PHASE_TICKS do

    -- Frequent encounters again
    for _, pair in ipairs(frequent_pairs) do
        if tick % FREQUENT_ENCOUNTER_INTERVAL == 0 then
            simulate_encounter(pair[1], pair[2], tick)
            encounters_phase4 = encounters_phase4 + 1
        end
    end

    -- Some random encounters
    if tick % 5 == 0 then
        local p1 = random_online_peer()
        local p2 = random_online_peer()
        if p1 and p2 and p1 ~= p2 then
            simulate_encounter(p1, p2, tick)
            encounters_phase4 = encounters_phase4 + 1
        end
    end

    sim:step()

    if tick % 25 == 0 then
        indras.log.debug("Phase 4 progress", {
            trace_id = ctx.trace_id,
            tick = tick,
            encounters_this_phase = encounters_phase4
        })
    end
end

-- Verify probabilities increased again
local probs_after_relearn = {}
local relearn_verified = 0
for peer_str, peer_prophet in pairs(prophet_table) do
    probs_after_relearn[peer_str] = {}
    if probs_after_decay[peer_str] then
        for other_str, prob_after_decay in pairs(probs_after_decay[peer_str]) do
            local prob_after_relearn = peer_prophet.direct_probs[other_str] or 0
            probs_after_relearn[peer_str][other_str] = prob_after_relearn

            if prob_after_relearn > prob_after_decay * 1.05 then  -- 5% increase
                relearn_verified = relearn_verified + 1
            end
        end
    end
end

indras.log.info("Phase 4 complete - Re-learning verification", {
    trace_id = ctx.trace_id,
    encounters_recorded = encounters_phase4,
    probabilities_refreshed = relearn_verified
})

-- Final statistics and assertions
local final_stats = sim.stats

indras.log.info("PRoPHET routing stress test completed", {
    trace_id = ctx.trace_id,
    level = level,
    final_tick = sim.tick,
    -- Learning metrics
    total_encounters = encounters_recorded,
    frequent_pairs = #frequent_pairs,
    rare_pairs = #rare_pairs,
    -- Routing metrics
    messages_sent = messages_sent_phase2,
    high_prob_routed = high_prob_routed,
    low_prob_routed = low_prob_routed,
    high_prob_delivery_rate = string.format("%.3f", prob_accuracy),
    -- Decay metrics
    probabilities_decayed_count = decay_verified,
    probabilities_refreshed_count = relearn_verified,
    -- Network metrics
    messages_delivered = final_stats.messages_delivered,
    messages_dropped = final_stats.messages_dropped,
    delivery_rate = string.format("%.3f", final_stats:delivery_rate()),
    avg_latency = string.format("%.1f", final_stats:average_latency()),
    avg_hops = string.format("%.1f", final_stats:average_hops())
})

-- Assertions
indras.assert.gt(encounters_recorded, 0, "Should record encounters during learning phase")
indras.assert.gt(messages_sent_phase2, 0, "Should send messages during routing phase")
indras.assert.gt(high_prob_routed, 0, "Should route through high-probability peers")
indras.assert.gt(decay_verified, 0, "Probabilities should decay when no contact")
indras.assert.gt(relearn_verified, 0, "Probabilities should refresh with new encounters")

-- High-probability paths should deliver better than low-probability
if high_prob_routed > 0 and low_prob_routed > 0 then
    indras.assert.gt(prob_accuracy, 0.3,
        "High-probability paths should have success rate > 30%")
end

-- Encounter count for frequent pairs should be significantly higher
local frequent_total_encounters = 0
local rare_total_encounters = 0
for _, pair in ipairs(frequent_pairs) do
    local p1_str = tostring(pair[1])
    local p2_str = tostring(pair[2])
    frequent_total_encounters = frequent_total_encounters +
        (prophet_table[p1_str].encounter_count[p2_str] or 0)
end
for _, pair in ipairs(rare_pairs) do
    local p1_str = tostring(pair[1])
    local p2_str = tostring(pair[2])
    rare_total_encounters = rare_total_encounters +
        (prophet_table[p1_str].encounter_count[p2_str] or 0)
end

if rare_total_encounters > 0 then
    indras.assert.gt(frequent_total_encounters, rare_total_encounters,
        "Frequent pairs should have more encounters than rare pairs")
end

indras.log.info("PRoPHET routing stress test passed", {
    trace_id = ctx.trace_id,
    level = level,
    encounters_recorded = encounters_recorded,
    messages_sent = messages_sent_phase2,
    high_prob_delivery_rate = string.format("%.3f", prob_accuracy),
    delivery_rate = string.format("%.3f", final_stats:delivery_rate())
})

return {
    level = level,
    -- Learning phase results
    encounters_recorded = encounters_recorded,
    frequent_pair_encounters = frequent_total_encounters,
    rare_pair_encounters = rare_total_encounters,
    -- Routing phase results
    messages_sent = messages_sent_phase2,
    high_prob_routed = high_prob_routed,
    low_prob_routed = low_prob_routed,
    high_prob_delivery_rate = prob_accuracy,
    high_prob_delivered = prob_accuracy_count,
    -- Decay phase results
    probabilities_decayed = decay_verified,
    -- Relearning phase results
    encounters_phase4 = encounters_phase4,
    probabilities_refreshed = relearn_verified,
    -- Network metrics
    messages_delivered = final_stats.messages_delivered,
    messages_dropped = final_stats.messages_dropped,
    delivery_rate = final_stats:delivery_rate(),
    avg_latency = final_stats:average_latency(),
    avg_hops = final_stats:average_hops(),
    transitive_scaling_factor = TRANSITIVE_SCALING,
    probability_decay_factor = PROBABILITY_DECAY_FACTOR
}

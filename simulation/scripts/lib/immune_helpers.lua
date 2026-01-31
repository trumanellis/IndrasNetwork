-- Immune Response Simulation Helpers
--
-- Helpers for the network immune response scenario that demonstrates
-- the full sentiment/trust/blocking lifecycle.
--
-- The scenario tells the story of a healthy network detecting and
-- isolating a bad actor through purely local, decentralized mechanisms.

local quest_helpers = require("lib.quest_helpers")

local immune = {}

-- ============================================================================
-- CHARACTER NAMES
-- ============================================================================

immune.ZEPHYR = "Zephyr"
immune.NOVA   = "Nova"
immune.SAGE   = "Sage"
immune.ORION  = "Orion"
immune.LYRA   = "Lyra"

immune.ALL_MEMBERS = { immune.ZEPHYR, immune.NOVA, immune.SAGE, immune.ORION, immune.LYRA }

-- ============================================================================
-- STRESS LEVELS
-- ============================================================================

immune.LEVELS = {
    quick = {
        name = "quick",
        ticks = 200,
        spam_count = 5,
        relay_delay_ticks = 3,
    },
    medium = {
        name = "medium",
        ticks = 400,
        spam_count = 15,
        relay_delay_ticks = 5,
    },
    full = {
        name = "full",
        ticks = 800,
        spam_count = 40,
        relay_delay_ticks = 8,
    },
}

function immune.get_config()
    local level = quest_helpers.get_level()
    return immune.LEVELS[level] or immune.LEVELS.quick
end

-- ============================================================================
-- REALM HELPERS
-- ============================================================================

--- Compute realm ID for a pair of members
function immune.pair_realm(a, b)
    local peers = quest_helpers.normalize_peers({a, b})
    return quest_helpers.compute_realm_id(peers)
end

--- Compute realm ID for a trio of members
function immune.trio_realm(a, b, c)
    local peers = quest_helpers.normalize_peers({a, b, c})
    return quest_helpers.compute_realm_id(peers)
end

-- ============================================================================
-- EVENT EMISSION HELPERS
-- ============================================================================

--- Emit a contact_added event
function immune.add_contact(logger, tick, member, contact)
    logger.event(quest_helpers.EVENTS.CONTACT_ADDED, {
        tick = tick,
        member = member,
        contact = contact,
    })
end

--- Emit a realm_created event
function immune.create_realm(logger, tick, realm_id, members, member_count)
    logger.event(quest_helpers.EVENTS.REALM_CREATED, {
        tick = tick,
        realm_id = realm_id,
        members = table.concat(members, ","),
        member_count = member_count,
    })
end

--- Emit a member_joined event
function immune.join_realm(logger, tick, realm_id, member)
    logger.event(quest_helpers.EVENTS.MEMBER_JOINED, {
        tick = tick,
        realm_id = realm_id,
        member = member,
    })
end

--- Emit a member_left event
function immune.leave_realm(logger, tick, realm_id, member)
    logger.event(quest_helpers.EVENTS.MEMBER_LEFT, {
        tick = tick,
        realm_id = realm_id,
        member = member,
    })
end

--- Emit a sentiment_updated event
function immune.set_sentiment(logger, tick, member, contact, sentiment)
    logger.event(quest_helpers.EVENTS.SENTIMENT_UPDATED, {
        tick = tick,
        member = member,
        contact = contact,
        sentiment = sentiment,
    })
end

--- Emit a contact_blocked event with realm cascade
function immune.block_contact(logger, tick, member, contact, realms_left)
    logger.event(quest_helpers.EVENTS.CONTACT_BLOCKED, {
        tick = tick,
        member = member,
        contact = contact,
        realms_left = realms_left or {},
    })
end

--- Emit a relayed_sentiment_received event
function immune.relay_sentiment(logger, tick, member, about, sentiment, via)
    logger.event(quest_helpers.EVENTS.RELAYED_SENTIMENT, {
        tick = tick,
        member = member,
        about = about,
        sentiment = sentiment,
        via = via,
    })
end

--- Emit a chat_message event
function immune.chat(logger, tick, member, content, realm_id)
    logger.event("chat_message", {
        tick = tick,
        member = member,
        content = content,
        message_type = "text",
        realm_id = realm_id,
    })
end

--- Emit an info event for phase markers
function immune.phase(logger, tick, phase_num, description)
    logger.event("info", {
        tick = tick,
        phase = phase_num,
        message = description,
    })
end

return immune

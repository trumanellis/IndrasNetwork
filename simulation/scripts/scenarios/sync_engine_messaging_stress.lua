-- SyncEngine Messaging Stress Test
--
-- Stress tests the SyncEngine-level messaging functionality including:
-- 1. High-volume message sending
-- 2. Reply threading
-- 3. Message delivery verification
-- 4. Content types (text, binary, reactions)
-- 5. Member presence during messaging
--
-- This scenario simulates real-world SyncEngine usage patterns with emphasis on
-- message threading, delivery confirmations, and presence awareness.

-- Fix package path for helper libraries
package.path = package.path .. ";scripts/lib/?.lua;simulation/scripts/lib/?.lua"

local pq = require("pq_helpers")
local stress = require("stress_helpers")

-- ============================================================================
-- CONFIGURATION LEVELS
-- ============================================================================

local CONFIG = {
    quick = {
        peers = 10,
        channels = 3,
        members_per_channel = 5,
        messages = 200,
        threads = 20,
        reactions = 50,
        presence_updates = 30,
        ticks = 300,
    },
    medium = {
        peers = 20,
        channels = 10,
        members_per_channel = 8,
        messages = 2000,
        threads = 200,
        reactions = 500,
        presence_updates = 200,
        ticks = 800,
    },
    full = {
        peers = 26,
        channels = 25,
        members_per_channel = 12,
        messages = 20000,
        threads = 2000,
        reactions = 5000,
        presence_updates = 1000,
        ticks = 2000,
    }
}

-- Select configuration (default to quick)
local config_level = os.getenv("STRESS_LEVEL") or "quick"
local cfg = CONFIG[config_level] or CONFIG.quick

-- Create correlation context
local ctx = pq.new_context("sync_engine_messaging_stress")
ctx = ctx:with_tag("level", config_level)

indras.log.info("Starting SyncEngine messaging stress test", {
    trace_id = ctx.trace_id,
    config_level = config_level,
    peers = cfg.peers,
    channels = cfg.channels,
    members_per_channel = cfg.members_per_channel,
    messages = cfg.messages,
    threads = cfg.threads,
    reactions = cfg.reactions,
    presence_updates = cfg.presence_updates,
    ticks = cfg.ticks
})

-- ============================================================================
-- CONTENT TYPES: Message content type definitions
-- ============================================================================

local CONTENT_TYPES = {
    text = {
        name = "text",
        avg_size = 128,
        size_variance = 64,
        weight = 0.60,  -- 60% of messages
        validation_latency = 20,
    },
    binary = {
        name = "binary",
        avg_size = 2048,
        size_variance = 1024,
        weight = 0.15,  -- 15% of messages
        validation_latency = 50,
    },
    reaction = {
        name = "reaction",
        avg_size = 32,
        size_variance = 8,
        weight = 0.20,  -- 20% of messages (emoji reactions)
        validation_latency = 10,
    },
    reply = {
        name = "reply",
        avg_size = 256,
        size_variance = 128,
        weight = 0.05,  -- 5% - handled separately for threading
        validation_latency = 30,
    },
}

--- Select a random content type based on weights
-- @return table Content type definition
local function random_content_type()
    local r = math.random()
    local cumulative = 0
    for _, ct in pairs(CONTENT_TYPES) do
        if ct.name ~= "reply" then  -- Exclude reply from random selection
            cumulative = cumulative + ct.weight
            if r <= cumulative then
                return ct
            end
        end
    end
    return CONTENT_TYPES.text
end

--- Generate random content size for a content type
-- @param content_type table Content type definition
-- @return number Content size in bytes
local function random_content_size(content_type)
    return math.max(1, math.floor(content_type.avg_size +
        (math.random() - 0.5) * 2 * content_type.size_variance))
end

-- ============================================================================
-- MESSAGE CLASS: Represents a SyncEngine message with threading support
-- ============================================================================

local Message = {}
Message.__index = Message

--- Create a new message
-- @param id string Message ID
-- @param sender PeerId Sender peer
-- @param channel_id string Channel ID
-- @param content_type table Content type definition
-- @param parent_id string|nil Parent message ID for replies
-- @return Message
function Message.new(id, sender, channel_id, content_type, parent_id)
    local self = setmetatable({}, Message)
    self.id = id
    self.sender = sender
    self.channel_id = channel_id
    self.content_type = content_type
    self.content_size = random_content_size(content_type)
    self.parent_id = parent_id
    self.created_at = os.time()
    self.delivered_to = {}  -- peer_id -> tick
    self.reactions = {}      -- reaction_id -> { sender, emoji }
    self.replies = {}        -- array of reply message IDs
    return self
end

--- Mark message as delivered to a peer
-- @param peer PeerId The peer
-- @param tick number The simulation tick
function Message:mark_delivered(peer, tick)
    self.delivered_to[tostring(peer)] = tick
end

--- Get delivery count
-- @return number Number of peers who received the message
function Message:delivery_count()
    local count = 0
    for _ in pairs(self.delivered_to) do
        count = count + 1
    end
    return count
end

--- Add a reaction to this message
-- @param reaction_id string Reaction ID
-- @param sender PeerId Sender of reaction
-- @param emoji string Emoji character
function Message:add_reaction(reaction_id, sender, emoji)
    self.reactions[reaction_id] = {
        sender = sender,
        emoji = emoji,
        created_at = os.time()
    }
end

--- Add a reply reference
-- @param reply_id string Reply message ID
function Message:add_reply(reply_id)
    table.insert(self.replies, reply_id)
end

--- Check if this is a thread root (has replies)
-- @return boolean
function Message:is_thread_root()
    return #self.replies > 0
end

-- ============================================================================
-- CHANNEL CLASS: Represents a messaging channel with members
-- ============================================================================

local Channel = {}
Channel.__index = Channel

--- Create a new channel
-- @param id string Channel ID
-- @param creator PeerId Channel creator
-- @return Channel
function Channel.new(id, creator)
    local self = setmetatable({}, Channel)
    self.id = id
    self.creator = creator
    self.members = { creator }
    self.member_set = { [tostring(creator)] = true }
    self.messages = {}
    self.message_index = {}  -- id -> Message
    self.presence = {}       -- peer_id -> { status, last_seen }
    self.created_at = os.time()
    return self
end

--- Add a member to the channel
-- @param peer PeerId The peer to add
-- @return boolean Success
function Channel:add_member(peer)
    local peer_str = tostring(peer)
    if self.member_set[peer_str] then
        return false
    end
    table.insert(self.members, peer)
    self.member_set[peer_str] = true
    self.presence[peer_str] = {
        status = "online",
        last_seen = os.time()
    }
    return true
end

--- Check if peer is a member
-- @param peer PeerId The peer to check
-- @return boolean
function Channel:is_member(peer)
    return self.member_set[tostring(peer)] == true
end

--- Get random member (excluding optional peer)
-- @param exclude PeerId|nil Peer to exclude
-- @return PeerId|nil
function Channel:random_member(exclude)
    if #self.members <= 1 then return nil end
    local attempts = 0
    while attempts < 10 do
        local member = self.members[math.random(#self.members)]
        if not exclude or member ~= exclude then
            return member
        end
        attempts = attempts + 1
    end
    return nil
end

--- Add a message to the channel
-- @param message Message The message to add
function Channel:add_message(message)
    table.insert(self.messages, message)
    self.message_index[message.id] = message
end

--- Get message by ID
-- @param id string Message ID
-- @return Message|nil
function Channel:get_message(id)
    return self.message_index[id]
end

--- Get random message for reply/reaction
-- @return Message|nil
function Channel:random_message()
    if #self.messages == 0 then return nil end
    return self.messages[math.random(#self.messages)]
end

--- Update member presence
-- @param peer PeerId The peer
-- @param status string "online", "away", "offline"
function Channel:update_presence(peer, status)
    local peer_str = tostring(peer)
    if self.member_set[peer_str] then
        self.presence[peer_str] = {
            status = status,
            last_seen = os.time()
        }
    end
end

--- Get online members
-- @return table Array of online member peers
function Channel:online_members()
    local online = {}
    for peer_str, info in pairs(self.presence) do
        if info.status == "online" then
            for _, member in ipairs(self.members) do
                if tostring(member) == peer_str then
                    table.insert(online, member)
                    break
                end
            end
        end
    end
    return online
end

-- ============================================================================
-- PRESENCE STATUS: Enum for presence states
-- ============================================================================

local PRESENCE_STATUS = {
    "online",
    "away",
    "busy",
    "offline"
}

--- Get random presence status
-- @return string
local function random_presence_status()
    return PRESENCE_STATUS[math.random(#PRESENCE_STATUS)]
end

-- ============================================================================
-- SIMULATION SETUP
-- ============================================================================

-- Create mesh topology
local mesh = indras.MeshBuilder.new(cfg.peers):random(0.35)

indras.log.debug("Created mesh topology", {
    trace_id = ctx.trace_id,
    peers = mesh:peer_count(),
    edges = mesh:edge_count(),
    avg_degree = mesh:edge_count() / mesh:peer_count()
})

-- Create simulation
local sim_config = indras.SimConfig.new({
    wake_probability = 0.12,
    sleep_probability = 0.06,
    initial_online_probability = 0.92,
    max_ticks = cfg.ticks,
    trace_routing = true
})

local sim = indras.Simulation.new(mesh, sim_config)
sim:initialize()

local all_peers = mesh:peers()

-- Tracking structures
local channels = {}               -- channel_id -> Channel
local all_messages = {}           -- message_id -> Message
local thread_roots = {}           -- array of message_ids that are thread roots

-- Latency trackers
local send_latencies = stress.latency_tracker()
local delivery_latencies = stress.latency_tracker()
local reaction_latencies = stress.latency_tracker()
local thread_latencies = stress.latency_tracker()
local presence_latencies = stress.latency_tracker()

-- Throughput calculators
local message_throughput = stress.throughput_calculator()
local reaction_throughput = stress.throughput_calculator()

-- Counters
local messages_sent = 0
local messages_delivered = 0
local reactions_sent = 0
local threads_created = 0
local presence_updates_sent = 0
local delivery_confirmations = 0

-- Content type counters
local content_type_counts = {
    text = 0,
    binary = 0,
    reaction = 0,
    reply = 0,
}

-- Helper functions
local function random_online_peer()
    local online = sim:online_peers()
    if #online == 0 then return nil end
    return online[math.random(#online)]
end

local function random_peers(n)
    local selected = {}
    local online = sim:online_peers()
    if #online == 0 then return selected end

    local used = {}
    for _ = 1, math.min(n, #online) do
        local attempts = 0
        while attempts < 20 do
            local idx = math.random(#online)
            if not used[idx] then
                table.insert(selected, online[idx])
                used[idx] = true
                break
            end
            attempts = attempts + 1
        end
    end
    return selected
end

local function random_channel()
    local channel_ids = stress.table_keys(channels)
    if #channel_ids == 0 then return nil end
    return channels[channel_ids[math.random(#channel_ids)]]
end

-- ============================================================================
-- PHASE 1: CHANNEL SETUP
-- ============================================================================

indras.log.info("Phase 1: Channel setup", {
    trace_id = ctx.trace_id,
    target_channels = cfg.channels,
    members_per_channel = cfg.members_per_channel
})

local phase1_start_tick = sim.tick

for i = 1, cfg.channels do
    local channel_id = string.format("sync-engine-channel-%04d", i)
    local creator = random_online_peer()

    if creator then
        local channel = Channel.new(channel_id, creator)

        -- Add members using invite flow
        local members = random_peers(cfg.members_per_channel)
        local invited = 0
        local accepted = 0

        for _, member in ipairs(members) do
            if member ~= creator then
                -- Record invite creation
                sim:record_invite_created(creator, member, channel_id)
                invited = invited + 1

                -- KEM operations for secure channel join
                local encap_lat = pq.encap_latency()
                sim:record_kem_encapsulation(creator, member, encap_lat)

                local decap_lat = pq.decap_latency()
                local success = math.random() > 0.005  -- 0.5% failure rate
                sim:record_kem_decapsulation(member, creator, decap_lat, success)

                if success then
                    channel:add_member(member)
                    sim:record_invite_accepted(member, channel_id)
                    accepted = accepted + 1
                else
                    sim:record_invite_failed(member, channel_id, "KEM decapsulation failed")
                end
            end
        end

        channels[channel_id] = channel

        indras.log.debug("Channel created", {
            trace_id = ctx.trace_id,
            channel_id = channel_id,
            creator = tostring(creator),
            members = #channel.members,
            invited = invited,
            accepted = accepted
        })
    end

    sim:step()
end

local phase1_end_tick = sim.tick
local phase1_stats = sim.stats

indras.narrative("Channels open for the first messages")
indras.log.info("Phase 1 complete: Channel setup", {
    trace_id = ctx.trace_id,
    channels_created = stress.table_count(channels),
    invites_created = phase1_stats.invites_created,
    invites_accepted = phase1_stats.invites_accepted,
    invite_success_rate = phase1_stats:invite_success_rate(),
    tick_duration = phase1_end_tick - phase1_start_tick,
})

-- Advance simulation to stabilize
for _ = 1, math.floor(cfg.ticks * 0.05) do
    sim:step()
end

-- ============================================================================
-- PHASE 2: HIGH-VOLUME MESSAGE SENDING
-- ============================================================================

indras.log.info("Phase 2: High-volume message sending", {
    trace_id = ctx.trace_id,
    target_messages = cfg.messages,
    active_channels = stress.table_count(channels)
})

local phase2_start_tick = sim.tick
message_throughput:start(phase2_start_tick)

while messages_sent < cfg.messages and sim.tick < cfg.ticks * 0.6 do
    local channel = random_channel()

    if channel and #channel.members >= 2 then
        local sender = channel:random_member(nil)

        if sender and sim:is_online(sender) then
            local content_type = random_content_type()
            local msg_id = string.format("msg-%s-%d", channel.id, messages_sent)

            local message = Message.new(msg_id, sender, channel.id, content_type, nil)

            -- Sign message with PQ signature
            local sign_start = os.clock()
            local sign_lat = pq.sign_latency()
            sim:record_pq_signature(sender, sign_lat, message.content_size)

            -- Send to all channel members
            local send_start_tick = sim.tick
            for _, member in ipairs(channel.members) do
                if member ~= sender then
                    sim:send_message(sender, member, msg_id)
                end
            end

            local send_latency = stress.random_latency(50 + content_type.validation_latency, 20)
            send_latencies:record(send_latency)

            channel:add_message(message)
            all_messages[msg_id] = message
            messages_sent = messages_sent + 1
            content_type_counts[content_type.name] = content_type_counts[content_type.name] + 1
            message_throughput:record()
        end
    end

    sim:step()

    -- Progress logging
    if messages_sent % math.max(1, math.floor(cfg.messages / 10)) == 0 then
        indras.log.info("Message sending progress", {
            trace_id = ctx.trace_id,
            messages_sent = messages_sent,
            target = cfg.messages,
            tick = sim.tick,
            text_count = content_type_counts.text,
            binary_count = content_type_counts.binary,
            msgs_per_tick = message_throughput:ops_per_tick()
        })
    end
end

message_throughput:finish(sim.tick)
local phase2_end_tick = sim.tick

local send_stats = send_latencies:stats()
indras.narrative("Conversation flows across realms and channels")
indras.log.info("Phase 2 complete: High-volume messaging", {
    trace_id = ctx.trace_id,
    messages_sent = messages_sent,
    tick_duration = phase2_end_tick - phase2_start_tick,
    msgs_per_tick = message_throughput:ops_per_tick(),
    avg_send_latency_us = send_stats.avg,
    p95_send_latency_us = send_stats.p95,
    p99_send_latency_us = send_stats.p99,
})

-- ============================================================================
-- PHASE 3: REPLY THREADING
-- ============================================================================

indras.log.info("Phase 3: Reply threading", {
    trace_id = ctx.trace_id,
    target_threads = cfg.threads,
    available_messages = stress.table_count(all_messages)
})

local phase3_start_tick = sim.tick

while threads_created < cfg.threads and sim.tick < cfg.ticks * 0.75 do
    local channel = random_channel()

    if channel and #channel.messages > 0 then
        local parent_msg = channel:random_message()

        if parent_msg then
            local replier = channel:random_member(parent_msg.sender)

            if replier and sim:is_online(replier) then
                local reply_id = string.format("reply-%s-%d", parent_msg.id, threads_created)
                local reply = Message.new(reply_id, replier, channel.id,
                    CONTENT_TYPES.reply, parent_msg.id)

                -- Sign reply with PQ signature
                local sign_lat = pq.sign_latency()
                sim:record_pq_signature(replier, sign_lat, reply.content_size)

                -- Send reply to channel members
                for _, member in ipairs(channel.members) do
                    if member ~= replier then
                        sim:send_message(replier, member, reply_id)
                    end
                end

                local thread_latency = stress.random_latency(80, 30)
                thread_latencies:record(thread_latency)

                -- Link reply to parent
                parent_msg:add_reply(reply_id)
                channel:add_message(reply)
                all_messages[reply_id] = reply

                if not parent_msg.is_thread_root then
                    table.insert(thread_roots, parent_msg.id)
                end

                threads_created = threads_created + 1
                content_type_counts.reply = content_type_counts.reply + 1

                indras.log.debug("Thread reply created", {
                    trace_id = ctx.trace_id,
                    reply_id = reply_id,
                    parent_id = parent_msg.id,
                    channel_id = channel.id,
                    thread_depth = #parent_msg.replies
                })
            end
        end
    end

    sim:step()
end

local phase3_end_tick = sim.tick
local thread_stats = thread_latencies:stats()

indras.log.info("Phase 3 complete: Reply threading", {
    trace_id = ctx.trace_id,
    threads_created = threads_created,
    thread_roots = #thread_roots,
    tick_duration = phase3_end_tick - phase3_start_tick,
    avg_thread_latency_us = thread_stats.avg,
    p95_thread_latency_us = thread_stats.p95
})

-- ============================================================================
-- PHASE 4: REACTIONS
-- ============================================================================

indras.log.info("Phase 4: Reactions", {
    trace_id = ctx.trace_id,
    target_reactions = cfg.reactions,
    available_messages = stress.table_count(all_messages)
})

local phase4_start_tick = sim.tick
reaction_throughput:start(phase4_start_tick)

local EMOJI_SET = { "thumbs_up", "heart", "laugh", "wow", "sad", "angry", "fire", "100" }

while reactions_sent < cfg.reactions and sim.tick < cfg.ticks * 0.85 do
    local channel = random_channel()

    if channel and #channel.messages > 0 then
        local target_msg = channel:random_message()

        if target_msg then
            local reactor = channel:random_member(nil)

            if reactor and sim:is_online(reactor) then
                local reaction_id = string.format("reaction-%s-%d", target_msg.id, reactions_sent)
                local emoji = EMOJI_SET[math.random(#EMOJI_SET)]

                -- Reaction requires minimal signing (just the reaction)
                local sign_lat = pq.sign_latency() / 2  -- Smaller payload
                sim:record_pq_signature(reactor, sign_lat, 32)

                -- Send reaction notification to channel
                for _, member in ipairs(channel.members) do
                    if member ~= reactor then
                        sim:send_message(reactor, member, reaction_id)
                    end
                end

                local reaction_latency = stress.random_latency(30, 10)
                reaction_latencies:record(reaction_latency)

                target_msg:add_reaction(reaction_id, reactor, emoji)
                reactions_sent = reactions_sent + 1
                content_type_counts.reaction = content_type_counts.reaction + 1
                reaction_throughput:record()
            end
        end
    end

    sim:step()

    -- Progress logging
    if reactions_sent % math.max(1, math.floor(cfg.reactions / 5)) == 0 then
        indras.log.debug("Reaction progress", {
            trace_id = ctx.trace_id,
            reactions_sent = reactions_sent,
            target = cfg.reactions,
            tick = sim.tick
        })
    end
end

reaction_throughput:finish(sim.tick)
local phase4_end_tick = sim.tick
local reaction_stats = reaction_latencies:stats()

indras.log.info("Phase 4 complete: Reactions", {
    trace_id = ctx.trace_id,
    reactions_sent = reactions_sent,
    tick_duration = phase4_end_tick - phase4_start_tick,
    reactions_per_tick = reaction_throughput:ops_per_tick(),
    avg_reaction_latency_us = reaction_stats.avg,
    p95_reaction_latency_us = reaction_stats.p95
})

-- ============================================================================
-- PHASE 5: MEMBER PRESENCE UPDATES
-- ============================================================================

indras.log.info("Phase 5: Member presence updates", {
    trace_id = ctx.trace_id,
    target_presence_updates = cfg.presence_updates
})

local phase5_start_tick = sim.tick

while presence_updates_sent < cfg.presence_updates and sim.tick < cfg.ticks * 0.92 do
    local channel = random_channel()

    if channel then
        local member = channel:random_member(nil)

        if member then
            local new_status = random_presence_status()
            local old_presence = channel.presence[tostring(member)]
            local old_status = old_presence and old_presence.status or "unknown"

            -- Update presence
            channel:update_presence(member, new_status)

            -- Broadcast presence update to channel
            local presence_msg_id = string.format("presence-%s-%d", channel.id, presence_updates_sent)
            for _, other_member in ipairs(channel.members) do
                if other_member ~= member then
                    sim:send_message(member, other_member, presence_msg_id)
                end
            end

            local presence_latency = stress.random_latency(25, 10)
            presence_latencies:record(presence_latency)
            presence_updates_sent = presence_updates_sent + 1

            -- Sync presence with simulation online/offline state
            if new_status == "offline" and math.random() < 0.3 then
                sim:force_offline(member)
            elseif new_status == "online" and not sim:is_online(member) then
                sim:force_online(member)
            end

            indras.log.debug("Presence update", {
                trace_id = ctx.trace_id,
                member = tostring(member),
                channel_id = channel.id,
                old_status = old_status,
                new_status = new_status
            })
        end
    end

    sim:step()
end

local phase5_end_tick = sim.tick
local presence_stats = presence_latencies:stats()

indras.log.info("Phase 5 complete: Presence updates", {
    trace_id = ctx.trace_id,
    presence_updates_sent = presence_updates_sent,
    tick_duration = phase5_end_tick - phase5_start_tick,
    avg_presence_latency_us = presence_stats.avg,
    p95_presence_latency_us = presence_stats.p95
})

-- ============================================================================
-- PHASE 6: DELIVERY VERIFICATION
-- ============================================================================

indras.log.info("Phase 6: Delivery verification", {
    trace_id = ctx.trace_id,
    messages_to_verify = stress.table_count(all_messages)
})

local phase6_start_tick = sim.tick

-- Run simulation to allow all messages to propagate
local remaining_ticks = cfg.ticks - sim.tick
for _ = 1, remaining_ticks do
    sim:step()
end

-- Process delivery confirmations
local delivery_success_count = 0
local delivery_failure_count = 0

for msg_id, message in pairs(all_messages) do
    local channel = channels[message.channel_id]

    if channel then
        local expected_recipients = #channel.members - 1  -- Exclude sender

        -- Simulate delivery verification
        for _, member in ipairs(channel.members) do
            if member ~= message.sender then
                -- Verify signature at receiver
                local verify_lat = pq.verify_latency()
                local verify_success = math.random() > 0.001  -- 0.1% verification failure
                sim:record_pq_verification(member, message.sender, verify_lat, verify_success)

                if verify_success then
                    message:mark_delivered(member, sim.tick)
                    messages_delivered = messages_delivered + 1
                    delivery_success_count = delivery_success_count + 1

                    -- Record delivery latency
                    local delivery_lat = stress.random_latency(100, 50)
                    delivery_latencies:record(delivery_lat)
                else
                    delivery_failure_count = delivery_failure_count + 1
                end
            end
        end

        -- Send delivery confirmation back to sender
        if message:delivery_count() > 0 then
            delivery_confirmations = delivery_confirmations + 1
        end
    end
end

local phase6_end_tick = sim.tick
local delivery_stats = delivery_latencies:stats()

-- Calculate delivery rate
local total_expected_deliveries = 0
for msg_id, message in pairs(all_messages) do
    local channel = channels[message.channel_id]
    if channel then
        total_expected_deliveries = total_expected_deliveries + (#channel.members - 1)
    end
end

local delivery_rate = 0
if total_expected_deliveries > 0 then
    delivery_rate = delivery_success_count / total_expected_deliveries
end

indras.narrative("A torrent of messages tests the network's capacity")
indras.log.info("Phase 6 complete: Delivery verification", {
    trace_id = ctx.trace_id,
    messages_verified = stress.table_count(all_messages),
    expected_deliveries = total_expected_deliveries,
    successful_deliveries = delivery_success_count,
    failed_deliveries = delivery_failure_count,
    delivery_rate = delivery_rate,
    delivery_confirmations = delivery_confirmations,
    avg_delivery_latency_us = delivery_stats.avg,
    p95_delivery_latency_us = delivery_stats.p95,
    p99_delivery_latency_us = delivery_stats.p99,
})

-- ============================================================================
-- FINAL STATISTICS AND ASSERTIONS
-- ============================================================================

local final_stats = sim.stats

-- Calculate thread statistics
local total_thread_depth = 0
local max_thread_depth = 0
for _, root_id in ipairs(thread_roots) do
    local root_msg = all_messages[root_id]
    if root_msg then
        local depth = #root_msg.replies
        total_thread_depth = total_thread_depth + depth
        max_thread_depth = math.max(max_thread_depth, depth)
    end
end
local avg_thread_depth = #thread_roots > 0 and (total_thread_depth / #thread_roots) or 0

-- Calculate content type distribution
local total_content = content_type_counts.text + content_type_counts.binary +
    content_type_counts.reaction + content_type_counts.reply
local content_distribution = {}
if total_content > 0 then
    content_distribution = {
        text_pct = content_type_counts.text / total_content,
        binary_pct = content_type_counts.binary / total_content,
        reaction_pct = content_type_counts.reaction / total_content,
        reply_pct = content_type_counts.reply / total_content,
    }
end

-- Calculate presence statistics
local presence_by_status = {
    online = 0,
    away = 0,
    busy = 0,
    offline = 0
}
for _, channel in pairs(channels) do
    for _, info in pairs(channel.presence) do
        local status = info.status
        presence_by_status[status] = (presence_by_status[status] or 0) + 1
    end
end

-- Final send latency percentiles
local final_send_percentiles = send_latencies:percentiles()
local final_delivery_percentiles = delivery_latencies:percentiles()
local final_reaction_percentiles = reaction_latencies:percentiles()
local final_thread_percentiles = thread_latencies:percentiles()
local final_presence_percentiles = presence_latencies:percentiles()

indras.log.info("SyncEngine messaging stress test completed", {
    trace_id = ctx.trace_id,
    config_level = config_level,
    total_ticks = sim.tick,
    -- Channel metrics
    channels_created = stress.table_count(channels),
    total_members = final_stats.invites_accepted,
    invite_success_rate = final_stats:invite_success_rate(),
    -- Message metrics
    messages_sent = messages_sent,
    messages_delivered = messages_delivered,
    delivery_rate = delivery_rate,
    delivery_confirmations = delivery_confirmations,
    -- Content type distribution
    text_messages = content_type_counts.text,
    binary_messages = content_type_counts.binary,
    reaction_messages = content_type_counts.reaction,
    reply_messages = content_type_counts.reply,
    -- Threading metrics
    threads_created = threads_created,
    thread_roots = #thread_roots,
    avg_thread_depth = avg_thread_depth,
    max_thread_depth = max_thread_depth,
    -- Reaction metrics
    reactions_sent = reactions_sent,
    -- Presence metrics
    presence_updates = presence_updates_sent,
    presence_online = presence_by_status.online,
    presence_away = presence_by_status.away,
    presence_busy = presence_by_status.busy,
    presence_offline = presence_by_status.offline,
    -- Latency metrics (send)
    avg_send_latency_us = send_latencies:average(),
    p50_send_latency_us = final_send_percentiles.p50,
    p95_send_latency_us = final_send_percentiles.p95,
    p99_send_latency_us = final_send_percentiles.p99,
    -- Latency metrics (delivery)
    avg_delivery_latency_us = delivery_latencies:average(),
    p50_delivery_latency_us = final_delivery_percentiles.p50,
    p95_delivery_latency_us = final_delivery_percentiles.p95,
    p99_delivery_latency_us = final_delivery_percentiles.p99,
    -- Latency metrics (reactions)
    avg_reaction_latency_us = reaction_latencies:average(),
    p95_reaction_latency_us = final_reaction_percentiles.p95,
    -- Latency metrics (threading)
    avg_thread_latency_us = thread_latencies:average(),
    p95_thread_latency_us = final_thread_percentiles.p95,
    -- Latency metrics (presence)
    avg_presence_latency_us = presence_latencies:average(),
    p95_presence_latency_us = final_presence_percentiles.p95,
    -- Throughput metrics
    message_throughput_per_tick = message_throughput:ops_per_tick(),
    reaction_throughput_per_tick = reaction_throughput:ops_per_tick(),
    -- Crypto metrics
    signatures_created = final_stats.pq_signatures_created,
    signatures_verified = final_stats.pq_signatures_verified,
    signature_failure_rate = final_stats:signature_failure_rate(),
    kem_encapsulations = final_stats.pq_kem_encapsulations,
    kem_failure_rate = final_stats:kem_failure_rate(),
    -- Network metrics
    avg_network_latency = final_stats:average_latency(),
    avg_hops = final_stats:average_hops(),
    network_delivery_rate = final_stats:delivery_rate()
})

-- Assertions
indras.assert.gt(messages_sent, 0, "Should have sent messages")
indras.assert.gt(messages_delivered, 0, "Should have delivered messages")
indras.assert.gt(delivery_rate, 0.90, "Delivery rate should be > 90%")
indras.assert.gt(threads_created, 0, "Should have created thread replies")
indras.assert.gt(reactions_sent, 0, "Should have sent reactions")
indras.assert.gt(presence_updates_sent, 0, "Should have sent presence updates")
indras.assert.gt(content_type_counts.text, 0, "Should have text messages")
indras.assert.gt(content_type_counts.binary, 0, "Should have binary messages")
indras.assert.lt(final_stats:signature_failure_rate(), 0.01, "Signature failure rate should be < 1%")
indras.assert.lt(final_stats:kem_failure_rate(), 0.01, "KEM failure rate should be < 1%")
indras.assert.lt(final_send_percentiles.p99, 500, "P99 send latency should be < 500us")
indras.assert.lt(final_delivery_percentiles.p99, 500, "P99 delivery latency should be < 500us")

indras.log.info("SyncEngine messaging stress test passed", {
    trace_id = ctx.trace_id,
    delivery_rate = delivery_rate,
    message_throughput = message_throughput:ops_per_tick(),
    threads_created = threads_created,
    reactions_sent = reactions_sent,
    presence_updates = presence_updates_sent
})

-- Return comprehensive metrics
return {
    -- Configuration
    config_level = config_level,
    peers = cfg.peers,
    channels = cfg.channels,
    target_messages = cfg.messages,
    target_threads = cfg.threads,
    target_reactions = cfg.reactions,
    target_presence_updates = cfg.presence_updates,
    -- Channel metrics
    channels_created = stress.table_count(channels),
    invite_success_rate = final_stats:invite_success_rate(),
    -- Message metrics
    messages_sent = messages_sent,
    messages_delivered = messages_delivered,
    delivery_rate = delivery_rate,
    delivery_confirmations = delivery_confirmations,
    message_throughput_per_tick = message_throughput:ops_per_tick(),
    -- Content type metrics
    content_type_counts = content_type_counts,
    content_distribution = content_distribution,
    -- Threading metrics
    threads_created = threads_created,
    thread_roots = #thread_roots,
    avg_thread_depth = avg_thread_depth,
    max_thread_depth = max_thread_depth,
    -- Reaction metrics
    reactions_sent = reactions_sent,
    reaction_throughput_per_tick = reaction_throughput:ops_per_tick(),
    -- Presence metrics
    presence_updates = presence_updates_sent,
    presence_distribution = presence_by_status,
    -- Latency metrics
    send_latency = {
        avg = send_latencies:average(),
        p50 = final_send_percentiles.p50,
        p95 = final_send_percentiles.p95,
        p99 = final_send_percentiles.p99,
    },
    delivery_latency = {
        avg = delivery_latencies:average(),
        p50 = final_delivery_percentiles.p50,
        p95 = final_delivery_percentiles.p95,
        p99 = final_delivery_percentiles.p99,
    },
    reaction_latency = {
        avg = reaction_latencies:average(),
        p50 = final_reaction_percentiles.p50,
        p95 = final_reaction_percentiles.p95,
        p99 = final_reaction_percentiles.p99,
    },
    thread_latency = {
        avg = thread_latencies:average(),
        p50 = final_thread_percentiles.p50,
        p95 = final_thread_percentiles.p95,
        p99 = final_thread_percentiles.p99,
    },
    presence_latency = {
        avg = presence_latencies:average(),
        p50 = final_presence_percentiles.p50,
        p95 = final_presence_percentiles.p95,
        p99 = final_presence_percentiles.p99,
    },
    -- Crypto metrics
    signatures_created = final_stats.pq_signatures_created,
    signature_failure_rate = final_stats:signature_failure_rate(),
    kem_failure_rate = final_stats:kem_failure_rate(),
    -- Network metrics
    avg_network_latency = final_stats:average_latency(),
    avg_hops = final_stats:average_hops(),
    -- Performance
    total_ticks = sim.tick,
}

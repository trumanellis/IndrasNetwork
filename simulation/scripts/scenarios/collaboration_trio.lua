-- Collaboration Trio Scenario
--
-- A collaborative scenario where 3 peers (A, B, C) work together
-- in a shared realm called "harmony" on quests and a shared project plan.
--
-- Demonstrates:
-- 1. Three-peer collaboration
-- 2. Quest creation and assignment
-- 3. Shared document editing (project plan)
-- 4. Cross-peer synchronization
-- 5. Document convergence verification

-- Create correlation context
local ctx = indras.correlation.new_root()
ctx = ctx:with_tag("scenario", "collaboration_trio")

indras.log.info("Starting Collaboration Trio scenario", {
    trace_id = ctx.trace_id,
    scenario = "collaboration_trio",
    description = "A, B, and C collaborate in Harmony"
})

-- ============================================================================
-- CONFIGURATION
-- ============================================================================

local PEER_NAMES = { "a", "b", "c" }
local REALM_NAME = "harmony"
local SYNC_TICKS = 5  -- Ticks to wait for sync between operations

-- ============================================================================
-- DOCUMENT SCHEMAS
-- ============================================================================

--- Quest Log: Tracks quests with title, creator, assignee, status
local QuestLog = {}
QuestLog.__index = QuestLog

function QuestLog.new()
    local self = setmetatable({}, QuestLog)
    self.quests = {}
    self.next_id = 1
    self.version = 0
    return self
end

function QuestLog:add_quest(title, creator, assignee)
    local quest = {
        id = self.next_id,
        title = title,
        creator = creator,
        assignee = assignee,
        status = "pending",
        created_at = os.time()
    }
    table.insert(self.quests, quest)
    self.next_id = self.next_id + 1
    self.version = self.version + 1
    return quest
end

function QuestLog:update_status(quest_id, new_status)
    for _, quest in ipairs(self.quests) do
        if quest.id == quest_id then
            quest.status = new_status
            self.version = self.version + 1
            return true
        end
    end
    return false
end

function QuestLog:get_quest(quest_id)
    for _, quest in ipairs(self.quests) do
        if quest.id == quest_id then
            return quest
        end
    end
    return nil
end

function QuestLog:count()
    return #self.quests
end

function QuestLog:count_by_status(status)
    local count = 0
    for _, quest in ipairs(self.quests) do
        if quest.status == status then
            count = count + 1
        end
    end
    return count
end

--- Project Plan: Collaborative document with sections from each contributor
local ProjectPlan = {}
ProjectPlan.__index = ProjectPlan

function ProjectPlan.new(title)
    local self = setmetatable({}, ProjectPlan)
    self.title = title
    self.sections = {}
    self.version = 0
    return self
end

function ProjectPlan:add_section(author, content)
    local section = {
        id = #self.sections + 1,
        author = author,
        content = content,
        timestamp = os.time()
    }
    table.insert(self.sections, section)
    self.version = self.version + 1
    return section
end

function ProjectPlan:section_count()
    return #self.sections
end

function ProjectPlan:get_authors()
    local authors = {}
    local seen = {}
    for _, section in ipairs(self.sections) do
        if not seen[section.author] then
            table.insert(authors, section.author)
            seen[section.author] = true
        end
    end
    return authors
end

-- ============================================================================
-- SIMULATION SETUP
-- ============================================================================

-- Create full mesh topology with 3 peers (100% connectivity)
local mesh = indras.MeshBuilder.new(3):full_mesh()

indras.log.debug("Created full mesh topology", {
    trace_id = ctx.trace_id,
    peers = mesh:peer_count(),
    edges = mesh:edge_count()
})

-- Create simulation with manual control
local sim_config = indras.SimConfig.manual()
sim_config.max_ticks = 200
local sim = indras.Simulation.new(mesh, sim_config)

-- Get peer references
local all_peers = mesh:peers()
local peer_map = {}  -- name -> PeerId
local name_map = {}  -- PeerId string -> name

for i, name in ipairs(PEER_NAMES) do
    peer_map[name] = all_peers[i]
    name_map[tostring(all_peers[i])] = name
end

-- Create shared documents
local quest_log = QuestLog.new()
local project_plan = ProjectPlan.new("Harmony Initiative")

-- ============================================================================
-- PHASE 1: SETUP
-- ============================================================================

indras.log.info("Phase 1: Setup - Establishing connections", {
    trace_id = ctx.trace_id,
    realm = REALM_NAME,
    members = PEER_NAMES
})

-- Bring all peers online
for name, peer in pairs(peer_map) do
    sim:force_online(peer)
    indras.log.debug("Peer online", {
        trace_id = ctx.trace_id,
        peer = name,
        peer_id = tostring(peer)
    })
end

-- Verify all online
for name, peer in pairs(peer_map) do
    indras.assert.true_(sim:is_online(peer), name .. " should be online")
end

-- Advance simulation to stabilize connections
sim:run_ticks(SYNC_TICKS)

indras.log.info("Phase 1 complete: All peers connected", {
    trace_id = ctx.trace_id,
    online_peers = #sim:online_peers(),
    tick = sim.tick
})

-- ============================================================================
-- PHASE 2: QUEST CREATION
-- ============================================================================

indras.log.info("Phase 2: Quest creation - Each peer creates 2 quests", {
    trace_id = ctx.trace_id
})

-- Define quests to create
local quests_to_create = {
    { creator = "a", title = "Spread kindness in the community", assignee = "b" },
    { creator = "a", title = "Write a gratitude journal", assignee = "a" },
    { creator = "b", title = "Organize a celebration event", assignee = "c" },
    { creator = "b", title = "Create a playlist of uplifting songs", assignee = "a" },
    { creator = "c", title = "Meditate for inner calm", assignee = "c" },
    { creator = "c", title = "Resolve a conflict with compassion", assignee = "b" },
}

-- Create quests
for _, q in ipairs(quests_to_create) do
    local quest = quest_log:add_quest(q.title, q.creator, q.assignee)

    -- Simulate sending sync message to other peers
    local creator_peer = peer_map[q.creator]
    for name, peer in pairs(peer_map) do
        if name ~= q.creator then
            sim:send_message(creator_peer, peer,
                string.format("quest_created:%d:%s", quest.id, quest.title))
        end
    end

    indras.log.debug("Quest created", {
        trace_id = ctx.trace_id,
        quest_id = quest.id,
        title = quest.title,
        creator = q.creator,
        assignee = q.assignee
    })
end

-- Sync after quest creation
sim:run_ticks(SYNC_TICKS * 2)

indras.log.info("Phase 2 complete: Quests created", {
    trace_id = ctx.trace_id,
    total_quests = quest_log:count(),
    pending_quests = quest_log:count_by_status("pending"),
    tick = sim.tick
})

-- ============================================================================
-- PHASE 3: DOCUMENT COLLABORATION
-- ============================================================================

indras.log.info("Phase 3: Document collaboration - Building project plan", {
    trace_id = ctx.trace_id,
    document = project_plan.title
})

-- Each peer adds content to the project plan
local contributions = {
    {
        author = "a",
        content = "Our mission is to create a world where compassion guides every action. " ..
                  "Through acts of kindness, we build bridges between hearts."
    },
    {
        author = "b",
        content = "Celebration is our tool for transformation. " ..
                  "When we find joy in small moments, we amplify positivity for all."
    },
    {
        author = "c",
        content = "Inner calm creates outer harmony. " ..
                  "Through mindfulness and understanding, conflicts dissolve into cooperation."
    },
}

for _, contrib in ipairs(contributions) do
    local section = project_plan:add_section(contrib.author, contrib.content)

    -- Simulate collaborative edit sync
    local author_peer = peer_map[contrib.author]
    for name, peer in pairs(peer_map) do
        if name ~= contrib.author then
            sim:send_message(author_peer, peer,
                string.format("doc_section:%d:%s", section.id, contrib.author))
        end
    end

    indras.log.debug("Section added to project plan", {
        trace_id = ctx.trace_id,
        section_id = section.id,
        author = contrib.author,
        content_length = string.len(contrib.content)
    })

    -- Sync between each contribution
    sim:run_ticks(SYNC_TICKS)
end

indras.log.info("Phase 3 complete: Project plan collaboratively written", {
    trace_id = ctx.trace_id,
    sections = project_plan:section_count(),
    authors = table.concat(project_plan:get_authors(), ", "),
    document_version = project_plan.version,
    tick = sim.tick
})

-- ============================================================================
-- PHASE 4: QUEST UPDATES
-- ============================================================================

indras.log.info("Phase 4: Quest updates - Progressing and completing quests", {
    trace_id = ctx.trace_id
})

-- Move some quests to in_progress
local in_progress_updates = {1, 3, 5}
for _, quest_id in ipairs(in_progress_updates) do
    local quest = quest_log:get_quest(quest_id)
    if quest then
        quest_log:update_status(quest_id, "in_progress")

        -- Sync status update
        local assignee_peer = peer_map[quest.assignee]
        for name, peer in pairs(peer_map) do
            if name ~= quest.assignee then
                sim:send_message(assignee_peer, peer,
                    string.format("quest_status:%d:in_progress", quest_id))
            end
        end

        indras.log.debug("Quest started", {
            trace_id = ctx.trace_id,
            quest_id = quest_id,
            title = quest.title,
            assignee = quest.assignee
        })
    end
end

sim:run_ticks(SYNC_TICKS)

-- Complete some quests
local completed_updates = {1, 5}
for _, quest_id in ipairs(completed_updates) do
    local quest = quest_log:get_quest(quest_id)
    if quest then
        quest_log:update_status(quest_id, "completed")

        -- Sync completion
        local assignee_peer = peer_map[quest.assignee]
        for name, peer in pairs(peer_map) do
            if name ~= quest.assignee then
                sim:send_message(assignee_peer, peer,
                    string.format("quest_status:%d:completed", quest_id))
            end
        end

        indras.log.debug("Quest completed", {
            trace_id = ctx.trace_id,
            quest_id = quest_id,
            title = quest.title,
            completed_by = quest.assignee
        })
    end
end

sim:run_ticks(SYNC_TICKS)

indras.log.info("Phase 4 complete: Quest statuses updated", {
    trace_id = ctx.trace_id,
    pending = quest_log:count_by_status("pending"),
    in_progress = quest_log:count_by_status("in_progress"),
    completed = quest_log:count_by_status("completed"),
    tick = sim.tick
})

-- ============================================================================
-- PHASE 5: VERIFICATION
-- ============================================================================

indras.log.info("Phase 5: Verification - Checking document convergence", {
    trace_id = ctx.trace_id
})

-- Verify quest log
indras.assert.eq(quest_log:count(), 6, "Should have 6 quests total")
indras.assert.eq(quest_log:count_by_status("completed"), 2, "Should have 2 completed quests")
indras.assert.eq(quest_log:count_by_status("in_progress"), 1, "Should have 1 in_progress quest")
indras.assert.eq(quest_log:count_by_status("pending"), 3, "Should have 3 pending quests")

-- Verify project plan
indras.assert.eq(project_plan:section_count(), 3, "Project plan should have 3 sections")
indras.assert.eq(#project_plan:get_authors(), 3, "All 3 peers should have contributed")

-- Verify network stats
local stats = sim.stats
indras.assert.gt(stats.messages_sent, 0, "Messages should have been sent")

-- Calculate convergence (all messages delivered)
local delivery_rate = stats:delivery_rate()

indras.log.info("Phase 5 complete: Verification passed", {
    trace_id = ctx.trace_id,
    quests_verified = quest_log:count(),
    sections_verified = project_plan:section_count(),
    messages_sent = stats.messages_sent,
    messages_delivered = stats.messages_delivered,
    delivery_rate = delivery_rate,
    tick = sim.tick
})

-- ============================================================================
-- FINAL SUMMARY
-- ============================================================================

-- Log quest summary
indras.log.info("=== QUEST LOG SUMMARY ===", { trace_id = ctx.trace_id })
for _, quest in ipairs(quest_log.quests) do
    indras.log.info(string.format("Quest #%d: %s", quest.id, quest.title), {
        trace_id = ctx.trace_id,
        status = quest.status,
        creator = quest.creator,
        assignee = quest.assignee
    })
end

-- Log project plan summary
indras.log.info("=== PROJECT PLAN SUMMARY ===", {
    trace_id = ctx.trace_id,
    title = project_plan.title,
    version = project_plan.version
})
for _, section in ipairs(project_plan.sections) do
    indras.log.info(string.format("Section by %s", section.author), {
        trace_id = ctx.trace_id,
        preview = string.sub(section.content, 1, 50) .. "..."
    })
end

-- Final statistics
indras.log.info("Collaboration Trio scenario completed successfully", {
    trace_id = ctx.trace_id,
    realm = REALM_NAME,
    peers = table.concat(PEER_NAMES, ", "),
    total_quests = quest_log:count(),
    completed_quests = quest_log:count_by_status("completed"),
    project_sections = project_plan:section_count(),
    messages_sent = stats.messages_sent,
    messages_delivered = stats.messages_delivered,
    delivery_rate = delivery_rate,
    total_ticks = sim.tick
})

-- Return scenario results
return {
    success = true,
    realm = REALM_NAME,
    peers = PEER_NAMES,
    quests = {
        total = quest_log:count(),
        pending = quest_log:count_by_status("pending"),
        in_progress = quest_log:count_by_status("in_progress"),
        completed = quest_log:count_by_status("completed"),
        version = quest_log.version
    },
    project_plan = {
        title = project_plan.title,
        sections = project_plan:section_count(),
        authors = project_plan:get_authors(),
        version = project_plan.version
    },
    network = {
        messages_sent = stats.messages_sent,
        messages_delivered = stats.messages_delivered,
        delivery_rate = delivery_rate,
        total_ticks = sim.tick
    }
}

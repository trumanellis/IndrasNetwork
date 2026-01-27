-- SDK Inline Image Chat Test
--
-- Tests the inline image chat feature using the Logo_black.png asset.
-- Demonstrates both embedded images and gallery sharing in chat.
--
-- Phases:
-- 1. Setup: Create mesh with members, initialize simulation
-- 2. Share Inline Image: Member shares an image inline in chat
-- 3. Share Gallery: Member shares a gallery of images
-- 4. Verify Display: Events logged for viewer rendering
--
-- JSONL Output: All events logged for indras-realm-viewer consumption

local artifact = require("lib.artifact_helpers")
local quest_helpers = require("lib.quest_helpers")

-- ============================================================================
-- FEATURED TEST ASSET
-- ============================================================================
-- Use the real Logo_black.png asset for realistic testing

local FEATURED_ASSET = {
    path = "assets/Logo_black.png",
    name = "Logo_black.png",
    size = 830269,  -- Actual file size in bytes (~811KB, under 2MB threshold)
    mime_type = "image/png",
    dimensions = {1024, 1024},  -- Image dimensions
    description = "IndrasNetwork logo - 1024x1024 PNG",
}

-- ============================================================================
-- SETUP
-- ============================================================================

local ctx = artifact.new_context("sdk_inline_image")
local logger = artifact.create_logger(ctx)

-- Configuration - use minimal settings
local config = {
    members = 3,
    ticks = 20,
}

logger.info("Starting inline image chat scenario", {
    members = config.members,
    ticks = config.ticks,
    featured_asset = FEATURED_ASSET.name,
})

-- Create mesh with N members
local mesh = indras.MeshBuilder.new(config.members):full_mesh()
local sim_config = indras.SimConfig.new({
    wake_probability = 0,
    sleep_probability = 0,
    initial_online_probability = 1,
    max_ticks = config.ticks,
})
local sim = indras.Simulation.new(mesh, sim_config)
sim:initialize()

local peers = mesh:peers()

-- Create realm from all peers
local peer_ids = {}
for _, peer in ipairs(peers) do
    table.insert(peer_ids, tostring(peer))
end
local realm_id = quest_helpers.compute_realm_id(peer_ids)

-- ============================================================================
-- PHASE 1: SETUP - Bring all peers online, create realm
-- ============================================================================

logger.info("Phase 1: Setup - Bringing peers online and creating realm", {
    phase = 1,
    peer_count = #peers,
})

for _, peer in ipairs(peers) do
    sim:force_online(peer)
end

sim:step()

logger.event("realm_created", {
    tick = sim.tick,
    realm_id = realm_id,
    member_count = #peers,
    members = table.concat(peer_ids, ","),
})

-- Add members to realm
for _, peer in ipairs(peers) do
    logger.event("member_joined", {
        tick = sim.tick,
        realm_id = realm_id,
        member = tostring(peer),
    })
end

logger.info("Phase 1 complete: All peers online, realm created", {
    phase = 1,
    tick = sim.tick,
    realm_id = realm_id,
})

-- ============================================================================
-- PHASE 2: TEXT CHAT MESSAGES
-- ============================================================================

logger.info("Phase 2: Initial chat messages", { phase = 2 })

sim:step()

-- First member sends a greeting
logger.event("chat_message", {
    tick = sim.tick,
    member = tostring(peers[1]),
    content = "Hello everyone! I have a cool logo to share.",
    message_type = "text",
    message_id = "msg-" .. sim.tick .. "-" .. tostring(peers[1]),
})

sim:step()

-- Second member responds
logger.event("chat_message", {
    tick = sim.tick,
    member = tostring(peers[2]),
    content = "Oh nice! Please share it!",
    message_type = "text",
    message_id = "msg-" .. sim.tick .. "-" .. tostring(peers[2]),
})

-- ============================================================================
-- PHASE 3: SHARE INLINE IMAGE
-- ============================================================================

logger.info("Phase 3: Share inline image", {
    phase = 3,
    asset = FEATURED_ASSET.name,
    size = FEATURED_ASSET.size,
})

sim:step()

-- First member shares the logo as an inline image
-- Using asset_path for the viewer to load the actual file
logger.event("chat_image", {
    tick = sim.tick,
    member = tostring(peers[1]),
    mime_type = FEATURED_ASSET.mime_type,
    filename = FEATURED_ASSET.name,
    dimensions = FEATURED_ASSET.dimensions,
    alt_text = "IndrasNetwork Logo",
    asset_path = FEATURED_ASSET.path,
    message_id = "img-" .. sim.tick .. "-" .. tostring(peers[1]),
})

logger.info("Inline image shared", {
    tick = sim.tick,
    sharer = tostring(peers[1]),
    filename = FEATURED_ASSET.name,
})

sim:step()

-- Third member reacts
logger.event("chat_message", {
    tick = sim.tick,
    member = tostring(peers[3]),
    content = "That logo looks great!",
    message_type = "text",
    message_id = "msg-" .. sim.tick .. "-" .. tostring(peers[3]),
})

-- ============================================================================
-- PHASE 4: SHARE GALLERY
-- ============================================================================

logger.info("Phase 4: Share gallery", { phase = 4 })

sim:step()

-- Second member shares a gallery (simulated)
logger.event("chat_gallery", {
    tick = sim.tick,
    member = tostring(peers[2]),
    folder_id = "gallery-vacation-001",
    title = "Project Screenshots",
    items = {
        {
            name = "screenshot1.png",
            mime_type = "image/png",
            size = 256000,
            artifact_hash = artifact.generate_hash(),
            dimensions = {1920, 1080},
            asset_path = FEATURED_ASSET.path,
        },
        {
            name = "screenshot2.png",
            mime_type = "image/png",
            size = 312000,
            artifact_hash = artifact.generate_hash(),
            dimensions = {1920, 1080},
            asset_path = FEATURED_ASSET.path,
        },
        {
            name = "logo.png",
            mime_type = "image/png",
            size = FEATURED_ASSET.size,
            artifact_hash = artifact.generate_hash(),
            dimensions = FEATURED_ASSET.dimensions,
            asset_path = FEATURED_ASSET.path,
        },
    },
    message_id = "gallery-" .. sim.tick .. "-" .. tostring(peers[2]),
})

logger.info("Gallery shared", {
    tick = sim.tick,
    sharer = tostring(peers[2]),
    item_count = 3,
})

sim:step()

-- First member comments on gallery
logger.event("chat_message", {
    tick = sim.tick,
    member = tostring(peers[1]),
    content = "Nice collection of screenshots!",
    message_type = "text",
    message_id = "msg-" .. sim.tick .. "-" .. tostring(peers[1]),
})

-- ============================================================================
-- PHASE 5: ANOTHER IMAGE
-- ============================================================================

logger.info("Phase 5: Another inline image", { phase = 5 })

sim:step()

-- Third member shares an image
logger.event("chat_image", {
    tick = sim.tick,
    member = tostring(peers[3]),
    mime_type = "image/png",
    filename = "my_contribution.png",
    dimensions = {800, 600},
    alt_text = "My contribution to the project",
    asset_path = FEATURED_ASSET.path,
    message_id = "img-" .. sim.tick .. "-" .. tostring(peers[3]),
})

-- ============================================================================
-- COMPLETION
-- ============================================================================

sim:step()

logger.info("Inline image chat scenario complete", {
    tick = sim.tick,
    images_shared = 2,
    galleries_shared = 1,
    messages_total = 4,
})

-- Final info event for viewer
logger.event("info", {
    tick = sim.tick,
    message = "Scenario complete: Inline images and gallery demonstrated",
    phase = 99,
})

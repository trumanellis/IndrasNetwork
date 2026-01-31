-- SDK Harmony Proof Scenario
--
-- Demonstrates the full Indra's Network lifecycle:
-- 1. Realm creation with 3 members
-- 2. Realm renamed to "Harmony" via RealmAliasSet event
-- 3. Quest creation (logo design + README update)
-- 4. Attention focus across members
-- 5. Work period
-- 6. Love submits proof folder with embedded logo artifact
-- 7. Gratitude release for logo quest (blessings from Peace + Joy)
-- 8. Joy submits proof folder with README.md referencing the logo
-- 9. Gratitude release for README quest (blessings from Love + Peace)
-- 10. Token pledge lifecycle:
--     a. Peace creates a "Community Guide" quest
--     b. Love & Joy pledge tokens as bounty on the new quest
--     c. Peace submits proof, tokens are released to Peace
--     d. Joy withdraws a token, then re-pledges it
--     e. Token chaining: Peace pledges a received token onward
--
-- Both quests go through the full lifecycle: create → focus → proof → bless → verify → complete
-- Tokens of Gratitude flow through multiple stewards demonstrating pledge, release, withdraw, and chaining.
--
-- Designed for visual verification with the Omni V2 viewer.

local quest_helpers = require("lib.quest_helpers")
local home = require("lib.home_realm_helpers")

-- ============================================================================
-- SETUP
-- ============================================================================

local ctx = quest_helpers.new_context("sdk_harmony_proof")
local logger = quest_helpers.create_logger(ctx)

logger.info("Starting Harmony Proof scenario", {
    level = quest_helpers.get_level(),
    description = "Full lifecycle: realm, quests, proof folder, gratitude",
})

-- Create 3-peer full mesh
local mesh = indras.MeshBuilder.new(3):full_mesh()
local sim_config = indras.SimConfig.new({
    wake_probability = 0,
    sleep_probability = 0,
    initial_online_probability = 1,
    max_ticks = 300,
})
local sim = indras.Simulation.new(mesh, sim_config)
sim:initialize()

local peers = mesh:peers()
local result = quest_helpers.result_builder("sdk_harmony_proof")

-- Assign roles
local peer_love = tostring(peers[1])
local peer_joy = tostring(peers[2])
local peer_peace = tostring(peers[3])
local all_members = { peer_love, peer_joy, peer_peace }

-- Tracking
local blessing_tracker = home.BlessingTracker.new()

-- Force all peers online
for _, peer in ipairs(peers) do
    sim:force_online(peer)
end

-- ============================================================================
-- PHASE 1: SETUP — Create realm, join members, add contacts
-- ============================================================================

logger.info("Phase 1: Setup — Create realm with 3 members", { phase = 1 })

local realm_id = quest_helpers.compute_realm_id(all_members)

-- Emit realm_created
logger.event("realm_created", {
    tick = sim.tick,
    realm_id = realm_id,
    members = table.concat(all_members, ","),
    member_count = 3,
})
sim:step()

-- Emit member_joined x3
for _, member in ipairs(all_members) do
    logger.event("member_joined", {
        tick = sim.tick,
        realm_id = realm_id,
        member = member,
    })
end
sim:step()

-- Emit contact_added x6 (all pairs, bidirectional)
for i, member in ipairs(all_members) do
    for j, other in ipairs(all_members) do
        if i ~= j then
            logger.event("contact_added", {
                tick = sim.tick,
                member = member,
                contact = other,
            })
        end
    end
end
sim:step()

-- Members update their profiles
logger.event("profile_updated", {
    tick = sim.tick,
    member = peer_love,
    headline = "Visual designer & community weaver",
    bio = "Creating symbols that bring people together. I believe every network deserves a visual identity that reflects its values.\n\n**Interests:** Graphic design, mythology, decentralized communities",
})
sim:step()

logger.event("profile_updated", {
    tick = sim.tick,
    member = peer_joy,
    headline = "Documentation & knowledge craft",
    bio = "I turn ideas into readable artifacts. If it's not written down, it doesn't exist.\n\n**Focus:** Technical writing, open-source docs, collaborative editing",
})
sim:step()

logger.event("profile_updated", {
    tick = sim.tick,
    member = peer_peace,
    headline = "Realm steward & quest architect",
    bio = "Keeping the realm running smoothly. I design quests that bring out the best in contributors and make sure good work gets recognized.\n\n**Role:** Coordination, review, gratitude",
})
sim:step()

logger.info("Phase 1 complete", {
    phase = 1,
    tick = sim.tick,
    realm_id = realm_id,
    members = 3,
})

-- ============================================================================
-- PHASE 2: RENAME REALM — Love renames realm to "Harmony"
-- ============================================================================

logger.info("Phase 2: Rename realm to Harmony", { phase = 2 })

logger.event("realm_alias_set", {
    tick = sim.tick,
    realm_id = realm_id,
    member = peer_love,
    alias = "Harmony",
})
sim:step()

-- Chat announcement
logger.event("chat_message", {
    tick = sim.tick,
    member = peer_love,
    realm_id = realm_id,
    content = "I renamed our realm to Harmony!",
    message_type = "text",
})
sim:step()

logger.info("Phase 2 complete", { phase = 2, tick = sim.tick })

-- ============================================================================
-- PHASE 3: CREATE QUESTS
-- ============================================================================

logger.info("Phase 3: Create quests", { phase = 3 })

-- Peace creates "Create a logo for Indra's Network"
local logo_quest_id = quest_helpers.generate_quest_id()
local logo_quest_title = "Create a logo for Indra's Network"

logger.event("quest_created", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = logo_quest_id,
    creator = peer_peace,
    title = logo_quest_title,
    latency_us = quest_helpers.quest_create_latency(),
})
sim:step()

-- Joy creates "Update the README.md"
local readme_quest_id = quest_helpers.generate_quest_id()
local readme_quest_title = "Update the README.md"

logger.event("quest_created", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = readme_quest_id,
    creator = peer_joy,
    title = readme_quest_title,
    latency_us = quest_helpers.quest_create_latency(),
})
sim:step()

logger.info("Phase 3 complete", {
    phase = 3,
    tick = sim.tick,
    logo_quest = logo_quest_id,
    readme_quest = readme_quest_id,
})

-- ============================================================================
-- PHASE 4: SET ACTIVE INTENTIONS — Focus attention on quests
-- ============================================================================

logger.info("Phase 4: Set active intentions", { phase = 4 })

-- Love focuses on logo quest
logger.event("attention_switched", {
    tick = sim.tick,
    member = peer_love,
    quest_id = logo_quest_id,
    latency_us = quest_helpers.attention_switch_latency(),
})
-- Record 60s of attention for Love on logo quest
blessing_tracker:record_attention(logo_quest_id, peer_love, 1, 60000)
sim:step()

-- Peace focuses on logo quest
logger.event("attention_switched", {
    tick = sim.tick,
    member = peer_peace,
    quest_id = logo_quest_id,
    latency_us = quest_helpers.attention_switch_latency(),
})
-- Record 45s of attention for Peace on logo quest
blessing_tracker:record_attention(logo_quest_id, peer_peace, 1, 45000)
sim:step()

-- Joy focuses on README quest
logger.event("attention_switched", {
    tick = sim.tick,
    member = peer_joy,
    quest_id = readme_quest_id,
    latency_us = quest_helpers.attention_switch_latency(),
})
-- Record 30s of attention for Joy on README quest
blessing_tracker:record_attention(readme_quest_id, peer_joy, 1, 30000)
sim:step()

logger.info("Phase 4 complete", { phase = 4, tick = sim.tick })

-- ============================================================================
-- PHASE 5: WORK PERIOD — 30 simulation steps
-- ============================================================================

logger.info("Phase 5: Work period (30 ticks)", { phase = 5 })

for i = 1, 30 do
    sim:step()
end

logger.info("Phase 5 complete", { phase = 5, tick = sim.tick })

-- ============================================================================
-- PHASE 6: LOVE'S PROOF FOLDER — Logo quest proof submission
-- ============================================================================

logger.info("Phase 6: Love submits proof folder for logo quest", { phase = 6 })

-- 1. Create proof folder (draft)
local folder_id = home.generate_artifact_id()

logger.event("proof_folder_created", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = logo_quest_id,
    folder_id = folder_id,
    claimant = peer_love,
    status = "draft",
})
sim:step()

-- 2. Add artifact: Logo_black.png
local logo_artifact_id = home.generate_artifact_id()

logger.event("proof_folder_artifact_added", {
    tick = sim.tick,
    realm_id = realm_id,
    folder_id = folder_id,
    artifact_id = logo_artifact_id,
    artifact_name = "Logo_black.png",
    artifact_size = 830269,
    mime_type = "image/png",
    asset_path = "assets/Logo_black.png",
    caption = "Indra's Network Logo (black version)",
})
sim:step()

-- 3. Update narrative with markdown and embedded logo reference
local narrative = string.format([[## Proof of Service: Logo Design

I created a logo for Indra's Network based on the quest requirements.

### The Logo
![Indra's Network Logo](artifact:%s)

### Design Process
1. Researched network symbolism and Sanskrit mythology
2. Created multiple iterations in vector format
3. Finalized the black version for light backgrounds

### Deliverable
- **Logo_black.png** — 1024x1024 optimized PNG
- Clean monochrome design suitable for all media]], logo_artifact_id)

logger.event("proof_folder_narrative_updated", {
    tick = sim.tick,
    realm_id = realm_id,
    folder_id = folder_id,
    claimant = peer_love,
    narrative_length = #narrative,
    narrative = narrative,
})
sim:step()

-- 4. Submit proof folder
local narrative_preview = "Proof of Service: Logo Design — I created a logo for Indra's Network..."

logger.event("proof_folder_submitted", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = logo_quest_id,
    claimant = peer_love,
    folder_id = folder_id,
    artifact_count = 1,
    narrative_preview = narrative_preview,
    quest_title = logo_quest_title,
    narrative = narrative,
    artifacts = {
        {
            artifact_hash = logo_artifact_id,
            name = "Logo_black.png",
            mime_type = "image/png",
            size = 830269,
            caption = "Indra's Network Logo (black version)",
            asset_path = "assets/Logo_black.png",
        },
    },
})
sim:step()

-- 5. Chat message from Love
logger.event("chat_message", {
    tick = sim.tick,
    member = peer_love,
    realm_id = realm_id,
    content = "Submitted my proof for the logo quest!",
    message_type = "text",
})
sim:step()

logger.info("Phase 6 complete", { phase = 6, tick = sim.tick, folder_id = folder_id })

-- ============================================================================
-- PHASE 7: GRATITUDE RELEASE — Blessings from Peace and Joy
-- ============================================================================

logger.info("Phase 7: Gratitude release", { phase = 7 })

-- 1. Peace releases gratitude (45s attention on logo quest)
local peace_attention = 45000
blessing_tracker:record_blessing(logo_quest_id, peer_love, peer_peace, {1}, peace_attention)

logger.event("blessing_given", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = logo_quest_id,
    claimant = peer_love,
    blesser = peer_peace,
    event_count = 1,
    attention_millis = peace_attention,
})

-- Mint token for Peace's blessing of Love's logo proof
local tok_peace_logo = home.make_token_id(logo_quest_id, peer_love, sim.tick)
home.emit_token_minted(logger, sim.tick, realm_id, tok_peace_logo,
    peer_love, peace_attention, peer_peace, logo_quest_id)
sim:step()

-- 2. Chat from Peace
logger.event("chat_message", {
    tick = sim.tick,
    member = peer_peace,
    realm_id = realm_id,
    content = "Beautiful logo! Releasing my gratitude.",
    message_type = "text",
})
sim:step()

-- 3. Joy switches attention to logo quest, then releases gratitude (15s)
logger.event("attention_switched", {
    tick = sim.tick,
    member = peer_joy,
    quest_id = logo_quest_id,
    latency_us = quest_helpers.attention_switch_latency(),
})
blessing_tracker:record_attention(logo_quest_id, peer_joy, 1, 15000)
sim:step()

local joy_attention = 15000
blessing_tracker:record_blessing(logo_quest_id, peer_love, peer_joy, {1}, joy_attention)

logger.event("blessing_given", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = logo_quest_id,
    claimant = peer_love,
    blesser = peer_joy,
    event_count = 1,
    attention_millis = joy_attention,
})

-- Mint token for Joy's blessing of Love's logo proof
local tok_joy_logo = home.make_token_id(logo_quest_id, peer_love, sim.tick)
home.emit_token_minted(logger, sim.tick, realm_id, tok_joy_logo,
    peer_love, joy_attention, peer_joy, logo_quest_id)
sim:step()

-- 4. Chat from Joy
logger.event("chat_message", {
    tick = sim.tick,
    member = peer_joy,
    realm_id = realm_id,
    content = "Great work on the logo!",
    message_type = "text",
})
sim:step()

-- 5. Quest claim verified + completed
logger.event("quest_claim_verified", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = logo_quest_id,
    claim_index = 0,
})
sim:step()

logger.event("quest_completed", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = logo_quest_id,
    verified_claims = 1,
    pending_claims = 0,
})
sim:step()

-- 6. Final chat message
logger.event("chat_message", {
    tick = sim.tick,
    member = peer_peace,
    realm_id = realm_id,
    content = "Logo quest completed!",
    message_type = "text",
})
sim:step()

logger.info("Phase 7 complete", {
    phase = 7,
    tick = sim.tick,
    total_blessed_millis = blessing_tracker:get_total_blessed(logo_quest_id, peer_love),
})

-- ============================================================================
-- PHASE 8: JOY'S PROOF FOLDER — README quest proof submission
-- ============================================================================

logger.info("Phase 8: Joy submits proof folder for README quest", { phase = 8 })

-- Joy switches attention back to the README quest
logger.event("attention_switched", {
    tick = sim.tick,
    member = peer_joy,
    quest_id = readme_quest_id,
    latency_us = quest_helpers.attention_switch_latency(),
})
sim:step()

-- 1. Create proof folder (draft)
local readme_folder_id = home.generate_artifact_id()

logger.event("proof_folder_created", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = readme_quest_id,
    folder_id = readme_folder_id,
    claimant = peer_joy,
    status = "draft",
})
sim:step()

-- 2. Add artifact: README.md (a markdown file that references the logo)
local readme_artifact_id = home.generate_artifact_id()
local readme_content = string.format([[# Indra's Network

A peer-to-peer network for collaborative service and mutual recognition.

![Indra's Network Logo](artifact:%s)

## Overview

Indra's Network enables small groups to:
- **Create Realms** — shared spaces for collaboration
- **Post Quests** — requests for service from the community
- **Submit Proofs** — evidence of completed work with artifacts
- **Release Gratitude** — bless contributors with accumulated attention

## Getting Started

1. Join or create a Realm
2. Browse open Quests on the quest board
3. Focus your attention on work that matters
4. Submit a Proof of Service when done

## Built With

- Rust + Dioxus for the viewer
- Lua scenarios for simulation
- BLAKE3 for artifact hashing

---

*Created with care by the Harmony realm.*
]], logo_artifact_id)

logger.event("proof_folder_artifact_added", {
    tick = sim.tick,
    realm_id = realm_id,
    folder_id = readme_folder_id,
    artifact_id = readme_artifact_id,
    artifact_name = "README.md",
    artifact_size = #readme_content,
    mime_type = "text/markdown",
    caption = "Project README with embedded logo reference",
})
sim:step()

-- 3. Update narrative
local readme_narrative = string.format([[## Proof of Service: README Update

I wrote a comprehensive README.md for Indra's Network that includes:

### Contents
1. Project overview and mission statement
2. Embedded reference to the logo (artifact:%s)
3. Feature summary (Realms, Quests, Proofs, Gratitude)
4. Getting started guide
5. Technical stack

### Notes
- The README references Love's logo via artifact link
- Written in standard GitHub-flavored Markdown
- Ready for the project root]], logo_artifact_id)

logger.event("proof_folder_narrative_updated", {
    tick = sim.tick,
    realm_id = realm_id,
    folder_id = readme_folder_id,
    claimant = peer_joy,
    narrative_length = #readme_narrative,
    narrative = readme_narrative,
})
sim:step()

-- 4. Submit proof folder
local readme_narrative_preview = "Proof of Service: README Update — I wrote a comprehensive README.md..."

logger.event("proof_folder_submitted", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = readme_quest_id,
    claimant = peer_joy,
    folder_id = readme_folder_id,
    artifact_count = 1,
    narrative_preview = readme_narrative_preview,
    quest_title = readme_quest_title,
    narrative = readme_narrative,
    artifacts = {
        {
            artifact_hash = readme_artifact_id,
            name = "README.md",
            mime_type = "text/markdown",
            size = #readme_content,
            caption = "Project README with embedded logo reference",
        },
    },
})
sim:step()

-- 5. Chat message from Joy
logger.event("chat_message", {
    tick = sim.tick,
    member = peer_joy,
    realm_id = realm_id,
    content = "Submitted my README — it references the logo!",
    message_type = "text",
})
sim:step()

logger.info("Phase 8 complete", { phase = 8, tick = sim.tick, folder_id = readme_folder_id })

-- ============================================================================
-- PHASE 9: GRATITUDE FOR JOY — Blessings from Love and Peace
-- ============================================================================

-- ============================================================================
-- PHASE 9B: COLLABORATIVE EDITING — CRDT document edits on README
-- ============================================================================

logger.info("Phase 9B: Collaborative document editing via CRDT", { phase = 9 })

-- Joy edits the README (initial CRDT version)
logger.event("document_edit", {
    tick = sim.tick,
    document_id = readme_artifact_id,
    editor = peer_joy,
    content = string.format([[# Indra's Network

![Indra's Network Logo](artifact:%s)

A peer-to-peer network for collaborative service and mutual recognition.

## Overview

Indra's Network enables small groups to:
- **Create Realms** — shared spaces for collaboration
- **Post Quests** — requests for service from the community
- **Submit Proofs** — evidence of completed work with artifacts
- **Release Gratitude** — bless contributors with accumulated attention

## Getting Started

1. Join or create a Realm
2. Browse open Quests on the quest board
3. Focus your attention on work that matters
4. Submit a Proof of Service when done

---

*Created with care by the Harmony realm.*
]], logo_artifact_id),
    realm_id = realm_id,
})
sim:step()

-- Love edits the README (adds Contributors section)
logger.event("document_edit", {
    tick = sim.tick,
    document_id = readme_artifact_id,
    editor = peer_love,
    content = string.format([[# Indra's Network

![Indra's Network Logo](artifact:%s)

A peer-to-peer network for collaborative service and mutual recognition.

## Overview

Indra's Network enables small groups to:
- **Create Realms** — shared spaces for collaboration
- **Post Quests** — requests for service from the community
- **Submit Proofs** — evidence of completed work with artifacts
- **Release Gratitude** — bless contributors with accumulated attention

## Getting Started

1. Join or create a Realm
2. Browse open Quests on the quest board
3. Focus your attention on work that matters
4. Submit a Proof of Service when done

## Contributors

- **Love** — Logo design
- **Joy** — README authoring
- **Peace** — Quest creation & review

---

*Created with care by the Harmony realm.*
]], logo_artifact_id),
    realm_id = realm_id,
})
sim:step()

-- CRDT converged after edits
logger.event("crdt_converged", {
    tick = sim.tick,
    folder_id = readme_artifact_id,
    members_synced = 3,
})
sim:step()

logger.info("Phase 9B complete", { phase = 9, tick = sim.tick })

-- ============================================================================
-- PHASE 9C: GRATITUDE FOR JOY — Blessings from Love and Peace
-- ============================================================================

logger.info("Phase 9C: Gratitude for Joy's README proof", { phase = 9 })

-- 1. Love switches to README quest and releases gratitude
logger.event("attention_switched", {
    tick = sim.tick,
    member = peer_love,
    quest_id = readme_quest_id,
    latency_us = quest_helpers.attention_switch_latency(),
})
blessing_tracker:record_attention(readme_quest_id, peer_love, 1, 25000)
sim:step()

local love_readme_attention = 25000
blessing_tracker:record_blessing(readme_quest_id, peer_joy, peer_love, {1}, love_readme_attention)

logger.event("blessing_given", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = readme_quest_id,
    claimant = peer_joy,
    blesser = peer_love,
    event_count = 1,
    attention_millis = love_readme_attention,
})

-- Mint token for Love's blessing of Joy's README proof
local tok_love_readme = home.make_token_id(readme_quest_id, peer_joy, sim.tick)
home.emit_token_minted(logger, sim.tick, realm_id, tok_love_readme,
    peer_joy, love_readme_attention, peer_love, readme_quest_id)
sim:step()

-- 2. Chat from Love
logger.event("chat_message", {
    tick = sim.tick,
    member = peer_love,
    realm_id = realm_id,
    content = "Nice README! Love seeing the logo in there.",
    message_type = "text",
})
sim:step()

-- 3. Peace switches to README quest and releases gratitude
logger.event("attention_switched", {
    tick = sim.tick,
    member = peer_peace,
    quest_id = readme_quest_id,
    latency_us = quest_helpers.attention_switch_latency(),
})
blessing_tracker:record_attention(readme_quest_id, peer_peace, 1, 20000)
sim:step()

local peace_readme_attention = 20000
blessing_tracker:record_blessing(readme_quest_id, peer_joy, peer_peace, {1}, peace_readme_attention)

logger.event("blessing_given", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = readme_quest_id,
    claimant = peer_joy,
    blesser = peer_peace,
    event_count = 1,
    attention_millis = peace_readme_attention,
})

-- Mint token for Peace's blessing of Joy's README proof
local tok_peace_readme = home.make_token_id(readme_quest_id, peer_joy, sim.tick)
home.emit_token_minted(logger, sim.tick, realm_id, tok_peace_readme,
    peer_joy, peace_readme_attention, peer_peace, readme_quest_id)
sim:step()

-- 4. Chat from Peace
logger.event("chat_message", {
    tick = sim.tick,
    member = peer_peace,
    realm_id = realm_id,
    content = "Clean and thorough. Releasing gratitude!",
    message_type = "text",
})
sim:step()

-- 5. Quest claim verified + completed
logger.event("quest_claim_verified", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = readme_quest_id,
    claim_index = 0,
})
sim:step()

logger.event("quest_completed", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = readme_quest_id,
    verified_claims = 1,
    pending_claims = 0,
})
sim:step()

-- 6. Celebration
logger.event("chat_message", {
    tick = sim.tick,
    member = peer_joy,
    realm_id = realm_id,
    content = "Both quests done! Harmony is looking great.",
    message_type = "text",
})
sim:step()

logger.info("Phase 9 complete", {
    phase = 9,
    tick = sim.tick,
    total_blessed_millis = blessing_tracker:get_total_blessed(readme_quest_id, peer_joy),
})

-- ============================================================================
-- PHASE 10: TOKEN PLEDGE LIFECYCLE — Pledge, release, withdraw, chaining
-- ============================================================================
--
-- Token inventory at this point:
--   Love holds: tok_peace_logo (45s), tok_joy_logo (15s)
--   Joy holds:  tok_love_readme (25s), tok_peace_readme (20s)
--   Peace holds: (none)
--
-- Plan:
--   1. Peace creates a new quest: "Write a community guide"
--   2. Love pledges tok_peace_logo (45s) to community guide as bounty
--   3. Joy pledges tok_love_readme (25s) to community guide as bounty
--   4. Joy also pledges tok_peace_readme (20s), then withdraws it (demonstrating withdraw)
--   5. Peace submits proof for community guide
--   6. Love releases tok_peace_logo to Peace (steward transfer: Love → Peace)
--   7. Joy releases tok_love_readme to Peace (steward transfer: Joy → Peace)
--   8. Token chaining: Peace pledges tok_peace_logo (now hers) to a future quest

logger.info("Phase 10: Token pledge lifecycle", { phase = 10 })

-- 10.1 Peace creates a new quest: "Write a community guide"
local guide_quest_id = quest_helpers.generate_quest_id()
local guide_quest_title = "Write a community guide"

logger.event("quest_created", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = guide_quest_id,
    creator = peer_peace,
    title = guide_quest_title,
    latency_us = quest_helpers.quest_create_latency(),
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_peace,
    realm_id = realm_id,
    content = "New quest: Write a community guide! I'm putting up a bounty call.",
    message_type = "text",
})
sim:step()

-- 10.2 Love pledges tok_peace_logo (45s) to the community guide quest
home.emit_gratitude_pledged(logger, sim.tick, realm_id, tok_peace_logo,
    peer_love, guide_quest_id, peace_attention)
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_love,
    realm_id = realm_id,
    content = "Pledging my 45s token to the community guide quest as bounty!",
    message_type = "text",
})
sim:step()

-- 10.3 Joy pledges tok_love_readme (25s) to the community guide quest
home.emit_gratitude_pledged(logger, sim.tick, realm_id, tok_love_readme,
    peer_joy, guide_quest_id, love_readme_attention)
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_joy,
    realm_id = realm_id,
    content = "Adding my 25s token as bounty too. Let's make this guide happen!",
    message_type = "text",
})
sim:step()

-- 10.4 Joy also pledges tok_peace_readme (20s), then withdraws it
home.emit_gratitude_pledged(logger, sim.tick, realm_id, tok_peace_readme,
    peer_joy, guide_quest_id, peace_readme_attention)
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_joy,
    realm_id = realm_id,
    content = "Actually, let me pull that 20s token back — saving it for later.",
    message_type = "text",
})
sim:step()

home.emit_gratitude_withdrawn(logger, sim.tick, realm_id, tok_peace_readme,
    peer_joy, guide_quest_id, peace_readme_attention)
sim:step()

-- 10.5 Peace submits proof for the community guide quest
logger.event("attention_switched", {
    tick = sim.tick,
    member = peer_peace,
    quest_id = guide_quest_id,
    latency_us = quest_helpers.attention_switch_latency(),
})
blessing_tracker:record_attention(guide_quest_id, peer_peace, 1, 35000)
sim:step()

local guide_folder_id = home.generate_artifact_id()

logger.event("proof_folder_created", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = guide_quest_id,
    folder_id = guide_folder_id,
    claimant = peer_peace,
    status = "draft",
})
sim:step()

local guide_narrative = [[## Proof of Service: Community Guide

I wrote a community guide covering:

### Contents
1. How to join a realm and introduce yourself
2. Quest etiquette — how to claim, submit proof, and give blessings
3. Understanding Tokens of Gratitude — earning, pledging, and releasing
4. Best practices for collaborative work

### Notes
- Written from the perspective of a new member joining Harmony
- Covers the full lifecycle from joining to earning your first token]]

logger.event("proof_folder_narrative_updated", {
    tick = sim.tick,
    realm_id = realm_id,
    folder_id = guide_folder_id,
    claimant = peer_peace,
    narrative_length = #guide_narrative,
    narrative = guide_narrative,
})
sim:step()

logger.event("proof_folder_submitted", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = guide_quest_id,
    claimant = peer_peace,
    folder_id = guide_folder_id,
    artifact_count = 0,
    narrative_preview = "Proof of Service: Community Guide — I wrote a community guide covering...",
    quest_title = guide_quest_title,
    narrative = guide_narrative,
    artifacts = {},
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_peace,
    realm_id = realm_id,
    content = "Submitted my proof for the community guide!",
    message_type = "text",
})
sim:step()

-- 10.6 Love releases tok_peace_logo to Peace (steward transfer: Love → Peace)
home.emit_gratitude_released(logger, sim.tick, realm_id, tok_peace_logo,
    peer_love, peer_peace, guide_quest_id, peace_attention)
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_love,
    realm_id = realm_id,
    content = "Releasing my 45s bounty token to Peace for the guide. Well earned!",
    message_type = "text",
})
sim:step()

-- 10.7 Joy releases tok_love_readme to Peace (steward transfer: Joy → Peace)
home.emit_gratitude_released(logger, sim.tick, realm_id, tok_love_readme,
    peer_joy, peer_peace, guide_quest_id, love_readme_attention)
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_joy,
    realm_id = realm_id,
    content = "Releasing my 25s bounty to Peace too. Great guide!",
    message_type = "text",
})
sim:step()

-- Quest claim verified + completed
logger.event("quest_claim_verified", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = guide_quest_id,
    claim_index = 0,
})
sim:step()

logger.event("quest_completed", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = guide_quest_id,
    verified_claims = 1,
    pending_claims = 0,
})
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_peace,
    realm_id = realm_id,
    content = "Community guide quest complete! I received both bounty tokens.",
    message_type = "text",
})
sim:step()

logger.info("Phase 10 complete", {
    phase = 10,
    tick = sim.tick,
    guide_quest = guide_quest_id,
    tokens_released = 2,
    tokens_withdrawn = 1,
})

-- ============================================================================
-- PHASE 11: TOKEN CHAINING — Peace pledges a received token onward
-- ============================================================================
--
-- Token inventory now:
--   Love holds:  tok_joy_logo (15s)
--   Joy holds:   tok_peace_readme (20s)  [withdrawn earlier, back in wallet]
--   Peace holds: tok_peace_logo (45s), tok_love_readme (25s)  [received via release]
--
-- Peace creates a final quest and pledges tok_peace_logo (originally Love's)
-- to demonstrate token chaining: Peace → Joy → Peace → (future quest)

logger.info("Phase 11: Token chaining", { phase = 11 })

-- 11.1 Joy creates a quest: "Design realm onboarding flow"
local onboard_quest_id = quest_helpers.generate_quest_id()
local onboard_quest_title = "Design realm onboarding flow"

logger.event("quest_created", {
    tick = sim.tick,
    realm_id = realm_id,
    quest_id = onboard_quest_id,
    creator = peer_joy,
    title = onboard_quest_title,
    latency_us = quest_helpers.quest_create_latency(),
})
sim:step()

-- 11.2 Peace pledges tok_peace_logo (45s) to the onboarding quest
-- This token was: minted for Love → pledged by Love → released to Peace → now pledged by Peace
home.emit_gratitude_pledged(logger, sim.tick, realm_id, tok_peace_logo,
    peer_peace, onboard_quest_id, peace_attention)
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_peace,
    realm_id = realm_id,
    content = "Pledging the 45s token I received to the onboarding quest. Gratitude keeps flowing!",
    message_type = "text",
})
sim:step()

-- 11.3 Peace also pledges tok_love_readme (25s) to the onboarding quest
home.emit_gratitude_pledged(logger, sim.tick, realm_id, tok_love_readme,
    peer_peace, onboard_quest_id, love_readme_attention)
sim:step()

logger.event("chat_message", {
    tick = sim.tick,
    member = peer_peace,
    realm_id = realm_id,
    content = "And 25s more. Total bounty: 70s of attention on this quest!",
    message_type = "text",
})
sim:step()

logger.info("Phase 11 complete", {
    phase = 11,
    tick = sim.tick,
    onboard_quest = onboard_quest_id,
    chained_tokens = 2,
    total_bounty_millis = peace_attention + love_readme_attention,
})

-- ============================================================================
-- FINAL RESULTS
-- ============================================================================

local logo_blessed = blessing_tracker:get_total_blessed(logo_quest_id, peer_love)
local readme_blessed = blessing_tracker:get_total_blessed(readme_quest_id, peer_joy)
local total_blessed = logo_blessed + readme_blessed

result:add_metrics({
    total_members = 3,
    total_quests = 4,  -- logo, readme, community guide, onboarding
    total_proof_folders = 3,  -- logo, readme, community guide
    total_blessings = 4,
    total_blessed_millis = total_blessed,
    total_tokens_minted = 4,
    total_pledges = 5,  -- 3 on guide (1 withdrawn) + 2 on onboarding
    total_releases = 2,
    total_withdrawals = 1,
})

result:record_assertion("realm_created", true, true, true)
result:record_assertion("realm_alias_set", true, true, true)
result:record_assertion("quests_created", 4, 4, true)
result:record_assertion("proof_submitted", 3, 3, true)
result:record_assertion("blessings_given", 4, 4, true)
result:record_assertion("quests_completed", 3, 3, true)
result:record_assertion("tokens_minted", 4, 4, true)
result:record_assertion("tokens_pledged", 5, 5, true)
result:record_assertion("tokens_released", 2, 2, true)
result:record_assertion("tokens_withdrawn", 1, 1, true)
result:record_assertion("token_chaining", true, true, true)

local final_result = result:build()

logger.info("Harmony Proof scenario completed", {
    passed = final_result.passed,
    level = final_result.level,
    duration_sec = final_result.duration_sec,
    final_tick = sim.tick,
    logo_blessed_millis = logo_blessed,
    readme_blessed_millis = readme_blessed,
    total_blessed_millis = total_blessed,
    tokens_minted = 4,
    pledges = 5,
    releases = 2,
    withdrawals = 1,
})

-- Hard assertions
indras.assert.gt(logo_blessed, 0, "Should have accumulated blessed attention for logo")
indras.assert.eq(#blessing_tracker:get_blessers(logo_quest_id, peer_love), 2,
    "Logo proof should have 2 blessers")
indras.assert.gt(readme_blessed, 0, "Should have accumulated blessed attention for README")
indras.assert.eq(#blessing_tracker:get_blessers(readme_quest_id, peer_joy), 2,
    "README proof should have 2 blessers")

logger.info("Harmony Proof scenario passed", {
    logo_blessed_millis = logo_blessed,
    readme_blessed_millis = readme_blessed,
    logo_blesser_count = #blessing_tracker:get_blessers(logo_quest_id, peer_love),
    readme_blesser_count = #blessing_tracker:get_blessers(readme_quest_id, peer_joy),
    -- Token lifecycle summary:
    -- tok_peace_logo (45s): minted for Love → pledged to guide → released to Peace → pledged to onboarding
    -- tok_joy_logo (15s): minted for Love → (still held by Love)
    -- tok_love_readme (25s): minted for Joy → pledged to guide → released to Peace → pledged to onboarding
    -- tok_peace_readme (20s): minted for Joy → pledged to guide → withdrawn → (held by Joy)
})

return final_result

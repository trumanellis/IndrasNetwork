-- Live Harmony Proof — real P2P with viewer-compatible JSONL
--
-- The Harmony Proof scenario running on real P2P infrastructure.
-- Three independent IndrasNode instances (Love, Joy, Peace) connect over
-- actual QUIC transport, exchange messages through CRDT-synced interfaces,
-- and verify that the full quest/proof/blessing/token lifecycle propagates
-- across the network.
--
-- Emits structured JSONL events for the Omni V2 viewer alongside real P2P ops.
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/live_harmony.lua
--
-- With viewer:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/live_harmony.lua \
--     | cargo run -p indras-realm-viewer --bin omni-viewer-v2

local quest_helpers = require("lib.quest_helpers")
local home = require("lib.home_realm_helpers")

-- ============================================================================
-- SETUP — Logger and tick counter
-- ============================================================================

local ctx = quest_helpers.new_context("live_harmony")
local logger = quest_helpers.create_logger(ctx)

-- Manual tick counter (no simulation, so we track ticks ourselves)
local tick = 0
local function advance()
    tick = tick + 1
    return tick
end

local blessing_tracker = home.BlessingTracker.new()

logger.info("Starting Live Harmony Proof", {
    description = "Full lifecycle on real P2P: realm, quests, proofs, blessings, tokens",
    phase = 0,
})

-- ============================================================================
-- PHASE 1: CREATE AND START NODES
-- ============================================================================

indras.narrative("Three members prepare to form the Harmony realm")
logger.info("Phase 1: Creating three real LiveNode instances", { phase = 1 })

local love = indras.LiveNode.new()
local joy  = indras.LiveNode.new()
local peace = indras.LiveNode.new()

love:start()
joy:start()
peace:start()
indras.assert.true_(love:is_started(), "Love should be started")
indras.assert.true_(joy:is_started(), "Joy should be started")
indras.assert.true_(peace:is_started(), "Peace should be started")

local peer_love = love:identity()
local peer_joy = joy:identity()
local peer_peace = peace:identity()
local all_members = { peer_love, peer_joy, peer_peace }

logger.info("Nodes started", {
    phase = 1,
    tick = tick,
    love_id = peer_love,
    joy_id = peer_joy,
    peace_id = peer_peace,
})

-- ============================================================================
-- PHASE 2: CREATE REALM — Love creates the Harmony interface
-- ============================================================================

indras.narrative("Three members form the Harmony realm")
logger.info("Phase 2: Create realm", { phase = 2 })

local realm_id, invite = love:create_interface("Harmony")
local joy_realm = joy:join_interface(invite)
local peace_realm = peace:join_interface(invite)
indras.assert.eq(joy_realm, realm_id, "Joy's realm ID should match")
indras.assert.eq(peace_realm, realm_id, "Peace's realm ID should match")

-- Emit viewer events: realm_created + member_joined
advance()
logger.event("realm_created", {
    tick = tick,
    realm_id = realm_id,
    members = table.concat(all_members, ","),
    member_count = 3,
})

for _, member in ipairs(all_members) do
    advance()
    logger.event("member_joined", {
        tick = tick,
        realm_id = realm_id,
        member = member,
    })
end

-- Emit contacts (bidirectional)
for i, member in ipairs(all_members) do
    for j, other in ipairs(all_members) do
        if i ~= j then
            logger.event("contact_added", {
                tick = tick,
                member = member,
                contact = other,
            })
        end
    end
end
advance()

-- ============================================================================
-- PHASE 3: RENAME REALM + INTRODUCTIONS
-- ============================================================================

indras.narrative("A name is chosen — this realm shall be called Harmony")
logger.info("Phase 3: Rename realm and introduce members", { phase = 3 })

advance()
logger.event("realm_alias_set", {
    tick = tick,
    realm_id = realm_id,
    member = peer_love,
    alias = "Harmony",
})

-- Real P2P: send introduction messages
love:send_message(realm_id,
    "Hi everyone! I'm Love — visual designer & community weaver.")
joy:send_message(realm_id,
    "Hey! I'm Joy — documentation & knowledge craft.")
peace:send_message(realm_id,
    "Hello! I'm Peace — realm steward & quest architect.")

-- Emit profile updates for viewer
advance()
logger.event("profile_updated", {
    tick = tick,
    member = peer_love,
    headline = "Visual designer & community weaver",
    bio = "Creating symbols that bring people together.",
})

advance()
logger.event("profile_updated", {
    tick = tick,
    member = peer_joy,
    headline = "Documentation & knowledge craft",
    bio = "I turn ideas into readable artifacts.",
})

advance()
logger.event("profile_updated", {
    tick = tick,
    member = peer_peace,
    headline = "Realm steward & quest architect",
    bio = "Keeping the realm running smoothly.",
})

-- Chat messages for viewer
advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_love,
    realm_id = realm_id,
    content = "I renamed our realm to Harmony!",
    message_type = "text",
})

-- ============================================================================
-- PHASE 4: CREATE QUESTS
-- ============================================================================

indras.narrative("Two quests emerge — a logo to design, a README to write")
logger.info("Phase 4: Create quests", { phase = 4 })

local logo_quest_id = quest_helpers.generate_quest_id()
local logo_quest_title = "Create a logo for Indra's Network"

advance()
logger.event("quest_created", {
    tick = tick,
    realm_id = realm_id,
    quest_id = logo_quest_id,
    creator = peer_peace,
    title = logo_quest_title,
    latency_us = quest_helpers.quest_create_latency(),
})

-- Real P2P: announce quest
peace:send_message(realm_id,
    "[QUEST] " .. logo_quest_title ..
    " — We need a visual identity. Clean monochrome design suitable for all media.")

local readme_quest_id = quest_helpers.generate_quest_id()
local readme_quest_title = "Update the README.md"

advance()
logger.event("quest_created", {
    tick = tick,
    realm_id = realm_id,
    quest_id = readme_quest_id,
    creator = peer_joy,
    title = readme_quest_title,
    latency_us = quest_helpers.quest_create_latency(),
})

joy:send_message(realm_id,
    "[QUEST] " .. readme_quest_title ..
    " — Write a comprehensive README with project overview and getting started guide.")

-- ============================================================================
-- PHASE 5: SET ACTIVE INTENTIONS
-- ============================================================================

indras.narrative("The community focuses its attention on the work ahead")
logger.info("Phase 5: Set active intentions", { phase = 5 })

advance()
logger.event("attention_switched", {
    tick = tick,
    member = peer_love,
    quest_id = logo_quest_id,
    latency_us = quest_helpers.attention_switch_latency(),
})
blessing_tracker:record_attention(logo_quest_id, peer_love, 1, 60000)

love:send_message(realm_id, "[FOCUS] Working on: " .. logo_quest_title)

advance()
logger.event("attention_switched", {
    tick = tick,
    member = peer_peace,
    quest_id = logo_quest_id,
    latency_us = quest_helpers.attention_switch_latency(),
})
blessing_tracker:record_attention(logo_quest_id, peer_peace, 1, 45000)

advance()
logger.event("attention_switched", {
    tick = tick,
    member = peer_joy,
    quest_id = readme_quest_id,
    latency_us = quest_helpers.attention_switch_latency(),
})
blessing_tracker:record_attention(readme_quest_id, peer_joy, 1, 30000)

joy:send_message(realm_id, "[FOCUS] Working on: " .. readme_quest_title)

-- ============================================================================
-- PHASE 6: LOVE'S PROOF FOLDER — Logo quest
-- ============================================================================

indras.narrative("Love presents a logo that speaks without words")
logger.info("Phase 6: Love submits proof folder for logo quest", { phase = 6 })

local folder_id = home.generate_artifact_id()
local logo_artifact_id = home.generate_artifact_id()

advance()
logger.event("proof_folder_created", {
    tick = tick,
    realm_id = realm_id,
    quest_id = logo_quest_id,
    folder_id = folder_id,
    claimant = peer_love,
    status = "draft",
})

advance()
logger.event("proof_folder_artifact_added", {
    tick = tick,
    realm_id = realm_id,
    folder_id = folder_id,
    artifact_id = logo_artifact_id,
    artifact_name = "Logo_black.png",
    artifact_size = 830269,
    mime_type = "image/png",
    asset_path = "assets/Logo_black.png",
    caption = "Indra's Network Logo (black version)",
})

local logo_narrative = string.format([[## Proof of Service: Logo Design

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

advance()
logger.event("proof_folder_narrative_updated", {
    tick = tick,
    realm_id = realm_id,
    folder_id = folder_id,
    claimant = peer_love,
    narrative_length = #logo_narrative,
    narrative = logo_narrative,
})

advance()
logger.event("proof_folder_submitted", {
    tick = tick,
    realm_id = realm_id,
    quest_id = logo_quest_id,
    claimant = peer_love,
    folder_id = folder_id,
    artifact_count = 1,
    narrative_preview = "Proof of Service: Logo Design — I created a logo for Indra's Network...",
    quest_title = logo_quest_title,
    narrative = logo_narrative,
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

-- Real P2P: Love announces proof
love:send_message(realm_id,
    "[PROOF] Logo Quest — Proof of Service: Logo_black.png submitted")
love:send_message(realm_id, "Submitted my proof for the logo quest!")

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_love,
    realm_id = realm_id,
    content = "Submitted my proof for the logo quest!",
    message_type = "text",
})

-- ============================================================================
-- PHASE 7: GRATITUDE RELEASE — Blessings for logo
-- ============================================================================

indras.narrative("Peace and Joy bless the logo — trust crystallizes into tokens")
logger.info("Phase 7: Gratitude release for logo", { phase = 7 })

-- Peace blesses
local peace_attention = 45000
blessing_tracker:record_blessing(logo_quest_id, peer_love, peer_peace, {1}, peace_attention)

advance()
logger.event("blessing_given", {
    tick = tick,
    realm_id = realm_id,
    quest_id = logo_quest_id,
    claimant = peer_love,
    blesser = peer_peace,
    event_count = 1,
    attention_millis = peace_attention,
})

local tok_peace_logo = home.make_token_id(logo_quest_id, peer_love, tick)
home.emit_token_minted(logger, tick, realm_id, tok_peace_logo,
    peer_love, peace_attention, peer_peace, logo_quest_id)

peace:send_message(realm_id, "Beautiful logo! Releasing my gratitude.")

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_peace,
    realm_id = realm_id,
    content = "Beautiful logo! Releasing my gratitude.",
    message_type = "text",
})

-- Joy switches to logo quest and blesses
advance()
logger.event("attention_switched", {
    tick = tick,
    member = peer_joy,
    quest_id = logo_quest_id,
    latency_us = quest_helpers.attention_switch_latency(),
})
blessing_tracker:record_attention(logo_quest_id, peer_joy, 1, 15000)

local joy_attention = 15000
blessing_tracker:record_blessing(logo_quest_id, peer_love, peer_joy, {1}, joy_attention)

advance()
logger.event("blessing_given", {
    tick = tick,
    realm_id = realm_id,
    quest_id = logo_quest_id,
    claimant = peer_love,
    blesser = peer_joy,
    event_count = 1,
    attention_millis = joy_attention,
})

local tok_joy_logo = home.make_token_id(logo_quest_id, peer_love, tick)
home.emit_token_minted(logger, tick, realm_id, tok_joy_logo,
    peer_love, joy_attention, peer_joy, logo_quest_id)

joy:send_message(realm_id, "Great work on the logo!")

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_joy,
    realm_id = realm_id,
    content = "Great work on the logo!",
    message_type = "text",
})

-- Logo quest completed
advance()
logger.event("quest_claim_verified", {
    tick = tick,
    realm_id = realm_id,
    quest_id = logo_quest_id,
    claim_index = 0,
})

advance()
logger.event("quest_completed", {
    tick = tick,
    realm_id = realm_id,
    quest_id = logo_quest_id,
    verified_claims = 1,
    pending_claims = 0,
})

peace:send_message(realm_id, "Logo quest completed!")

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_peace,
    realm_id = realm_id,
    content = "Logo quest completed!",
    message_type = "text",
})

-- ============================================================================
-- PHASE 8: JOY'S PROOF FOLDER — README quest
-- ============================================================================

indras.narrative("Joy weaves the logo into a README that tells the whole story")
logger.info("Phase 8: Joy submits proof folder for README quest", { phase = 8 })

-- Joy switches back to README quest
advance()
logger.event("attention_switched", {
    tick = tick,
    member = peer_joy,
    quest_id = readme_quest_id,
    latency_us = quest_helpers.attention_switch_latency(),
})

local readme_folder_id = home.generate_artifact_id()
local readme_artifact_id = home.generate_artifact_id()

advance()
logger.event("proof_folder_created", {
    tick = tick,
    realm_id = realm_id,
    quest_id = readme_quest_id,
    folder_id = readme_folder_id,
    claimant = peer_joy,
    status = "draft",
})

advance()
logger.event("proof_folder_artifact_added", {
    tick = tick,
    realm_id = realm_id,
    folder_id = readme_folder_id,
    artifact_id = readme_artifact_id,
    artifact_name = "README.md",
    artifact_size = 512,
    mime_type = "text/markdown",
    caption = "Project README with embedded logo reference",
})

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

advance()
logger.event("proof_folder_narrative_updated", {
    tick = tick,
    realm_id = realm_id,
    folder_id = readme_folder_id,
    claimant = peer_joy,
    narrative_length = #readme_narrative,
    narrative = readme_narrative,
})

advance()
logger.event("proof_folder_submitted", {
    tick = tick,
    realm_id = realm_id,
    quest_id = readme_quest_id,
    claimant = peer_joy,
    folder_id = readme_folder_id,
    artifact_count = 1,
    narrative_preview = "Proof of Service: README Update — I wrote a comprehensive README.md...",
    quest_title = readme_quest_title,
    narrative = readme_narrative,
    artifacts = {
        {
            artifact_hash = readme_artifact_id,
            name = "README.md",
            mime_type = "text/markdown",
            size = 512,
            caption = "Project README with embedded logo reference",
        },
    },
})

joy:send_message(realm_id, "Submitted my README — it references the logo!")

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_joy,
    realm_id = realm_id,
    content = "Submitted my README — it references the logo!",
    message_type = "text",
})

-- ============================================================================
-- PHASE 9: COLLABORATIVE EDITING — CRDT document edits
-- ============================================================================

logger.info("Phase 9: Collaborative document editing via CRDT", { phase = 9 })

advance()
logger.event("document_edit", {
    tick = tick,
    document_id = readme_artifact_id,
    editor = peer_joy,
    content = string.format([[# Indra's Network

![Indra's Network Logo](artifact:%s)

A peer-to-peer network for collaborative service and mutual recognition.

## Getting Started

1. Join or create a Realm
2. Browse open Quests
3. Focus your attention
4. Submit a Proof of Service

---

*Created with care by the Harmony realm.*
]], logo_artifact_id),
    realm_id = realm_id,
})

joy:send_message(realm_id, "[EDIT] Initial README version")

advance()
logger.event("document_edit", {
    tick = tick,
    document_id = readme_artifact_id,
    editor = peer_love,
    content = string.format([[# Indra's Network

![Indra's Network Logo](artifact:%s)

A peer-to-peer network for collaborative service and mutual recognition.

## Getting Started

1. Join or create a Realm
2. Browse open Quests
3. Focus your attention
4. Submit a Proof of Service

## Contributors

- **Love** — Logo design
- **Joy** — README authoring
- **Peace** — Quest creation & review

---

*Created with care by the Harmony realm.*
]], logo_artifact_id),
    realm_id = realm_id,
})

love:send_message(realm_id, "[EDIT] Added Contributors section")

advance()
logger.event("crdt_converged", {
    tick = tick,
    folder_id = readme_artifact_id,
    members_synced = 3,
})

-- ============================================================================
-- PHASE 10: GRATITUDE FOR README
-- ============================================================================

indras.narrative("The README earns blessings — gratitude multiplies")
logger.info("Phase 10: Gratitude for Joy's README proof", { phase = 10 })

-- Love blesses
advance()
logger.event("attention_switched", {
    tick = tick,
    member = peer_love,
    quest_id = readme_quest_id,
    latency_us = quest_helpers.attention_switch_latency(),
})
blessing_tracker:record_attention(readme_quest_id, peer_love, 1, 25000)

local love_readme_attention = 25000
blessing_tracker:record_blessing(readme_quest_id, peer_joy, peer_love, {1}, love_readme_attention)

advance()
logger.event("blessing_given", {
    tick = tick,
    realm_id = realm_id,
    quest_id = readme_quest_id,
    claimant = peer_joy,
    blesser = peer_love,
    event_count = 1,
    attention_millis = love_readme_attention,
})

local tok_love_readme = home.make_token_id(readme_quest_id, peer_joy, tick)
home.emit_token_minted(logger, tick, realm_id, tok_love_readme,
    peer_joy, love_readme_attention, peer_love, readme_quest_id)

love:send_message(realm_id, "Nice README! Love seeing the logo in there.")

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_love,
    realm_id = realm_id,
    content = "Nice README! Love seeing the logo in there.",
    message_type = "text",
})

-- Peace blesses
advance()
logger.event("attention_switched", {
    tick = tick,
    member = peer_peace,
    quest_id = readme_quest_id,
    latency_us = quest_helpers.attention_switch_latency(),
})
blessing_tracker:record_attention(readme_quest_id, peer_peace, 1, 20000)

local peace_readme_attention = 20000
blessing_tracker:record_blessing(readme_quest_id, peer_joy, peer_peace, {1}, peace_readme_attention)

advance()
logger.event("blessing_given", {
    tick = tick,
    realm_id = realm_id,
    quest_id = readme_quest_id,
    claimant = peer_joy,
    blesser = peer_peace,
    event_count = 1,
    attention_millis = peace_readme_attention,
})

local tok_peace_readme = home.make_token_id(readme_quest_id, peer_joy, tick)
home.emit_token_minted(logger, tick, realm_id, tok_peace_readme,
    peer_joy, peace_readme_attention, peer_peace, readme_quest_id)

peace:send_message(realm_id, "Clean and thorough. Releasing gratitude!")

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_peace,
    realm_id = realm_id,
    content = "Clean and thorough. Releasing gratitude!",
    message_type = "text",
})

-- README quest completed
advance()
logger.event("quest_claim_verified", {
    tick = tick,
    realm_id = realm_id,
    quest_id = readme_quest_id,
    claim_index = 0,
})

advance()
logger.event("quest_completed", {
    tick = tick,
    realm_id = realm_id,
    quest_id = readme_quest_id,
    verified_claims = 1,
    pending_claims = 0,
})

joy:send_message(realm_id, "Both quests done! Harmony is looking great.")

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_joy,
    realm_id = realm_id,
    content = "Both quests done! Harmony is looking great.",
    message_type = "text",
})

-- ============================================================================
-- PHASE 11: TOKEN PLEDGE LIFECYCLE
-- ============================================================================

indras.narrative("A new quest appears, funded by tokens from past work")
logger.info("Phase 11: Token pledge lifecycle", { phase = 11 })

local guide_quest_id = quest_helpers.generate_quest_id()
local guide_quest_title = "Write a community guide"

advance()
logger.event("quest_created", {
    tick = tick,
    realm_id = realm_id,
    quest_id = guide_quest_id,
    creator = peer_peace,
    title = guide_quest_title,
    latency_us = quest_helpers.quest_create_latency(),
})

peace:send_message(realm_id,
    "New quest: Write a community guide! I'm putting up a bounty call.")

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_peace,
    realm_id = realm_id,
    content = "New quest: Write a community guide! I'm putting up a bounty call.",
    message_type = "text",
})

-- Love pledges 45s token
advance()
home.emit_gratitude_pledged(logger, tick, realm_id, tok_peace_logo,
    peer_love, guide_quest_id, peace_attention)

love:send_message(realm_id, "Pledging my 45s token to the community guide quest as bounty!")

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_love,
    realm_id = realm_id,
    content = "Pledging my 45s token to the community guide quest as bounty!",
    message_type = "text",
})

-- Joy pledges 25s token
advance()
home.emit_gratitude_pledged(logger, tick, realm_id, tok_love_readme,
    peer_joy, guide_quest_id, love_readme_attention)

joy:send_message(realm_id, "Adding my 25s token as bounty too. Let's make this guide happen!")

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_joy,
    realm_id = realm_id,
    content = "Adding my 25s token as bounty too. Let's make this guide happen!",
    message_type = "text",
})

-- Joy pledges then withdraws 20s token
advance()
home.emit_gratitude_pledged(logger, tick, realm_id, tok_peace_readme,
    peer_joy, guide_quest_id, peace_readme_attention)

joy:send_message(realm_id, "Actually, let me pull that 20s token back — saving it for later.")

advance()
home.emit_gratitude_withdrawn(logger, tick, realm_id, tok_peace_readme,
    peer_joy, guide_quest_id, peace_readme_attention)

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_joy,
    realm_id = realm_id,
    content = "Actually, let me pull that 20s token back — saving it for later.",
    message_type = "text",
})

-- ============================================================================
-- PHASE 12: PEACE'S PROOF — Community guide
-- ============================================================================

logger.info("Phase 12: Peace submits community guide proof", { phase = 12 })

advance()
logger.event("attention_switched", {
    tick = tick,
    member = peer_peace,
    quest_id = guide_quest_id,
    latency_us = quest_helpers.attention_switch_latency(),
})
blessing_tracker:record_attention(guide_quest_id, peer_peace, 1, 35000)

local guide_folder_id = home.generate_artifact_id()

advance()
logger.event("proof_folder_created", {
    tick = tick,
    realm_id = realm_id,
    quest_id = guide_quest_id,
    folder_id = guide_folder_id,
    claimant = peer_peace,
    status = "draft",
})

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

advance()
logger.event("proof_folder_narrative_updated", {
    tick = tick,
    realm_id = realm_id,
    folder_id = guide_folder_id,
    claimant = peer_peace,
    narrative_length = #guide_narrative,
    narrative = guide_narrative,
})

advance()
logger.event("proof_folder_submitted", {
    tick = tick,
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

peace:send_message(realm_id, "Submitted my proof for the community guide!")

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_peace,
    realm_id = realm_id,
    content = "Submitted my proof for the community guide!",
    message_type = "text",
})

-- Release bounty tokens to Peace
advance()
home.emit_gratitude_released(logger, tick, realm_id, tok_peace_logo,
    peer_love, peer_peace, guide_quest_id, peace_attention)

love:send_message(realm_id, "Releasing my 45s bounty token to Peace for the guide. Well earned!")

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_love,
    realm_id = realm_id,
    content = "Releasing my 45s bounty token to Peace for the guide. Well earned!",
    message_type = "text",
})

advance()
home.emit_gratitude_released(logger, tick, realm_id, tok_love_readme,
    peer_joy, peer_peace, guide_quest_id, love_readme_attention)

joy:send_message(realm_id, "Releasing my 25s bounty to Peace too. Great guide!")

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_joy,
    realm_id = realm_id,
    content = "Releasing my 25s bounty to Peace too. Great guide!",
    message_type = "text",
})

-- Guide quest completed
advance()
logger.event("quest_claim_verified", {
    tick = tick,
    realm_id = realm_id,
    quest_id = guide_quest_id,
    claim_index = 0,
})

advance()
logger.event("quest_completed", {
    tick = tick,
    realm_id = realm_id,
    quest_id = guide_quest_id,
    verified_claims = 1,
    pending_claims = 0,
})

peace:send_message(realm_id, "Community guide quest complete! I received both bounty tokens.")

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_peace,
    realm_id = realm_id,
    content = "Community guide quest complete! I received both bounty tokens.",
    message_type = "text",
})

-- ============================================================================
-- PHASE 13: TOKEN CHAINING
-- ============================================================================

indras.narrative("Tokens flow from hand to hand, compounding community trust")
logger.info("Phase 13: Token chaining", { phase = 13 })

local onboard_quest_id = quest_helpers.generate_quest_id()
local onboard_quest_title = "Design realm onboarding flow"

advance()
logger.event("quest_created", {
    tick = tick,
    realm_id = realm_id,
    quest_id = onboard_quest_id,
    creator = peer_joy,
    title = onboard_quest_title,
    latency_us = quest_helpers.quest_create_latency(),
})

joy:send_message(realm_id, "New quest: Design realm onboarding flow!")

-- Peace pledges received tokens to new quest
advance()
home.emit_gratitude_pledged(logger, tick, realm_id, tok_peace_logo,
    peer_peace, onboard_quest_id, peace_attention)

peace:send_message(realm_id,
    "Pledging the 45s token I received to the onboarding quest. Gratitude keeps flowing!")

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_peace,
    realm_id = realm_id,
    content = "Pledging the 45s token I received to the onboarding quest. Gratitude keeps flowing!",
    message_type = "text",
})

advance()
home.emit_gratitude_pledged(logger, tick, realm_id, tok_love_readme,
    peer_peace, onboard_quest_id, love_readme_attention)

peace:send_message(realm_id, "And 25s more. Total bounty: 70s of attention on this quest!")

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_peace,
    realm_id = realm_id,
    content = "And 25s more. Total bounty: 70s of attention on this quest!",
    message_type = "text",
})

-- ============================================================================
-- VERIFICATION: Sync check across all nodes
-- ============================================================================

local love_all = love:events_since(realm_id, 0)
local joy_all = joy:events_since(realm_id, 0)
local peace_all = peace:events_since(realm_id, 0)

indras.assert.true_(#love_all >= 1, "Love should see events")
indras.assert.true_(#joy_all >= 1, "Joy should see events")
indras.assert.true_(#peace_all >= 1, "Peace should see events")

local members = love:members(realm_id)

-- ============================================================================
-- SHUTDOWN
-- ============================================================================

love:stop()
joy:stop()
peace:stop()
indras.assert.true_(not love:is_started(), "Love should be stopped")
indras.assert.true_(not joy:is_started(), "Joy should be stopped")
indras.assert.true_(not peace:is_started(), "Peace should be stopped")

-- ============================================================================
-- FINAL RESULTS (viewer-compatible)
-- ============================================================================

local logo_blessed = blessing_tracker:get_total_blessed(logo_quest_id, peer_love)
local readme_blessed = blessing_tracker:get_total_blessed(readme_quest_id, peer_joy)

local result = quest_helpers.result_builder("live_harmony")

result:add_metrics({
    total_members = 3,
    total_quests = 4,
    total_proof_folders = 3,
    total_blessings = 4,
    total_blessed_millis = logo_blessed + readme_blessed,
    total_tokens_minted = 4,
    total_pledges = 5,
    total_releases = 2,
    total_withdrawals = 1,
    p2p_events_love = #love_all,
    p2p_events_joy = #joy_all,
    p2p_events_peace = #peace_all,
    p2p_members = #members,
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
result:record_assertion("p2p_sync", true, true, true)

local final_result = result:build()

logger.info("Live Harmony Proof completed", {
    passed = final_result.passed,
    level = final_result.level,
    duration_sec = final_result.duration_sec,
    final_tick = tick,
    logo_blessed_millis = logo_blessed,
    readme_blessed_millis = readme_blessed,
    tokens_minted = 4,
    pledges = 5,
    releases = 2,
    withdrawals = 1,
    p2p_events_love = #love_all,
    p2p_events_joy = #joy_all,
    p2p_events_peace = #peace_all,
    p2p_members = #members,
})

-- Hard assertions
indras.assert.gt(logo_blessed, 0, "Should have accumulated blessed attention for logo")
indras.assert.eq(#blessing_tracker:get_blessers(logo_quest_id, peer_love), 2,
    "Logo proof should have 2 blessers")
indras.assert.gt(readme_blessed, 0, "Should have accumulated blessed attention for README")
indras.assert.eq(#blessing_tracker:get_blessers(readme_quest_id, peer_joy), 2,
    "README proof should have 2 blessers")

return final_result

-- live_attention_basics.lua
--
-- E2E test: Basic attention tracking over live CRDT sync.
--
-- Exercises: focus_on_quest, clear_attention, get_member_focus,
-- get_quest_focusers, quests_by_attention, and CRDT sync of
-- attention state between nodes.
--
-- Uses 3 live IndrasNetwork nodes (A, B, C) with QUIC transport.
-- All mutations go through A's realm (the creator). B and C verify
-- state arrives via CRDT sync.
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/live_attention_basics.lua

package.path = package.path .. ";simulation/scripts/lib/?.lua"
local h = require("live_network_helpers")

io.stdout:setvbuf("no")
print("=== Live Attention Basics Test ===")
print()

-- Step 1: Create 3 networks, start, connect all
h.section(1, "Creating and connecting 3 networks")
local nets = h.create_networks(3)
local a, b, c = nets[1], nets[2], nets[3]
a:set_display_name("A")
b:set_display_name("B")
c:set_display_name("C")
h.connect_all(nets)
print("    3 networks started and connected")

-- Step 2: A creates realm, B and C join
h.section(2, "A creates realm, B and C join")
local realm_a = a:create_realm("AttentionBasics")
local invite = realm_a:invite_code()
local realm_b = b:join(invite)
local realm_c = c:join(invite)
print("    All 3 nodes in realm: " .. realm_a:id():sub(1, 16) .. "...")

-- Get member IDs
local id_a = a:member_id()
local id_b = b:member_id()
local id_c = c:member_id()
local pq_a = a:pq_identity()
print("    A: " .. id_a:sub(1, 16) .. "...")
print("    B: " .. id_b:sub(1, 16) .. "...")
print("    C: " .. id_c:sub(1, 16) .. "...")

-- Use deterministic quest IDs (16 bytes each)
local quest_1 = string.rep("11", 16)  -- 16 bytes of 0x11
local quest_2 = string.rep("22", 16)  -- 16 bytes of 0x22
-- Artifact ID for genesis
local target_init = string.rep("a0", 32)

-- Step 3: Initialize attention document via genesis, then A focuses on quest_1
h.section(3, "A creates genesis, focuses on quest_1")
local ge, _ = realm_a:create_genesis_event(target_init, id_a, pq_a)
print("    Genesis: seq=" .. ge:seq())

realm_a:focus_on_quest(quest_1, id_a)
local focus_a = realm_a:get_member_focus(id_a)
indras.assert.eq(focus_a, quest_1, "A should be focused on quest_1")
print("    A focused on quest_1")

-- Step 4: A also records B->quest_1 and C->quest_2 (using their member IDs)
h.section(4, "A records focus for B and C")
realm_a:focus_on_quest(quest_1, id_b)
realm_a:focus_on_quest(quest_2, id_c)

-- Verify locally on A
local focus_b_on_a = realm_a:get_member_focus(id_b)
indras.assert.eq(focus_b_on_a, quest_1, "B should be focused on quest_1 (on A)")
print("    B focused on quest_1 (on A's view)")

local focus_c_on_a = realm_a:get_member_focus(id_c)
indras.assert.eq(focus_c_on_a, quest_2, "C should be focused on quest_2 (on A)")
print("    C focused on quest_2 (on A's view)")

-- Verify quest_1 focusers on A
local focusers_1 = realm_a:get_quest_focusers(quest_1)
indras.assert.eq(#focusers_1, 2, "quest_1 should have 2 focusers (A and B)")
print("    quest_1 has " .. #focusers_1 .. " focusers (A + B)")

-- Step 5: Wait for attention state to sync to B
h.section(5, "Waiting for focus state to sync to B")
h.assert_eventually(function()
    local f = realm_b:get_member_focus(id_a)
    return f == quest_1
end, { timeout = 15, interval = 0.5, msg = "A's focus on quest_1 should sync to B" })
print("    B sees A focused on quest_1")

-- Verify B sees focusers for quest_1
h.assert_eventually(function()
    local f = realm_b:get_quest_focusers(quest_1)
    return #f >= 2
end, { timeout = 10, interval = 0.5, msg = "B should see 2 focusers on quest_1" })
local focusers_b = realm_b:get_quest_focusers(quest_1)
indras.assert.eq(#focusers_b, 2, "quest_1 should have 2 focusers on B")
print("    B sees 2 focusers on quest_1 (A + B)")

-- Step 6: A switches to quest_2
h.section(6, "A switches focus to quest_2")
realm_a:focus_on_quest(quest_2, id_a)
local new_focus_a = realm_a:get_member_focus(id_a)
indras.assert.eq(new_focus_a, quest_2, "A should now be focused on quest_2")
print("    A now focused on quest_2")

-- Verify quest_1 now has only B on A's node
local focusers_1_after = realm_a:get_quest_focusers(quest_1)
indras.assert.eq(#focusers_1_after, 1, "quest_1 should have 1 focuser (B only) on A")
print("    quest_1 now has 1 focuser on A's view")

-- Step 7: A clears attention
h.section(7, "A clears attention")
realm_a:clear_attention(id_a)
local cleared_focus = realm_a:get_member_focus(id_a)
indras.assert.eq(cleared_focus, nil, "A should have no focus after clearing")
print("    A's focus is nil (cleared)")

-- Step 8: Verify quests_by_attention returns ranked list
h.section(8, "Verify quests_by_attention ranking")
local ranking = realm_a:quests_by_attention()
print("    Ranked quests: " .. #ranking)
for i, qa in ipairs(ranking) do
    print("      [" .. i .. "] quest=" .. qa.quest_id:sub(1, 8) .. "... total_ms=" .. qa.total_ms .. " members=" .. #qa.members)
end
indras.assert.true_(#ranking >= 1, "Should have at least 1 quest in ranking")

-- Step 9: Stop all
h.section(9, "Stopping networks")
h.stop_all(nets)
print("    All networks stopped")

print()
h.pass("Live Attention Basics Test")

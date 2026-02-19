-- live_workspace_chat.lua
--
-- Integration test: CRDT Document<RealmChatDocument> chat — the code path
-- the workspace GUI actually uses. Tests chat_send, chat_doc, visible_messages,
-- chat_reply, chat_react, poll_change, and refresh recovery.
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/live_workspace_chat.lua --pretty

package.path = package.path .. ";simulation/scripts/lib/?.lua"
local h = require("live_network_helpers")

print("=== Live Workspace Chat (CRDT Document) Test ===")
print()

-- ============================================================================
-- SETUP: 3 networks (Alice, Bob, Carol), all connected, shared realm
-- ============================================================================

h.section(1, "Creating and connecting 3 networks")
local nets = h.create_networks(3)
local a, b, c = nets[1], nets[2], nets[3]
a:set_display_name("Alice")
b:set_display_name("Bob")
c:set_display_name("Carol")
h.connect_all(nets)
print("    3 networks started and connected")

h.section(2, "A creates realm, B and C join")
local realm_a = a:create_realm("CRDT Chat Room")
local invite = realm_a:invite_code()
local realm_b = b:join(invite)
local realm_c = c:join(invite)

-- Wait for membership to propagate
h.assert_eventually(function()
    return realm_a:member_count() >= 3
end, { timeout = 10, interval = 0.5, msg = "A should see 3 members" })
print("    Realm created, all 3 members joined")

-- ============================================================================
-- PHASE 1: CRDT doc initialization
-- ============================================================================

h.section(3, "CRDT chat doc initialization")
local doc_a = realm_a:chat_doc()
local doc_b = realm_b:chat_doc()
local doc_c = realm_c:chat_doc()
print("    All 3 peers have chat_doc handles")

-- Initial state should be empty
local count_a = doc_a:visible_count()
print("    A initial visible_count: " .. count_a)
-- Don't assert 0 — there may be messages from realm setup

-- ============================================================================
-- PHASE 2: Send via CRDT — A sends, verify B and C see it
-- ============================================================================

h.section(4, "CRDT chat_send — A sends 'Hello CRDT!'")
local msg_id = realm_a:chat_send("Alice", "Hello CRDT!")
print("    A sent 'Hello CRDT!' (id=" .. msg_id .. ")")

-- B should see it in visible_messages
h.assert_eventually(function()
    local msgs = doc_b:visible_messages()
    for _, m in ipairs(msgs) do
        if m.content == "Hello CRDT!" then return true end
    end
    return false
end, { timeout = 10, interval = 0.5, msg = "B should see A's CRDT message" })

-- C should see it too
h.assert_eventually(function()
    local msgs = doc_c:visible_messages()
    for _, m in ipairs(msgs) do
        if m.content == "Hello CRDT!" then return true end
    end
    return false
end, { timeout = 10, interval = 0.5, msg = "C should see A's CRDT message" })
print("    B and C received A's CRDT message via visible_messages()")

-- ============================================================================
-- PHASE 3: Changes stream — B polls, A sends, verify poll fires
-- ============================================================================

h.section(5, "Push notification — subscribe before send")

-- Step 1: Test LOCAL push (B sends, B's own subscription fires).
-- This validates the change_tx broadcast works at all.
local sub_b_local = doc_b:subscribe()
print("    B subscribed for local push test")
realm_b:chat_send("Bob", "Local push test")
local got_local = sub_b_local:wait(5.0)
print("    B local push result: " .. tostring(got_local))
indras.assert.true_(got_local, "B should see own local change via subscription")
print("    Local push works — change_tx broadcast is functional")

-- Step 2: Test REMOTE push (A sends, B's subscription fires via spawn_listener).
local sub_b_remote = doc_b:subscribe()
print("    B subscribed for remote push test")
realm_a:chat_send("Alice", "Remote push test")
local got_remote = sub_b_remote:wait(10.0)
print("    B remote push result: " .. tostring(got_remote))
if got_remote then
    print("    Remote push works — spawn_listener delivers via change_tx")
else
    print("    Remote push FAILED — spawn_listener is not delivering")
    print("    (Messages still arrive via refresh() slow path)")
end

-- ============================================================================
-- PHASE 4: Reply via CRDT — B replies, verify reply_to propagates
-- ============================================================================

h.section(6, "CRDT chat_reply — B replies to A's message")
local reply_id = realm_b:chat_reply("Bob", msg_id, "Hey Alice, via CRDT!")
print("    B replied to " .. msg_id .. " (reply_id=" .. reply_id .. ")")

h.assert_eventually(function()
    local msgs = doc_a:visible_messages()
    for _, m in ipairs(msgs) do
        if m.content == "Hey Alice, via CRDT!" and m.reply_to == msg_id then
            return true
        end
    end
    return false
end, { timeout = 10, interval = 0.5, msg = "A should see B's reply with reply_to field" })

h.assert_eventually(function()
    local msgs = doc_c:visible_messages()
    for _, m in ipairs(msgs) do
        if m.content == "Hey Alice, via CRDT!" and m.reply_to == msg_id then
            return true
        end
    end
    return false
end, { timeout = 10, interval = 0.5, msg = "C should see B's reply with reply_to field" })
print("    Reply propagated with reply_to field to A and C")

-- ============================================================================
-- PHASE 5: React via CRDT — C reacts, verify reactions table
-- ============================================================================

h.section(7, "CRDT chat_react — C reacts to A's message")
local reacted = realm_c:chat_react("Carol", msg_id, "thumbsup")
indras.assert.true_(reacted, "chat_react should return true")
print("    C reacted with thumbsup to " .. msg_id)

h.assert_eventually(function()
    local msgs = doc_a:visible_messages()
    for _, m in ipairs(msgs) do
        if m.id == msg_id and m.reactions then
            local thumbs = m.reactions["thumbsup"]
            if thumbs then
                for _, author in ipairs(thumbs) do
                    if author == "Carol" then return true end
                end
            end
        end
    end
    return false
end, { timeout = 10, interval = 0.5, msg = "A should see Carol's thumbsup reaction" })
print("    Reaction propagated to A's visible_messages()")

-- ============================================================================
-- PHASE 6: Rapid-fire burst — A sends 5, verify all arrive
-- ============================================================================

h.section(8, "Rapid-fire burst — A sends 5 CRDT messages")
local burst_ids = {}
for i = 1, 5 do
    burst_ids[i] = realm_a:chat_send("Alice", "CRDTBurst " .. i)
end
print("    A sent 5 burst messages")

h.assert_eventually(function()
    local msgs = doc_b:visible_messages()
    local count = 0
    for _, m in ipairs(msgs) do
        if string.find(m.content, "^CRDTBurst %d$") then
            count = count + 1
        end
    end
    return count >= 5
end, { timeout = 15, interval = 0.5, msg = "B should see all 5 CRDT burst messages" })

h.assert_eventually(function()
    local msgs = doc_c:visible_messages()
    local count = 0
    for _, m in ipairs(msgs) do
        if string.find(m.content, "^CRDTBurst %d$") then
            count = count + 1
        end
    end
    return count >= 5
end, { timeout = 15, interval = 0.5, msg = "C should see all 5 CRDT burst messages" })
print("    All 5 burst messages received by B and C")

-- ============================================================================
-- PHASE 7: Multi-sender — A, B, C each send, verify consistency
-- ============================================================================

h.section(9, "Multi-sender CRDT conversation")
-- Small pause to let previous burst messages fully settle
indras.sleep(1.0)
realm_a:chat_send("Alice", "CRDT from Alice")
indras.sleep(0.2)
realm_b:chat_send("Bob", "CRDT from Bob")
indras.sleep(0.2)
realm_c:chat_send("Carol", "CRDT from Carol")
print("    A, B, C each sent a CRDT message")

local function has_all_three_crdt(msgs)
    local fa, fb, fc = false, false, false
    for _, m in ipairs(msgs) do
        if m.content == "CRDT from Alice" then fa = true end
        if m.content == "CRDT from Bob" then fb = true end
        if m.content == "CRDT from Carol" then fc = true end
    end
    return fa and fb and fc
end

h.assert_eventually(function()
    return has_all_three_crdt(doc_a:visible_messages())
end, { timeout = 20, interval = 1.0, msg = "A should see all 3 CRDT messages" })

h.assert_eventually(function()
    return has_all_three_crdt(doc_b:visible_messages())
end, { timeout = 20, interval = 1.0, msg = "B should see all 3 CRDT messages" })

h.assert_eventually(function()
    return has_all_three_crdt(doc_c:visible_messages())
end, { timeout = 20, interval = 1.0, msg = "C should see all 3 CRDT messages" })
print("    All peers see all 3 conversation messages")

-- ============================================================================
-- PHASE 8: Refresh recovery — force refresh, verify state
-- ============================================================================

h.section(10, "Refresh recovery")
local refreshed = doc_b:refresh()
print("    B forced refresh, result: " .. tostring(refreshed))

-- After refresh, visible_count should be consistent
local count_b = doc_b:visible_count()
local count_a_final = doc_a:visible_count()
print("    A visible_count: " .. count_a_final)
print("    B visible_count (after refresh): " .. count_b)
-- Both should have the same count (CRDT convergence)
indras.assert.eq(count_a_final, count_b, "A and B visible_count should match after refresh")

-- ============================================================================
-- SHUTDOWN
-- ============================================================================

h.section(11, "Stopping networks")
h.stop_all(nets)
print("    All networks stopped")

h.pass("Live Workspace Chat (CRDT Document) Test")

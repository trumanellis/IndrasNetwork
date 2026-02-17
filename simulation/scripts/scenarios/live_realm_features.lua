-- live_realm_features.lua
--
-- Integration test: Realm aliases, read tracking, unread counts, and documents.
-- Two real Network instances (Alice, Bob) exercise the full realm feature set
-- beyond basic messaging: aliases, mark_read, unread_count, last_read_seq,
-- and document_names.
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/live_realm_features.lua

package.path = package.path .. ";simulation/scripts/lib/?.lua"
local h = require("live_network_helpers")

print("=== Live Realm Features Test ===")
print()

-- Step 1: Create 2 networks, start, connect
h.section(1, "Creating and starting 2 networks")
local nets = h.create_networks(2)
local a = nets[1]
local b = nets[2]
a:set_display_name("Alice")
b:set_display_name("Bob")
h.connect_all(nets)
print("    Alice: " .. a:id():sub(1, 16) .. "...")
print("    Bob:   " .. b:id():sub(1, 16) .. "...")
indras.assert.true_(a:is_running(), "Alice should be running")
indras.assert.true_(b:is_running(), "Bob should be running")
print("    Both networks started and connected")

local b_id = b:id()

-- Step 2: A creates realm "General"
h.section(2, "Alice creates realm 'General'")
local realm_a = a:create_realm("General")
indras.assert.true_(realm_a ~= nil, "Realm should be created")
local realm_id = realm_a:id()
print("    Realm id: " .. realm_id:sub(1, 16) .. "...")
print("    Realm name: " .. realm_a:name())

-- Step 3: B joins via invite, sleep
h.section(3, "Bob joins via invite")
local invite = realm_a:invite_code()
indras.assert.true_(#invite > 0, "Invite code should be non-empty")
print("    Invite code length: " .. #invite .. " chars")
local realm_b = b:join(invite)
indras.assert.true_(realm_b ~= nil, "Bob should get a realm handle")
indras.sleep(0.5)
print("    Bob joined realm")

-- ============================================================================
-- SECTION: Alias tests
-- ============================================================================

-- Step 4: Test aliases
h.section(4, "Testing realm aliases")

-- 4a: A sets alias "Team Chat"
print("    [4a] Alice sets alias 'Team Chat'")
realm_a:set_alias("Team Chat")
indras.sleep(0.2)

-- 4b: A reads get_alias() — verify "Team Chat"
print("    [4b] Alice reads get_alias()")
local a_alias = realm_a:get_alias()
print("         Alice alias: " .. tostring(a_alias))
indras.assert.eq(a_alias, "Team Chat", "Alice's alias should be 'Team Chat'")
print("         Alice alias verified")

-- 4c: B reads get_alias() — should eventually see "Team Chat"
print("    [4c] Bob reads get_alias() — waiting for sync")
h.assert_eventually(function()
    local alias = realm_b:get_alias()
    return alias == "Team Chat"
end, { timeout = 10, interval = 0.5, msg = "Bob should see 'Team Chat' alias after sync" })
local b_alias = realm_b:get_alias()
print("         Bob alias: " .. tostring(b_alias))
indras.assert.eq(b_alias, "Team Chat", "Bob's alias should be 'Team Chat'")
print("         Bob alias verified")

-- 4d: A clears alias
print("    [4d] Alice clears alias")
realm_a:clear_alias()
indras.sleep(0.2)
local a_alias_cleared = realm_a:get_alias()
print("         Alice alias after clear: " .. tostring(a_alias_cleared))
indras.assert.true_(a_alias_cleared == nil, "Alice's alias should be nil after clear")
print("         Alias cleared and verified nil")

-- ============================================================================
-- SECTION: Read tracking tests
-- ============================================================================

-- Step 5: Test read tracking
h.section(5, "Testing read tracking and unread counts")

-- 5a: A sends 3 messages
print("    [5a] Alice sends 3 messages")
realm_a:send("First message")
realm_a:send("Second message")
realm_a:send("Third message")

-- 5b: Sleep for sync
indras.sleep(0.5)
print("    [5b] Sync wait complete")

-- 5c: B checks unread_count(b_id) — should be > 0
print("    [5c] Bob checks unread_count")
h.assert_eventually(function()
    return realm_b:unread_count(b_id) > 0
end, { timeout = 10, interval = 0.5, msg = "Bob should have unread messages" })
local unread_before = realm_b:unread_count(b_id)
print("         Bob unread_count: " .. unread_before)
indras.assert.true_(unread_before > 0, "Bob should have unread messages after Alice sent 3")
print("         Unread count verified > 0")

-- 5d: B marks read
print("    [5d] Bob marks read")
realm_b:mark_read(b_id)

-- 5e: B checks unread_count(b_id) — should be 0
print("    [5e] Bob checks unread_count after mark_read")
local unread_after_mark = realm_b:unread_count(b_id)
print("         Bob unread_count after mark_read: " .. unread_after_mark)
indras.assert.eq(unread_after_mark, 0, "Bob should have 0 unread after mark_read")
print("         Unread count == 0 verified")

-- 5f: A sends 1 more message
print("    [5f] Alice sends 1 more message")
realm_a:send("Fourth message")
indras.sleep(0.5)

-- 5g: B checks unread_count(b_id) — should be 1
print("    [5g] Bob checks unread_count — expecting 1")
h.assert_eventually(function()
    return realm_b:unread_count(b_id) == 1
end, { timeout = 10, interval = 0.5, msg = "Bob should have 1 unread message" })
local unread_new = realm_b:unread_count(b_id)
print("         Bob unread_count: " .. unread_new)
indras.assert.eq(unread_new, 1, "Bob should have 1 unread message")
print("         Unread count == 1 verified")

-- 5h: Check last_read_seq(b_id)
print("    [5h] Checking last_read_seq")
local last_seq = realm_b:last_read_seq(b_id)
print("         Bob last_read_seq: " .. tostring(last_seq))
indras.assert.true_(last_seq ~= nil, "last_read_seq should be non-nil after mark_read")
indras.assert.true_(last_seq > 0, "last_read_seq should be > 0")
print("         last_read_seq verified > 0")

-- ============================================================================
-- SECTION: Document names
-- ============================================================================

-- Step 6: Test document_names()
h.section(6, "Testing document_names()")

-- Write to a couple of documents to ensure they appear in the list
local doc = realm_a:document("notes")
doc:update("Meeting notes go here")
local doc2 = realm_a:document("agenda")
doc2:update("Agenda item 1")
indras.sleep(0.2)

local doc_names = realm_a:document_names()
print("    document_names count: " .. #doc_names)
for i, name in ipairs(doc_names) do
    print("      [" .. i .. "] " .. name)
end
indras.assert.true_(#doc_names >= 2, "Should have at least 2 document names")

-- Verify the expected names appear
local found_notes = false
local found_agenda = false
for _, name in ipairs(doc_names) do
    if name == "notes" then found_notes = true end
    if name == "agenda" then found_agenda = true end
end
indras.assert.true_(found_notes, "document_names should include 'notes'")
indras.assert.true_(found_agenda, "document_names should include 'agenda'")
print("    document_names verified: 'notes' and 'agenda' present")

-- Step 7: Stop all
h.section(7, "Stopping all networks")
h.stop_all(nets)
print("    All networks stopped")

h.pass("Live Realm Features Test")

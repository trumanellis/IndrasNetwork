-- live_contacts_social.lua
--
-- Integration test: Contacts realm and social features.
-- Three real Network instances (Alice, Bob, Charlie) exercise the full
-- contacts API: adding, confirming, sentiment, relayable, and removal.
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/live_contacts_social.lua

package.path = package.path .. ";simulation/scripts/lib/?.lua"
local h = require("live_network_helpers")

print("=== Live Contacts Social Test ===")
print()

-- Step 1: Create 3 networks, set names, start, connect all
h.section(1, "Creating and starting 3 networks")
local nets = h.create_networks(3)
local a = nets[1]
local b = nets[2]
local c = nets[3]
a:set_display_name("Alice")
b:set_display_name("Bob")
c:set_display_name("Charlie")
h.connect_all(nets)
print("    Alice:   " .. a:id():sub(1, 16) .. "...")
print("    Bob:     " .. b:id():sub(1, 16) .. "...")
print("    Charlie: " .. c:id():sub(1, 16) .. "...")
indras.assert.true_(a:is_running(), "Alice should be running")
indras.assert.true_(b:is_running(), "Bob should be running")
indras.assert.true_(c:is_running(), "Charlie should be running")
print("    All 3 networks started and connected")

local a_id = a:id()
local b_id = b:id()
local c_id = c:id()

-- Step 2: A gets contacts_realm
h.section(2, "Alice gets contacts_realm")
local contacts = a:contacts_realm()
indras.assert.true_(contacts ~= nil, "contacts_realm should be non-nil")
print("    contacts_realm id: " .. contacts:id():sub(1, 16) .. "...")

-- Step 3: A adds B as contact with name
h.section(3, "Alice adds Bob as contact with name")
contacts:add_contact_with_name(b_id, "Bob")
print("    Added Bob with name 'Bob'")

-- Step 4: A adds C as contact
h.section(4, "Alice adds Charlie as contact")
contacts:add_contact(c_id)
print("    Added Charlie")

-- Step 5: Verify contact_count() == 2
h.section(5, "Verifying contact count == 2")
local count = contacts:contact_count()
print("    contact_count: " .. count)
indras.assert.eq(count, 2, "Alice should have 2 contacts")
print("    contact_count verified")

-- Step 6: Verify is_contact(b_id) == true
h.section(6, "Verifying is_contact(b_id) == true")
indras.assert.true_(contacts:is_contact(b_id), "Bob should be a contact")
indras.assert.true_(contacts:is_contact(c_id), "Charlie should be a contact")
print("    is_contact verified for Bob and Charlie")

-- Step 7: Verify contacts_list() has 2 entries
h.section(7, "Verifying contacts_list has 2 entries")
local list = contacts:contacts_list()
print("    contacts_list length: " .. #list)
indras.assert.eq(#list, 2, "contacts_list should have 2 entries")
print("    contacts_list verified")

-- Step 8: A confirms B
h.section(8, "Alice confirms Bob")
contacts:confirm_contact(b_id)
print("    confirm_contact(b_id) called")

-- Step 9: Verify get_status(b_id) == "confirmed"
h.section(9, "Verifying Bob status == 'confirmed'")
local b_status = contacts:get_status(b_id)
print("    Bob status: " .. tostring(b_status))
indras.assert.eq(b_status, "confirmed", "Bob should be confirmed")
print("    Bob confirmed status verified")

-- Step 10: Verify get_status(c_id) == "pending"
h.section(10, "Verifying Charlie status == 'pending'")
local c_status = contacts:get_status(c_id)
print("    Charlie status: " .. tostring(c_status))
indras.assert.eq(c_status, "pending", "Charlie should be pending")
print("    Charlie pending status verified")

-- Step 11: A updates sentiment for B to 1 (positive)
h.section(11, "Alice sets positive sentiment for Bob")
contacts:update_sentiment(b_id, 1)
print("    update_sentiment(b_id, 1) called")

-- Step 12: A updates sentiment for C to -1 (negative)
h.section(12, "Alice sets negative sentiment for Charlie")
contacts:update_sentiment(c_id, -1)
print("    update_sentiment(c_id, -1) called")

-- Step 13: Verify get_sentiment(b_id) == 1
h.section(13, "Verifying Bob sentiment == 1")
local b_sent = contacts:get_sentiment(b_id)
print("    Bob sentiment: " .. tostring(b_sent))
indras.assert.eq(b_sent, 1, "Bob sentiment should be 1")
print("    Bob sentiment verified")

-- Step 14: Verify get_sentiment(c_id) == -1
h.section(14, "Verifying Charlie sentiment == -1")
local c_sent = contacts:get_sentiment(c_id)
print("    Charlie sentiment: " .. tostring(c_sent))
indras.assert.eq(c_sent, -1, "Charlie sentiment should be -1")
print("    Charlie sentiment verified")

-- Step 15: A sets B as relayable
h.section(15, "Alice sets Bob as relayable")
contacts:set_relayable(b_id, true)
print("    set_relayable(b_id, true) called")

-- Step 16: A removes C as contact
h.section(16, "Alice removes Charlie as contact")
contacts:remove_contact(c_id)
print("    remove_contact(c_id) called")

-- Step 17: Verify contact_count() == 1
h.section(17, "Verifying contact count == 1 after removal")
local count_after = contacts:contact_count()
print("    contact_count: " .. count_after)
indras.assert.eq(count_after, 1, "Alice should have 1 contact after removing Charlie")
indras.assert.true_(contacts:is_contact(b_id), "Bob should still be a contact")
indras.assert.true_(not contacts:is_contact(c_id), "Charlie should no longer be a contact")
print("    contact_count and membership verified")

-- Step 18: Stop all
h.section(18, "Stopping all networks")
h.stop_all(nets)
print("    All networks stopped")

h.pass("Live Contacts Social Test")

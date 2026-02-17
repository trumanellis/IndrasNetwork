-- live_documents_sync.lua
--
-- Integration test: CRDT document sync between 2 nodes via a shared realm.
-- Verifies that document writes from one node propagate to the other.
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/live_documents_sync.lua

package.path = package.path .. ";simulation/scripts/lib/?.lua"
local h = require("live_network_helpers")

print("=== Live Documents Sync Test ===")
print()

-- Step 1: Create 2 networks (A, B), start, connect
h.section(1, "Creating and connecting 2 networks")
local nets = h.create_networks(2)
local a = nets[1]
local b = nets[2]
a:set_display_name("Alice")
b:set_display_name("Bob")
h.connect_all(nets)
print("    2 networks started and connected")

-- Step 2: A creates realm "Doc Sync"
h.section(2, "A creates realm 'Doc Sync'")
local realm_a = a:create_realm("Doc Sync")
print("    Realm created: " .. realm_a:name())
indras.assert.eq(realm_a:name(), "Doc Sync", "Realm name should be 'Doc Sync'")

-- Step 3: B joins via invite
h.section(3, "B joins via invite")
local invite = realm_a:invite_code()
local realm_b = b:join(invite)
print("    B joined realm: " .. realm_b:id():sub(1, 16) .. "...")
indras.assert.eq(realm_b:id(), realm_a:id(), "B's realm ID should match A's")
print("    B joined realm")

-- Step 4: A gets document "shared_state" from realm
h.section(4, "A gets document 'shared_state'")
local doc_a = realm_a:document("shared_state")
print("    A has document: " .. doc_a:name())
indras.assert.eq(doc_a:name(), "shared_state", "Document name should be 'shared_state'")

-- Step 5: A updates document: {counter = 1, label = "hello"}
h.section(5, "A writes to document")
doc_a:update({counter = 1, label = "hello"})
print("    A updated 'shared_state': {counter=1, label='hello'}")

-- Step 6: B gets same document "shared_state" and waits for A's write to sync
h.section(6, "B gets document 'shared_state' and waits for sync")
local doc_b = realm_b:document("shared_state")
print("    B has document: " .. doc_b:name())
indras.assert.eq(doc_b:name(), "shared_state", "B document name should be 'shared_state'")

-- Step 7: (combined) B reads document - wait for A's data to arrive
h.section(7, "B reads document, verifies A's data")
h.assert_eventually(function()
    local data = doc_b:read()
    return data.counter == 1 and data.label == "hello"
end, { timeout = 10, interval = 0.5, msg = "B should see counter=1, label='hello' from A's write" })
local b_data = doc_b:read()
print("    B read document: counter=" .. tostring(b_data.counter) .. ", label=" .. tostring(b_data.label))
indras.assert.eq(b_data.counter, 1, "B should see counter=1 from A's write")
indras.assert.eq(b_data.label, "hello", "B should see label='hello' from A's write")
print("    B successfully read A's document data")

-- Step 8: (was 9) B updates: {counter = 2, label = "updated"}
h.section(8, "B updates document")
doc_b:update({counter = 2, label = "updated"})
print("    B updated 'shared_state': {counter=2, label='updated'}")

-- Step 9: (was 11) A reads document again - wait for B's update to sync
h.section(9, "A reads document, verifies B's update")
h.assert_eventually(function()
    local data = doc_a:read()
    return data.counter == 2 and data.label == "updated"
end, { timeout = 10, interval = 0.5, msg = "A should see counter=2, label='updated' from B's write" })
local a_data = doc_a:read()
print("    A read document: counter=" .. tostring(a_data.counter) .. ", label=" .. tostring(a_data.label))
indras.assert.eq(a_data.counter, 2, "A should see counter=2 from B's update")
indras.assert.eq(a_data.label, "updated", "A should see label='updated' from B's update")
print("    A successfully observed B's document update")

-- Step 10: (was 12) Verify document_names() includes "shared_state"
h.section(10, "Verifying document_names()")
local a_doc_names = realm_a:document_names()
print("    A document names: " .. #a_doc_names .. " document(s)")
for _, name in ipairs(a_doc_names) do
    print("      - " .. name)
end
local found_a = false
for _, name in ipairs(a_doc_names) do
    if name == "shared_state" then found_a = true break end
end
indras.assert.true_(found_a, "A's document_names() should include 'shared_state'")
-- B's document_names() — wait for sync
local found_b = false
h.assert_eventually(function()
    local b_doc_names = realm_b:document_names()
    for _, name in ipairs(b_doc_names) do
        if name == "shared_state" then return true end
    end
    return false
end, { timeout = 10, interval = 0.5, msg = "B's document_names() should include 'shared_state'" })
found_b = true
indras.assert.true_(found_b, "B's document_names() should include 'shared_state'")
print("    'shared_state' found in document_names() for both nodes")

-- Step 13: Stop all
h.section(13, "Stopping networks")
h.stop_all(nets)
print("    All networks stopped")

print()
h.pass("Live Documents Sync Test")

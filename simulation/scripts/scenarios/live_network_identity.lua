-- live_network_identity.lua
--
-- Integration test: Identity management for real Network instances.
-- Verifies unique IDs, display names, identity codes, URIs, and export.
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/live_network_identity.lua

package.path = package.path .. ";simulation/scripts/lib/?.lua"
local h = require("live_network_helpers")

print("=== Live Network Identity Test ===")
print()

-- Step 1: Create 2 networks (A, B), start them
h.section(1, "Creating and starting 2 networks")
local nets = h.create_networks(2)
local a = nets[1]
local b = nets[2]
print("    A: " .. tostring(a))
print("    B: " .. tostring(b))
indras.assert.true_(a:is_running(), "A should be running")
indras.assert.true_(b:is_running(), "B should be running")
print("    Both networks started successfully")

-- Step 2: Verify each has a unique id (hex string, 64 chars)
h.section(2, "Verifying unique IDs")
local a_id = a:id()
local b_id = b:id()
print("    A id: " .. a_id)
print("    B id: " .. b_id)
indras.assert.true_(#a_id == 64, "A id should be 64 hex chars, got " .. #a_id)
indras.assert.true_(#b_id == 64, "B id should be 64 hex chars, got " .. #b_id)
indras.assert.true_(a_id ~= b_id, "IDs should be unique")
print("    Both IDs are 64-char hex strings and unique")

-- Step 3: Set display names
h.section(3, "Setting display names")
a:set_display_name("Alice")
b:set_display_name("Bob")
print("    A display name set to 'Alice'")
print("    B display name set to 'Bob'")

-- Step 4: Verify display_name() returns the set names
h.section(4, "Verifying display names")
local a_name = a:display_name()
local b_name = b:display_name()
print("    A display_name(): " .. a_name)
print("    B display_name(): " .. b_name)
indras.assert.eq(a_name, "Alice", "A display name should be 'Alice'")
indras.assert.eq(b_name, "Bob", "B display name should be 'Bob'")
print("    Display names verified")

-- Step 5: Get identity_code() for each, verify non-empty strings
h.section(5, "Getting identity codes")
local a_code = a:identity_code()
local b_code = b:identity_code()
print("    A identity_code: " .. a_code)
print("    B identity_code: " .. b_code)
indras.assert.true_(#a_code > 0, "A identity code should be non-empty")
indras.assert.true_(#b_code > 0, "B identity code should be non-empty")
indras.assert.true_(a_code ~= b_code, "Identity codes should differ between nodes")
print("    Identity codes verified (non-empty, unique)")

-- Step 6: Get identity_uri() - verify it contains the identity code and name
h.section(6, "Getting identity URIs")
local a_uri = a:identity_uri()
local b_uri = b:identity_uri()
print("    A identity_uri: " .. a_uri)
print("    B identity_uri: " .. b_uri)
indras.assert.true_(#a_uri > 0, "A identity URI should be non-empty")
indras.assert.true_(#b_uri > 0, "B identity URI should be non-empty")
indras.assert.true_(
    string.find(a_uri, a_code, 1, true) ~= nil,
    "A URI should contain A's identity code"
)
indras.assert.true_(
    string.find(b_uri, b_code, 1, true) ~= nil,
    "B URI should contain B's identity code"
)
print("    Identity URIs contain expected identity codes")

-- Step 7: Export A's identity, verify non-empty bytes (base64 string)
h.section(7, "Exporting A's identity")
local a_export = a:export_identity()
print("    Exported identity length: " .. #a_export .. " chars")
indras.assert.true_(#a_export > 0, "Exported identity should be non-empty")
print("    Identity exported successfully")

-- Step 8: Stop both
h.section(8, "Stopping networks")
h.stop_all(nets)
print("    Both networks stopped")

print()
h.pass("Live Network Identity Test")

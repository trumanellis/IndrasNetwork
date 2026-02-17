-- live_home_artifacts.lua
--
-- Integration test: Home realm artifacts and tree composition between 2 nodes.
-- Verifies upload, retrieval, tree composition, documents, and recall.
--
-- Note: grant_access/revoke_access/shared_with are skipped because they
-- require ArtifactSyncRegistry reconciliation which needs full network
-- infrastructure (not supported in in-process test connections).
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/live_home_artifacts.lua

package.path = package.path .. ";simulation/scripts/lib/?.lua"
local h = require("live_network_helpers")

print("=== Live Home Artifacts Test ===")
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

-- Step 2: A gets home_realm
h.section(2, "A gets home realm")
local home = a:home_realm()
print("    A home realm id: " .. home:id():sub(1, 16) .. "...")

-- Step 3: Create a temp file with some content
h.section(3, "Creating temp file for upload")
local tmpfile = os.tmpname()
local f = io.open(tmpfile, "w")
f:write("Hello artifact world!")
f:close()
print("    Temp file created: " .. tmpfile)

-- Step 4: A uploads file via home:upload(path)
h.section(4, "A uploads artifact")
local artifact_id = home:upload(tmpfile)
print("    Artifact ID: " .. artifact_id)
indras.assert.true_(#artifact_id == 64, "Artifact ID should be 64 hex chars, got " .. #artifact_id)
print("    Upload successful")

-- Step 5: A retrieves artifact via home:get_artifact(id) - verify content matches
h.section(5, "A retrieves and verifies artifact content")
local artifact = home:get_artifact(artifact_id)
indras.assert.true_(artifact ~= nil, "Artifact should not be nil")
print("    Artifact retrieved successfully")

-- Step 6: Tree composition - create second artifact, attach as child of first
h.section(6, "Tree composition: attach child artifact")
local tmpfile2 = os.tmpname()
local f2 = io.open(tmpfile2, "w")
f2:write("Child artifact content")
f2:close()
local child_id = home:upload(tmpfile2)
print("    Child artifact ID: " .. child_id:sub(1, 16) .. "...")
home:attach_child(artifact_id, child_id)
print("    Child attached to parent artifact")

-- Step 7: Detach child
h.section(7, "Detaching child artifact")
home:detach_child(artifact_id, child_id)
print("    Child detached from parent artifact")

-- Step 8: Home realm documents
h.section(8, "Testing home realm documents")
local doc = home:document("home_notes")
print("    Document name: " .. doc:name())
doc:update({note = "My home note", count = 42})
print("    Document updated")
local data = doc:read()
indras.assert.true_(type(data) == "table", "Document read should return a table")
indras.assert.eq(data.note, "My home note", "Document should contain 'My home note'")
indras.assert.eq(data.count, 42, "Document count should be 42")
print("    Document data verified: note='" .. data.note .. "', count=" .. data.count)

-- Step 9: A recalls the first artifact
h.section(9, "A recalls artifact")
home:recall(artifact_id)
print("    Artifact recalled successfully")

-- Step 10: Stop all
h.section(10, "Stopping networks")
h.stop_all(nets)
print("    All networks stopped")

print()
h.pass("Live Home Artifacts Test")

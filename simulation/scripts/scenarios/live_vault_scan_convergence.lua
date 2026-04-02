-- live_vault_scan_convergence.lua
--
-- Integration test: vault scan of pre-populated files, hash-level convergence,
-- and binary content deduplication across nodes.
--
-- Tests:
--   1. Initial scan of pre-populated vault (files written to disk before sync starts)
--   2. Hash-level convergence (every file has identical hash_hex on all nodes)
--   3. Binary content dedup (identical content -> identical BLAKE3 hash regardless of path)
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/live_vault_scan_convergence.lua --pretty

package.path = package.path .. ";simulation/scripts/lib/?.lua"
local h = require("live_network_helpers")

print("=== Vault Scan & Hash Convergence Test ===")
print()

-- ---------------------------------------------------------------------------
-- Helpers
-- ---------------------------------------------------------------------------

-- Write a file to disk directly (bypassing vault API) to simulate pre-existing content.
local function write_raw(dir, rel_path, content)
    local full = dir .. "/" .. rel_path
    local parent = full:match("(.+)/[^/]+$")
    if parent then os.execute("mkdir -p " .. parent) end
    local f = io.open(full, "wb")
    assert(f, "Failed to open for write: " .. full)
    f:write(content)
    f:close()
end

-- Build a {path -> hash_hex} map from list_files() output.
local function hash_map(file_list)
    local m = {}
    for _, f in ipairs(file_list) do
        m[f.path] = f.hash_hex
    end
    return m
end

-- Sort a list of file entries by path for deterministic comparison.
local function sorted_by_path(file_list)
    local copy = {}
    for i, f in ipairs(file_list) do copy[i] = f end
    table.sort(copy, function(a, b) return a.path < b.path end)
    return copy
end

-- ============================================================================
-- PHASE 1: Pre-populate A's vault directory on disk before any sync
-- ============================================================================

h.section(1, "Pre-populate A's vault directory on disk (raw I/O)")

local dir_a = os.tmpname() .. "_vault_scan_a"
local dir_b = os.tmpname() .. "_vault_scan_b"

os.execute("mkdir -p " .. dir_a)
os.execute("mkdir -p " .. dir_b)
print("    A dir: " .. dir_a)
print("    B dir: " .. dir_b)

-- Fake PNG binary content: PNG magic bytes + repeated data
local png_content = string.rep("\x89PNG" .. string.char(0, 0, 0), 200)

write_raw(dir_a, "README.md",        "# Project\n\nPre-existing README.\n")
write_raw(dir_a, "notes/alpha.md",   "# Alpha\n\nFirst note.\n")
write_raw(dir_a, "notes/beta.md",    "# Beta\n\nSecond note.\n")
write_raw(dir_a, "img/photo.png",    png_content)
write_raw(dir_a, "config.toml",      "[settings]\nversion = 1\n")

print("    Wrote 5 files to disk in A's directory")

-- ============================================================================
-- PHASE 2: Create 2 networks, connect them, A creates vault at pre-populated dir
-- ============================================================================

h.section(2, "Create networks; A opens vault at pre-populated directory")

local nets = h.create_networks(2)
local net_a, net_b = nets[1], nets[2]
h.connect_all(nets)
print("    2 networks started and connected")

local vault_a, invite = indras.VaultSync.create(net_a, "ScanVault", dir_a)
print("    A created vault at pre-populated dir")

-- ============================================================================
-- PHASE 3: A calls scan() -- must return 5 (all pre-existing files indexed)
-- ============================================================================

h.section(3, "A scans pre-populated directory; expect 5 indexed files")

local scanned = vault_a:scan()
indras.assert.eq(scanned, 5, "scan() should return 5 for the 5 pre-existing files")
print("    scan() returned " .. scanned .. " (expected 5)")

local files_a_after_scan = vault_a:list_files()
indras.assert.eq(#files_a_after_scan, 5, "list_files() should list 5 files after scan")
print("    list_files() confirms 5 files in index")

-- ============================================================================
-- PHASE 4: B joins the vault; wait for CRDT sync; verify B has 5 files
-- ============================================================================

h.section(4, "B joins vault; wait for CRDT sync; verify 5 files")

local vault_b = indras.VaultSync.join(net_b, invite, dir_b)
print("    B joined vault at " .. dir_b)

h.assert_eventually(function()
    return #vault_b:list_files() >= 5
end, { timeout = 15, interval = 1, msg = "B should sync and have 5 files" })

local files_b = vault_b:list_files()
indras.assert.eq(#files_b, 5, "B should have exactly 5 files after sync")
print("    B synced: " .. #files_b .. " files")

-- ============================================================================
-- PHASE 5: Hash-level convergence -- every path has identical hash_hex on A and B
-- ============================================================================

h.section(5, "Hash-level convergence: every file hash must match on A and B")

local hm_a = hash_map(vault_a:list_files())
local hm_b = hash_map(vault_b:list_files())

local paths = { "README.md", "notes/alpha.md", "notes/beta.md", "img/photo.png", "config.toml" }
for _, path in ipairs(paths) do
    local ha = hm_a[path]
    local hb = hm_b[path]
    indras.assert.true_(ha ~= nil, "A is missing hash for: " .. path)
    indras.assert.true_(hb ~= nil, "B is missing hash for: " .. path)
    indras.assert.eq(ha, hb, "Hash mismatch for " .. path .. ": A=" .. tostring(ha) .. " B=" .. tostring(hb))
    print("    " .. path .. " -> " .. ha:sub(1, 12) .. "...  (match)")
end

-- ============================================================================
-- PHASE 6: Binary dedup -- B writes same binary content to a different path
-- ============================================================================

h.section(6, "Binary dedup: B writes same content as img/photo.png to img/photo_copy.png")

vault_b:write_file("img/photo_copy.png", png_content)
print("    B wrote img/photo_copy.png with identical binary content")

-- Wait for the new file to appear on A
h.assert_eventually(function()
    local fa = vault_a:list_files()
    for _, f in ipairs(fa) do
        if f.path == "img/photo_copy.png" then return true end
    end
    return false
end, { timeout = 15, interval = 1, msg = "A should see img/photo_copy.png after sync" })

-- Verify hash equality (content-addressed dedup)
local fa_dedup = vault_a:list_files()
local hm_a_dedup = hash_map(fa_dedup)
indras.assert.true_(hm_a_dedup["img/photo.png"] ~= nil,       "A must have img/photo.png")
indras.assert.true_(hm_a_dedup["img/photo_copy.png"] ~= nil,  "A must have img/photo_copy.png")
indras.assert.eq(
    hm_a_dedup["img/photo.png"],
    hm_a_dedup["img/photo_copy.png"],
    "Same binary content must produce same BLAKE3 hash (photo vs photo_copy on A)"
)
print("    img/photo.png and img/photo_copy.png have identical hash on A: "
      .. hm_a_dedup["img/photo.png"]:sub(1, 12) .. "...")

-- ============================================================================
-- PHASE 7: A writes a third copy; all 3 copies must share the same hash on both nodes
-- ============================================================================

h.section(7, "A writes img/another_copy.png; all 3 copies must share hash on both nodes")

vault_a:write_file("img/another_copy.png", png_content)
print("    A wrote img/another_copy.png with identical binary content")

-- Wait for B to receive the new file
h.assert_eventually(function()
    local fb = vault_b:list_files()
    for _, f in ipairs(fb) do
        if f.path == "img/another_copy.png" then return true end
    end
    return false
end, { timeout = 15, interval = 1, msg = "B should see img/another_copy.png after sync" })

local copy_paths = { "img/photo.png", "img/photo_copy.png", "img/another_copy.png" }

-- Check all 3 hashes match on A
local fa_final = vault_a:list_files()
local hm_a_final = hash_map(fa_final)
local reference_hash = hm_a_final["img/photo.png"]
indras.assert.true_(reference_hash ~= nil, "A must have img/photo.png hash")
for _, cp in ipairs(copy_paths) do
    indras.assert.true_(hm_a_final[cp] ~= nil, "A missing: " .. cp)
    indras.assert.eq(hm_a_final[cp], reference_hash,
        "A: hash mismatch for " .. cp .. " (expected same as photo.png)")
end
print("    A: all 3 copies share hash " .. reference_hash:sub(1, 12) .. "...")

-- Check all 3 hashes match on B
local fb_final = vault_b:list_files()
local hm_b_final = hash_map(fb_final)
for _, cp in ipairs(copy_paths) do
    indras.assert.true_(hm_b_final[cp] ~= nil, "B missing: " .. cp)
    indras.assert.eq(hm_b_final[cp], reference_hash,
        "B: hash mismatch for " .. cp .. " (expected same as photo.png)")
end
print("    B: all 3 copies share hash " .. reference_hash:sub(1, 12) .. "...")

-- ============================================================================
-- PHASE 8: Full convergence -- sort by path, compare every hash on A and B
-- ============================================================================

h.section(8, "Full convergence: sort by path, compare every hash on A and B")

indras.sleep(2)

local sorted_a = sorted_by_path(vault_a:list_files())
local sorted_b = sorted_by_path(vault_b:list_files())

indras.assert.eq(#sorted_a, #sorted_b,
    "A and B must have same file count for full convergence check")

for i = 1, #sorted_a do
    local fa_entry = sorted_a[i]
    local fb_entry = sorted_b[i]
    indras.assert.eq(fa_entry.path, fb_entry.path,
        "Path mismatch at index " .. i .. ": " .. fa_entry.path .. " vs " .. fb_entry.path)
    indras.assert.eq(fa_entry.hash_hex, fb_entry.hash_hex,
        "Hash mismatch at path " .. fa_entry.path)
    print("    [" .. i .. "] " .. fa_entry.path .. " -> " .. fa_entry.hash_hex:sub(1, 12) .. "...  OK")
end

print()
print("=== All phases passed! ===")

-- Cleanup
vault_a:stop()
vault_b:stop()
h.stop_all(nets)

return true

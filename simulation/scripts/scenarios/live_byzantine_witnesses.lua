-- live_byzantine_witnesses.lua
--
-- E2E test: Certificate validation rejects insufficient/invalid signatures.
--
-- Exercises: submit_certificate rejection on insufficient quorum,
-- submit_certificate rejection on wrong public key, successful
-- certificate with valid quorum, and certificate CRDT sync.
--
-- Uses 4 live IndrasNetwork nodes (A, B, C, D) with QUIC transport.
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/live_byzantine_witnesses.lua

package.path = package.path .. ";simulation/scripts/lib/?.lua"
local h = require("live_network_helpers")

print("=== Live Byzantine Witnesses Test ===")
print()

-- Step 1: Create 4 networks, start, connect all
h.section(1, "Creating and connecting 4 networks")
local nets = h.create_networks(4)
local a, b, c, d = nets[1], nets[2], nets[3], nets[4]
a:set_display_name("A")
b:set_display_name("B")
c:set_display_name("C")
d:set_display_name("D")
h.connect_all(nets)
print("    4 networks started and connected")

-- Step 2: A creates realm, all join
h.section(2, "A creates realm, B-D join")
local realm_a = a:create_realm("ByzantineWitness")
local invite = realm_a:invite_code()
local realm_b = b:join(invite)
local realm_c = c:join(invite)
local realm_d = d:join(invite)
print("    All 4 nodes in realm: " .. realm_a:id():sub(1, 16) .. "...")

-- Step 3: Get PQ identities and member IDs
h.section(3, "Getting PQ identities and member IDs")
local pq_a = a:pq_identity()
local pq_b = b:pq_identity()
local pq_c = c:pq_identity()
local pq_d = d:pq_identity()

local id_a = a:member_id()
local id_b = b:member_id()
local id_c = c:member_id()
local id_d = d:member_id()

print("    A: " .. id_a:sub(1, 16) .. "...")
print("    B: " .. id_b:sub(1, 16) .. "...")
print("    C: " .. id_c:sub(1, 16) .. "...")
print("    D: " .. id_d:sub(1, 16) .. "...")

-- Deterministic artifact IDs
local target_1 = string.rep("b1", 32)
local target_2 = string.rep("b2", 32)
local scope_hex = string.rep("bb", 32)

-- Step 4: A creates genesis + switch
h.section(4, "A creates genesis + switch (seq=0, seq=1)")
local genesis, author_state = realm_a:create_genesis_event(target_1, id_a, pq_a)
indras.assert.eq(genesis:seq(), 0, "Genesis should have seq=0")
print("    Genesis: seq=0 hash=" .. genesis:event_hash_hex():sub(1, 16) .. "...")

local switch_ev, author_state = realm_a:switch_attention_conserved(
    target_1, target_2, id_a, pq_a, author_state
)
indras.assert.eq(switch_ev:seq(), 1, "Switch should have seq=1")
local switch_hash = switch_ev:event_hash_hex()
print("    Switch: seq=1 hash=" .. switch_hash:sub(1, 16) .. "...")

-- Step 5: Set witness roster = [B, C, D] with k=2
h.section(5, "A sets witness roster: [B, C, D], k=2")
local roster_ids = { id_b, id_c, id_d }
realm_a:set_witness_roster(scope_hex, roster_ids)
local roster = realm_a:get_witness_roster(scope_hex)
indras.assert.eq(#roster, 3, "Roster should have 3 witnesses")
print("    Witness roster set: B, C, D (k=2 required)")

local k = 2
local author_pubkey = pq_a:public_key_hex()

-- Step 6: Only B signs — 1 signature (insufficient for k=2)
h.section(6, "Only B signs — attempt certificate with 1 sig (needs 2)")
local sig_b = realm_b:request_witness_signature(switch_ev, scope_hex, id_b, pq_b, author_pubkey)
print("    B signed: witness=" .. sig_b.witness:sub(1, 16) .. "...")

-- Attempt submit with only 1 signature — should fail
local pubkeys_b_only = {}
pubkeys_b_only[id_b] = pq_b:public_key_hex()

local ok, err = pcall(function()
    realm_a:submit_certificate(
        switch_hash, scope_hex,
        { sig_b },
        roster_ids, k, pubkeys_b_only
    )
end)
indras.assert.eq(ok, false, "Should fail with insufficient signatures")
print("    Certificate rejected: insufficient signatures (1 < k=2)")
print("    Error: " .. tostring(err))

-- Verify no certificate exists yet
local has_cert = realm_a:has_certificate(switch_hash)
indras.assert.eq(has_cert, false, "Should have no certificate after rejection")
print("    Confirmed: no certificate stored")

-- Step 7: C also signs — now 2 signatures (meets k=2)
h.section(7, "C also signs — submit certificate with 2 sigs (meets k=2)")
local sig_c = realm_c:request_witness_signature(switch_ev, scope_hex, id_c, pq_c, author_pubkey)
print("    C signed: witness=" .. sig_c.witness:sub(1, 16) .. "...")

local pubkeys_bc = {}
pubkeys_bc[id_b] = pq_b:public_key_hex()
pubkeys_bc[id_c] = pq_c:public_key_hex()

realm_a:submit_certificate(
    switch_hash, scope_hex,
    { sig_b, sig_c },
    roster_ids, k, pubkeys_bc
)
print("    Certificate submitted with 2 witness signatures (k=2) — SUCCESS")

-- Step 8: Verify certificate exists
h.section(8, "Verify certificate on A")
local has_cert_now = realm_a:has_certificate(switch_hash)
indras.assert.true_(has_cert_now, "A should have certificate after valid submission")
print("    A has certificate: YES")

local has_quorum = realm_a:has_quorum(switch_hash, k)
indras.assert.true_(has_quorum, "A should have quorum (k=2)")
print("    A has quorum k=2: YES")

-- Step 9: Test wrong-key rejection — D signs but submit with C's pubkey
h.section(9, "Test wrong-key rejection: D's sig with C's pubkey")
local sig_d = realm_d:request_witness_signature(switch_ev, scope_hex, id_d, pq_d, author_pubkey)
print("    D signed: witness=" .. sig_d.witness:sub(1, 16) .. "...")

-- Build a pubkeys table that maps D's member ID to C's public key (wrong!)
local pubkeys_wrong = {}
pubkeys_wrong[id_d] = pq_c:public_key_hex()  -- WRONG: D's sig verified with C's key

local ok2, err2 = pcall(function()
    realm_a:submit_certificate(
        switch_hash, scope_hex,
        { sig_d },
        roster_ids, k, pubkeys_wrong
    )
end)
indras.assert.eq(ok2, false, "Should fail with invalid signature (wrong key)")
print("    Certificate rejected: invalid signature (D's sig with C's key)")
print("    Error: " .. tostring(err2))

-- Step 10: Wait for valid certificate to sync to B
h.section(10, "Waiting for valid certificate to sync to B")
h.assert_eventually(function()
    return realm_b:has_certificate(switch_hash)
end, { timeout = 15, interval = 0.5, msg = "Certificate should sync to B via CRDT" })
print("    Certificate synced to B")

local has_quorum_b = realm_b:has_quorum(switch_hash, k)
indras.assert.true_(has_quorum_b, "B should see quorum (k=2)")
print("    B confirms quorum k=2 (event is FINAL)")

-- Step 11: Verify certificate also syncs to C
h.section(11, "Verify certificate syncs to C")
h.assert_eventually(function()
    return realm_c:has_certificate(switch_hash)
end, { timeout = 10, interval = 0.5, msg = "Certificate should sync to C" })
print("    C sees certificate")

-- Step 12: Stop all
h.section(12, "Stopping networks")
h.stop_all(nets)
print("    All networks stopped")

print()
h.pass("Live Byzantine Witnesses Test")

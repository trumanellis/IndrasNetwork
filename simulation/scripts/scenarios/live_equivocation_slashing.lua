-- live_equivocation_slashing.lua
--
-- E2E test: Equivocation detection, fraud evidence CRDT propagation,
-- and slashing (certified vs uncertified events).
--
-- Exercises: switch_attention_conserved (equivocation path), is_fraudulent,
-- fraudulent_authors, witness signing, certificate submission, has_certificate.
--
-- Uses 4 live IndrasNetwork nodes (A, B, C, D) with QUIC transport.
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/live_equivocation_slashing.lua

package.path = package.path .. ";simulation/scripts/lib/?.lua"
local h = require("live_network_helpers")

print("=== Live Equivocation & Slashing Test ===")
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
local realm_a = a:create_realm("EquivocationTest")
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
local target_1 = string.rep("e1", 32)
local target_2 = string.rep("e2", 32)
local target_3 = string.rep("e3", 32)
local scope_hex = string.rep("aa", 32)

-- Step 4: A creates genesis + legitimate switch (seq=0, seq=1)
h.section(4, "A creates genesis + legitimate switch")
local genesis, author_state = realm_a:create_genesis_event(target_1, id_a, pq_a)
indras.assert.eq(genesis:seq(), 0, "Genesis should have seq=0")
print("    Genesis: seq=0 hash=" .. genesis:event_hash_hex():sub(1, 16) .. "...")

-- Save a copy of author_state BEFORE the legitimate switch
-- We'll reuse this to create the equivocating event
local saved_state = {
    latest_seq = author_state.latest_seq,
    latest_hash = author_state.latest_hash,
    current_attention = author_state.current_attention
}

local legit_ev, author_state = realm_a:switch_attention_conserved(
    target_1, target_2, id_a, pq_a, author_state
)
indras.assert.eq(legit_ev:seq(), 1, "Legitimate switch should have seq=1")
local legit_hash = legit_ev:event_hash_hex()
print("    Legitimate switch: seq=1 hash=" .. legit_hash:sub(1, 16) .. "...")

-- Verify no fraud yet
local fraud_before = realm_a:is_fraudulent(id_a)
indras.assert.eq(fraud_before, false, "A should NOT be fraudulent before equivocation")
print("    A is not fraudulent (yet)")

-- Step 5: A creates a CONFLICTING event at seq=1 (equivocation)
h.section(5, "A creates conflicting event at seq=1 (equivocation!)")
-- Use the saved_state (same prev_hash) but different target (target_3 instead of target_2)
-- This creates a fork: two seq=1 events with the same prev_hash but different content
local equivoc_ev, _ = realm_a:switch_attention_conserved(
    target_1, target_3, id_a, pq_a, saved_state
)
indras.assert.eq(equivoc_ev:seq(), 1, "Equivocating event should also have seq=1")
local equivoc_hash = equivoc_ev:event_hash_hex()
print("    Equivocating event: seq=1 hash=" .. equivoc_hash:sub(1, 16) .. "...")
indras.assert.ne(legit_hash, equivoc_hash, "Two seq=1 events must have different hashes")
print("    Confirmed: two different events at seq=1 (FORK)")

-- Step 6: Verify fraud detected locally on A
h.section(6, "Verify fraud detected on A")
local fraud_after = realm_a:is_fraudulent(id_a)
indras.assert.true_(fraud_after, "A should be fraudulent after equivocation")
print("    A is now fraudulent (equivocation detected)")

local fraudsters_a = realm_a:fraudulent_authors()
indras.assert.true_(#fraudsters_a >= 1, "Should have at least 1 fraudulent author")
print("    Fraudulent authors on A: " .. #fraudsters_a)

-- Step 7: Wait for fraud evidence to sync to B
h.section(7, "Waiting for fraud evidence to sync to B")
h.assert_eventually(function()
    return realm_b:is_fraudulent(id_a)
end, { timeout = 15, interval = 0.5, msg = "Fraud evidence for A should sync to B" })
print("    B sees A as fraudulent")

local fraudsters_b = realm_b:fraudulent_authors()
indras.assert.true_(#fraudsters_b >= 1, "B should see at least 1 fraudulent author")
print("    B sees " .. #fraudsters_b .. " fraudulent author(s)")

-- Step 8: Wait for fraud to sync to C and D
h.section(8, "Waiting for fraud evidence to sync to C and D")
h.assert_eventually(function()
    return realm_c:is_fraudulent(id_a)
end, { timeout = 15, interval = 0.5, msg = "Fraud evidence for A should sync to C" })
print("    C sees A as fraudulent")

h.assert_eventually(function()
    return realm_d:is_fraudulent(id_a)
end, { timeout = 15, interval = 0.5, msg = "Fraud evidence for A should sync to D" })
print("    D sees A as fraudulent")

-- Step 9: Witness-certify the LEGITIMATE event (B, C, D sign it)
h.section(9, "B, C, D witness-sign the legitimate event")
local author_pubkey = pq_a:public_key_hex()

-- Set witness roster
realm_a:set_witness_roster(scope_hex, { id_b, id_c, id_d })

local sig_b = realm_b:request_witness_signature(legit_ev, scope_hex, id_b, pq_b, author_pubkey)
print("    B signed legitimate event")

local sig_c = realm_c:request_witness_signature(legit_ev, scope_hex, id_c, pq_c, author_pubkey)
print("    C signed legitimate event")

local sig_d = realm_d:request_witness_signature(legit_ev, scope_hex, id_d, pq_d, author_pubkey)
print("    D signed legitimate event")

-- Step 10: Submit quorum certificate for the legitimate event
h.section(10, "Submit quorum certificate for legitimate event")
local pubkeys = {}
pubkeys[id_b] = pq_b:public_key_hex()
pubkeys[id_c] = pq_c:public_key_hex()
pubkeys[id_d] = pq_d:public_key_hex()

local k = 2  -- quorum threshold for 3 witnesses: floor(3/2) + 1 = 2
local roster_ids = { id_b, id_c, id_d }

realm_a:submit_certificate(
    legit_hash, scope_hex,
    { sig_b, sig_c, sig_d },
    roster_ids, k, pubkeys
)
print("    Certificate submitted for legitimate event (k=" .. k .. ")")

-- Step 11: Verify certified vs uncertified
h.section(11, "Verify certified vs uncertified events")
local cert_legit = realm_a:has_certificate(legit_hash)
indras.assert.true_(cert_legit, "Legitimate event should have a certificate")
print("    Legitimate event (hash=" .. legit_hash:sub(1, 16) .. "...) has certificate: YES")

local cert_equivoc = realm_a:has_certificate(equivoc_hash)
indras.assert.eq(cert_equivoc, false, "Equivocating event should NOT have a certificate")
print("    Equivocating event (hash=" .. equivoc_hash:sub(1, 16) .. "...) has certificate: NO")

print("    Slashing distinction: certified events survive, uncertified do not")

-- Step 12: B and C are NOT fraudulent
h.section(12, "Verify B, C, D are NOT fraudulent")
indras.assert.eq(realm_a:is_fraudulent(id_b), false, "B should not be fraudulent")
indras.assert.eq(realm_a:is_fraudulent(id_c), false, "C should not be fraudulent")
indras.assert.eq(realm_a:is_fraudulent(id_d), false, "D should not be fraudulent")
print("    B, C, D are all clean (not fraudulent)")

-- Step 13: Stop all
h.section(13, "Stopping networks")
h.stop_all(nets)
print("    All networks stopped")

print()
h.pass("Live Equivocation & Slashing Test")

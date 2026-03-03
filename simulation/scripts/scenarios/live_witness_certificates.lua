-- live_witness_certificates.lua
--
-- E2E test: Phase 2 witness certificates and finality.
--
-- Exercises: genesis events, attention switching with PQ signatures,
-- witness co-signing, quorum certificate assembly, certificate CRDT
-- sync between nodes, finality classification via has_quorum.
--
-- Uses 5 live IndrasNetwork nodes (A-E) with QUIC transport.
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/live_witness_certificates.lua

package.path = package.path .. ";simulation/scripts/lib/?.lua"
local h = require("live_network_helpers")

print("=== Live Witness Certificates Test ===")
print()

-- Step 1: Create 5 networks (A-E), start, connect all
h.section(1, "Creating and connecting 5 networks")
local nets = h.create_networks(5)
local a, b, c, d, e = nets[1], nets[2], nets[3], nets[4], nets[5]
a:set_display_name("A")
b:set_display_name("B")
c:set_display_name("C")
d:set_display_name("D")
e:set_display_name("E")
h.connect_all(nets)
print("    5 networks started and connected")

-- Step 2: A creates realm, all join
h.section(2, "A creates realm, B-E join")
local realm_a = a:create_realm("WitnessCerts")
local invite = realm_a:invite_code()
local realm_b = b:join(invite)
local realm_c = c:join(invite)
local realm_d = d:join(invite)
local realm_e = e:join(invite)
print("    All 5 nodes in realm: " .. realm_a:id():sub(1, 16) .. "...")

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
local id_e = e:member_id()

print("    A: " .. id_a:sub(1, 16) .. "...")
print("    B: " .. id_b:sub(1, 16) .. "...")
print("    C: " .. id_c:sub(1, 16) .. "...")
print("    D: " .. id_d:sub(1, 16) .. "...")
print("    E: " .. id_e:sub(1, 16) .. "...")

-- Use a doc-type artifact ID as the intention scope (deterministic)
local scope_hex = string.rep("aa", 32)  -- 32 bytes of 0xAA

-- Step 4: Set witness roster on A's realm
h.section(4, "A sets witness roster: [B, C, D, E]")
realm_a:set_witness_roster(scope_hex, { id_b, id_c, id_d, id_e })

-- Verify roster was set
local roster = realm_a:get_witness_roster(scope_hex)
indras.assert.eq(#roster, 4, "Roster should have 4 witnesses")
print("    Witness roster set with 4 members")

-- Step 5: A creates genesis event
h.section(5, "A creates genesis event")
local genesis_event, author_state = realm_a:create_genesis_event(scope_hex, id_a, pq_a)
local genesis_hash = genesis_event:event_hash_hex()
indras.assert.eq(genesis_event:seq(), 0, "Genesis event should have seq=0")
print("    Genesis event created: hash=" .. genesis_hash:sub(1, 16) .. "... seq=0")
print("    Author state: seq=" .. author_state.latest_seq .. " hash=" .. author_state.latest_hash:sub(1, 16) .. "...")

-- Step 6: A switches attention (from=nil to scope, chained from genesis)
h.section(6, "A switches attention (chained from genesis)")
local switch_event, author_state = realm_a:switch_attention_conserved(
    nil, scope_hex, id_a, pq_a, author_state
)
local switch_hash = switch_event:event_hash_hex()
indras.assert.eq(switch_event:seq(), 1, "Switch event should have seq=1")
print("    Switch event created: hash=" .. switch_hash:sub(1, 16) .. "... seq=1")
print("    Author state: seq=" .. author_state.latest_seq .. " hash=" .. author_state.latest_hash:sub(1, 16) .. "...")

-- Step 7: Witnesses B, C, D co-sign the switch event
h.section(7, "B, C, D witness-sign the switch event")
local author_pubkey = pq_a:public_key_hex()

local sig_b = realm_b:request_witness_signature(switch_event, scope_hex, id_b, pq_b, author_pubkey)
print("    B signed: witness=" .. sig_b.witness:sub(1, 16) .. "...")

local sig_c = realm_c:request_witness_signature(switch_event, scope_hex, id_c, pq_c, author_pubkey)
print("    C signed: witness=" .. sig_c.witness:sub(1, 16) .. "...")

local sig_d = realm_d:request_witness_signature(switch_event, scope_hex, id_d, pq_d, author_pubkey)
print("    D signed: witness=" .. sig_d.witness:sub(1, 16) .. "...")

-- Step 8: Assemble quorum certificate and submit
h.section(8, "A assembles and submits quorum certificate")

-- Build pubkeys table: { [member_hex] = pubkey_hex }
local pubkeys = {}
pubkeys[id_b] = pq_b:public_key_hex()
pubkeys[id_c] = pq_c:public_key_hex()
pubkeys[id_d] = pq_d:public_key_hex()

-- k = floor(4/2) + 1 = 3 (quorum threshold for 4 witnesses)
local k = 3
local roster_ids = { id_b, id_c, id_d, id_e }

realm_a:submit_certificate(
    switch_hash, scope_hex,
    { sig_b, sig_c, sig_d },
    roster_ids, k, pubkeys
)
print("    Certificate submitted with 3 witness signatures (k=" .. k .. ")")

-- Step 9: Verify certificate exists locally on A
h.section(9, "Verify certificate on A")
local has_cert_a = realm_a:has_certificate(switch_hash)
indras.assert.true_(has_cert_a, "A should have certificate for switch event")
local has_quorum_a = realm_a:has_quorum(switch_hash, k)
indras.assert.true_(has_quorum_a, "A should have quorum (k=3) for switch event")
print("    A has certificate with quorum k=" .. k)

-- Step 10: Wait for certificate to sync to B via CRDT
h.section(10, "Waiting for certificate CRDT sync to B")
h.assert_eventually(function()
    return realm_b:has_certificate(switch_hash)
end, { timeout = 15, interval = 0.5, msg = "Certificate should sync to B via CRDT" })
print("    Certificate synced to B")

-- Verify quorum on B's side
local has_quorum_b = realm_b:has_quorum(switch_hash, k)
indras.assert.true_(has_quorum_b, "B should see quorum (k=3) for switch event")
print("    B confirms quorum k=" .. k .. " (event is FINAL)")

-- Step 11: Verify witness roster also syncs
h.section(11, "Verify witness roster syncs to B")
h.assert_eventually(function()
    local r = realm_b:get_witness_roster(scope_hex)
    return #r >= 4
end, { timeout = 10, interval = 0.5, msg = "Witness roster should sync to B" })
local roster_b = realm_b:get_witness_roster(scope_hex)
indras.assert.eq(#roster_b, 4, "B should see 4 witnesses in roster")
print("    B sees roster with " .. #roster_b .. " witnesses")

-- Step 12: Stop all
h.section(12, "Stopping networks")
h.stop_all(nets)
print("    All networks stopped")

print()
h.pass("Live Witness Certificates Test")

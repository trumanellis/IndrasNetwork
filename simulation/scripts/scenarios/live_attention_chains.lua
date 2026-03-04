-- live_attention_chains.lua
--
-- E2E test: Hash-chained PQ-signed events, chain sync between nodes,
-- and multi-author chain coexistence.
--
-- Exercises: create_genesis_event, switch_attention_conserved,
-- chain_events_for (new binding), and CRDT sync of chain events.
--
-- Uses 3 live IndrasNetwork nodes (A, B, C) with QUIC transport.
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/live_attention_chains.lua

package.path = package.path .. ";simulation/scripts/lib/?.lua"
local h = require("live_network_helpers")

print("=== Live Attention Chains Test ===")
print()

-- Step 1: Create 3 networks, start, connect all
h.section(1, "Creating and connecting 3 networks")
local nets = h.create_networks(3)
local a, b, c = nets[1], nets[2], nets[3]
a:set_display_name("A")
b:set_display_name("B")
c:set_display_name("C")
h.connect_all(nets)
print("    3 networks started and connected")

-- Step 2: A creates realm, B and C join
h.section(2, "A creates realm, B and C join")
local realm_a = a:create_realm("AttentionChains")
local invite = realm_a:invite_code()
local realm_b = b:join(invite)
local realm_c = c:join(invite)
print("    All 3 nodes in realm: " .. realm_a:id():sub(1, 16) .. "...")

-- Step 3: Get PQ identities and member IDs
h.section(3, "Getting PQ identities and member IDs")
local pq_a = a:pq_identity()
local pq_b = b:pq_identity()

local id_a = a:member_id()
local id_b = b:member_id()
local id_c = c:member_id()

print("    A: " .. id_a:sub(1, 16) .. "...")
print("    B: " .. id_b:sub(1, 16) .. "...")
print("    C: " .. id_c:sub(1, 16) .. "...")

-- Deterministic artifact IDs for attention targets
local target_1 = string.rep("a1", 32)  -- 32 bytes
local target_2 = string.rep("a2", 32)
local target_3 = string.rep("a3", 32)

-- Step 4: A creates genesis event
h.section(4, "A creates genesis event (seq=0)")
local genesis_event, author_state_a = realm_a:create_genesis_event(target_1, id_a, pq_a)
local genesis_hash = genesis_event:event_hash_hex()
indras.assert.eq(genesis_event:seq(), 0, "Genesis event should have seq=0")
print("    Genesis: seq=0 hash=" .. genesis_hash:sub(1, 16) .. "...")
print("    Author state: seq=" .. author_state_a.latest_seq)

-- Step 5: A switches attention 3 more times
h.section(5, "A switches attention 3 times (seq=1,2,3)")

-- Switch 1: target_1 -> target_2
local ev1, author_state_a = realm_a:switch_attention_conserved(
    target_1, target_2, id_a, pq_a, author_state_a
)
indras.assert.eq(ev1:seq(), 1, "First switch should have seq=1")
print("    Switch 1: seq=1 hash=" .. ev1:event_hash_hex():sub(1, 16) .. "...")

-- Switch 2: target_2 -> target_3
local ev2, author_state_a = realm_a:switch_attention_conserved(
    target_2, target_3, id_a, pq_a, author_state_a
)
indras.assert.eq(ev2:seq(), 2, "Second switch should have seq=2")
print("    Switch 2: seq=2 hash=" .. ev2:event_hash_hex():sub(1, 16) .. "...")

-- Switch 3: target_3 -> target_1
local ev3, author_state_a = realm_a:switch_attention_conserved(
    target_3, target_1, id_a, pq_a, author_state_a
)
indras.assert.eq(ev3:seq(), 3, "Third switch should have seq=3")
print("    Switch 3: seq=3 hash=" .. ev3:event_hash_hex():sub(1, 16) .. "...")

-- Verify chain locally on A
local chain_a_local = realm_a:chain_events_for(id_a)
indras.assert.eq(#chain_a_local, 4, "A should have 4 chain events locally")
for i, cev in ipairs(chain_a_local) do
    indras.assert.eq(cev.seq, i - 1, "Event " .. i .. " should have seq=" .. (i - 1))
end
print("    A's local chain has 4 events with seq 0..3")

-- Step 6: Wait for A's chain events to sync to B
h.section(6, "Waiting for A's chain to sync to B")
h.assert_eventually(function()
    local chain = realm_b:chain_events_for(id_a)
    return #chain >= 4
end, { timeout = 15, interval = 0.5, msg = "A's chain events should sync to B" })
local chain_a_on_b = realm_b:chain_events_for(id_a)
indras.assert.eq(#chain_a_on_b, 4, "B should see 4 chain events for A")
print("    B sees A's 4 chain events")

-- Verify seq ordering on B
for i, cev in ipairs(chain_a_on_b) do
    indras.assert.eq(cev.seq, i - 1, "B: event " .. i .. " should have seq=" .. (i - 1))
end
print("    B confirms correct seq ordering 0..3")

-- Step 7: B creates its own genesis + 2 switches
h.section(7, "B creates genesis + 2 switches (independent chain)")
local b_genesis, author_state_b = realm_b:create_genesis_event(target_2, id_b, pq_b)
indras.assert.eq(b_genesis:seq(), 0, "B's genesis should have seq=0")
print("    B genesis: seq=0 hash=" .. b_genesis:event_hash_hex():sub(1, 16) .. "...")

local b_ev1, author_state_b = realm_b:switch_attention_conserved(
    target_2, target_3, id_b, pq_b, author_state_b
)
indras.assert.eq(b_ev1:seq(), 1, "B's first switch should have seq=1")
print("    B switch 1: seq=1")

local b_ev2, author_state_b = realm_b:switch_attention_conserved(
    target_3, target_1, id_b, pq_b, author_state_b
)
indras.assert.eq(b_ev2:seq(), 2, "B's second switch should have seq=2")
print("    B switch 2: seq=2")

-- Verify B's local chain
local chain_b_local = realm_b:chain_events_for(id_b)
indras.assert.eq(#chain_b_local, 3, "B should have 3 chain events locally")
print("    B's local chain has 3 events with seq 0..2")

-- Step 8: Wait for B's chain to sync to A
h.section(8, "Waiting for B's chain to sync to A")
h.assert_eventually(function()
    local chain = realm_a:chain_events_for(id_b)
    return #chain >= 3
end, { timeout = 15, interval = 0.5, msg = "B's chain events should sync to A" })
local chain_b_on_a = realm_a:chain_events_for(id_b)
indras.assert.eq(#chain_b_on_a, 3, "A should see 3 chain events for B")
print("    A sees B's 3 chain events")

-- Step 9: Verify both chains coexist
h.section(9, "Verify both chains coexist")
local chain_a_final = realm_a:chain_events_for(id_a)
local chain_b_final = realm_a:chain_events_for(id_b)
indras.assert.eq(#chain_a_final, 4, "A's chain should still have 4 events")
indras.assert.eq(#chain_b_final, 3, "B's chain should have 3 events on A")
print("    A's chain: 4 events, B's chain: 3 events — both coexist on A's node")

-- C should have no chain (never created events)
local chain_c = realm_a:chain_events_for(id_c)
indras.assert.eq(#chain_c, 0, "C should have no chain events")
print("    C has 0 chain events (as expected)")

-- Step 10: Stop all
h.section(10, "Stopping networks")
h.stop_all(nets)
print("    All networks stopped")

print()
h.pass("Live Attention Chains Test")

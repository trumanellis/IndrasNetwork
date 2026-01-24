-- PRoPHET Routing Example
--
-- Demonstrates the Lua bindings for the PRoPHET (Probabilistic Routing Protocol) protocol.
-- This script shows how to query delivery probabilities and trigger encounters between peers.

-- indras is a global table, no need to require it

-- Create a simple line topology: A - B - C
local mesh = indras.MeshBuilder.new(3):line()
local config = indras.SimConfig.manual()
local sim = indras.Simulation.new(mesh, config)

-- Bring all peers online
sim:force_online(indras.PeerId.new('A'))
sim:force_online(indras.PeerId.new('B'))
sim:force_online(indras.PeerId.new('C'))

print("=== Initial State ===")
print("All peers online")
print()

-- Check initial probabilities (should all be 0)
print("Initial delivery probabilities (A -> B):", sim:get_prophet_probability('A', 'B'))
print("Initial delivery probabilities (A -> C):", sim:get_prophet_probability('A', 'C'))
print()

-- Trigger an encounter between A and B
print("=== Triggering Encounter: A meets B ===")
sim:trigger_encounter('A', 'B')

-- Check updated probabilities
local prob_a_b = sim:get_prophet_probability('A', 'B')
print(string.format("After encounter: A -> B = %.4f", prob_a_b))
print()

-- Trigger another encounter
print("=== Triggering Encounter: B meets C ===")
sim:trigger_encounter('B', 'C')

local prob_b_c = sim:get_prophet_probability('B', 'C')
print(string.format("After encounter: B -> C = %.4f", prob_b_c))
print()

-- Now trigger A meeting B again - A should learn about C through transitivity
print("=== Triggering Encounter: A meets B again ===")
sim:trigger_encounter('A', 'B')

local prob_a_c = sim:get_prophet_probability('A', 'C')
print(string.format("Transitive probability: A -> C = %.4f", prob_a_c))
print("(A learned about C through B via transitivity)")
print()

-- Get all probabilities for peer A
print("=== All Probabilities for Peer A ===")
local state = sim:get_prophet_state('A')
for dest, prob in pairs(state) do
    print(string.format("  %s -> %.4f", dest, prob))
end
print()

-- Send a message and check routing stats
print("=== Sending Message: A -> C ===")
sim:send_message('A', 'C', "Hello via PRoPHET!")
sim:run_ticks(10)

local stats = sim:get_routing_stats()
print(string.format("Messages delivered: %d", stats.messages_delivered))
print(string.format("Messages dropped: %d", stats.messages_dropped))
print(string.format("Relayed deliveries: %d", stats.relayed_deliveries))
print(string.format("Direct deliveries: %d", stats.direct_deliveries))
print()

print("=== PRoPHET Routing Demo Complete ===")

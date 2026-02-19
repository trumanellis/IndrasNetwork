# Simulation Log: comprehensive_test

| | |
|---|---|
| **Scenario** | comprehensive_test |
| **Generated** | 2026-01-22 17:29:45 |
| **Log Entries** | 12 |
| **Errors** | 1 |
| **Warnings** | 1 |

---

## Events

→ Running Lua script: /Users/truman/Code/IndrasNetwork/simulation/scripts/scenarios/comprehensive_test.lua
→ Starting comprehensive Lua feature test `trace_id=5288249c-a8f9-4b97-8a01-6c4b6a0c10f5`
→ Packet A#0 delivered to B via A at tick 7 (latency: 0 ticks)
→ Packet B#0 delivered to A via B at tick 12 (latency: 0 ticks)
→ Info message `level=info`
⚠️ Warn message `level=warn`
   └─ indras_simulation::lua::bindings::logging (simulation/src/lua/bindings/logging.rs:102)
❌ Error message `level=error`
   └─ indras_simulation::lua::bindings::logging (simulation/src/lua/bindings/logging.rs:109)
→ Test with all field types `object_field={"nested":"value"}` `string_field=hello` `float_field=3.14` `number_field=42` `bool_field=true` `array_field=[1,2,3]`
→ Packet A#0 delivered to C via A at tick 6 (latency: 5 ticks)
→ Full integration test passed `latency=5.0` `trace_id=414990d8-bd59-4f60-aaf2-a58c9180764b` `delivered=1`
→ All comprehensive tests passed! `trace_id=5288249c-a8f9-4b97-8a01-6c4b6a0c10f5` `tests_run=12`
→ Script completed successfully

---

*Source: `logs/comprehensive_test.log`*

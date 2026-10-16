# Logistics runtime batch 1

Base: `e6d0e08d807aa3df4328257f0100c8bb00168b94`. Prototype: `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.

The physical handoff adapter now accepts represented garrison occupants. It uses the carrier first, then the recorded house entry, then the prototype's MOVE-only nearest-free fallback. `UnitControl.garrison_entry` holds the optional entry coordinate. Ordinary actors ignore stale entries. Validation rejects invalid active entries before supply mutations.

| Row | Native behavior tests | Prototype read scope |
| --- | --- | --- |
| LOG-18 | `match_logistics::runtime_tests::intent_log18_garrison_entry_fallback_carrier_and_real_service`; `intent_log18_footprint_and_center_range_boundaries`; `intent_log18_bounded_detours_and_epoch_terrain_cache` | `shared/sim.js:1886-1913`, `shared/supply-convoys.js:191-212,560-571` |
| LOG-31 | `match_logistics::runtime_tests::intent_log31_real_truck_birth_has_no_combat_cost_reserves_or_auto_retreat` | `shared/sim.js:358`, `shared/supply-convoys.js:389-393` |
| LOG-32 | `match_logistics::runtime_tests::intent_log32_real_fleet_cap_spawn_beat_ring_retirement_and_loss_delay`; `intent_log32_busy_demand_own_source_and_obstructed_ring`; `intent_log32_retirement_requires_idle_empty_orderless_reachable_source` | `shared/supply-convoys.js:362-386`, `shared/sim.js:1973-1980` |

The LOG-18 tests exercise actual support payment and local reserve transfer. They also cover the carrier and garrison start positions, exact radius, footprint clipping, short and overlong detours, disconnection, forty-tick retry epochs, infantry/obstruction invalidation and vehicle-only cache retention. Caller distance prefilters keep their existing center-based behavior.

The truck tests use the canonical Match adapter for birth, fleet resize and retirement. They check compiled truck values, recruitment exclusion, reserve exclusion, initialized metadata, automatic-retreat state, one-second growth, the four-truck cap, ring placement, spacing, newest-first retirement and thirty-second replacement delay. They supply explicit job counts to isolate fleet behavior; they do not prove demand generation or dispatch scheduling.

`logistics_off_keeps_reserves_and_convoys_absent` protects disabled initialization and service. R1 remains physically logistics-off under decision 097. The full tick runner continues to reject logistics-enabled execution. This batch does not add garrison entry commands, replay ingestion, World logistics, network delivery or client presentation. A future garrison-entry producer must retain its actual entry in the new control field; absent entries already use the source fallback.

The parity tables are unchanged. Native intent review and coordinator-owned status updates remain separate from these runtime tests. No Source recorder or new captured fixture is added.

An independent read-only reviewer accepted the three row mappings against the pinned prototype after checking the effective-entry selection, cache witness and fleet retirement counterexamples. The reviewer ran no code and made no edits or public posts. Native execution results are recorded separately below.

The final focused Vostro run passed all 33 selected tests, with 526 unrelated tests skipped. It includes the eight new tests and retained command, reserve, convoy, route, Match startup, actual supply-hook and withdrawal checks. Cargo ran under an exclusive S3 lock with a 6 GiB memory limit, zero swap and four build jobs, with more than 20 GiB free. No whole-workspace test run or Source execution was performed.

Earlier authored cache witnesses failed before this final run: one changed infantry terrain without its version, and the next detour was too short for the route-length budget to bind before the center radius. Those setup errors were corrected without changing runtime routing, expected fixture outputs or existing tests.

Strict locked workspace/all-target Clippy passed under S3 with the same memory and build-job limits. Final formatting and `git diff --check` passed.

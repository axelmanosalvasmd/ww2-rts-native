# PR 322 logistics admission repair

Restore the `options.logistics || convoys.is_some()` full-tick guard. The complete recipient adapter still rejects logistics. This follows condition 3 of [issue 5 comment 6209727939](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6209727939). No recipient or admission contract is expanded.

`logistics_runtime_full_tick_and_complete_snapshot_guards_stay_closed` checks coherent enabled state and each option/convoy mismatch. The full tick returns before changing the clock, reserves or controls, with no events or permanent fault. Complete list snapshot capture returns `Unsupported(Logistics)` without mutating delivery state. `logistics_runtime_ordinary_logistics_off_ticks_and_complete_snapshots_still_work` runs 45 ordinary ticks with complete snapshots for both seats and checks that logistics phases remain reported unavailable.

The logistics row tests call the real Match adapters directly. `withdrawal_turn` invokes the path budget, reserve and withdrawal owners; it does not run or simulate a full match tick. The former partial recipient projection checks are replaced by checks that complete snapshot and full-tick admission remain closed after the component transitions. The convoy birth witness calls the convoy adapter directly. No logistics-enabled full-match or server proof is claimed.

Each existing row proof remains available with the guard closed:

- LOG-11: yes. `log11_support_pays_source_then_local_stock_then_own_reserve`, `log11_reinforcement_and_vehicle_repair_pay_from_actual_recovery`, `log11_medic_healing_spends_medics_own_provisions`, `log11_engineer_repair_spends_engineers_own_provisions`. These use support, recovery, healing and construction adapters directly and are unchanged.
- LOG-12: yes, as Match-adapter proof. `logistics_runtime_log12_withdrawal_cancels_orders_and_routes_without_teleport`, `logistics_runtime_log12_stranded_retries_and_nearby_handoff_needs_no_path`, `logistics_runtime_log12_withdrawal_cancels_canonical_build_and_shared_project`, `logistics_runtime_log12_empty_fuel_and_emergency_strands_without_removal`, `logistics_runtime_log12_forced_garrison_exits_through_withdrawal_adapter`. These cover cancellation, building exit, nearest-store route selection, handoff, retries and stranding without full ticks.
- LOG-13: yes, as Match-adapter proof. `logistics_runtime_log13_player_hold_escape_stances_and_recovery`, `logistics_runtime_log13_stranded_defense_cannot_chase_or_interrupt_escape`. Actual command admission and withdrawal state transitions remain exercised.
- LOG-22: yes. `log22_network_sources_follow_hq_fallback_point_kind_and_point_status` directly computes the network, including Annihilation's point sources. Mode admission stays closed.
- LOG-47: yes. `log47_held_allied_uncut_depot_reinforces_and_repairs_only_within_radius` directly prepares and runs recovery for held/allied depots, with cut, hostile and distance exclusions.

LOG-14 and LOG-20 retain the original PR's dropped status. No FFI, C header, wire, contract or privacy change.

Validation on the repaired Rust tree after merging main `7ed527b04b0244d9affaec4fc42d88612ccf3c98`: focused core/simtest nextest ran 63 tests, all passed (828 skipped). Formatting and strict workspace all-target Clippy passed. The selection covered this batch's row tests, Match logistics adapters, logistics features and the simtest tick regressions. Checks used Vostro slot S3, four Cargo jobs, a 6 GiB memory scope and no swap. A separate source-only review found no concrete defect and ran no tests.

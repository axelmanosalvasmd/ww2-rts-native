# Logistics runtime batch 2

This batch targets LOG-07, LOG-09, LOG-34, LOG-40 and LOG-42 against the prototype at `e4e579a`. Tests use authored native component inputs. No new Source recorder, captured fixture or provenance bundle is added.

## Fuel and recovery checks

LOG-07 connects the existing reserve owner to the actual Match movement wrapper. The final speed multiplier limits powered movement after retreat/sprint speed selection. Actual displacement determines the driving-time debit, capped by the tick interval and using the prototype's minimum speed divisor. Reserve time is not charged again.

- `match_commands::tests::log07_parked_and_riding_movement_preserve_fuel`
- `match_commands::tests::log07_acceleration_charges_actual_distance_without_reticking_reserves`
- `match_commands::tests::log07_ordinary_depletion_limits_speed_and_debits_emergency_overrun`
- `match_commands::tests::log07_forced_escape_scales_retreat_speed_and_empty_fuel_strands`

Prototype correspondence: `shared/logistics.js:103-114` and `shared/sim.js:5484-5509`. Riding facts are authored component inputs, not a boarding journey. No powered sprint kind exists in the inspected catalog; retreat checks the branch that bypasses the ordinary suppression-speed input.

LOG-09 exercises the existing recovery consumers at full rate, the 30-second boundary, half rate below it, zero provisions and latched exhaustion. A reachable source isolates base recovery from support affordability. Medic cases test both participants independently and together without positive-stock or missing-health caps masking their rates. Engineer repair retains diminishing crew returns and averages a mixed four-person crew.

- `match_recovery::tests::log09_reinforcement_timer_scales_with_provisions_and_exhaustion`
- `match_recovery::tests::log09_base_vehicle_repair_scales_with_provisions_and_exhaustion`
- `match_recovery::tests::log09_medic_healing_multiplies_patient_and_medic_recovery_rates`
- `match_construction::tests::log09_building_repair_uses_average_engineer_recovery_rate`

Prototype correspondence: `shared/logistics.js:131-136` and `shared/sim.js:4962-4965`, `:5590-5595`, `:5663-5668`. The construction check authors a paid, damaged site as completed; it does not prove natural construction completion.

## Convoy checks

- LOG-34: `logistics_features::feature_log34_manual_supply_selects_nearest_own_stock_excluding_grants` invokes manual Supply. It checks nearest usable stock, donation exclusion, allied/active origin filtering, exclusion of the destination's own inventory, the strict upstream-distance boundary and source fallback when private stock is at most one.
- LOG-40: `logistics_features::feature_log40_relief_preserves_payer_and_recipient_permissions` exercises two manual cache deliveries, checks both payer buckets and recipient grants, then runs recipient local service across them. The two donors do not double the recipient's transfer rate. Automatic origins cannot use granted stock.
- LOG-42: `logistics_features::feature_log42_death_discards_cargo_and_delays_replacement` removes a loaded truck and an entity-backed store, then exercises fleet replacement immediately before and at the thirty-second boundary. The replacement starts empty at its own source.

Prototype correspondence: `shared/supply-convoys.js:43-77` (payer pools and grants), `:191-212` (local service), `:268-273` (origins), `:320-341` (cache delivery), `:362-386` (replacement delay) and `:549-558` (death cleanup).

## Review and validation

Independent read-only reviewers inspected the pinned prototype and accepted all five row intents with no must-fix findings. They ran no execution gates.

On the Vostro, the 13 selected checks pass. The broader affected filter passes 96 tests across core and simtest, with 650 skipped. Strict locked workspace/all-target Clippy, formatting and `git diff --check` pass. Each Cargo invocation uses an exclusive shared slot, that slot's target directory, a 6 GiB memory limit, zero swap and at least 20 GiB free. No full-workspace test run or Source execution was performed.

The branch incorporates main `40020946731a2ea17bd85f6ef0ebf8f14e00e493`. Its incoming changes do not overlap the five edited logistics source/test files.

## Limits and follow-ups

These are component and Match-adapter checks, not full-tick logistics or whole-match proof. The existing full-tick logistics admission guard is outside this batch. Physical handoff and route hooks in the convoy tests remain explicit inputs; this batch does not depend on PR #291. Movement uses the existing full-roster supply validation. Its cost should be measured before full-tick logistics is enabled; no performance result is claimed here.

LOG-05 and LOG-08 are dropped from this batch because their terrain-targeted firing consumer is not implemented. Adding that firing path is larger than the selected batch. The remaining assigned rows stay for the next worker. No FFI, C header, wire format, contract, privacy, reference pin or parity-status file changes are included.

Acknowledged: Axel's owner decision `6206861267` permits native intent tests for behavior rows. Existing interface approvals remain limited to their agreed scope. Runtime acknowledgment, hosted checks, merge and later parity-status updates belong to the coordinator.

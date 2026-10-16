# PR 322 repair and PR 375 overlap audit

Inputs: PR 322 `e63f825cc8e18a4cf337f1ca36d79c0e5469f2a5`,
PR 373 `4edfcd5e8b795aa47fde114de7d51d7eb6430078`, and
PR 375 `9ecc71a5509dccac70276088df990a968c083fce`.
The clean `fix-322` checkout fast-forwarded its existing branch to PR 322,
then merged PR 373 as `04c066dd` and PR 375 as `2e66d282`.
No rebase, force push, issue post or parity-status edit is part of this repair.

## Literal overlap inventory

- `match_tick.rs`: PR 375 covers reserve and withdrawal calls, the network beat,
  convoy service, logistics phase reporting and ordinary full-tick admission.
  Retain its initialized-convoy validation and World refusal. Main's mode-specific
  victory dispatch and garrison construction/exit preflight remain intact.
- `match_withdrawal.rs`: both cancel construction, digging and shared work through
  `cancel_owned_work_state`. Retain PR 375's canonical placement and sorting,
  work-state validation and cancellation without redundant flag overwrites.
- `match_logistics.rs`: both permit typed nontruck queues and price truck waypoints.
  PR 375 additionally rejects `Command::Unsupported` in troop queues. Keep that
  stricter implementation, both call-site boolean meanings and its regression.
- `match_commands.rs`: PR 322 added forced-unit `Stance` admission. Drop that
  change and preserve main/PR 375's refusal. At prototype `e4e579a`,
  `shared/sim.js:3419` permits obsolete command names `holdFire`, `holdPos` and
  `autoRetreat`, whereas the actual switch command is `stance` at `3574-3580`.
  The added broad admission does not match the executed prototype path.
- `supply_network.rs` and its Match call: PR 322 alone implements Annihilation's
  rule that every eligible held point supplies, so keep `compute_for_mode` and its
  caller, matching prototype `shared/sim.js:5025-5032`, while the original
  `compute` retains its Conquest default. This component proof opens no additional room or logistics mode.
- PR 322's three test modules and registrations remain unique. Preserve their
  payment, withdrawal, source and depot checks, adapting only expectations for
  deliberately superseded admission. PR 375's full lifecycle, save/load and
  recipient-privacy tests remain registered and unchanged.

## Hand-edited merge hunks

Locations below refer to the resulting source, not conflict-marker positions.

- First merge, `core/src/lib.rs:211`: union of all three PR 322 modules and every
  incoming main module, fixture declaration and save module.
- First merge, `core/src/match_tick.rs:417`: keep main's complete automatic
  garrison-exit preflight and construction refusal. Drop PR 322's bypass and
  disconnected-exit error; main already supplies the canonical exit.
- First merge, `core/src/match_tick.rs:891`: convoy service precedes main's full
  Assault, Annihilation, Classic, Tutorial and Conquest victory dispatch.
- Second merge, `docs/contracts/save-format.md:99`: keep PR 373's separate exact
  elimination field row and all its following tagged-codec and compatibility text;
  retain PR 375's logistics save admission elsewhere in the document.
- Second merge, `core/src/match_tick.rs:862` and `:893`: keep PR 375's convoy-state
  predicates and nonzero network beat, with main's complete victory dispatch.
- Second merge, `core/src/match_logistics.rs:966`: keep PR 375's `non_truck`
  signature, unsupported-command refusal and both matching call sites. Remove
  unused `unit_will_force_withdrawal`; main's canonical exit preflight replaces it.
- Second merge, `core/src/match_withdrawal.rs:76` and `:138`: keep PR 375's work
  validation and canonical cancellation, including removal of redundant build
  and entrench assignments.
- Second merge, `core/src/match_commands.rs:837`: remove PR 322's added Stance
  allowlist entry, preserving main/PR 375's exact forced-unit refusal.

The follow-up also restores the convoy body-obstacle truck predicate at
`core/src/match_logistics.rs:846` to exact PR 375 after reconciling the queue
booleans. Only the two queue projection call sites use `non_truck`.

## Follow-up test edits

`core/src/logistics_runtime_tests.rs:87` now checks complete delivery and valid
ordinary tick state after component transitions. Its former blanket refusal was
intentionally replaced by PR 375. The admission test at `:99` proves coherent
logistics can tick and deliver while both option/convoy mismatches still reject
before clock, reserve or control mutation. The convoy witness retains truck
birth and repeated service checks with supported complete delivery.

The player-control test at `:231` retains stop, escape, blocked attack-move,
retreat and recovery checks. Its former allowed-stance assertion now requires
`Needs` and exact unchanged controls/hold position, matching main and the
prototype's real command path. No inherited main assertion is weakened.

`simtest/tests/match_tick.rs:790` retains every pre-write refusal and equality
assertion. Only the logistics/convoy mismatch error text changes from the old
blanket guard to PR 375's initialized-state refusal.

## Each row literal test mapping

- LOG-11: `log11_support_pays_source_then_local_stock_then_own_reserve`,
  `log11_reinforcement_and_vehicle_repair_pay_from_actual_recovery`,
  `log11_medic_healing_spends_medics_own_provisions`,
  `log11_engineer_repair_spends_engineers_own_provisions`.
- LOG-12: `logistics_runtime_log12_withdrawal_cancels_orders_and_routes_without_teleport`,
  `logistics_runtime_log12_stranded_retries_and_nearby_handoff_needs_no_path`,
  `logistics_runtime_log12_withdrawal_cancels_canonical_build_and_shared_project`,
  `logistics_runtime_log12_empty_fuel_and_emergency_strands_without_removal`,
  `logistics_runtime_log12_forced_garrison_exits_through_withdrawal_adapter`.
- LOG-22: `log22_network_sources_follow_hq_fallback_point_kind_and_point_status`.
- LOG-47: `log47_held_allied_uncut_depot_reinforces_and_repairs_only_within_radius`.
- LOG-13: partial, not counted as a complete row. Stop, move, retreat, stranded
  defense and recovery remain checked. The row's stance wording does not match
  the executed prototype `stance` path. Preserve the refusal pending owner review.
- LOG-14 and LOG-20: remain dropped for incomplete full side-effect coverage and
  Classic/World cache integration respectively. No differs candidate is adopted.

The older `fix-322-logistics-admission.md` is a historical guard-closed receipt.
This stacked repair supersedes its admission narrative and LOG-13 full-row claim.
Fresh Hermes acknowledgment is required. Merges after PR 373 and PR 375.

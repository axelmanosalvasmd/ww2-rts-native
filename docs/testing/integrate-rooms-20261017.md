# Rooms integration, 2026-10-17

Base: `61947c5b8a8438eb6ccbefa02e7d269c4e032472`. This branch merges exact PR heads in the authorized order. It does not publish main or open a PR. World admission remains closed.

| PR | Merged head |
| --- | --- |
| #374 | `0a712ac4f909f130fa3518b1586b2089e6e07b0f` |
| #378 | `743dd4c5f609bd908d1658fc13d579be6afae959` |
| #384 | `13da1dead3f6cba80ff35f69cc9974f2b899bf31` |
| #385 | `549dc66f043f6afea148b79549e30a32ad647776` |

## Manual merge resolutions

- #374: retain frozen-main spectator and command-ingress test registrations in `lobby.rs`, latency telemetry in `room_runtime.rs`, and the incoming lifecycle additions plus existing load/remapping test in `operative_room_tests.rs`.
- #378: automatic merge. A source audit found the automatic #374 merge duplicated the operative room-test registration; remove the duplicate in the #385 integration commit.
- #384: retain the earlier Classic partial-delivery helpers in the commander, retreat and mode-startup tests. They already assert the exact incoming mode literals for every seat and add unit identity, retreat flags, ghost and capability checks. Retain all incoming restart tests and runtime changes.
- #385 contracts: retain the current protocol status and R1 ingress entry, plus both incoming contract entries.
- #385 snapshot cache: retain extracted shared validation and cache-build instrumentation, move the incoming ordinary-mode and state validation into that validator, and preserve companion admission. Keep frozen-main Classic building memory and incoming nodes. Complete list delivery permits represented Classic nodes; World still fails mode admission.
- #385 startup: retain authored-mission definition binding and select the correct ordinary mode's StartPlan. Carry named armies, spectators, readiness and logistics into the incoming async request owner.
- #385 lobby: use the incoming async helpers and result publisher; preserve host pause/resume, ordinary ingress, operative dispatch, spectator behavior and every test module. Host end runs before the pause gate through one end implementation. Keep commander seats fixed during generation while preserving spectator and operative membership. Suppress both commander and spectator resync during generation.

## Rewritten tests

- `mode01_non_world_mode_rows_match_the_prototype_public_fields`: expect exact every-seat rows for newly admitted Assault and Annihilation; retain all Classic literal, ghost and missing-capability assertions and closed-mode refusals.
- `intent_ai50_gate_order_completes_in_each_real_ordinary_mode_with_supported_seat_snapshots`: use delivery for newly admitted Assault and Annihilation; retain all existing Gate, payment, completion and Classic assertions.
- `cmb130_classic_remembers_last_seen_buildings_until_an_airborne_observer_revisits`: retain final empty-ghost checks for every seat and add the incoming destroyed-building absence assertion.

## Validation

The first four-merge build at `89b6ca27` failed before execution: two incoming test helpers omitted `Output::Watcher` arms, and the async server fixture omitted `static_files`. Those fixture integration errors were repaired.

The next full three-package attempt at `88ad16b0` reached the added operative rollback test, whose hello fixture omitted required names. The source repair supplies them and strengthens the test to prove accepted companion input and delivery restoration. Source review also found that pending operative and spectator reconnects needed affected-connection restoration after failed generation. Historical PR results do not qualify this branch. Final exact-head validation follows below.

The repaired server package at `406a457f` passes all 224 tests, including both pending-recipient regressions. The full initial checkpoint had 1,400 passes, one failed fixture and 477 unrun tests; the final full three-package run remains required.

#387 resolutions retain the existing flight contact/projector and port incoming exact flight tests to it. Shared mode validation and StartPlan now include Tutorial alongside all previously admitted modes. Keep all existing ordinary result timeline guards and add Tutorial's non-Conquest timeline. The base-field fixture retains unconditional exact flight comparisons. The public lifecycle refusal test removes only the newly supported Start denial; the startup row test now checks Tutorial for every seat.

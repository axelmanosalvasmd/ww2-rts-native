# Rooms integration, 2026-10-17

Base: `61947c5b8a8438eb6ccbefa02e7d269c4e032472`. This branch merges exact PR heads in the authorized order. It does not publish main or open a PR. World admission remains closed.

| PR | Merged head |
| --- | --- |
| #374 | `0a712ac4f909f130fa3518b1586b2089e6e07b0f` |
| #378 | `743dd4c5f609bd908d1658fc13d579be6afae959` |
| #384 | `13da1dead3f6cba80ff35f69cc9974f2b899bf31` |
| #385 | `549dc66f043f6afea148b79549e30a32ad647776` |
| #387 | `5a11b36c67722dc98fab60a3899287946b6ca4f3` |
| #393 | `800a67a233237d9d511520ba4a7436e8d6e100f0` |
| #398 | `2aef908c7b7a45bb3f9775dcd760567fe245c030` |
| #410 | `f8e53f13eca1a865fd9c607736eedeb28ab7297d` |

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

The next full three-package attempt at `88ad16b0` reached the added operative rollback test, whose hello fixture omitted required names. The source repair supplies them and strengthens the test to prove accepted companion input and delivery restoration. Source review also found that pending operative and spectator reconnects needed affected-connection restoration after failed generation. Historical PR results do not qualify this branch. Final exact-head validation is reported in the chain handoff.

The repaired server package at `406a457f` passes all 224 tests, including both pending-recipient regressions. The full initial checkpoint had 1,400 passes, one failed fixture and 477 unrun tests; the final full three-package run remains required.

#387 resolutions retain the existing flight contact/projector and port incoming exact flight tests to it. Shared mode validation and StartPlan now include Tutorial alongside all previously admitted modes. Keep all existing ordinary result timeline guards and add Tutorial's non-Conquest timeline. The base-field fixture retains unconditional exact flight comparisons. The public lifecycle refusal test removes only the newly supported Start denial; the startup row test now checks Tutorial for every seat.

#393 preserves all ordinary ingress beside Spawn and moves the sandbox flag into the async request owner. The existing NET-37 full cancellation/finish journey now runs sandbox off and on, including canonical and recipient flag checks. Incoming sandbox live-flight tests use the same projector as the earlier ordinary tests.

#398 keeps the current ordinary/operative/terrain-firing capabilities while removing only the obsolete pending-grenade firing refusal. Movement and economy-research registrations are retained together. #410 samples weather seeds in both trusted async requests and public startup; the injected entropy failure path shares that request owner. NET-41 and ordinary binary restart tests retain all seed rollover, spawn, environment and stable-settings assertions while adding the explicit 31-bit weather receipt checks.

All eight recorded PR heads are ancestors of the integration branch. No PR was skipped. No PR was opened and main has not been pushed by this chain integrator.

Final publication requires the full core/server/simtest package run, `cargo fmt --all --check` and strict all-target workspace Clippy on the committed integration source. The completion handoff reports their results and the pushed head. Raw local logs use `/tmp/rooms-final-*`.

## Operative source-review P1 repair (unrun)

At integration head `6a8069e7`, server operative admission already accepts non-World rooms and publishes a companion before constructing its recipient stream. The tick preflight and both combat effect methods still required Default/Conquest with no Game mode. An operative in an admitted Tutorial or Assault room therefore faulted delivery at join or startup. Existing trusted Classic/Annihilation attachments had the same mismatch. The prototype permits these non-World operative joins in `server.js:308-310,604-606`.

`prepare_companion_tick`, `fire_companion` and `throw_companion` now use the existing `ordinary_ground_mode` predicate. It checks the exact option/Game pairs for Default, Conquest, Assault, Annihilation, Classic and Tutorial. The clock, identity, ownership, finite-number, finish, online, ammo and cooldown checks are retained. Inspection of companion input, movement, join, death, respawn and status found no additional stale mode guard requiring a change. World/Horde room admission and public Classic/Annihilation Start admission are unchanged.

The existing `tutorial_room_tests` helpers capture every game seat, including Tutorial's unattended Enemy, and each operative stream during the journeys and ending hold. Both operatives receive a real start: one joins the lobby before publication and the other joins the live match. Shared activity checks send FPS input through the server, assert accepted sequences, operative identity, movement, fire events, grenade consumption and flight creation/completion. Tutorial operatives cross the hedgerow through the authored road using FPS movement; commander orders exclude operative units.

| Regression | Change and finish path |
| --- | --- |
| `tutorial_room_runs_all_steps_snapshots_dialogue_and_ending_hold` | Extended with both operative join paths and activity; retains all nine scripted steps, 18 dialogue lines, canonical recruitment, retreat, grenade and artillery lifecycle, bridgehead victory and full Enemy result. |
| `assault_room_timer_victory_survives_every_seat_snapshot_and_result_delivery` | Extended with both operative join paths and activity; retains ordinary timer victory and every-seat result delivery. |
| `attached_elimination_rooms_run_lobby_and_live_operatives_through_victory_and_hold` | Added one parameterized regression using existing constructors and trusted attachment for Annihilation and Classic. Destroying the losing bunkers or HQ triggers the ordinary victory phase. No private finish state is forced. |

The shared hold checks reject fresh FPS input after victory without changing operative status. They assert 120 callbacks, 60 further ticks, 60 network snapshots per human and operative, stable identity/ammo/combat counters, operative private-field exclusions, no room fault and runtime release. Every game seat and operative stream is also captured before each hold callback. No Cargo, rustfmt or tests were executed for this repair. Formatting, compilation, execution and exact-head qualification remain with the parent; the earlier integration results do not validate these edits.

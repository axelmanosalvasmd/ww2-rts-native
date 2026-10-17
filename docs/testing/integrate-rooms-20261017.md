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

## Three final-run fixture corrections (source only)

Baseline: `7590780d19d33895fd7ec8917f5479948f39f37e`. Inspected the three raw failures in `/tmp/rooms-final-tests.log` and both relevant merge parents: #387 at `b4f212fb`, #393 at `77bfbe9e` and #398 at `8a2bba6f`. No runtime implementation or guard changes are needed for these failures.

- `intent_log45_nonconquest_point_supply_seam_refuses_before_mutation`: the raw failure at line 417 is `Some(UnsupportedMode)` versus `Some(Rules)` for Tutorial. #387 brought the Tutorial runtime check and its `Rules` expectation; #398 brought the earlier ordinary-finish admission guard. That guard calls `validate_ordinary_finish_state`, which rejects this deliberately incomplete Tutorial state before projection. Expect exactly `UnsupportedMode`, retaining the exact point-supply error, both supply settings, unchanged objectives and wallets, no tick fault, and Horde/World refusals. This remains a malformed-state seam, not an admitted Tutorial room. The prototype disables Tutorial supply during construction (`shared/sim.js:746`) and initializes Tutorial separately (`shared/sim.js:786`).
- `malformed_live_flight_refuses_before_delivery_memory_changes`: the raw failure at `flights_tests.rs:137` occurs in `DeliveryState::new`, before NaN injection. #387 brought full firing validation into snapshot preflight, while the retained fixture gave an on-map ground projectile `start_cell: usize::MAX`. The canonical validator permits that sentinel only for an off-map aircraft launch. Initialize the fixture's cell from its launch origin, matching the prototype's recorded launch cell (`shared/sim.js:4124-4133`). Keep delivery construction before NaN injection and retain the refusal and unchanged delivery/world assertions.
- `sandbox_live_flights_use_current_pose_exact_shape_and_recipient_visibility`: the raw failure at `shots.rs:825` differs only in zero representation. #393's test expected floating zeros from its separate projector. The retained canonical projector normalizes numeric zero to integer `0`, matching JavaScript JSON serialization. Change only the expected velocity's two zero components, gravity and launch time. Keep exact full-row equality, current-pose coordinates, future-launch omission, hidden-shooter exclusion, reveal identities and complete-frame equality. The prototype uses the same current-pose and recipient visibility rules (`shared/sim.js:5966-5995`).

Only these three test sources and appends to the two authorized fragments changed. The canonical flight projector, firing validators, snapshot admission and prototype files are unchanged. No Cargo, rustfmt, tests, commits, GitHub operations or delegation ran in this repair. The parent's already compiled test binary does not qualify these edits. Compilation, formatting and final execution remain with the parent. Changes are left unstaged and uncommitted.

## Completed rooms-chain validation, 2026-10-17

Qualified source commit: `5fa99e7b24e09a00c71a7a3c2a1bfbfcd78e9887`.
Qualified Rust tree: `36848727c7f8a51405395908c29bcaabc599be48`.
Machine: Debian cloud, 9 logical CPUs, 9.7 GiB RAM, no swap. Rust 1.99.0,
cargo-nextest 0.9.148. Source, Rust tree and Cargo.lock remained unchanged
throughout this complete qualification. Other heavy work was paused.

- `cargo test --locked --profile ci-test --no-run -j1 -p ww2-core -p ww2-server -p ww2-simtest`: passed.
- `cargo nextest run --locked --cargo-profile ci-test --profile ci -j4 -p ww2-core -p ww2-server -p ww2-simtest`, with the external resource configuration below: **1,918 passed, zero failures, 10 pre-existing ignored tests**, 687.002 seconds.
- `cargo fmt --all --check`: passed.
- `cargo clippy --locked --workspace --all-targets -- -D warnings`: passed.
- `cargo test --locked --workspace --profile ci-test --doc`: passed; all four crates contain zero runnable doctests.

The cloud-only nextest tool configuration, supplied with
`--tool-config-file cloud:<absolute-config-path>`, contains only:

```toml
[[profile.ci.overrides]]
filter = 'test(engineering_consumers::support_selected::) | test(movement_existing_checks::) | binary(trusted_room_process)'
threads-required = 'num-test-threads'
```

Large fixture and trusted-process tests reserve the four slots. Other tests run
four-way. No retries, timeouts, fail-fast, ignore or default-filter setting was
changed, and the repository CI configuration is unchanged. This avoids the
cloud's observed memory contention without filtering out any enabled tests.
Build jobs are serialized. Debug information and incremental compilation are
disabled; the named ci-test profile retains its normal assertions and checks.

Earlier compile SIGKILL attempts are not qualifications. The first completed
run at `46e6ca49e5a52dbc282e77283750c88fffc2e590` passed 1,915 of 1,918 tests
and found three failures in the newly added operative mode-lifecycle fixtures.
The shared ending helper started at 50 ms even though the host clock was due at
zero, executing two callbacks on its first poll. Polling from zero now retains
all 120 callbacks and all original 60-frame/60-tick/result assertions, and checks
the exact hold count and runtime presence on every callback.

The elimination fixture also threw a live grenade before its operative cleared
an adjacent steel bunker. Actual recipient contact events showed the immediate
blast killing that operative; indexing its original unit ID then failed. The
fixture now throws after its existing eight movement inputs in Annihilation and
Classic only. All original shot, throw, live-flight, ammunition, movement,
recipient, privacy and ending assertions remain, with an additional live-body
check after every exercise tick. Runtime combat, spawn, respawn, clock and
admission code are unchanged. These repairs and all three test names are listed
in `changelog.d/2026-10-17-integrate-rooms-lifecycle-fixtures.md`.

The preceding lockfile repair only normalized dependency ordering generated by
Cargo. It changed no dependency version, checksum or membership. All eight
exact PR heads listed above remain ancestors. This is chain qualification;
final combined-workspace and three-platform CI validation remain separate.

### Reference follow-up and diagnostic correction

The first diagnostic description above called the contact a steel bunker. That
was too specific: the native contact names allied unit 1, the first ordinary
starting rifle; the nearby bunker is a separate damaged unit. The reference
uses the material label `steel` for all unit-body contacts, so that label alone
does not identify a bunker. The relevant fixture issue is an immediate explosive
contact with a nearby friendly body. The timing repair and all passing assertions
are unchanged. The fixture comment will be clarified during final integration.

A small component diagnostic ran the unmodified pinned JavaScript modules at
`e4e579a838aabae3b1d9dadbc17a0a87147bc52d` under Node v24.19.0, separately from
the native repository's untouched reference submodule. It constructed a flat
64-by-64 map, placed a 20-HP operative at (20,20), placed one allied body at
(21,20), rebuilt the source grid, sent the initial zero grenade counter and then
counter one, and advanced the actual source `step` function. Both a bunker and
a rifle body produced terminal explosive contact at time
`0.10000000000000002`, owner 0, material `steel`, and lethal self-damage (HP -20,
operative unit ID reset to zero). This confirms the relevant contact/self-damage
rule; it is not an exact replay of the entire native Annihilation room and does
not replace the pinned fixture recorders' Node-version requirements.

Source locations: `shared/sim.js:2098-2105` (operative grenade launch),
`:4226-4248` (explosive contacts include allied bodies), `:4287-4316`
(terminal-contact blast), and `:4729-4744` (blast includes the thrower and allies).
No gameplay change or new owner decision is proposed.

The clock repair touches only this test helper. The production 50 ms interval
and multi-callback catch-up implementation are unchanged. Existing
`original_meter_windows_and_server_polling_loop` covers catch-up and
`final_hold_frame_precedes_result_and_old_tickets_survive_new_attachment` covers
last-frame retirement and ordering, separately. A combined multi-due-callback
poll ending in retirement is not explicitly covered by those two witnesses;
the final integration will extend the existing retirement test with that case,
retaining its original single-callback case and every assertion.

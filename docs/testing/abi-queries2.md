# ABI-QUERIES-2

This stream starts at #418 `1ce78e1231bf0bf4c1b825d447eb95e10251f64b` and stacks on #418 (#389). Reference remains `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. Owner message `julio-373-merged-decisions-20261016` authorizes additive queries and retains signature review before Unreal adoption. The later owner message `julio-merge-without-ack-20261017` ([6225924617](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6225924617)) removes exact-head Hermes acknowledgement as a merge gate; the orchestrator owns CI and integration, and this stream still stops after publication. Existing C declarations, scalar major 2, minor 1, layouts and error conventions remain unchanged; no function is added.

## Literal row mapping

IF-12, additional ordinary fairness and detached-preview witnesses through the public C entry points:

- `fairness_finishes_real_ai_matches_and_credits_actual_shuffled_starts`: completed matches, mirrored authored bias, winner/team/VP consistency and exact native replay.
- `fairness_moves_beyond_home_capture_range_before_winning`: no VP at tick 80, followed by a normal completed match that requires AI movement and repeats deterministically.
- `sixty_seeded_c_matches_reproduce_the_editor_unfair_verdict_by_authored_spawn`: seeds 7 through 66, 60 completed matches, actual shuffled-start attribution and the editor's percentage/verdict calculation. Expected source outcome is 60 wins at authored start 0, zero timeouts, percentages `[100,0]`, verdict `unfair`.
- `sixty_seeded_c_matches_reproduce_the_editor_fair_and_timeout_verdicts`: symmetric-map seeded verdict agrees with the source, and zero-tick batches preserve timeout precedence.
- `every_committed_map_builds_a_detached_conquest_preview_through_c`: every shipped map constructs detached ordinary setup with complete dimensions, terrain rows, players and units. Large results use the C capacity/retry path.
- `preview_exposes_classic_resources_and_horde_roles`: ready HQs, both resource marker kinds, shared defenders, internal Horde seat, offset gates, bunker and break clock.
- `preview_assault_and_annihilation_preserve_roles_forts_and_point_filtering`: defender starts/teams, forts, authored Assault clock, bunker counts and omitted VP-only points.
- `horde_preview_without_defenders_keeps_the_prototype_conquest_fallback`: unavailable Horde selection retains ordinary setup instead of throwing or admitting logistics.
- `fairness_all_editor_modes_and_logistics_cross_real_delivery_beats`: all five editor modes, requested logistics off/on, tech enabled, 80 ordinary ticks, every-seat delivery and exact replay.
- `authored_scenarios_setup_units_and_run_through_every_editor_mode_via_c`: every mode creates one additional authored defender at its exact position, then runs announcement/objective/reinforcement triggers through 80 healthy ticks and exact replay. The C result does not expose individual trigger effects; canonical scenario tests assert those effects separately.
- `horde_single_defender_endless_army_reaches_a_live_wave_through_c`: one defender, Endless catalog settings, 1,000 ticks, returned active wave 1 after the ordinary break, and replay.
- `horde_authored_terrain_damage_and_internal_seat_arrival_refusals_run_via_c`: scenario terrain damage remains deliverable, while internal Horde initial units and reinforcements return an error without changing output bytes.
- `logistics_fairness_finishes_a_real_match_and_replays_through_c`: physical logistics through a normal completed Conquest victory and replay.
- `authored_hq_destruction_reaches_classic_result_through_c`: authored structural HQ damage yields an actual elimination result and terminal snapshot.
- `authored_assault_clock_reaches_a_terminal_result_through_c`: a valid authored 300-second timer yields an actual Assault result and exact replay.
- `empty_scenario_migration_results_remain_valid_for_preview_and_match_via_c`: null/false/zero/empty-string scenario values use the same no-mission fallback as the migration owner, then validate, preview and tick successfully.

The prior twelve authoring witnesses, twelve placement witnesses and ABI/header/error checks from #418 remain registered. Additional C witnesses cover the five editor modes, logistics, explicit authored scenarios, a one-defender Endless Horde wave, terminal Classic structural damage, a terminal Assault clock, and forbidden internal Horde arrivals. World runtime remains deferred under the owner decision. Tutorial is outside the editor selector (`client/editor.js:43`). No parity status file is edited. Final row qualification depends on the committed-head checks recorded in the PR.

## Prototype comparison and execution

Direct Node v26.11.1/x64 execution on Vostro runs the unchanged pinned `playMatch`, including its full human AI schedule, using the test documents and seeds. No Source recorder, captured behavior fixture or provenance bundle is created. The three near-home `vp:5,mp:0` points yield a source win at tick 1760; the remote-point document at seed 77 yields a source win at tick 2032. Both credit the favored authored spawn. Finish clocks are diagnostics, not exact native assertions. Direct source lifecycle probes additionally confirmed Classic authored HQ destruction finishes at tick 4, the valid 300-second Assault clock finishes at tick 6000, and one-defender Endless Horde reaches active wave 1 at tick 1000. A first probe's 17-by-17 damage box exceeded the source's 256-cell limit; the corrected 16-by-16 authored box is used. Terrain damage alone does not kill a Horde bunker, so that C witness requires healthy running delivery rather than an invented terminal outcome.

The first Rust build diagnosed a missing QueueError conversion. The next focused run reached real matches and exposed the base's ShotsAndFlights refusal, and a stale gate assertion. Those failed runs do not qualify the final tree. Projectile/event delivery comes from #369, whose inherited claims remain with that PR. The PR records final committed-head checks; the complete merge record follows.

## Integration record

- Authoring implementation commit: `b184d7c3`.
- #369 source head: `161bbe3880f0ac06187c64bfe3ff2dab0856841c`.
- Merge commit: `06879f503fadefd8b1b4c119033eb95d80fcba4a`.
- The actual merge tree is `8bc5ea51a13ee2f06b2a257bfa2f7e82939ec361`, exactly matching `git merge-tree --write-tree b184d7c3 161bbe38`.
- Both automatic merges (`DESIGN.md` and `match_tutorial/tests.rs`) retained their parent changes. There were no conflicts, manual resolutions or hand-edited merge hunks. Incoming registrations and runtime changes remain inherited #369 work.

- Main `9e5bf12439cead3ede90fdf7e62b73fcb7ed1881` merged at `fc8ee37d7b2b478cbb5aa84edfe243304b86ab31`.
- This merge had exactly two conflict hunks. In `commander_tactics.rs`, retain the entire existing ground-army/front/medic-rule block and append main's `world_free` initialization before the shared unit loop. This preserves medic regrouping and garrison shelling from #349 and World free-unit collection from #401. In `core/src/lib.rs`, retain all four exports: `authoring_queries`, `recipient_placement`, `commander_field_policy`, `commander_world_policy`.
- The resolved main-merge tree `67c6451bf1e6600b442fb2ce69d12ecace41ae0f` differs from the automatic conflict tree only in those two files, solely by removing markers and retaining both alternatives. No other manual merge hunk exists. Chores, field-policy, field-assault and field-review test registrations are retained.
- Main then advanced to `47ecd597`. Merge `1bdf5b171476ffb8a59f5bb3d152783ef66c108e` has actual tree `a5b69f63c944bca1823afb8de991ad1c428fd943`, exactly equal to the automatic merge-tree result. It has no conflicts or manual hunks.
- Independent source review accepted the authoring increment and both main integrations at `1bdf5b17`. That review ran no tests and does not substitute for runtime qualification. Hermes' post-merge review remains welcome under the updated owner process.

## Complete runtime composition

Main `4a88afd8b1fd33f717532f8f31f1bc87bb8ca2b3` merged at `50229f7058b7d14265655cce303146d8b1209c58`, after own head `8fbc5667`. Actual tree `9533d20269e843ad8119942d6570b71c778ed43f` differs from the automatic conflict tree only in three files. `DESIGN.md` retains the complete ABI section and Horde core section. `recipient_snapshot.rs` retains both flights and modes module declarations. `commander_ingress.rs` retains both complete parent tests, `fire_at_and_compound_orders_remain_blocked_at_public_ingress` and `base_commands_remain_outside_public_r1_ingress`, including each original before/after tuple. Independent source inspection accepted these resolutions.

The earlier head `8fbc5667638a4fb79198a8ff9ed06d07ad99dedc`, Rust tree `8a53dff920b1513b7bdd89fa4dd454d1f1a9a57b`, passed 43 FFI tests, 45 focused core tests, formatting, strict workspace/all-target Clippy, release build and strict C11 link/replay (32 cases, 215 spots). This is historical producing-head evidence only. Later runtime composition requires fresh qualification.

### Logistics and scenarios, #419

Merge `b3107d46f5726076c9f1c8e6c07eea6e6a5ca147` joins `50229f70` and exact #419 `e4c0801df68f886c1f019c680f7e6da304d9cffa`. Actual tree: `8d53bdfde6bb03b93d2c885d8a83a84d5b2c0cae`. Automatic conflict tree: `974d9403c5e64b65d3913e522542d2afd99d45cd`. Exactly seven files have manual resolutions:

- `docs/contracts/README.md`: retain both appended ABI and editor/record links.
- `elimination_modes/tests.rs`: retain the incoming physical-logistics test and main's `pub(super)` visibility for the shared `ids` helper.
- `core/src/lib.rs`: retain main's bases test registration and four public exports, plus incoming logistics and production lifecycle registrations.
- `match_construction.rs`: retain the canonical Horde/ordinary mode validator and incoming shared depot-claim release method. Cancellation retains aviation removal and uses the shared release method for both elimination and World nodes, replacing the duplicate World-only loop.
- `match_recruitment.rs`: preserve current admitted aviation by omitting the incoming obsolete bound-room aircraft refusal.
- `recipient_snapshot.rs`: retain current projectile documentation, both flight and logistics modules, and both projections.
- `recipient_snapshot/shots.rs`: retain admitted aviation and add authored-scenario events instead of restoring the incoming aviation refusal.

All other changes are automatic. Inherited #419 behavior and rows remain attributed to that PR and its dependencies #408/#414/#386.

### Mode-aware AI, #420

Merge `94f8128d5126ac86a0e951d63afa6ca985eb8346` joins `b3107d46` and exact #420 `f0b947e3b4b73fa8ee8eef00a4e299462728db7c`. Actual tree: `287a72744c571389144e9b661aee70f3b349ed03`. Automatic conflict tree: `a9c7bdee851f8efa092e9ca65f269ef0ee79be5d`. Exactly sixteen files have manual resolutions:

- `commander_fortification.rs`: retain main's cardinal direction, complete visible footprint, three-cell gate gap, World ground ownership and gate/wall payment ordering. The resolved file equals the first parent.
- `commander_shopping_inputs_tests.rs`: retain incoming catalog-derived gate cost, command count/kind/parser assertions and main's geometry; remove duplicate spawn assignment. A follow-up retains the incoming literal test name used in its mapping.
- `commander_tactics.rs`: retain both Context/default/state sets. Use incoming mirrored observations with main's World construction, endangered-engineer and newly-busy exclusions. Main field chores see prior mirrored assignments. Incoming engineering repairs and holder/cover policy run as fallback when richer main field facts are absent, avoiding duplicate orders. Existing medic and house-shelling behavior remains.
- `human_commander_policy_tests.rs`: retain main's field test module and incoming garrison refusal regression. Automatic adaptive-delivery/logistics assertion changes remain incoming #420 behavior: `intent_ai28_delivered_stock_uses_public_two_decimal_precision` is replaced by `undelivered_logistics_refuses_before_publishing_human_observation`, since that direct authoritative producer no longer claims logistics; the detached recipient producer carries delivered stock.
- `match_commander.rs`: retain field/World defaults and incoming initial terrain, mine and ground-flight facts.
- `match_construction.rs`: keep one canonical mode validator and the shared depot-release method, removing the duplicate moved validator.
- `match_human_observation.rs`: recognize both authored-scenario and Horde events as non-hurt events.
- `match_tick.rs`: retain both event variants, the canonical Horde validator and #419's initialized logistics/supply path; do not restore the obsolete blanket logistics refusal.
- `recipient_snapshot.rs`: retain the logistics module alongside the incoming mode-aware cache.
- `recipient_snapshot/shots.rs`: retain both authored-scenario and Horde variants, conversions and validation/projection arms.
- `server/src/lib.rs`: retain editor/records and authored-map configuration fields, defaults and hub setup.
- `server/src/lobby.rs`: use incoming complete result names (including the internal Horde seat), then apply existing record handling. Retain records and authored-map storage, defaults and configuration. Native attachment uses Horde's extra seat while retaining current spectator behavior. The trusted roster retains current reconnect/spectator checks and the incoming Horde cap of five. Retain mode-options tests and hosted Horde module.
- `server/src/main.rs`: retain hosted Horde/private map help and the current ordinary mode-selected-logistics description.
- `server/src/shards.rs`: configure editor, records and authored maps.
- `simtest/tests/common/companion_tick.rs`: retain both scenario and Horde event JSON arms.
- `simtest/tests/match_tick_events.rs`: retain both event JSON arms and refuse both extra families in the unchanged four-family fixture's classification assertion.

All other changes are automatic. Inherited #420 behavior and rows remain with that PR and its dependencies, including #407 and #413. A separate follow-up retains detached delivered ground so main's field policy receives the same facts through the new recipient adapter. A focused test checks road under rubble, bridge base ground, later delivered changes, immutable old views and transactional refusal of malformed ground.

## Required qualification

Run on Vostro from `rust/`, with Rust 1.99.0, a shared S1/S2/S3 lock and matching target, `MemoryMax=6G`, `MemorySwapMax=0` and `-j4`: formatting check; all `ww2-ffi` tests (including generated header, unchanged layouts/signatures, buffer errors and C-entry fairness); focused commander, detached view, mode privacy, snapshots, Horde, scenario, logistics, save/map-save and production lifecycle tests; server ingress/mode/logistics/Horde/save/runtime tests and simulation event regressions; strict locked workspace/all-target Clippy; release static library build and strict C11 link/replay. No full workspace test run or Unreal adoption is claimed. The PR gives the actual producing head and results.

The first post-main qualification stopped at formatting: rustfmt required the four retained module exports in alphabetical order. The follow-up changes only that declaration order; all four exports and every test registration remain present. No test or Clippy result is claimed from that stopped attempt.


### Training integration repair

The expanded C-entry run failed on Classic's first logistics training queue with `production published state: InvalidState("training publication context")`. The fix carries exactly the ground-reserve hunks from #405 at `28c6a0f5dae8840462595474d8129f022d57b5dd`: remove the two blanket logistics-training refusals, prepare `initial_unit_reserves` before the fallible publication boundary, and insert the prepared reserves in the existing publication callback. Existing aircraft admission and every identity/rally check remain. The canonical `base21_logistics_training_stages_full_vehicle_reserves_before_fifo_publication` witness is retained once in the existing lifecycle module, checking vehicle reserve values, FIFO order and healthy every-seat delivery. This is a narrow source adoption, not a merge of #405's entire branch.

The same first build exposed an omitted `#[cfg(test)]` on the retained `logistics_support_tests` registration from the #419 conflict. The follow-up restores that attribute, preserving the module while removing test-helper warnings from production builds. Workspace rustfmt also normalizes the combined event-family assertion in `simtest/tests/match_tick_events.rs`; no assertion is removed.

Independent source-only review inspected every nonautomatic hunk of both runtime composition merges and accepted the current tree with its declared follow-ups. It confirmed exactly seven and sixteen changed files relative to the respective automatic conflict trees, all module registrations, event/privacy arms and both parents' setup/configuration semantics. This inspection ran no tests.

The repaired focused run passed nine C-entry cases (18 skipped) on the uncommitted follow-up tree. That run is diagnostic evidence, not final-head qualification. Final follow-up also centralizes the prototype's truthy explicit-scenario selection for validation, startup and shell creation, preventing null migration output from becoming an invalid mission.

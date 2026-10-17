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


### Final-head qualification diagnostics

Candidate `fd8d35d69e1b72140ad2fd8d6e2f03f9dee5e033` (Rust tree `568bc59324b77f33a9e37929ddab66e66aad9526`) passed formatting and all 52 FFI tests, including both 60-match verdict batches. Its core selection stopped at two genuine failures after 264 passes. These are producing-head receipts only, not qualification of a later repair.

`eng12_unrepresented_modes_and_logistics_cannot_publish_paid_or_waiting_gates` exposed an invalid Horde-shaped state admitted by the expanded actor predicate without an actual Horde owner. Gate admission now additionally requires canonical Horde validation before payment; the refusal witness remains unchanged. The other failure, `intent_ai50_gate_order_completes_in_each_real_ordinary_mode_with_supported_seat_snapshots`, still expected the old non-Conquest snapshot refusal. Its original payment, work, completion and post-completion assertions remain, and every supported ordinary mode now collects every-seat delivery throughout those phases. No test registration is removed.

Candidate `92922997a059636cd1b77a1e1664343598939b8a` (Rust tree `b6b57d0f4fceeccd541757801ba08e3fb408eb5e`) passed formatting, all 52 FFI tests and 349 core tests before stopping at the older `elimination_snapshot_admission_rejects_missing_state_and_other_modes` error-kind assertion. Both prior Gate failures and the Classic logistics reserve lifecycle passed. Snapshot admission correctly recognizes Horde and then refuses a relabeled Classic state for missing canonical Horde rules, rather than reporting an unsupported mode. The witness now expects that rules refusal, retains all unsupported-mode/tag assertions, and restores both Classic tags before its malformed/missing-elimination checks. The 66 remaining selected core tests and downstream checks did not run on that candidate.

Candidate `9c2d1f29d8acdef0176508ce2ac58497fcfbc393` (Rust tree `a7084f2fae3c2844a15aac14f6edcad9db4e9893`) passed formatting, all 52 FFI tests and 415 of 416 selected core tests. The remaining failure was `cmb127_horde_reveals_only_active_final_ground_enemies_and_bosses_to_defenders`, whose tail still expected Horde mode to be unsupported. Its direct visibility matrix is unchanged. The query-only plane remains intentionally ownerless and must fail the sortie guard and snapshot rules check. A separate canonical Horde setup now ends its break and runs 16 healthy active-wave ticks with every-seat event delivery. Server/simulation checks, Clippy and release/C linkage did not run on that candidate.

## Main integration at b43a4f98

Merge `41d57bf60ef73ac3a43aa7972b13b8ac3aeb4a9d` joins own `45d76ba10fd24c149c2078918ede1a24c0ee367e` and main `b43a4f986ce106c395422b04d3842c13641c2d20`. Actual tree: `85b3949f52b923b7db018e03c3c11e552ffeadc9`. Automatic conflict tree: `1a20343d1aaabec43a4b275022868694e92af8c5`. Exactly 28 files differ from that automatic tree: 23 conflicted files, two required source/test adaptations and three own contract/design/changelog updates. Every manual resolution follows.

- `DESIGN.md`, two hunks: retain the complete medic/FireAt sections and incoming external AI save section; retain the ABI section and incoming bounded World fort/shared placement decisions. No World admission is added.
- `docs/contracts/README.md`: retain the incoming combat-filtering link and existing additive ABI 2.1 candidate wording.
- `docs/contracts/save-format.md`, two hunks: retain both optional authored-owner rows and both complete full-v2/external-AI sections. Document that new maps use full v2, while timed v1 saves retain their original owner and clock checks.
- `authored_scenario.rs`, adaptation outside conflicts: add crate-private `matches_map`, comparing the private definition with the existing truthy explicit-scenario selector.
- `cmb5_ability_tests.rs`: retain main's stronger Classic owner-row/cooldown/empty-ghost assertions and World refusal, followed by the existing healthy Classic continuation.
- `cmb5_visibility_tests.rs`: retain one cache for every actual seat and main's explicit empty-ghost assertions. All automatically combined Classic delivery and our Horde visibility/lifecycle cases remain.
- `horde_commander.rs`: keep paid-Dig `is_digging` and main's numeric-safe intent comparison, path-generation handling and `OwnUnit` serialization. Add serde derives to retained `DeliveredPlan`/`DeliveredIntent`, required by the incoming commander save owner.
- `human_commander_policy_tests.rs`, two hunks: retain `field_live_tests` exactly once at its original registration; remove the incoming duplicate at EOF. AI12 remains registered.
- `legacy_scenario_runtime.rs`: retain full preflight plus incoming timed-owner validation and saved-clock checks. Reject every pair of legacy/full/timed owners. Add public `validate_authored_map_binding`, checking one exact authored definition or no truthy definition when no authored owner exists.
- `core/src/lib.rs`, two hunks: retain both authored modules, numeric-key helper, all four authoring/placement/field/World exports and existing production/privacy registrations.
- `map_save/tests.rs`: retain complete group validation, exact authored rifle position, three healthy ticks and every-seat snapshots. Replace the incoming obsolete group refusal with explicit full-owner/timed-owner selection and its Ready Say check through `AuthoredScenario`.
- `match_construction.rs`: use main's shared borrowed placement query with actual canonical Classic nodes in stable order. Preserve claimed-node filtering, visibility and reserved footprints, eight-metre selection and the existing transactional claim using `placement.node`. World construction remains guarded.
- `match_human_observation.rs`: retain `AuthoredScenario`, `Horde` and `AuthoredMission` as non-hurt events, with main's saved-observation additions.
- `match_runtime.rs`, four hunks: keep full-definition startup validation, both defaulted optional save fields, truthy explicit selection and legacy fallback. New maps create only the full owner and set timed owner to None; never apply the old timed-only group/localization limits to new maps.
- `match_tick.rs`, two hunks: retain all three event variants. Restored timed owners use their original stepper; other states use existing legacy/full steppers. Preflight exclusion prevents duplicate producers; existing phase/winner guards remain.
- `match_world.rs`: initialize both authored owners to None, retaining incoming bounded World behavior elsewhere.
- `recipient_snapshot.rs`, four hunks: retain admitted ordinary/Horde mode validation, elimination/Assault/Horde ownership checks and projectile validation in main's shared validator. Keep cache-build instrumentation, owned row reuse and the common cached/uncached privacy filter. Shared recipient construction retains falling, flights and logistics and does not mark delivered logistics missing. Keep production-state and combat-filter test modules. Preserve crate-visible `seen_by` for existing projectile adapters.
- `recipient_snapshot/public_state.rs`, two hunks: retain Classic/World remembered ghosts with main's borrowed iteration and existing Classic nodes. Full-scenario objectives are primary and public-only for full-map spectators; saved timed and legacy projections retain their fallback paths.
- `recipient_snapshot/shots.rs`, five hunks: retain full/timed/Horde/aviation variants, conversions and payload validation. Do not restore obsolete aviation refusal. All scenario families apply public-only spectator policy and existing seated side/team filtering; geometry and flight filtering remain.
- `server/src/native_startup.rs`: keep mode-specific StartPlan constructors, canonical mode-row validation and the Horde internal-seat path. Replace restrictive timed-only re-parsing with exact full-or-timed `validate_authored_map_binding`.
- `server/src/room_runtime.rs`: retain construction/production, scenario-recipient and incoming spectator-mission test modules, each under cfg(test).
- `server/src/spectator_mission_tests.rs`, adaptation outside conflicts: restored normal missions compare the full owner and explicitly exclude the timed owner. Preserve timed v1 save/load and stale-clock refusal through an explicitly installed compatibility owner, then four ticks with every-seat/watcher collection. Destruction now uses the full owner, an unknown action still refuses, and changed-map substitution still refuses. Existing exact public/private spectator assertions remain.
- `simtest/tests/common/companion_tick.rs`: retain full, timed and Horde event JSON arms.
- `simtest/tests/legacy_scenario_tick.rs`: retain timed-event JSON and explicit refusal of unexpected full/Horde fixture output.
- `simtest/tests/match_tick_events.rs`, two hunks: retain all event JSON arms and exclude all three scenario/Horde additions from the unchanged four-family fixture classification.
- `docs/contracts/abi-queries.md`: answer Hermes' exact message `axel-ack-julio-ack-merge-decision-20261017`: closed five-value reason domain, JSON-only preview lifetime, synchronous 36,000-tick limit and pending fresh Win64 MSVC/macOS arm64 builds. Unreal stays pinned at `9e618217` pending Axel.
- `design.d/2026-10-16-abi-queries2.md`: record single new-map scenario ownership, compatible timed saves, spectator privacy, cached row reuse and stable Classic node claims.
- `changelog.d/2026-10-16-abi-queries2.md`: list the exact rewritten tests and the consumer documentation answers.

All other main-merge changes are automatic. Incoming status sweeps are inherited main changes, not parity promotions by this stream. The attempted `45d76ba1` run acquired S3 and passed formatting, then was stopped during compilation to integrate refreshed main. No test/Clippy/link receipt is claimed from that canceled run. Own Cargo processes were verified stopped before resolving source.

Independent source review inspected all 28 manual files at `dcf84b66974a8ffa274c853c18d7b1c01274858f` and requested one repair: full-definition JSON equality ignored insertion order although no-English localized fallback observes it. Full-owner binding now compares ordered serialized definitions with explicit serialization failure refusal. The existing server startup admission test accepts its original no-English definition and refuses the reversed translation order. All other merge resolutions were accepted by that source pass. The queued qualification at `dcf84b66` was canceled before taking a slot; it produced no execution receipt.

Candidate `667a9fb3e6c65a4e6001dde3a8593349439478d2` (Rust tree `93969a6a9df4bc832c69e2f12bc4daf4011bf949`) passed formatting under S3's required caps, then failed compilation because #420 engineering policy state lacked the serde derives required by main's AI save owner, and main's pending Horde fixture omitted the delivered intent field. Add engineering serialization and defaults for engineering/armor fields absent in older non-null saves; initialize the inherited fixture. Existing policy-memory and engineering-repair tests now exercise nondefault memory and tuple-keyed crew continuation through the tagged codec. No tests, Clippy or link result is claimed from the stopped candidate.

Candidate `1d0dbb7b2975b3cff94044f804f3e661dea2c017` stopped at the formatting check: rustfmt required one assignment line wrap in the extended engineering repair witness. The formatter applied that wrap. No compilation, tests, Clippy or linkage ran in that attempt.

Candidate `c7c42cc6c9b088239b61977c056a3da1541a0032` (Rust tree `750b45782709d9285edd0e4d52b448c5f059fdf8`) passed formatting and compilation. The expanded 489-case core selection stopped after 303 passes and two failures (184 unrun), both old mode-startup expectations: Horde Endless must fail, and Horde snapshots must be unsupported. The repair retains every exact wallet/population/mode-row assertion, runs all army choices through three healthy ticks with every-seat snapshots for admitted editor modes, and retains Tutorial snapshot refusal. The engineering tuple-key crew restore witness passed on that candidate. FFI/server/simulation/Clippy/linkage stages did not run after the core stop.

### Final R1 integration boundary

Merge `44442aca85dc1fafea9bfcc9a8e3864e1bcbbcbb` joins accepted authoring head `1d445c6786dbaf31b0948ba105eafa7a8b79bd78` and main `f02e09e0549ae5e49b40b086d7f301706582ef51`. Actual tree: `64cd28373adf4251e808c7710891b4ddff5b4c47`; automatic conflict tree: `6507814e438ab0775de74e7a6a65d04d83836338`. Exactly one file differs from that automatic tree: `server/src/commander_ingress.rs`. Its single conflict retains the complete `fire_at_and_compound_orders_remain_blocked_at_public_ingress` witness, then incoming `conquest_production_cancel_is_refused_without_reserving_input`. Incoming `base_commands_reserve_canonical_input_before_mutating_state` replaces the obsolete `base_commands_remain_outside_public_r1_ingress`, since main now queues those commands. Its reservation, no-early-mutation, denial, completion and every-seat snapshot assertions remain unchanged. No new runtime behavior is invented by the resolution.

All other hunks are automatic, including main's cover/garrison fixes, raw selection limit, host controls, socket regressions, contract and parity status sweep. The status sweep is inherited main work. The focused qualification includes the incoming garrison unit tests and R1 socket module alongside existing authoring selections. The `1d445c67` run acquired S2 but was canceled before test execution to integrate this main update; it supplies no passing receipt. Read #5 replies `6227692315`, `6227711003` and owner result `6227998561`: the four ABI questions are answered, parent #418 has separate platform receipts, and ABI 2.1 adoption remains pending Axel. Those parent receipts do not qualify this stream's runtime head.

Candidate `b6dbc54bab970264313e45cfb4fb188b171717c9` (Rust tree `d63511097de916247f52a4d5372dae37fc7a143c`) passed formatting, all 499 selected core tests (663 skipped) and all 52 FFI tests (zero skipped). Server test compilation then found four non-exhaustive matches for main's `Output::Watcher`: trusted-room logistics, complete logistics, ordinary-mode hosting and Horde hosting. Those helpers receive seated connections, so the repair explicitly panics on spectator delivery, matching the existing save/load helper's privacy assertion. No runtime path changes. Server tests, simulation, Clippy and release linkage did not run after that compile failure.

Candidate `39285e78e6734df61367a5bd403121eef4625650` (Rust tree `150b815eae4d22fd3c4810f9f658678cb94e797e`) passed formatting, 499 core tests (663 skipped) and all 52 FFI tests (zero skipped). Server compilation passed; the 57-case selection stopped with 24 passes, one failure and 32 unrun. The failure was `mode11_horde_switches_and_shared_base_are_constructed_without_admitting_full_runner`, whose last two assertions still required Horde tick/runtime refusal. Its replacement `mode11_horde_switches_and_shared_base_survive_ticks_and_every_seat_delivery` retains every switch, player, HQ, owner and technology assertion, then runs one direct tick and the existing four-tick every-seat delivery helper. World admission refusal remains. The successor changes only this server test, changelog and documentation, so already-passing core/FFI sources remain byte-identical. Qualification resumes with formatting, the complete bounded server selection, simulation, strict Clippy and release C linkage.

Candidate `4a35d9eb443b591125e1cce45c0c454e3ee7b2d7` (Rust tree `957b2c6802b649018427ab144023d55c4135292b`) passed formatting and server compilation. All 57 selected server cases ran: 54 passed, three failed, 132 skipped. The repaired Horde options witness passed. Two save-envelope tests pinned alphabetical `Value` iteration order; `preserve_order` now returns insertion order. Their replacement compares the same complete sorted field sets and retains all content, authority, restoration and terminal lifecycle assertions. The Horde record witness still required public start refusal; it now retains every record key/count/army check and runs the admitted start through the existing every-seat delivery helper. No save shape, runtime code or ABI changes. Core/FFI trees remain identical to the passing `39285e78` receipts; simulation, Clippy and linkage remain unrun until the server selection passes.

Candidate `10c18aa5fd22475144a318b388dad94554331d8d` stopped at formatting because the pinned formatter condensed the new no-denial assertion onto one line. That format-only correction is applied; no tests or later checks ran on this candidate.

### Publication integration with main's command and operative owners

Merge `cb9e4ba82dd5a0e1c630a65323a9c345c42ca07b` joins qualified head `4a83ff288feec40af5e06e0d79911a8346fbec14` with main `9ade72a209a317f41cef6864c8d8d77a2ebe3a12`. Actual tree: `74b266957d359c3123b55cb4e6fbc594d868f8ff`; automatic conflict tree: `b39bcea52bcd092b6d853d041af51794a71b4017`. Exactly twelve files differ from the automatic tree, containing all manual resolutions:

- `DESIGN.md`: resolve the single blank-line conflict by retaining the existing section boundary.
- `commander_field_runtime_tests.rs`: retain complete existing Gate lifecycle delivery in every admitted ordinary mode. Incoming partial Classic/missing-capability and non-Conquest refusal branches are obsolete for this composed runtime; units/works and every-seat checks remain. The resolved file equals our parent.
- `core/src/lib.rs`: retain one `bases_runtime_construction_tests` registration at its existing location, keeping all incoming operative/companion registrations and all authoring modules.
- `mode_startup_tests.rs`: retain all exact mode rows, all army choices and complete every-seat snapshots. Also retain incoming Classic DeliveryState capture, seat accounting, mode-row and empty-ghost checks, retaining its exact three missing capabilities because this lower-level API is deliberately partial. Complete delivery remains covered separately. Tutorial refusal remains. Only the mode-row witness needs a mutable match for capture; the separate starting-bases witness retains its immutable binding.
- `recipient_snapshot.rs`: retain complete ordinary-mode documentation; add both operative modules. Join existing projectile validation with incoming operative flight and companion-state validation. Keep complete cached/uncached mode and projectile paths, flags, operative fields and privacy filtering.
- `recipient_snapshot/shots.rs`: retain Horde, aviation, full-scenario and timed-mission conversions. Add main's operative SmallArms validation without removing Horde Breath/BreathShell checks. Incoming operative shooter checks in other launch variants remain.
- `server/src/lib.rs`: retain static fallback and authenticated editor Extension. The fallback now extracts and forwards editor and Body into the existing five-argument map handler, correcting an automatic call-signature mismatch.
- `server/src/lobby.rs`: dispatch incoming operative whitelist first, then Horde handling and existing save/trusted/commander handling. Retain all mode/options/logistics/Horde registrations and incoming room-command tests once.
- `server/src/main.rs`: combine incoming static-root/room-capacity help/config with existing Horde/authored-map/logistics options, editor initialization and defaulted records settings.
- `server/src/shards.rs`: retain both editor/static owners and initialize both before Config moves into workers. Keep editor Extension on static fallback; pass editor and request Body through its helper.
- `server/src/shards/tests.rs`: retain both the editor fixture and incoming `static_files: None`.
- `server/src/trusted_room_lobby_tests.rs`: keep the existing seated-watcher panic wording. The resolved file equals our parent.

All other hunks are automatic. Static routing keeps existing authenticated map semantics; no new authorization rule is introduced. Operative events are copied before scheduled capture clears them, and saved commander identities publish after successful restoration. Main's public command whitelist is inherited unchanged, including its bounded production-cancellation refusal. Full scenario ownership and version 2, exact private map binding, timed-save compatibility, all ordinary modes and Horde remain. The subsequent test repair removes obsolete `fire_at_and_compound_orders_remain_blocked_at_public_ingress`; main's existing `compound_orders_remain_blocked_at_public_ingress` retains every nonmutation check, while its `fire_at_reserves_canonical_input_without_immediate_world_mutation` and FireAt lifecycle module protect its new admission.

Before this merge, `4a83ff28` passed formatting, all 61 selected server tests (128 skipped), seven simulation tests (267 skipped), strict locked workspace/all-target Clippy, release build and strict C11 link/replay (32 cases, 215 spots, zero differences). The core and FFI component trees exactly matched `39285e78`, where all 499 selected core and all 52 FFI tests passed. These are bounded parent receipts only; this publication merge requires fresh checks.

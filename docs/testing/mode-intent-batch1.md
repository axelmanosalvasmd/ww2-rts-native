# MODE intent batch 1

Baseline: main `40fe9e296b46060241693c6c627ecd615d4f8ea4`, prototype
`e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. This test-only branch starts
from main and imports no private Tutorial runtime or history.

Acknowledged [Axel's approval of rules 1 to 5](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6206861267).
Behavior checks exercise the native intent. Cheap exact comparisons reuse
retained prototype data. No recorder, fixture or provenance bundle is added.
The coordinator owns parity status updates, review, CI and merging.

## Covered rows

| Row | Test in core's consolidated `it` binary | Scope |
|---|---|---|
| MODE-24 | `mode_point_services::mode24_radio_accelerates_only_the_uncut_holding_team`; `mode_point_services::mode24_depot_reinforces_and_repairs_only_allies_within_home_radius`; `mode_point_services::mode24_all_named_shipped_maps_keep_their_point_kinds` | Radio decay for holders, allies, enemies, neutral and cut points. Infantry reinforcement and vehicle repair at owned/allied depots, including radius boundaries, HP, MP and story spending. All five named maps' point kinds. |
| MODE-32 | `mode_intent_batch1::mode32_assault_authoring_checks_ranges_sets_flags_and_shared_spawns`; retained `authoritative_maps::assault_authoring_errors_match_original_validator_messages` | Assault clock limits and types, defender sets, boolean flags, optional fields and the minimum two shared spawns. Existing exact validator errors remain checked. |
| MODE-72 | `mode_intent_batch1::mode72_home_choices_match_original_seeds_for_one_through_six_players` | Exact home indices, teams and spawn positions from the retained original geography fixture: five seeds, both sizes and 1-6 players, 60 settings. Home selection is a stable prefix independent of the eventual player count. |
| MODE-73 | `mode_intent_batch1::mode73_generated_world_keeps_local_defenders_out_of_player_seats` | Actual generated Huge and Massive matches, a regional base in every non-home region, exact guard rosters for all four observed region kinds, hold and auto-retreat controls, hidden local actor and alliances. The native local actor has no display-name field. |
| MODE-76 | `mode_intent_batch1::mode76_world_finish_checks_threshold_rivals_draw_and_capture_location` | Real finish preparation on both World sizes. Below/at the 60 percent goal, full ownership, last surviving nation, draw, one-team requirement, current capture selection and centre fallback. |

MODE-24 supplies explicit phase inputs for damage, ownership, cut state and
proximity. It retains every constructor actor and adds one vehicle through
the public unit initializer. It proves the point-service behavior, not an
entire capture, supply and recovery journey.

MODE-76 supplies explicit staged post-capture/post-elimination region,
wallet and complete surviving/removed unit facts to the canonical finish
owner. That owner is used and published by the region phase. It proves the
victory decision, not a full World tick or capture/elimination execution.
MODE-74's fort-transfer refusal and MODE-75's unsupported removal cases stay
outside this batch.

Prototype review inspected `shared/sim.js:589-593`, `:1164-1194`,
`:1335-1347`, `:5634-5636`, `:5648-5668` and
`shared/world-conquest.js:155-174` at the pin. MODE-72 uses the existing
`world-generation.json` exact oracle. No Source process was run.

## Deferred rows

All 83 assigned rows are accounted for: five covered and 78 NEXT. No row was
dropped from this batch after an unrepairable test failure. The old branch's
MODE-03 failure and Tutorial journey failures remain on their original refs;
their test bodies and runtime are not imported.

| NEXT rows | Remaining work |
|---|---|
| MODE-01, MODE-02, MODE-03, MODE-04, MODE-06, MODE-07, MODE-09, MODE-10, MODE-11, MODE-16, MODE-17, MODE-21, MODE-22 | Full mode constructors, public start and server setting lifecycles remain outside this test batch. MODE-03 needs new decisive lobby tests; MODE-09 lacks human-to-AI handover. Existing native purpose RNG does not reproduce the prototype's global creation draw order. |
| MODE-29, MODE-30, MODE-31 | Bunker behavior across supported contexts and full Assault execution need further work; the explicit late victory phase alone is not the ordinary Assault runner. |
| MODE-33, MODE-34, MODE-35, MODE-36, MODE-37, MODE-38, MODE-39, MODE-40 | Missing Annihilation and Classic runners and complete out-player consequences. |
| MODE-41, MODE-42, MODE-43, MODE-44, MODE-45, MODE-46, MODE-47, MODE-48, MODE-49, MODE-50, MODE-51, MODE-52, MODE-53, MODE-54, MODE-55, MODE-56 | Horde runtime and server lifecycle, including fresh map validation, waves, orders and records. |
| MODE-59, MODE-62, MODE-65, MODE-70 | Priority NEXT runtime batch. `Match::new_tutorial` is absent on main. Do not import private Tutorial ancestry to make their old witnesses compile. |
| MODE-57, MODE-58, MODE-60, MODE-64, MODE-66, MODE-68, MODE-69 | Tutorial entry, runtime and complete lesson progression remain separate. The earlier MODE-64 floor witness was non-decisive. |
| MODE-71, MODE-74, MODE-75, MODE-77 | World public creation, fort transfer, complete elimination effects and full recipient metadata remain separate. MODE-73 constructor coverage does not establish server seed/start behavior. |
| MODE-79, MODE-80, MODE-81, MODE-82, MODE-83, MODE-84, MODE-85, MODE-86 | General scenarios, contexts, conditions, actions, arrivals and recipient filtering remain beyond the existing legacy subset. |
| MODE-89, MODE-90, MODE-91, MODE-92, MODE-93, MODE-94, MODE-95, MODE-96 | Complete ending/server lifecycle, event wiring, all-mode sampling, restart, away propagation and snapshots need further qualification. |
| MODE-97, MODE-98, MODE-99, MODE-100, MODE-101, MODE-102, MODE-103 | Client presentation belongs to the Unreal stream. Native-only checks do not prove it. |

## Validation

Final qualification passes on the committed Rust contents: `cargo fmt --all --check`,
strict locked workspace/all-target Clippy, and all nine selected core `it`
checks (seven new tests, the retained exact validator check and test-registration
coverage), with retries disabled. The Vostro run used slot S3, four build
jobs, a 6 GiB memory cap, zero swap and its slot target. There was no full
workspace test run. Raw local logs remain under
`/tmp/mode-intent-batch1-20261016/`.

The first focused run had
eight passes and one authored assertion failure: the added MODE-73 faction
assertion incorrectly called numeric faction 0 Germany. Both pinned Source
and the native enum identify faction 0 as USA. The correction changes only
that test expectation. Final World checks also require all four observed
guard kinds and exercise the no-capture end-position fallback.

Strict Clippy also caught a copied index loop in the guard test. Iterating
with `enumerate()` resolves it without changing the behavior assertions.
The final nine-test run follows both corrections and passes completely.

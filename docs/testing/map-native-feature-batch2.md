# MAP native feature tests, second batch

This batch starts from main, not PR #292's private history. It adds tests and test registrations only. It does not change runtime behavior, parity statuses, the reference pin, wire formats, FFI, contracts or privacy. Public World admission stays off.

## Proven row coverage

The prototype paths below are relative to `reference/ww2-rts` at `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. Behavior tests exercise native intent. Data and pure query assertions use the prototype's literal values.

| Row | Test | Coverage and prototype anchor |
| --- | --- | --- |
| MAP-27 | `match_firing::map_intent_batch2::map27_queued_tank_crushes_hedge_and_damage_clears_burning_cover` | Authored hedge flags and 60 HP; an actual queued tank move crushes the hedge and emits one collapse and a cell log; independent damage clears it; ignition lasts 14 seconds. `shared/sim.js:78,99,175,5513`. |
| MAP-28 | `match_firing::map_intent_batch2::map28_queued_tank_crushes_wall_without_sight_or_movement_blocking` | Authored cover-only wall flags and 150 HP; queued movement and damage both replace it with a crater. `shared/sim.js:78,175,5513`. |
| MAP-30 | `match_firing::map_intent_batch2::map30_queued_tank_crushes_trench_with_directional_settled_cover` | Exact infantry speed and fresh, settling, dug and rear-facing cover; queued tank crossing replaces the trench with a crater. `shared/sim.js:60,175,2306-2309,2366,5513`; `test.js:560-569`. |
| MAP-35 | `match_firing::map_intent_batch2::map35_queued_tank_crushes_wire_that_slows_infantry`; `pathfinding::named_fine_search_branches_match_original_find_path` | Authored wire flags and 40 HP, exact 0.35 infantry speed, actual queued tank crossing and damage replacement. The existing pathfinding component tests retain wire-cost coverage. `shared/sim.js:68,78,175,2366,3160,5513`. |
| MAP-52 | `match_firing::map_intent_batch2::map52_firing_uses_elevation_accuracy_and_both_clamps`; `match_firing::map_intent_batch2::map52_named_house_sight_overrides_combine_with_height_and_rain` | Actual weapon launches across ten elevation pairs and 128 fixed seeds per pair, including both clamps; exact sight queries for all six initialized house kinds, three heights and three rain levels. `shared/sim.js:138-158,4162,4546-4548`; `test.js:664-677`. |
| MAP-33 | `match_firing::map_intent_batch2::map33_timed_bridge_collapse_after_setup_kills_occupants_and_logs_the_span`; `authoritative_maps::authoritative_maps_match_pinned_validation_and_static_terrain` | A real timed trigger destroys a three-cell stone span after Match setup. Both an infantry squad and a tank retire, structural sections fail, and water cell logs and collapse effects are emitted. `shared/sim.js:78,148,175`; `test.js:688-706`; Great Bridge timed trigger behavior. |
| MAP-94 | `map_world_batch2::map94_all_six_literal_world_fingerprints_and_river_counts` | All six literal SHA-256 row prefixes and main/tributary counts from the complete dressed generator, before setup. The local test-only hash helper is checked against three standard vectors. `shared/world-conquest.js:22-28,183-205`; `shared/world-landforms.js:71-125`; `docs/world-waterways-verification.md:73-74`. |
| MAP-107 | `lobby::tests::map107_lobby_map_selection_obeys_mode_rules` | Actual lobby commands and output cover all 33 maps in four ordinary modes, the 17-map Horde subset and fallback, forced Tutorial selection, and the World lobby sentinel. Public start/restart remain denied. This does not claim hosted World startup. `server.js:49-65,338-345,649-671`. |

The Match tests retain starting buildings and their canonical metadata. Added actors use the existing publication owner. A stationary tick precedes each crush move, and no test installs an expected post-movement pose or terrain result.

## Rows left open

These gaps are outside this batch's proof at integrated main base `09e03d07`.

| Row | Remaining gap |
| --- | --- |
| MAP-06 | Native spawn filtering and lobby capacity checks exist, but the exact invalid-minimum-spawn diagnostic differs from the prototype. No runtime diagnostic change is included. |
| MAP-15 | General authored scenarios remain unsupported by the map loader. Legacy timed triggers do not supply a general scenario validator. |
| MAP-19 | Structure validation exists, but native editor move, save, invalid-move rollback, remove and prune consumers are missing. |
| MAP-22 | The native server uses validated embedded assets. It has no corrupt-file lobby fallback versus strict file-start path. Embedded storage is a possible deliberate-difference candidate. |
| MAP-23 | Editor writes are refused. No validated-field save projection or 8 MiB save-endpoint cap is implemented. |
| MAP-24 | Committed public assets do not supply an arbitrary material-bearing map sanitizer. Catalogue-only assertions would not close the previous gap. |
| MAP-34 | The complete rubble-fill operation and initial World spoil-tip exclusion are not connected. |
| MAP-57 | Gameplay height queries do not prove the rendered-ground sampler or mesh behavior. |
| MAP-58 | Membership/grid lookup exists, but bounds fallback and half-open explored-run containment consumers are missing. |
| MAP-60 | Territory runs exist, but there is no native run-to-outline helper. |
| MAP-79 | Geometric home reserves do not prove actual barracks placement after World setup. World construction admission stays off. |
| MAP-90 | Rail masks, stations and yards exist, but placement does not consume station-yard exclusions. |
| MAP-92 | Native validation rejects malformed Worlds, but the complete exact diagnostic matrix still differs. No partial matrix is presented as proof. |
| MAP-100 | World memory and metadata components do not provide complete delivered snapshot invariance. Full World snapshots remain refused. |
| MAP-104 | The missing arbitrary material-bearing startup projection remains, as for MAP-24. Existing visible-cell delivery checks are not a substitute. |

MAP-59 and MAP-62 are excluded from this stream by owner decision, not failed rows.

## Validation

At producer `5330b63c98ad3e3829fe07b9f396be65cd3099a8`, Rust tree `50cd6a364299c0e41e9b33753f914656c0dba6d0`, all 12 focused checks pass: seven Match tests, the six-fingerprint test, the lobby matrix, the two retained component witnesses and the integration-test registration guard. Formatting and strict locked workspace/all-targets Clippy also pass. The jobs ran on the Vostro in S1 with a 6 GiB memory cap, no swap and four build jobs; the slot was released after exit 0.

The subsequent integration of main through `09e03d07` adds only tests and status documentation. Both owned core test files remain byte-identical to that producer. The lobby change since that producer is only an incoming `cfg(test)` NET module registration, outside the MAP-107 test. Final merged-tree formatting and strict locked workspace/all-targets Clippy pass on Rust tree `5eea2eb637c7de18b5d5d38eb980f75e6247829c`, after a fresh S1 package rebuild at `4f205f1bf6f129d8241b6e0fa0c4ddd684cb6754`. The slot was released after exit 0. The focused results remain attributed to the earlier producer, not relabeled as new executions.

Independent prototype-intent reviews accept all eight row mappings, including the retained wire-cost and static-bridge witnesses named above. Strict Clippy's earlier rejection of two constant-size `chunks_exact` calls was fixed with its suggested `as_chunks` form; all six fingerprints then passed again. The first shared-cache attempt selected zero tests and is not evidence. A clean rebuild then reached the new tests and exposed a test-authoring error: building tags used `type` instead of `kind`. The authored input was corrected without changing runtime code.

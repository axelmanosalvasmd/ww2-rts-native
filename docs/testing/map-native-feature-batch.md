# MAP intent test batch 1

This test-only batch starts from main `40fe9e29` and targets 17 of the 42 assigned MAP rows. It applies [Axel's approved native-intent rules](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6206861267). Behavior tests need prototype-intent review; tables, pure functions and retained component fixtures keep exact comparisons. Canonical parity statuses are left for the orchestrator's separate status PR.

Only owned native test additions from `eff82a91..0ad70e5d` are transferred, plus the native MAP-49 function from `eff82a91`. The old Source checker and inherited unpublished history are excluded. Existing main crushing/validation code and all retained fixtures and test assertions are preserved. The only addition to an existing runtime-source file is a `cfg(test)` module registration.

The prior reviewer accepted MAP-03/16/21/29/32/37/38/40/49/53/95/96/99/102 at producing `549b5a8c`. This batch closes three specific objections: MAP-01 now inspects actual Match spawns/objectives, MAP-51 checks solid distance falloff and both sides of the 60-degree corner boundary, and MAP-64 directly checks the floor operation used by native World generation. Source review of the three strengthened tests confirmed coverage after the spawn comparison was corrected to use float bits. Final PR review and status updates remain with the orchestrator. Historical results stay attributed to their original heads.

## Row and test map

Tests are registered through the existing consolidated integration binaries and the new test-only `match_firing::map_feature_tests` module. Names below omit module prefixes for readability. Prototype anchors refer to `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.

| Row | Tests | Prototype | Covered behavior |
| --- | --- | --- | --- |
| MAP-01 | `map01_cell_coordinates_centres_and_seeded_boundary_queries` | `shared/sim.js:26,723-727,2257-2260` | Actual default Match spawns and objectives, 2 m cells, row-major indexes and 10,000 deterministic boundary queries. |
| MAP-03 | `map03_off_map_flags_and_rays_block_all_four_edges` | `shared/sim.js:2261,2800-2808` | Off-map movement/sight flags and rays on all four edges, with a clear control. |
| MAP-16 | `map16_layer_composition_material_round_trip_and_invalid_stacks`; `authoritative_maps_match_pinned_validation_and_static_terrain` | `shared/world-layers.js:16-51` | Layer version, dimensions, symbols, composition, materials and dry/water placement. |
| MAP-21 | `map21_all_authored_moods_defaults_overrides_and_fog_lift`; `map_defaults_and_seeded_plans_match_original_javascript` | `shared/weather.js:31-50` | All 33 authored defaults, ordered fallbacks, overrides and fog lift at 240 seconds. |
| MAP-29 | `map29_crater_start_depth_quality_clamp_and_woods_sight_budget`; `map30_map51_trench_settle_facing_cover_wear_and_garrison`; `authoritative_maps_match_pinned_validation_and_static_terrain` | `shared/sim.js:2266-2269,2347-2348` | Initial crater wear, exact retained bits and cover quality through the 1.4 cap. |
| MAP-32 | `map32_map37_map38_authored_depth_road_presence_and_wet_speeds`; `map25_map47_every_character_wear_and_wetness_reach_ground_mul`; `infantry_naval_and_vehicle_paths_match_original_find_path`; `authoritative_maps_match_pinned_validation_and_static_terrain` | `shared/sim.js:175-180,2348,2363-2365` | Ford flags, initial depths, depth/wetness speed and boat-access routes. |
| MAP-37 | `map32_map37_map38_authored_depth_road_presence_and_wet_speeds`; `infantry_naval_and_vehicle_paths_match_original_find_path`; `authoritative_maps_match_pinned_validation_and_static_terrain` | `shared/sim.js:739,2368,3107,3158-3160` | Ground-layer roads, wear speed and retained routes distinguishing road costs. |
| MAP-38 | `map32_map37_map38_authored_depth_road_presence_and_wet_speeds`; `map25_map47_every_character_wear_and_wetness_reach_ground_mul`; `authoritative_maps_match_pinned_validation_and_static_terrain` | `shared/sim.js:85,2348,2363-2370` | Exact initial mud depth, speed endpoints and stronger below-zero wetness effect. |
| MAP-40 | `map27_map28_map35_map40_cover_flags_health_fire_and_destruction`; `map29_crater_start_depth_quality_clamp_and_woods_sight_budget`; `map25_map47_every_character_wear_and_wetness_reach_ground_mul` | `shared/sim.js:78-104,175,2268,2367,2806` | Woods cover, three/four-cell sight budget, vehicle speed, 200 HP, destruction and 20-second burning. |
| MAP-49 | `map49_complete_original_cliff_and_depression_checks` | `shared/sim.js:2851-2861,3153-3155; test.js:646-685` | One-level route control, two-level detour/drop refusal and eight diagonal-corner cases. |
| MAP-51 | `map30_map51_trench_settle_facing_cover_wear_and_garrison`; `map51_directional_solids_corners_live_hulls_and_settled_wrecks`; `map51_solid_distance_falloff_and_house_corner_sixty_degree_boundary` | `shared/sim.js:2262-2269,2300-2337` | Terrain, wear, health, trench, garrison, solid distance falloff, house corner arc, hull and wreck controls. |
| MAP-53 | `infantry_naval_and_vehicle_paths_match_original_find_path`; `named_fine_search_branches_match_original_find_path` | `shared/sim.js:3107,3153-3161` | Exact retained routes and search fields distinguish diagonal/uphill/wall/wire/fire/mine costs. |
| MAP-64 | `ranges_match_node`; `edge_cases_match_node`; `map64_floor_preserves_exact_world_thresholds_and_priority_boundaries` | `shared/world-landforms.js:7-14,85-96; shared/world-roads.js:113-117` | Retained exact math ranges/edges plus direct IEEE floor boundary bits, signed zeros, subnormals, large values and infinities. |
| MAP-95 | `map95_map96_world_installations_nodes_and_guard_fort_positions`; `original_world_startup` | `shared/sim.js:1165-1187` | Actual World HQ footprints, neutral bases, clearing/flattening, nodes and industrial fuel, backed by complete startup fixtures. |
| MAP-96 | `map95_map96_world_installations_nodes_and_guard_fort_positions`; `original_world_startup` | `shared/sim.js:1207-1223` | Distance-ranked guards plus complete retained unit/rotation/footprint/terrain comparisons for fort shares, facing, towers and walls. |
| MAP-99 | `map99_map100_map102_hidden_changes_memory_reconnect_and_node_station_filter`; `original_world_metadata_readers_preserve_privacy_and_optional_fields` | `shared/sim.js:1444-1448` | Remembered map/reconnect projection, unknown placeholders, allied spawns, no points and public metadata only. |
| MAP-102 | `map99_map100_map102_hidden_changes_memory_reconnect_and_node_station_filter`; `original_world_metadata_readers_preserve_privacy_and_optional_fields` | `shared/sim.js:1470-1474` | Known/hidden node and station filtering, remembered replacements and node end-reveal controls. |

## Dropped from this batch

Dropping a row from this PR does not change its inventory status. Partial assertions shared with a passing row do not qualify these rows. Main now contains moved crushing; this batch makes no fresh moved-crushing claim.

| Row | Reason |
| --- | --- |
| MAP-06 | No complete usable-spawn lobby-capacity/start-path test. |
| MAP-15 | Native authored scenario validation is unavailable. |
| MAP-19 | No complete authoring edit/save/invalid-move/remove/prune test. |
| MAP-22 | Native corrupt/missing-file lobby fallback and strict-start workflow are unavailable. |
| MAP-23 | Native validated save round trip and 8 MiB endpoint cap are unavailable. |
| MAP-24 | Committed public assets do not exercise a native arbitrary material-bearing sanitizer. |
| MAP-27 | Transferred HP/fire/destruction assertions do not exercise moved-tank hedge crushing. |
| MAP-28 | Transferred cover/destruction assertions do not exercise moved-tank wall crushing. |
| MAP-30 | Transferred speed/cover assertions do not exercise trench crushing to crater. |
| MAP-33 | Earlier test failed at tick 0 with production building missing; collapse assertions were unreached. Excluded, preserved on the old branch. |
| MAP-34 | Native rubble-clearing and initial-World-scenery fillability behavior lack complete tests. |
| MAP-35 | Transferred speed/path/destruction assertions do not exercise moved-tank wire crushing. |
| MAP-52 | Accuracy factor/clamps and named house sight override consumers remain untested. |
| MAP-57 | Native rendered-ground sampler and mesh consumers are unavailable. |
| MAP-58 | Bounds fallback and half-open client territory containment remain uncovered. |
| MAP-59 | Complete World footprint visibility/ownership/rail-yard construction admission is unavailable. |
| MAP-60 | Native territory outline helper is unavailable. |
| MAP-62 | Complete public World match seed/start workflow is unavailable. |
| MAP-79 | Terrain-only footprint counts do not test actual placement after World setup. |
| MAP-90 | Rail yard metadata is not consumed by native construction exclusion. |
| MAP-92 | No complete World validator regression matrix is imported in this batch. |
| MAP-94 | The six literal fixed-seed fingerprints/river counts have no matching native assertions. |
| MAP-100 | World metadata tests do not cover complete delivered ghosts/fires/strikes and hidden actor perturbations. |
| MAP-104 | Initial material-bearing public sanitization remains uncovered, as for MAP-24. |
| MAP-107 | Complete native mode-specific lobby/map selection is unavailable. |

## Validation

Final S2 qualification passes on Rust tree `0d8582b61a146a4a618ac90cbe3bf9525f4205b8`: all 29 selected tests pass, with 526 skipped; formatting and strict workspace all-targets Clippy pass. The process exits 0 and releases S2. The initial integration run passed 29 tests. A later S1 run reused a core integration binary without the new floor test and ran only 28; that incomplete inventory is not final qualification. Its strict workspace Clippy found one byte-character-array style error, corrected to an equivalent byte string before the clean rebuild. Those logs are retained separately. Per-package cleanup in S2 then discovered 26 tests, including floor but omitting the three cover checks. Refreshing the three test-binary entrypoint timestamps forced all executables to rebuild from this checkout and restored the complete 29-test inventory; no source contents changed for that repair.

The final selection covers the complete terrain component module, all three test-only cover checks, exact math ranges/edges/floor, authored-map and weather fixtures, both retained pathfinding fixtures, compiled rules, World startup, memory and recipient metadata. Cargo uses a shared locked slot with a 6 GiB memory limit, no swap and four build jobs. No Source recorder or prototype execution occurs.


Final commands: `cargo +1.99.0 fmt --all --check --manifest-path rust/Cargo.toml`; focused nextest with locked dependencies, `ci-test`/`ci` profiles, core/simtest lib and consolidated `it` binaries, four build jobs, one test thread and zero retries; `cargo +1.99.0 clippy --manifest-path rust/Cargo.toml --locked --workspace --all-targets --jobs 4 -- -D warnings`.

Logs remain at `/tmp/ww2-map-intent-batch1-20261016/`: `check.log` is the earlier integration pass, `final-check.log` the incomplete 28-test run and initial Clippy failure, `rebuild-check.log` the incomplete 26-test inventory, and `force-check.log` the complete final pass. These are ordinary native validation logs, with no Source recording or fixture generation.

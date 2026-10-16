# World components

This batch proves detached World components against prototype `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. It starts from native main `b6d05874`. World room admission, full logistics ticks and complete logistics snapshots remain guarded.

| Row | Check | Scope |
| --- | --- | --- |
| MAP-59 | `map59_known_owned_visible_footprints_equal_prototype_placement_check` | Fourteen original `placementCheck` results cover sparse explored runs, unknown cells inside broad bounds, mixed and allied footprint ownership, requested-point and per-cell sight, explicit empty runs, legacy bounds and rail denial precedence. |
| MAP-79 | `map79_home_selection_matches_pinned_pure_generator_values`; `map79_generated_homes_have_twenty_real_barracks_sites_and_fair_spread` | Ten existing exact pure generator cases check home selection. Six Huge/Massive settings exercise actual native HQ/troop startup and count usable Barracks sites, nearby neutral expansion and the maximum five-site spread. |
| MAP-90 | `map90_setup_bits_stations_bridges_and_exclusion_queries_equal_prototype` | Three original `setupRails`/`railYard` cases compare ordered track masks, stations, bridge lists, station-map replacement and signed yard cells. Single-cell and adjacent-cell exclusion queries cover every cell of each 20 by 20 input. |
| BASE-46 | `base46_detached_regional_bases_are_neutral_and_release_capture_locks_on_death` | Exact catalog values and build-list exclusion, then actual native World startup and detached capture phases. Every neutral region has a defender-owned, finished 3 by 3 regional base; a live base locks capture and its death releases the lock. |
| LOG-26 | `log26_world_graph_adjacency_damage_and_version_cache_match_original_js` | Five graph cases and nine cache states compare ordered edges and float32 damage bits, including cardinal borders, blockers, slope limits, authored rubble/fords and terrain-version invalidation. |
| LOG-27 | `log27_world_usable_regions_sources_and_f32_rates_match_original_js` | Fifteen original pure rate cases compare team order and every float32 rate bit, including neutral and retreating armed occupants, allies, sources, blocked intermediates, damage, unreachable regions, minimum rate and stable cheapest-region scanning. |

The new fixture JSON contains pure function inputs and expected outputs. The original functions ran locally with Node v26.11.1 on Linux x64 against the verified pin. `world-components.json` uses the original `setupRails`, exported `railYard` and exported `placementCheck`, plus the original `worldbase` table and build lists. `world-supply-components.json` uses the original graph, supply, source, cell and fill predicates. Rust tests read committed JSON and require neither Node nor a checked-out prototype. The home-selection fixture already existed on main.

Source review anchors are `shared/sim.js:357,1005-1019,1123-1134,1165-1208,1228-1250,3261-3307,5027-5081`, `shared/world-conquest.js:155-174`, and `test-world-acceptance.js:327-365`. Regional-base capture is an intent test, without a new behavior recording.

The supply graph and rate state remain caller-owned detached values. The placement query preserves optional explored runs explicitly and does not dispatch a construction command. No new state enters ordinary matches. These tests do not prove World room hosting, a running World match, complete logistics ticking, recipient delivery or a public construction command.

Local qualification on Vostro: the focused `ww2-core` run selected seven row checks and the integration-test registration guard. All eight passed with `-j4`, the `ci-test` profile, a locked S2 target directory, a 6 GiB memory limit and zero swap. The two fixture files are pure component data; no behavior recorder or full workspace test run was added.

After fetching origin and merging current main (already up to date at `b6d05874`), `cargo fmt --all --check` and `cargo clippy --locked --workspace --all-targets -j4 -- -D warnings` both passed under S2 with the same memory and swap limits. The tested Rust tree is `e461de0a397ec429e19718e7de3b7a59385b7046`. The final documentation update preserves that tree.

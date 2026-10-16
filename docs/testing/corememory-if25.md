# CORE-MEMORY-ORDER: IF-25 consumer inventory

IF-25 remains incomplete. This pass inventories all 60 prototype sort expressions: 23 simulation, 28 AI and 9 supply. Nine expressions belong to World and are excluded by the standing admission boundary. The other 51 include ordinary planners that still need implementation and consumers whose tie behavior has source inspection but no direct test. A shared comparator test does not complete those consumers.

The prototype inspected here is `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. Initial inspection used the read-only source in `t3-29229094/reference/ww2-rts`, whose HEAD matches that pin. The stream worktree now has the same pinned submodule initialized locally. Native paths below are relative to `rust/crates/core/`. Existing comparator/key work comes from PR #392 at `084cbe3d9344e9687a903c001333d8e289d67e97`; its earlier run is historical evidence, not execution of this change.

Acknowledged: issue #5's IF-25 requests for per-consumer evidence, especially tactics regroup ties, and owner confirmation `axel-owner-confirm-a-to-p-20261016`. World remains excluded. This pass changes no interface, admission guard or parity status. The agreed ABI boundary stays unchanged.

## Comparison policy and evidence labels

`src/sort.rs` retains the existing V8 TimSort and its exact comparison sequence. Numeric differences below zero sort first, differences above zero sort last, and either zero sign or NaN compares equal. Owned-value sorting uses an index permutation with that same comparator sequence. `first_by` sorts before choosing its first result. `then` implements the prototype's numeric `a || b`, including NaN falling through to `b`. This follows the existing decision in `DESIGN.md` under "Sorts follow V8 exactly".

The existing `src/sort/if25_tests.rs` covers signed zero, NaN, infinities, chained keys, sorted-first selection and a 96-entry merge. These tests establish comparator behavior. They do not establish every planner's input order or eligibility rules. Its finite fixture asserts zero NaN comparisons only for that fixture. There is no global NaN counter or blanket no-NaN assertion: legitimate Infinity-minus-Infinity comparisons can occur in other AI scans.

In the tables, **direct** identifies a test that checks the named consumer's ties or input rejection. **Source** means the actual native comparator was inspected but a direct tie witness remains pending. **Missing** identifies an ordinary runtime consumer still absent. **World** identifies a concrete excluded branch. A test name is a source mapping until the parent records its execution result.

Distance-key consumers admitted from finite, bounded map positions cannot produce a NaN difference. AI `Observation::new` rejects nonfinite coordinates and weights before planning. Supply component APIs also permit tests of internal values that an ordinary hosted match does not generate. Those NaN tests retain the prototype's equal-comparison semantics and do not claim that malformed hosted state is admitted.

## Simulation: 23 expressions

All prototype lines in this table are in `shared/sim.js`.

| Line | Native consumer and tie rule | Evidence or remaining work |
| --- | --- | --- |
| 509 | `src/field_projects.rs::entrench_plan`: distance then original segment index | Direct: `if25_sim_entrench_sort_matches_original_ties_and_rejects_nan` |
| 1209 | `src/match_world.rs`: nearest home for neutral region | World: neutral-region startup and World admission |
| 1210 | `src/match_world.rs`: descending neutral-region distance | World: guard fortification setup |
| 1256 | `src/match_world_rails.rs::WorldRails::route`: descending queue cost, then pop | World: rail routing; existing component `if25_rail_route_equal_cost_pops_last_stable_todo_entry` is retained |
| 1697 | `src/elimination_modes.rs`: enemy spawns by distance, retaining player order | Source: direct equal-distance node-pair setup witness pending |
| 1699 | `src/elimination_modes.rs`: ascending two-seat pair | Source: integer min/max is equivalent; no float or NaN |
| 1727 | `src/match_ground_orders.rs`: convoy retreat home from active source insertion order | Source: ordinary Match wrapper tie witness pending; supply retreat provider tested separately below |
| 1728 | `src/match_reactions.rs::ground_home`: nearest own finished HQ, strict smaller-distance scan | Source: finite distances preserve first tie; direct HQ tie witness pending |
| 1997 | `src/match_withdrawal.rs`: nearby stores retain store insertion order | Source: direct withdrawal-store tie witness pending |
| 2460 | `src/match_garrison.rs::garrison_entry_cell`: stable distance in breadth-first edge order | Direct: `if25_garrison_equal_distance_keeps_breadth_first_edge_order_through_match`, including ordinary ticks and every-seat snapshots |
| 2622 | `src/environment_lifecycle.rs::sync_building_sections_emitting`: equal distance retains authored section order | Direct: new `if25_building_loss_equal_distance_keeps_authored_section_order`, reversing the canonical section sequence and checking the first actual damage recipient |
| 3283 | `src/match_construction.rs::execute_build` explicitly refuses Depot placement | Missing: ordinary unclaimed-node selection within 8 m, plus its canonical placement provider |
| 3533 | `src/match_carriers.rs::carrier_board_command`: distance retains selected squad/carrier Cartesian-product order | Direct: new `if25_boarding_equal_distance_uses_selected_squad_and_carrier_order`, four input permutations and nonfinite-input refusal before command mutation |
| 3549 | `src/match_world_rails.rs::apply_rail_order`: nearest origin station | World: rail order admission |
| 3551 | `src/match_world_rails.rs::apply_rail_order`: destination station by clicked distance | World: rail order admission |
| 3559 | `src/match_world_rails.rs::apply_rail_order`: passenger distance and car capacity | World: rail boarding admission |
| 3809 | `src/production.rs::prepare_queue_unit`: strict shorter integer queue scan retains producer input order | Direct: new `if25_production_shortest_queue_keeps_producer_input_order_on_ties`, both producer orders and three consecutive purchases |
| 3869 | `src/match_abilities.rs`: nearest covered grenade target in candidate order | Source: direct equal-distance autocast target witness pending |
| 4032 | `src/command_destinations.rs::end_spots`: projected lateral position | Source: existing destination/formation fixture coverage does not establish a dedicated lateral tie witness |
| 4736 | `src/match_combat.rs`: nearest surviving structural section in authored order | Source: direct equal-distance contact-retarget witness pending; section-loss test above exercises a separate call site |
| 4791 | `src/environment.rs`: descending building-cell blast weight | Source: direct equal-weight blast-cell witness pending; section-loss test above exercises a separate call site |
| 5801 | `src/elimination_modes.rs`: reuses the teammate vector after each unit's stable distance sort | Direct: `if25_elimination_reuses_teammate_sort_order_for_later_equal_distance_transfer`; its Annihilation ticks do not claim ordinary recipient support |
| 5859 | `src/observed_memory.rs` and `src/world.rs`: journal/pending cell indices use integer ordering | Source: integer keys cannot yield NaN; direct out-of-order pending-index witness pending |

The simulation count is 6 World expressions and 17 ordinary expressions. One ordinary consumer is missing. Of the other 16, six have direct tie/admission mappings in this inventory; ten retain explicit source-only limits.

## AI: 28 expressions

All prototype lines in this table are in `shared/ai.js`. An absent planner is future runtime work, not a dropped row. AI66 separately owns the Classic construction addition and its tests in this shared batch.

| Line | Native consumer and tie rule | Evidence or remaining work |
| --- | --- | --- |
| 45 | Remembered bridge-cell ordering | Missing: bridge-memory planner and sorted newly delivered cell ingestion |
| 65 | Nearest safe remembered blown bridge | Missing: bridge-rebuild planner |
| 72 | Nearest idle bridge crew | Missing: bridge-rebuild crew selection |
| 113 | Nearest idle fill crew | Missing: spoiled-ground fill planner; repeated sorts must retain earlier ordering |
| 132 | Nearest idle mine-clear crew | Missing: detected-mine planner; repeated sorts must retain earlier ordering |
| 316 | `src/commander_classic_rules.rs::targets`; Horde's observed bunker selector | Source: stable spawn distance; direct equal-distance target-list witness pending |
| 367 | `src/commander_tactics.rs`: held-threat distance then explicit ID fallback | Source: actual `first_by` and `then` inspected; direct held-threat tie witness pending |
| 450 | `src/commander_tactics.rs`: visible enemy vector traversal retains observation sequence | Source: native radius filtering uses the delivered sequence; direct multi-target traversal witness pending |
| 555 | `src/commander_classic_rules.rs`: unknown enemy-spawn recon | Source: `first_by` by spawn distance; direct tie witness pending |
| 562 | `src/commander_classic_rules.rs`: eligible known depot by spawn distance | Source: `first_by`; direct tie witness pending |
| 564 | `src/commander_classic_rules.rs`: armored-car rank then depot distance | Source: stable `then`, no extra ID fallback; direct raider tie witness pending |
| 579 | `src/commander_tactics.rs`: guard responders by distance | Direct: new `if25_ai_guard_dispatch_keeps_equal_distance_observation_order`, retaining tied responders and their value budget when an existing order suppresses reissue |
| 596 | Aircraft attack target nearest the front | Missing: dedicated aircraft attack planner |
| 599 | Fighter target nearest the front | Missing: dedicated fighter/intercept planner |
| 634 | Rail-hop destination | World: own-station rail planner |
| 686 | Region movement target | World: World region/base planner |
| 701 | Known building inside locked target region | World: World region/base planner |
| 726 | Enemy spawn nearest held point | Missing: point-holder fortification planner |
| 756 | `src/commander_tactics.rs`: reuses stable target list across attacking units | Source: array reuse inspected; direct later-equal-distance target witness pending |
| 884 | `src/commander_construction.rs`: free builder distance from spawn | Direct: `if25_construction_builder_ties_keep_available_order_and_repairs_keep_observation_order` |
| 888 | `src/commander_construction.rs`: unfinished rank then builder distance | Direct: same test checks repairs in valid canonical observation order |
| 921 | `src/commander_fortification.rs`: enemy-spawn distance from each HQ | Source: `first_by`; direct HQ-threat tie witness pending |
| 926 | `src/commander_fortification.rs`: crew distance from each HQ | Source: crew array is reused; direct later-HQ tie witness pending |
| 973 | `src/commander_tactics.rs` and `src/commander_ai.rs`: minimum integer research level in authored line order | Source: stable minimum preserves first integer tie; direct equal-level line witness pending |
| 992 | `src/commander_forward_hq.rs`: HQ distance from army front | Source: `first_by`; existing builder test does not prove HQ ties |
| 1000 | `src/commander_forward_hq.rs`: available builder distance from site | Direct: `if25_forward_hq_builder_ties_keep_available_order` |
| 1024 | `src/commander_construction.rs`: Classic repair distance from each engineer | Direct AI66 addition: `if25_classic_repair_ties_keep_observation_order_for_each_engineer`, reversing available engineer order independently of equal-distance building observation order |
| 1027 | Richness-adjusted Classic node score, retaining engineer/node assignment memory | Missing: Classic Depot-node planner |

The AI count is 3 World expressions and 25 ordinary expressions. Nine ordinary consumer sites remain absent after the Classic repair addition. The remaining sites still need the specific direct witnesses labeled above before the non-World inventory can be called complete.

## Supply: 9 expressions

All prototype lines in this table are in `shared/supply-convoys.js`; all native functions are in `src/supply_convoys.rs`. Direct tests are in its already-registered `if25_tests` child module. These are provider tests, not full logistics-enabled Match admission claims.

| Line | Native consumer and tie rule | Direct test mapping |
| --- | --- | --- |
| 92 | `sources`: own-owner rank, then store insertion order | New `if25_convoy_sources_rank_owner_before_preserving_insertion_ties`; excludes inactive, non-source and enemy stores |
| 196 | `local_service`: nearby-store distance before sequential stock use | New `if25_convoy_local_service_equal_distance_consumes_first_inserted_store`, both insertion orders; NaN distance fails the radius filter before sorting |
| 246 | `targets`: provisions before troop grouping | `if25_convoy_job_comparators_preserve_zero_ties_and_nan_order`, signed-zero and mixed-NaN identities |
| 265 | `targets`: final priority | Same test checks emitted job identities after the actual final sort |
| 272 | `origins`: eligible source/store distance | `if25_convoy_origins_keep_insertion_ties_and_nan_distance` |
| 314 | `finish`: fallback sources, own-owner rank followed by stable distance | New `if25_convoy_return_refresh_and_retreat_share_stable_source_ties`, both own-source insertion orders after an earlier allied source |
| 345 | `deliver`: member provisions before sequential cargo consumption | New `if25_convoy_delivery_sorts_members_by_provisions_and_keeps_signed_zero_ties`, both member orders and an earlier richer member |
| 478 | `refresh`: fallback source when origin disappears | New return/refresh/retreat test, checking actual origin, destination and returning state |
| 616 | `prepare_movement`: retreat-source route preflight | New return/refresh/retreat test, checking the actual route target and manual transition |

## Validation and handoff

Eight new tests are added in existing registered files. No test registration change is needed. The focused selection is `test(if25_)` in `ww2-core`'s library and `it` binaries. Parent-owned Cargo qualification must record the producing head and results; this subagent has run no Cargo, formatter or Source recorder.

The test set protects ordering already used by the runtime and does not add runtime fields or commands. The existing ordinary garrison lifecycle supplies tick/snapshot delivery evidence for that consumer. Provider tests do not substitute for the missing planners or their later full-match proof.

NEXT: implement the nine ordinary AI sites above and Depot placement; add the remaining simulation and AI consumer witnesses. IF-25 is neither PROVEN nor DROPPED by this pass. World expressions stay outside this batch.

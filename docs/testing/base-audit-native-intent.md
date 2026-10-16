# BASE audit native intent tests

The complete BASE rows are BASE-04, BASE-06, BASE-08, BASE-13 and BASE-16.

This batch starts from main `40fe9e296b46060241693c6c627ecd615d4f8ea4` and extracts only this stream's tests and registrations from `9aaf434a..2fc91ae3`. [Axel's approved native intent rules](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6206861267) govern behavior coverage. Existing exact tables and component fixtures remain unchanged.

The table accounts for the 16 BASE assigned rows. A partial passing test does not qualify a complete row. Parity status updates belong to the coordinator after this test PR merges.

| Row | Literal native test mapping | Prototype reference at e4e579a | Scope and remaining work |
|---|---|---|---|
| `BASE-01` | `economy_state::income_and_population_match_original_javascript_traces` | `shared/sim.js:723`, `:1177`, `:1682`, `:6017`, `:6019` | Held: mode startup and owner snapshot omission/rounding are not exercised. |
| `BASE-02` | `base_audit_features::feature_base02_03_conquest_starting_wallet_and_caps_scale_all_named_armies` | `shared/sim.js:29`, `:31-35`, `:793`, `:837`, `:845`, `:1177`, `:1682`, `:802` | Held: actual starting wallets for Classic, World, Assault, Horde and Annihilation remain unrepresented here. |
| `BASE-03` | `base_audit_features::feature_base02_03_conquest_starting_wallet_and_caps_scale_all_named_armies`; `economy_state::income_and_population_match_original_javascript_traces` | `shared/sim.js:66`, `:801-802`, `:545` | Held: unknown army-string fallback and all-mode startup are outside this typed component. |
| `BASE-04` | `base_audit_features::feature_base04_conquest_points_catchup_and_absent_players`; `economy_state::income_and_population_match_original_javascript_traces` | `shared/sim.js:31-35`, `:49`, `:727`, `:5728-5753` | Covered: point income, catch-up cap, VP and away/out behavior. Exact arithmetic retains the existing fixture test. |
| `BASE-05` | `objectives::supply_masks_flood_order_and_cut_flags_match_source` | `shared/sim.js:122-129`, `:746`, `:4986-5019`, `:5655`, `:5731`, `:5746` | Held: exact point flood exists; full native tick scheduling and all construction/Horde context exclusions are not covered by this batch. |
| `BASE-06` | `base_audit_features::feature_base06_classic_depot_completion_destruction_and_currency_income`; `economy_state::income_and_population_match_original_javascript_traces` | `shared/sim.js:164`, `:5734-5747` | Covered: current depot facts pay only after completion and cease after destruction or ownership loss; allied uncut points, currencies and out guard covered. Classic constructor remains guarded. |
| `BASE-08` | `base_audit_features::feature_base08_world_region_loss_removes_income_and_depot_payment`; `economy_state::income_and_population_match_original_javascript_traces` | `shared/sim.js:1348-1357`; `shared/world-conquest.js:7-18` | Covered: region loss removes passive income and owned depot payment through current facts. No World logistics graph claim. |
| `BASE-09` | None, held | `shared/sim.js:64`, `:5695-5697`, `:3760-3765` | Held: no native test in this batch covers every eligible-killer/death/scuttle clause. |
| `BASE-10` | None, held | `shared/sim.js:1688-1722` | Held: Classic resource-node generation is not exposed by the supported constructor. |
| `BASE-12` | None, held | `shared/sim.js:3281-3302`, `:3704`, `:1105`, `:3759` | Held: native node claiming, unseen occupancy, cancellation and destruction are not represented by this batch. |
| `BASE-13` | `base_audit_features::feature_base13_conquest_population_two_accepts_two_then_refuses_without_debit`; `economy::prices_factions_population_builders_and_support_match_javascript`; `production::base_production_existing_runtime_predicates_match_source`; `production::production_research_and_refunds_match_original_source_traces` | `shared/sim.js:383`, `:391` | Covered: all 47 unit prices across ten mode contexts compare exactly; real Classic/World queue purchases and ordinary purchases debit those prices. This row does not claim a native recruit-card UI. |
| `BASE-14` | `economy::ability_costs_match_reference_objects_and_nullish_overrides`; `economy::prices_factions_population_builders_and_support_match_javascript` | `shared/sim.js:515-529`, `:549`, `:1787`, `:3614`, `:3681-3685`, `:5419` | Held: exact price queries do not prove thrown-time ability payment, autocast defaults or live support debit. |
| `BASE-16` | `base_audit_features::feature_base16_world_capacity_loss_and_active_seat_split`; `economy_state::income_and_population_match_original_javascript_traces`; `economy_state::world_caps_split_active_seats_and_preview_is_unscaled` | `shared/sim.js:50`, `:166`, `:537-546`; `shared/world-conquest.js:10-11` | Covered: capacity before and after region loss, active-seat split, army scaling, clamp and a three-seat case that distinguishes rounding after scaling. Existing fixtures retain exact all-mode and active-seat comparisons. |
| `BASE-17` | `base_audit_features::feature_base17_recruitment_refusal_precedence_and_maker_validity` | `shared/sim.js:313`, `:321`, `:394`, `:3781-3792` | Held: new Conquest refusal/maker test and existing exact commands cover admission, but complete construction/max/queued-heavy branches require reviewer assessment. |
| `BASE-18` | `base_audit_features::feature_base18_instant_recruit_selects_owned_finished_maker_without_queue` | `shared/sim.js:350`, `:352`, `:388-390`, `:3793-3797`, `:1071-1097` | Held: instant maker/payment/no-queue test covers component; actual spawn, facing/rally and non-Conquest modes remain incomplete. |
| `BASE-19` | None, held | `shared/sim.js:3798-3803` | Held: ordinary recruitment explicitly supports Conquest only; Tutorial spawn/rally branch remains absent. |

Read-only prototype-intent review checked the complete rows against `REVIEWER_2-expectations.md` and the pinned prototype. It ran no Cargo or prototype code. Execution results follow.

## Validation

The corrected focused run selected 65 tests: all 36 new tests and 29 retained checks passed, with 113 unrelated tests skipped. Normal output is in `/tmp/base-log-audit-batch1-tests-corrected.log`. The selection includes economy, production, reserves, convoys, ground supply, physical routes and test-registration checks.

The earlier expanded run had eight passes, one authored BASE-16 expected-value failure and 56 unrun tests. Its assertion incorrectly assumed Large population scaling was two; the pinned value is 2.5. Correcting 49 to 62 preserves the witness that rounding follows scaling. That failed output remains in `/tmp/base-log-audit-batch1-tests-extended.log`. The initial extraction had 53 passes before these coverage extensions.

Final `cargo fmt --all --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings` and `git diff --check` pass. Cargo ran under an exclusive slot with a 6G memory limit and four build jobs. No full workspace test run or new Source capture was performed.

# BASE native intent test window


[Axel approved rules 1 to 5](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6206861267) without added conditions. Native behavior tests and reviewer prototype readback qualify behavior rows. Exact tables, pure functions and existing component fixtures retain their comparisons. No new behavior recorder, capture or provenance bundle is used.

This window contains 16 assigned rows. Tests are implemented but unrun pending a new literal Root grant. Every status above is unchanged. Candidates require an actual native pass and reviewer confirmation before promotion. Held entries are excluded from promotion while other rows continue.

| Row | Literal native test mapping | Prototype reference at e4e579a | Scope and remaining work |
|---|---|---|---|
| `BASE-01` | `economy_state::income_and_population_match_original_javascript_traces` | `shared/sim.js:723`, `:1177`, `:1682`, `:6017`, `:6019` | Held: mode startup and owner snapshot omission/rounding are not exercised. |
| `BASE-02` | `base_audit_features::feature_base02_03_conquest_starting_wallet_and_caps_scale_all_named_armies` | `shared/sim.js:29`, `:31-35`, `:793`, `:837`, `:845`, `:1177`, `:1682`, `:802` | Held: actual starting wallets for Classic, Horde, Annihilation and Tutorial remain unrepresented here. |
| `BASE-03` | `base_audit_features::feature_base02_03_conquest_starting_wallet_and_caps_scale_all_named_armies`; `economy_state::income_and_population_match_original_javascript_traces` | `shared/sim.js:66`, `:801-802`, `:545` | Held: unknown army-string fallback and all-mode startup are outside this typed component. |
| `BASE-04` | `base_audit_features::feature_base04_conquest_points_catchup_and_absent_players`; `economy_state::income_and_population_match_original_javascript_traces` | `shared/sim.js:31-35`, `:49`, `:727`, `:5728-5753` | Native intent candidate: point income, catch-up cap, VP and away/out behavior. Exact arithmetic retains the existing fixture test. |
| `BASE-05` | `objectives::supply_masks_flood_order_and_cut_flags_match_source` | `shared/sim.js:122-129`, `:746`, `:4986-5019`, `:5655`, `:5731`, `:5746` | Held: exact point flood exists; full native tick scheduling and all construction/Horde context exclusions are not covered by this batch. |
| `BASE-06` | `base_audit_features::feature_base06_classic_depot_completion_destruction_and_currency_income`; `economy_state::income_and_population_match_original_javascript_traces` | `shared/sim.js:164`, `:5734-5747` | Native intent candidate: current depot facts pay only after completion and cease after destruction or ownership loss; allied uncut points, currencies and out guard covered. Classic constructor remains guarded. |
| `BASE-08` | `base_audit_features::feature_base08_world_region_loss_removes_income_and_depot_payment`; `economy_state::income_and_population_match_original_javascript_traces` | `shared/sim.js:1348-1357`; `shared/world-conquest.js:7-18` | Native intent candidate: region loss removes passive income and owned depot payment through current facts. No World logistics graph claim. |
| `BASE-09` | None, held | `shared/sim.js:64`, `:5695-5697`, `:3760-3765` | Held: both historical Source bounty failures remain unchanged; this batch does not rerun them or establish all native death/scuttle clauses. |
| `BASE-10` | None, held | `shared/sim.js:1688-1722` | Held: Classic resource-node generation is not exposed by the supported constructor. |
| `BASE-12` | None, held | `shared/sim.js:3281-3302`, `:3704`, `:1105`, `:3759` | Held: native node claiming, unseen occupancy, cancellation and destruction are not represented by this batch. |
| `BASE-13` | `base_audit_features::feature_base13_conquest_population_two_accepts_two_then_refuses_without_debit`; `economy::prices_factions_population_builders_and_support_match_javascript`; `production::classic_queue_room_after_three_paid_rifles`; `production::classic_no_barracks_mg_card_refusal`; `production::classic_full_selected_maker_does_not_fall_back`; `production::classic_tank_fuel_denial_and_paid_debit` | `shared/sim.js:383`, `:391` | Held: exact prices and native population-two boundary plus retained paid queue/Fuel controls are mapped; recruit-card price consumer and full Classic admission remain absent. |
| `BASE-14` | `economy::ability_costs_match_reference_objects_and_nullish_overrides`; `economy::prices_factions_population_builders_and_support_match_javascript` | `shared/sim.js:515-529`, `:549`, `:1787`, `:3614`, `:3681-3685`, `:5419` | Held: exact price queries do not prove thrown-time ability payment, autocast defaults or live support debit. |
| `BASE-16` | `base_audit_features::feature_base16_world_capacity_loss_and_active_seat_split`; `economy_state::income_and_population_match_original_javascript_traces` | `shared/sim.js:50`, `:166`, `:537-546`; `shared/world-conquest.js:10-11` | Native intent candidate: live component capacity before and after region loss, active-seat split, army scaling and clamp. Existing fixture retains exact all-mode comparisons. |
| `BASE-17` | `base_audit_features::feature_base17_recruitment_refusal_precedence_and_maker_validity` | `shared/sim.js:313`, `:321`, `:394`, `:3781-3792` | Held: new Conquest refusal/maker test and existing exact commands cover admission, but complete construction/max/queued-heavy branches require reviewer assessment. |
| `BASE-18` | `base_audit_features::feature_base18_instant_recruit_selects_owned_finished_maker_without_queue` | `shared/sim.js:350`, `:352`, `:388-390`, `:3793-3797`, `:1071-1097` | Held: instant maker/payment/no-queue test covers component; actual spawn, facing/rally and non-Conquest modes remain incomplete. |
| `BASE-19` | None, held | `shared/sim.js:3798-3803` | Held: ordinary recruitment explicitly supports Conquest only; Tutorial spawn/rally branch remains absent. |

The first BASE14 invocation at `fe5d456091ddd36aa79dee9e9ee56d4208ff3eb7` stopped during compilation with five E0506/E0502 borrow errors. No native test was selected or executed. The process ended with exit 101 and its inherited lock was released. Original output remains at `/tmp/native-feature-batches-0315/base_audit/native.log`. The successor gives unchanged fact transitions separate input storage; all assertions and expected values are preserved. LOG has not run. A new exact-head grant is required for either selection.

## First actual native result

At producing `cb6717a7c7ef1b34afc9db222f72a5f12737beef`, Rust tree
`c4d9ea21121a35f1a5c3a5871567a63482f4f1bf`, Root's 0345 S1 grant selected
all 14 tests once: 13 passed, one failed, 168 were skipped. The process and
systemd launcher ended with exit 100 and the inherited lock was released.
Normal output remains at `/tmp/native-feature-batches-0345/base_audit/`.

Native-passing intent candidates remain BASE-04, BASE-06, BASE-08 and BASE-16,
subject to prototype review. The new BASE-13 population boundary, BASE-17
refusal ordering and BASE-18 maker component also passed, but their complete
rows remain held for the listed gaps. All six retained exact/component checks
passed. BASE-02/03's shared startup test failed before assertions because the
authored map supplied zero points and validation requires one to nine. This
is a test setup error, not a demonstrated native rule defect. Add one central
open-ground point without changing any startup wallet/cap assertion. The
corrected test requires a fresh exact-head grant; the 13 successful tests do
not need another run solely for this setup correction. All parity files stay
unchanged. LOG has not run.

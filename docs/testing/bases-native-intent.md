# BASE native tests on main

This candidate is rebased onto main `6431852ac50bed39f895e09f125950038c181301`.
It registers 18 tests through `match_construction::native_intent::`, using
existing canonical Match and production APIs. The nine tests marked HELD below
are absent from this candidate. Their literals refer to the preserved original
module at `33e36ad4b4a1a286657e43082f4bdf0820acf1e8`, under
`match_construction::classic_depot::tests::native_intent::`.

The current validation and row qualification are recorded below. Historical
results do not qualify this candidate. Owner decision
[6206861267](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6206861267)
permits native behavior tests plus prototype review; exact table, pure-function
and component comparisons remain required. Prototype references below are
`shared/sim.js` at `e4e579a`. The existing exact fixtures remain unchanged.

The earlier R2 review accepted only BASE-32 on the retained private branch.
Fresh source review at `c50a6c5d` accepts BASE-20 and BASE-32 against the
pinned prototype and the full reviewer expectations. Reviewer execution
counts are zero tests, Cargo runs, edits and GitHub writes. Passing a partial
witness does not prove its entire row. BASE-52 retains only Conquest startup. No parity status changes
are included in this test PR.

| Row | Candidate test or held original | Prototype lines | Exercised intent | Remaining scope or shipping dependency |
| --- | --- | --- | --- | --- |
| BASE-20 | `base20_paid_shortest_owned_five_slot_queues` | `3806-3811` | Shortest queues after each purchase, explicit own selection over a shorter queue, invalid/allied/unfinished/no makers, full queues, MP/Fuel payment, and unchanged wallets/jobs on refusals and Sudden Death. | Complete queue-admission intent in both Classic and World production contexts. This does not enable Classic Match startup or command ingress. |
| BASE-21 | `base21_training_pauses_and_emits_each_catalog_kind_at_its_duration` | `381,1065-1079,5352` | All 29 training durations, unfinished/deadline pause, completed requests. | Component requests only, with an HQ view for all kinds. Head/follower transitions and physical aircraft-on-building and ground/naval step-out births remain open. |
| BASE-22 | HELD original `base22_waiting_identity_refund_preserves_progress_and_owner_rows` | `1034-1050,3737-3747,6024` | No candidate test registered. | Absent paid Classic Buy/ticks, CancelProduction parser/dispatch and production_for snapshot helper. Original Fuel refund gap also remains. |
| BASE-23 | HELD original `base23_building_rally_overrides_personal_rally_for_real_buy` | `3766-3780,1071-1079,3389-3393` | No candidate test registered. | Absent Rally parser/dispatch. Main has internal rally storage, but no recorded Rally admission; personal-only and construction-mode controls also remain. |
| BASE-24 | `base24_horde_allies_share_finished_producers_and_labs_only` | `387,3696,3713,3751,3773` | Horde allied finished producer and lab access, enemy exclusion and ordinary ownership. | Horde facility/lab queries only. Shared Buy, buyer payment/ownership, rally, prerequisites, assist and owner-only site cancellation remain open. |
| BASE-26 | HELD original `base26_paid_site_binds_node_and_assigns_actual_builder` | `3689-3709` | No candidate test registered. | Absent Classic startup, Build admission, runtime nodes and crew routing. Queued/full/retreating worker controls and Pillbox direction also remain. |
| BASE-27 | HELD original `base27_blocked_unseen_and_node_placement_refuse_without_debit` | `3277-3313` | No candidate test registered. | Absent Classic constructor/node Build admission. Main-supported pure placement remains separate and unchanged; rounding/territory/rail/coast/overlap/preview cases remain. |
| BASE-29 | HELD original `base29_actual_engineer_completes_site_before_it_becomes_prerequisite` | `5375-5380,5587-5603,5352` | No candidate test registered. | Absent Classic constructor/ticks, crew movement and paid queue admission. Completion tally fix is deliberately held with this dependency; crew rates/firing/deadline also remain. |
| BASE-30 | HELD original `base30_supported_section_repair_restores_health_without_wallet_charge` | `3710-3722,5591-5596,2629-2645` | No candidate test registered. | Absent Classic site completion and section-repair admission. Original direct repair was partial; actual Assist/worker repair, provisions/rates/deadline/failed sections remain. |
| BASE-31 | HELD original `base31_cancel_paid_site_refunds_three_quarters_and_removes_sections` | `3748-3759` | No candidate test registered. | Absent CancelConstruction parser/dispatch and World/environment section-removal helpers. Finished-site refusal and no-ghost-rubble/debris controls also remain. |
| BASE-32 | `base32_scuttle_marks_only_owned_living_targets_without_refund` | `3760-3765,5697` | Own target scuttled, rival unchanged, actual retirement and no refund/bounty. | Scuttle and canonical retirement assertions retained. Fresh source review accepts the full scuttle intent. Final rebased-tree execution is recorded below. |
| BASE-33 | `base33_destroyed_owned_building_retires_to_rubble` | `1100-1106,5678,5762` | Real scuttle followed by canonical retirement and rubble footprint. | Rubble retirement only. All-section collapse/event, Depot node release and rebuilding on rubble remain open. |
| BASE-37 | `base37_motorpool_requires_finished_barracks_before_tier` | `353` | Unfinished Barracks prerequisite refused before tier; finished prerequisite reaches tier denial. | Controlled ordinary-Match prerequisite/tier denials only. Successful Motor Pool construction and full roster/production remain open. |
| BASE-38 | `base38_airfield_queues_catalog_aircraft` | `354,548` | Actual component aircraft queues and catalog maker/prerequisite. | Aircraft component queue/roster and Motor Pool prerequisite data only. Tier/construction gates and World nearby-Airfield paratrooper admission remain open. |
| BASE-39 | `base39_shipyard_naval_map_gate_precedes_payment` | `355,3303` | Naval-map gate refuses before payment, then actual LCVP queue succeeds. | Component naval-map denial and paid LCVP queue only. Tier/coastal construction and physical valid-water launch remain open. |
| BASE-44 | `base44_wall_and_gate_static_dimensions_and_producer_exclusion` | `367-370` | Exact Wall/Gate numbers and gate distance; excludes ordinary Build list and producer list. | Exact catalog numbers and Build/producer exclusions only. Fortification-created finished cells remain open. |
| BASE-45 | `base45_supplycache_catalog_and_nonproducer` | `371,3277` | Exact Cache data/logistics flag and finished Cache exclusion from producers. | Exact catalog/logistics flag and finished Cache nonproducer query only. Logistics placement, completed service and unfinished Cache Supply refusal remain open. |
| BASE-46 | `base46_regional_base_is_never_buyable_and_has_no_recruit_list` | `357,1183-1187,1300-1316,3781` | Worldbase purchase blocked without debit; free static base and absent recruit list. | Component forbidden purchase and static catalog data only. Neutral-region creation, World local ownership and live capture-lock release remain open. |
| BASE-47 | `base47_tech_off_blocks_research_and_removes_purchase_tier_gate` | `799-800` | Real no-tech research refusal; T1 component can queue mortar when tech is off. | Ordinary Match no-tech research denial plus component purchase-tier bypass. Supported-mode switch matrix and Horde startup tiers/empty labs remain open. |
| BASE-48 | `base48_tier_gating_refuses_mortar_before_wallet_debit` | `441-442,453,3639,3664,3698,3785` | Every named T2/T3 value and actual T1 mortar denial without debit. | Exact named T2/T3 values plus component T1 mortar denial. Default T1 and Dig/Entrench/Wall/Gate gates remain open. |
| BASE-52 | `base52_conquest_starts_with_its_own_forces` | `1575-1600,1679-1686,1176-1179,783-784` | Existing Conquest constructor: correct force, finished HQ and nine footprint cells. | Conquest startup only. Classic constructor is absent on main; fallback/trapped clearance, World, Horde, Tutorial and other modes remain open. |
| BASE-53 | `base53_assault_defender_gets_trenches_bunker_and_four_flak` | `975-1002,793,838,846` | Real Assault defender startup: trench/sandbag cells, bunker and four finished Flak; attacker has none. | Existing Assault constructor only. Forward arcs/gaps and obstacle/edge legality, Horde and Annihilation remain open. |
| BASE-54 | `base54_finished_wall_structure_primitive` | `1197-1225,1324` | Real finished Wall structure primitive registers cell, health and owner. | Finished Wall placement primitive only. World region ranking, farthest-region groups, towers and damage-preserving capture transfer remain open. |
| BASE-55 | `base55_engineer_queue_price_pop_and_capacity` | `1358-1378` | Engineer queue charges 60 MP and obeys capacity/pop checks; exact HQ recovery price. | Component Engineer price/pop/capacity and HQ recovery constant only. Both World recover operations and needs/territory/MP/blocked/partial-payment refusals remain open. |
| BASE-56 | HELD original `base56_real_classic_deadline_clears_paid_queues_and_starts_decay` | `5756-5762,3689,3727,3739,3807` | No candidate test registered. | Absent Classic clock/deadline/ticks and paid-command admission. Wallet non-refund, World non-transition and construction halt also remain. |
| BASE-57 | `base57_ordinary_retreat_selects_living_hq_then_spawn` | `1725-1735` | Real ordinary retreat selects live own HQ, then spawn after HQ death. | Ordinary living-HQ/spawn home query only. Nearest/accessibility choice, Classic producer, World territory/fallback and convoy source remain open. |
| BASE-59 | HELD original `base59_live_queue_snapshot_keeps_ids_progress_rally_and_owner` | `6018-6028` | No candidate test registered. | Absent paid Classic training/Rally and production_for snapshot helper/refactor. Full owner snapshot, positive IDs, tech/nodes/rallies/wallet privacy remain. |

The full old 27-test suite depends on unpublished shipping Classic startup,
clock/deadline, construction, crew routing and section-repair admission, paid
Classic Buy, cancellation and Rally commands, and the private production
snapshot refactor. Importing that ancestry would also conflict with current
main changes unrelated to this task. None is adopted here. There is no
replacement runtime under cfg(test); component wallets and UnitView values
are explicit inputs to existing production APIs, never installed in a Match.
Ordinary Match tests use real constructors and commands. The controlled
BASE-37 prerequisite state and BASE-57 dead HQ remain explicit test inputs.

The old `db3ab341300d6ba882dc4db3fb1ed2b5258aa011` result remains 27 PASS,
Rust tree `d6934f6584b631f23b0fc42a751ebc34a4f5ac85`, in
`/tmp/native-feature-batches-0345/impl_bases/native.log` and `RESULT.json`.
The earlier `309f02bc` result remains 25 PASS and 2 FAIL, and earlier parser,
compiler and five-consumer failures retain their original attribution. Old
refs, fixtures and logs are preserved; none proves current-main execution.

This candidate changes no shipping runtime, interface, default, reference pin
or parity file. The separable Built story tally fix is held with the missing
Classic completion test, so there is no new runtime head for Hermes to ACK.
The orchestrator owns CI, merge and the later proof ledger/status PR.
A merged partial test does not close the gaps listed above.

## Qualification and deferred work

The original `71bd8c3e` / Rust `9bcaa2a5e29f42c7dafcbb86ca315a92ead8ed35`
ran all 18 selected tests successfully on the Vostro. This first run preceded
BASE-20 strengthening and the main rebase. It is separate from final-tree
validation. Logs: `/tmp/impl-bases-20261016-validation/initial-nextest.log`.

Only BASE-20 and BASE-32 have complete source-qualified witnesses. The 16 other
registered tests remain useful partial coverage, with every missing part in
the table above. They are excluded from the PR's proven-row list.

The nine absent tests remain NEXT. Their complete runtime is too large for
this test PR: BASE-22 needs admitted construction-mode owner snapshots as well
as cancellation ingress; BASE-23 needs Rally admission and construction-mode
routing coverage; BASE-26/27 need node, World and naval placement;
BASE-29/30 need deadline-aware construction/repair; BASE-31 needs cancellation
and canonical debris/node cleanup; BASE-56 needs the Classic lifecycle;
BASE-59 needs construction snapshots and resource nodes. Existing low-level
cancellation arithmetic or rally storage alone does not complete those rows.

Locally acknowledged issue #5 through `6207938018`, owner decision `6206861267`,
and contract replies PR16 `6137379059` and PR195 `6156740603`. These require
no new runtime acknowledgment for this test-only change. Contracts retain
the six-call ABI 2 and accepted startup content/schema boundaries.

## Final validation

Producing commit `c50a6c5d8d77f832afc6d2f5977f787af9481563`, Rust tree
`35f335eb194ec018575c9ad2d16795a06e1229ac`, on the Vostro:

- All 18 selected native tests pass, with 158 unrelated core-library tests
  skipped, one test thread and zero retries.
- Rust 1.99.0 `cargo fmt --manifest-path rust/Cargo.toml --all --check` passes.
- Rust 1.99.0 `cargo clippy --manifest-path rust/Cargo.toml --locked --workspace --all-targets -j4 -- -D warnings` passes.

The final selection was `test(match_construction::native_intent::)` with
`-p ww2-core --lib`, four build jobs and the default test profile. All checks
used locked slot S3, its assigned target directory, a 6 GiB memory limit and
zero allowed swap; the slot was released at completion. Logs and producing
objects are under `/tmp/impl-bases-20261016-validation/`.

A final source review at that exact commit/Rust tree accepts BASE-20 and
BASE-32. Other rows retain their gaps. The subsequent documentation commit
records these results and preserves the validated Rust tree.

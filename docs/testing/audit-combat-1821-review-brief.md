# Whole-check review brief: six restored combat checks

Selected rows: CMB39/40/41/64/65/69. All six have their complete selected-check
proof mapped below. The status/documentation child changes only their
Rust/Status cells and preserves all 160 Acceptance cells and every other row.
It retains producing head 39b004f2's tested Rust tree. Independent exact-head
review remains with Root; this brief does not claim that review has occurred.
The reviewer executes zero compilation, discovery, tests, formatting, lint or
Source jobs. CMB42 stays on its separate source-only branch.

Actual producing head: `39b004f2e180b334a27a51c9b0ce47db1ff27126`.
Exact results: renewed fmt exit0, five selected simtests PASS (224 skipped), and
strict locked core/simtest all-target ci-test Clippy exit0. See the
[terminal qualification](audit-combat-1821-s3-result.md) for commands, names,
resources, logs and producing-head attribution. The initial 3ca fmt diagnostic
has only two mechanical wraps; no semantic repair or test retry followed.

## Existing Source qualifications

All referenced original behavior was inspected at pinned Source
`e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. The vision range fixture records
`shared/sim.js` blob `1d20ab8301553804e77e3eca04ab56db76b4a1d6`.
Existing committed captures were read directly. Their reference metadata agrees
with the pin. Where present, their producer metadata is x64 Node v24.21.0.
The production/economy captures do not carry Node/architecture metadata, so do
not invent those fields. No Source generation or check ran in this restoration.
Historical recorder success described by DESIGN remains historical; no new
regeneration receipt is claimed or needed to relabel an unchanged old check.

| Existing input and recorder | Actual recorded scope and native consumer |
| --- | --- |
| `core/data/rules.json`, `core/tests/fixtures/startup-tables.json`, `startup-table-values.json`; maintained rules/startup-table recorders | Complete post-initialization unit definitions, including flakpos AA and tower vision. The catalog test fingerprints the whole catalog; startup-table test compares complete producer bytes and all 1743 captured numeric leaves, with 64 tables. These static checks are not runtime aircraft, client adoption or new transport proof. |
| `simtest/tests/fixtures/vision-rows.json`; `harness/vision-rows-fixture.mjs` | 18 original range cases. `tower-unfinished` and `tower-complete` use fog/rain/height inputs, so their final ranges are not bare 12/60. The original native consumer compares both visionOf and visionRange in exact bits. Do not describe those weather-modified outputs as unmodified 12/60. |
| `core/tests/fixtures/production.json`; `harness/production-fixture.mjs` | 12 traces, 114 actions and 922 advance calls. The complete `queued-unit-limit` trace disables tier requirements, assigns the existing motor pool to Germany, admits one Tiger and refuses the second with exactly max. Native checks the complete paid queues and wallets after every action, for both split and monolithic advancement. |
| `core/tests/fixtures/match-combat.json`; `harness/match-combat-fixture.mjs` | Nine original traces, 35 actions, including enemy/friendly/out-player bounty, building retirement, zero-damage riders and wreck ordering. These are controlled component inputs, including already-dead actors. They do not establish an actual projectile death by themselves. |
| `simtest/tests/fixtures/match-aviation.json.gz`; `harness/match-aviation-fixture.mjs` | 18 real aircraft constructor observations, 42 component traces, 3410 complete checkpoints. Includes mg/flak/fighter/flakpos continuous AA, setup, finished/unready emplacement, visible/hidden, range, ammo, XP, events, nearest target, held ground and explicit held fighter target. External scene inputs isolate these phases; do not claim full movement/vision/lifecycle integration from this corpus. |
| `simtest/tests/fixtures/match-support.json.gz`; `harness/match-support-fixture.mjs` | 268 traces and 5922 native compared frames. Original flak-held support interception is represented separately from continuous on-map AA. Do not count this as all of B09: the relevant funded cases start at two rounds and end at one. |
| `simtest/tests/fixtures/match-tick.json`; `harness/match-tick-fixture.mjs` | 19 connected original-step scenes. Case 7 is the authentic 800-step/15-checkpoint retirement run, Combat seed 701 and environment seed 305419896. It has no HP input. Unit 2 actually retires at tick 483; its full cumulative tuple is [483,2,1,rifle,0]. It is not a whole-match outcome/statistics baseline. |

## Existing compiled/native qualifications

Opened [main push 38776054627](https://github.com/axelmanosalvasmd/ww2-rts-native/actions/runs/38776054627)
and downloaded its actual raw logs. Producing head is
`7a7348cbac35924b5876531e318140d13b7a42c1`, completed/success. Both it and frozen
main `379ba4e` have Rust tree `7bead84a8d1953d3397626fdb64e87ce3a180a8c`.
The following eight unique existing instances each have explicit PASS lines on
Ubuntu 24.04, Windows 2025 and macOS 15, for 24 named historical results:

- `economy::compiled_catalog_keeps_complete_post_initialization_tables`
- `startup_tables::complete_node_exports_keep_every_field_number_and_exact_bytes`
- `vision::rows::cmb124_sight_ranges_match_original_source`
- `production::production_research_and_refunds_match_original_source_traces`
- `match_combat::ordered_damage_riders_terrain_and_death_match_original_source`
- `match_aviation::held_flights_rearming_and_continuous_aa_match_every_original_call`
- `match_support::actual_support_admission_strikes_and_timers_match_source`
- `match_tick::connected_tick_phases_match_actual_reference_steps`

Raw old-main log is `/tmp/audit-combat-1821-main-38776054627.log`. These are
compiled executions of their producing main, not fresh executions of this
candidate. The extracted original connected replay now has a separate fresh
PASS at 39b004f2. Core64 retains its actual single PASS at daaa4743; its core
source is unchanged and the new grant explicitly excluded rerunning it.

## Six complete selected checks

CMB39 requires the complete flakpos fixture dump and tower vision 12 then 60
after completion. The old catalog/startup-table and AA components supply the
full table and unfinished-emplacement controls. The new paired positive tower
witness PASS uses the actual recorded tower actor with weather disabled,
built .99/1 and exact 12/60 visionOf bits. The older fog/rain/height cases stay
unchanged. No new full-match gate is introduced.

CMB40's selected check is the original Command feedback second-Tiger max
refusal. The old complete queued-unit-limit replay compares paid queues and
wallets after both purchases, in split and monolithic advancement. Static cap
values and field-plus-queued admission remain exact. This unchanged complete
component proof is sufficient under PARITY and has three-platform old-main PASS.

CMB41's named check is four Conscripts counting three population. The new PASS
buys four real Soviet Conscripts through recorded_buy over the retained native
constructor. It checks distinct actual IDs, ownership/type, recruitment metadata,
exact payment, every .75 increment and final population delta 3. Existing complete
tables retain Conscripts .75 and train/truck zero. No actor type rewrite or
synthetic field/queue input stands in for those four births.

CMB64's selected enemy-kill/bounty check combines a real connected projectile
casualty and complete Source wallet comparison with the exact bounty component.
The focused daaa PASS checks enemy/friendly retirement order [2,1,3], unchanged
friendly wallet, exact recorded killer wallet and 20% cost. Those component
actors start dead as authored; do not claim a projectile killed them there.
The actual projectile casualty is the separate connected replay, freshly PASS
at 39b. No unsupported Horde-runner claim follows from test.js:2294.

CMB65's selected check is the named MG/flak/fighter AA part, with support flak-held
interception supplied by its complete unchanged support component. New PASS
requires six held .05s AA calls per kind with unchanged target HP/shooter XP/no
events, then six permitted calls with real damage, XP gain and an AA event.
Ground units use ordinary Stance off. The fighter uses the complete unchanged
ready-fighter capture, ordinary Move/Patrol, one physical aircraft step and
ordinary Attack with committed mission/target/Out and unchanged positive ammo
checks while held. Both intervals use a live airborne visible in-range target.
The zero-ammo prior fixture remains unchanged and its failed admission stays a
failure at daaa. No ammo/HP/type/pose/RNG override, guard weakening or new Source
capture is claimed. Root accepted the bounded new native composition. Its
[precise source/caller declaration](cmb65-mission-eligible-source.md) retains the
contiguous-capture gap. Automatic aircraft ground weapons remain CMB67's gap.

CMB69's selected seeded death-tick check PASS accepts the genuine due death
prefix on every one of 800 actual canonical ticks, retains all 15 original
snapshots and observes the actual unit 2 retirement at 483. The eight oracle-only
mutants must be rejected at their exact first divergent tick:

| Control | Required first rejection |
| --- | --- |
| Death moved to tick 482 | 482 |
| Death delayed to tick 484 | 483 |
| Unit ID 2 changed to 1 | 483 |
| Owner 1 changed to 0 | 483 |
| Type rifle changed to mg | 483 |
| Last hitter 0 changed to 1 | 483 |
| Event omitted | 483 |
| Event duplicated | 483 |

All eight controls are asserted within the passing native instance. They never
initialize or mutate Match; the helper receives only a read-only callback after
advance_tick retirement. No HP prime, expected-state import, RNG reset, tolerance
or changed fixture supplies the genuine result. Its complete selected runner
already counts under PARITY; this does not claim every death family or an
additional whole-match outcome/statistics qualification.

## Preservation and review boundary

Held owned rows remain CMB38/42/62/63/66/67/68, with exact gaps in the
[assessment](audit-combat-1821-eligibility.md). CMB61 and prior262 promotions are
excluded. The original private five-row patch remains as a historical unapplied
proposal; actual current six-cell diff is the post-qualification status child.
All old failure logs and checkpoints remain intact.

Review the actual status child, its unchanged Rust tree and this per-row mapping,
then return ACCEPT or REVISE with one verdict per row and reviewer execution0.
Do not infer consent from acknowledgments or the pending general-defaults
proposal. Root owns publication lease transfer after CMB_IMPL, hosted/current-head
qualification, Hermes runtime readback and merge. User authorization permits
opening the reviewable row PR once the lease transfers, without waiting for
Hermes' merge acknowledgment before opening.

# AUDIT_COMBAT complete-check assessment at restored main

Latest native qualification stops at producing daaa4743: CMB-64 PASS, CMB-65 FAIL
on the reused fighter's zero-ammo new-mission refusal. Four selected simtest
instances and conditional strict Clippy remain unrun. See the
[terminal result](audit-combat-1821-s2-result.md). No repair/retry or row edit.
The per-row mappings below are the preparation assessment, not a later PASS.

Base: `379ba4e06c794708623c005ab6052bebd03d8809`. These are private proposals,
not completed reviews, current-head passes or actual status changes. New tests
must pass under Root's exact slot grant before the first five can be proposed
for publication. CMB_IMPL retains the combat-file lease.

## Existing compiled proof

[Main push 38776054627](https://github.com/axelmanosalvasmd/ww2-rts-native/actions/runs/38776054627)
is completed/success at producing `7a7348cbac35924b5876531e318140d13b7a42c1`.
The run's raw logs were opened, not inferred from a green badge. The existing
catalog, production, combat, aviation, firing and connected-tick instances each
show PASS on Ubuntu 24.04, Windows 2025 and macOS 15. Source comparison confirms
frozen main `379ba4e` retains that Rust tree. The old hosted results are separate from the new partial qualification above.
Raw downloaded log: `/tmp/audit-combat-1821-main-38776054627.log`.

## First five selected checks

| Row | Complete original check and current consumer | Evidence and precise remaining gate |
| --- | --- | --- |
| CMB-39 | Fixture dump plus tower vision 12 then 60 after built=1. `economy::compiled_catalog_keeps_complete_post_initialization_tables` and `startup_tables::complete_node_exports_keep_every_field_number_and_exact_bytes` retain the complete post-load table, including flakpos AA. `vision::rows::cmb124_sight_ranges_match_original_source` compares authentic unfinished/complete tower cases under fog, rain and height. | Existing named checks pass on producing main. New `vision::cmb39_existing_tower_actor_sees_twelve_then_sixty_when_complete` uses the actual recorded tower input with clear weather, checks built .99 then 1 and exact bits 12/60. Existing `aa-unfinished-emplacement` and flakpos AA traces remain intact. New focused native pass and whole-row review pending. |
| CMB-40 | Port the Command feedback second-Tiger max check. `production::production_research_and_refunds_match_original_source_traces` consumes `queued-unit-limit`: tech disabled, existing motor pool ownership input, first Germany Tiger buy succeeds, second returns exactly max. The complete trace compares paid queues/wallets after every operation in both monolithic and split advancement. | Existing test PASS on all three platforms at producing main. The catalog contains Tiger=1, Churchill=1 and Destroyer=2; the native admission counts fielded actors and paid queued jobs before comparing max. No new test or whole-match gate is required by this selected check. |
| CMB-64 | Port test.js:2294: an actual enemy death is removed and the finisher receives 20% cost. The connected retirement scene produces real projectile damage and unit 2's removal at tick 483; every retained complete wallet snapshot matches Source. `match_combat::ordered_damage_riders_terrain_and_death_match_original_source` separately matches exact enemy/friendly/out-player bounty and building retirement. | Existing component and connected tests PASS at producing main. New `match_combat::cmb64_actual_retirement_pays_enemy_cost_bounty_and_excludes_friendly_death` directly checks native retirement order, unchanged friendly wallet and exact recorded wallet plus catalog cost bounty, over the existing fixture. New native pass and whole-row review pending. No unsupported Horde runner claim. |
| CMB-65 | Port Hold fire applies to aircraft... AA portion for mg/flak/fighter. `match_aviation::held_flights_rearming_and_continuous_aa_match_every_original_call` runs 42 Source traces/3410 checkpoints for nearest/range/setup/finished/visible/airborne/ammo/event/XP/death cases, held ground and explicit held fighter target. Support's separate existing recorder covers flak-held interception. | Existing full component corpus PASS at producing main. New `match_aviation::cmb65_mg_flak_and_fighter_hold_then_resume_permitted_anti_air` uses the authentic three continuous-AA scene inputs, ordinary Stance dispatch, six held calls with no HP/XP/event change, then six permitted calls with actual damage/XP/AA event. The held fighter resumes through its existing explicit-air-target mission caller. New native pass and whole-row review pending. Automatic aircraft ground weapons remain guarded. |
| CMB-69 | Seeded run: same death tick per unit ID. `match_tick::connected_tick_phases_match_actual_reference_steps` calls canonical advance_tick and accumulates every retirement, comparing cumulative [tick,id,owner,type,lastHit] plus complete checkpoints. The authentic long scene has Combat seed 701, environment seed 305419896, 800 steps/15 checkpoints and death [483,2,1,rifle,0]. | Existing complete selected runner PASS at producing main. The new CMB-69 witness uses that same extracted replay once, compares every tick's due death prefix and requires eight oracle-only mutants to be rejected at tick 482/483. No new actor, HP prime, RNG reset or expected-state initialization. New witness and extracted original instance native passes pending. This selected component check adds no whole-match statistics gate. |

## Held checks in the same assigned batch

| Row | Current source evidence | Precise missing complete check |
| --- | --- | --- |
| CMB-38 | Complete catalog retains pillbox weapon constants; native section contact computes slit damage. | The maintained combat fixture has no pillbox front/rear rifle-and-AT scene. No complete selected scene proves both directional hit classes, so retain todo. |
| CMB-41 | Catalog and economy fixtures prove pop_use=.75 for Conscripts and 0 for train/truck; native EconomyState sums real ordered field and queue population. | The existing input has one fielded Conscript and one queued Conscript, not the original four-Conscripts difference of 3. A new source-only `match_tick::cmb41_four_actual_conscript_recruits_add_three_population` now buys four actual Soviet Conscripts and checks every population increment/payment over the retained constructor. Its native execution and complete-check review are pending. |
| CMB-42 | Exhaustive pure role helper checks plus native AT choice-role trace. | No named sniper choice trace proves a 30 m MG beats a 12 m rifle in the actual target selector. Preserve both named original checks, not only the AT half. |
| CMB-62 | Direct ordinary firing exists. | `match_firing.rs` explicitly rejects nonempty fire_at in the ground launcher. The Tank shells check requires same-house first-wall traversal, actual cell destruction, order clearing and rifle refusal; no complete ordinary consumer is proved. No guard is weakened here. |
| CMB-63 | Pure helper, damage and suppression fixtures cover star thresholds and some effects; existing snapshots can encode stars. | No single mapped proof set currently completes the named Veterancy check plus all three new effect checks, including actual firing accuracy at stars. Keep in progress until exact per-effect witnesses are mapped or added. |
| CMB-66 | Existing support corpus has empty/forced/stranded/held/unready/fighter/multiple-gun cases and exact OffMap state comparisons. | Original B09 explicitly checks one funded round ending at zero for both a hit and a miss. Existing relevant inputs start at two rounds; multi-flak supplies a hit and stranded a miss, both 2 to 1. Complete one-round hit/miss/empty witnesses are still needed. No source seed is reset to force an outcome. |
| CMB-67 | Explicit held-aircraft movement and AA are represented. | Aircraft automatic ground weapons and bomber area-salvo missions are explicit Unsupported paths. The original Hold Fire first half requires actual ground attack ammunition use; it cannot be claimed from AA. |
| CMB-68 | Combat helper emits rider requests and existing application fixture covers zero-damage unloading. Carrier tick fixture explicitly excludes combat/ejection. | No maintained full named halftrack wreck scene proves actual thrown rider escape with exactly 30% loss. Current helper evidence is not that caller proof; retain in progress. |

## Preservation and authority

All 160 Acceptance cells and every actual combat status remain byte-identical to
frozen main. No other table is edited. The five-row Rust/Status patch under
`docs/proposals/audit-combat-1821/` is unapplied and explicitly conditional on
native proof plus independent exact-head review. Earlier CMB-61 failure logs and
all prior branches remain intact. No translated/source model counts as native
execution. Source, Cargo, rustc, native, fmt, lint, discovery, full/release and CI
execution counts were zero at that preparation checkpoint. The later S2 result
above records one native pass, one native failure and all unrun work exactly.

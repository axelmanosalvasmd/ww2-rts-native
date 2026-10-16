# Native MODE feature batches

This is the test and review map for the 83 assigned remaining MODE rows.
The parity inventory stays unchanged. Native passes below stay attributed
to their producing head; a passing test is not an owner acceptance or a
complete-row promotion. Root owns publication and the separate status ledger.

## Native behavior batch, first 50 owned rows (2026-10-16)

[Axel approved rules 1 to 5](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6206861267) without added conditions. Behavior rows use native intent tests and prototype review. Existing exact component fixtures and historical failures remain intact. This window proposes no status promotion before actual execution and review. No runtime behavior changes.

Tests below live in `rust/crates/core/tests/mode_native_batch.rs` and `rust/crates/server/src/mode_native_batch.rs`; existing validator tests remain in `rust/crates/core/tests/authoritative_maps.rs`. Every named function is literal. Partial coverage does not complete its row.

| Row | Native test | Qualification or remaining reason |
|---|---|---|
| MODE-01 | `lobby::mode_native_batch::mode01_04_06_09_41_lobby_mode_capacity_army_and_ai_controls` | partial: constructor coverage lacks three modes |
| MODE-02 | none | incomplete: generic constructor rejects unsupported modes; silent fallback absent |
| MODE-03 | `lobby::mode_native_batch::mode03_host_settings_and_guest_rejection_cover_each_control`; `lobby::mode_native_batch::mode03_attached_match_locks_every_setting` | FAIL at 5b034faa: test reads absent lobby map key; retained body excluded, no repair or rerun. The separate lock test also reads that absent key. Row held. |
| MODE-04 | `lobby::mode_native_batch::mode01_04_06_09_41_lobby_mode_capacity_army_and_ai_controls` | partial: public start and Assault readiness unavailable |
| MODE-06 | `lobby::mode_native_batch::mode01_04_06_09_41_lobby_mode_capacity_army_and_ai_controls` | partial: all-mode army scaling unavailable |
| MODE-07 | none | incomplete: fresh server weather seed and full all-mode integration incomplete |
| MODE-09 | `lobby::mode_native_batch::mode01_04_06_09_41_lobby_mode_capacity_army_and_ai_controls` | partial: human handover unavailable |
| MODE-10 | none | incomplete: sandbox spawn command unavailable |
| MODE-11 | none | incomplete: all-mode startup feature switches unavailable |
| MODE-16 | `mode_native_batch::mode16_17_tutorial_roster_uses_shared_home_and_first_unused_enemy_faction` | partial: Tutorial only; Horde constructor absent |
| MODE-17 | `mode_native_batch::mode16_17_tutorial_roster_uses_shared_home_and_first_unused_enemy_faction` | partial: Tutorial only; Horde constructor absent |
| MODE-21 | none | incomplete: native purpose streams deliberately replace Source global draw order; all-mode/server coverage incomplete |
| MODE-22 | none | incomplete: Horde, Classic and Annihilation constructors absent |
| MODE-24 | none | incomplete: radio/depot combined native behavior check not yet implemented |
| MODE-29 | none | incomplete: ordinary Assault command/fog/veterancy consumers remain guarded |
| MODE-30 | none | incomplete: all-mode damage scaling check lacks Horde/Annihilation runtime |
| MODE-31 | `mode_native_batch::mode31_assault_timer_phase_finishes_once_and_freezes_clock` | partial: late timer phase only; ordinary Assault tick remains guarded |
| MODE-32 | `mode_native_batch::mode32_assault_authoring_rejects_each_invalid_field`; `authoritative_maps::assault_authoring_errors_match_original_validator_messages` | native PASS at 5b034faa, including retained exact validator oracle; prototype intent review pending |
| MODE-33 | none | incomplete: Annihilation constructor absent |
| MODE-34 | none | incomplete: Annihilation elimination consumer absent |
| MODE-35 | none | incomplete: Annihilation victory consumer absent |
| MODE-36 | none | incomplete: all-mode out-player command/income/bounty consumers incomplete |
| MODE-37 | none | incomplete: Classic constructor absent |
| MODE-38 | none | incomplete: Classic Sudden Death consumer absent |
| MODE-39 | none | incomplete: Classic elimination consumer absent |
| MODE-40 | none | incomplete: Classic victory consumer absent |
| MODE-41 | `lobby::mode_native_batch::mode01_04_06_09_41_lobby_mode_capacity_army_and_ai_controls` | partial: fresh map reload on start/restart unavailable |
| MODE-42 | none | incomplete: Horde constructor absent |
| MODE-43 | none | incomplete: Horde wave runner absent |
| MODE-44 | none | incomplete: nextwave unavailable |
| MODE-45 | none | incomplete: Horde profile runner absent |
| MODE-46 | none | incomplete: Horde unlock runner absent |
| MODE-47 | none | incomplete: Horde purchase/reserve runner absent |
| MODE-48 | none | incomplete: Horde entry runner absent |
| MODE-49 | none | incomplete: Horde wave ending runner absent |
| MODE-50 | none | incomplete: Horde wave MP runner absent |
| MODE-51 | none | incomplete: Horde plane runner absent |
| MODE-52 | none | incomplete: Horde Kaiju runner absent |
| MODE-53 | none | incomplete: Horde reveal runner absent |
| MODE-54 | none | incomplete: Horde ending runner absent |
| MODE-55 | none | incomplete: Horde record persistence and lobby integration absent |
| MODE-56 | none | incomplete: Horde server brain scheduling absent |
| MODE-57 | none | incomplete: Tutorial public autostart remains guarded |
| MODE-58 | `mode_native_batch::mode58_tutorial_starts_without_bases_and_recruits_at_home` | native PASS at 5b034faa; incomplete birth-distance assertion, held pending review |
| MODE-59 | `mode_native_batch::mode59_tutorial_tick_pays_attacker_base_income_without_vp` | native PASS at 5b034faa for initial base income; prototype intent review pending |
| MODE-60 | none | incomplete: retained tick7081 Trench failure blocks full nine-step progression |
| MODE-62 | `mode_native_batch::mode62_64_65_live_orchard_recruit_tank_progression` | native PASS at 5b034faa for live orchard clearance; prototype intent review pending |
| MODE-64 | `mode_native_batch::mode62_64_65_live_orchard_recruit_tank_progression` | native PASS at 5b034faa for paid AT completion; MP floor witness not decisive, row held |
| MODE-65 | `mode_native_batch::mode62_64_65_live_orchard_recruit_tank_progression` | native PASS at 5b034faa for actual tank death and advance; prototype intent review pending |
| MODE-66 | `mode_native_batch::mode62_64_65_live_orchard_recruit_tank_progression` | partial: trench entry only; retained tick7081 clearance failure blocks completion |

The first actual batch ran once at `5b034faa2db5019554e372341d9d1af3109ae5ec`
(Rust `5742a3e3d30c604f4dd8b16a7abd042b1925b685`): 10 selected,
9 passed, 1 failed, 354 skipped, retries 0. MODE-03 failed because its test
reads `lobby["map"]`; the actual payload names that field `mapName`.
The unchanged failed body is now ignored as an excluded diagnostic, with
no assertion repair or semantic retry. The S3 runner and systemd launcher
were terminal and its inherited lock was released before this report.
Normal output remains at `/tmp/native-feature-batches-0303/mode_impl/native.log`.
The separate earlier parser failure remains at its original 0258 path.
No first-window row is promoted here; passing candidates await prototype review.

## Native behavior batch, remaining 33 owned rows (2026-10-16)

The second window covers every remaining assigned ID below. New tests in
`rust/crates/core/tests/mode_native_later_batch.rs` exercise native-generated
World startup, authored Tutorial map data and actual Tutorial tick sampling.
The existing complete authored-map fixture supplies the cheap exact data
comparison for MODE-70. The three new tests and the existing exact map test passed once at c5eb12f4.
No new Source recorder, captured fixture or provenance bundle is used.

| Row | Native test | Qualification or remaining reason |
|---|---|---|
| MODE-68 | none | incomplete: full journey remains blocked at retained Trench failure; existing entry/predicate evidence unchanged |
| MODE-69 | none | incomplete: retained original full Tutorial fails at tick7081 before bridgehead; no repair or alternative victory claim |
| MODE-70 | `mode_native_later_batch::mode70_tutorial_map_keeps_both_zero_vp_objectives`; `authoritative_maps::authoritative_maps_match_pinned_validation_and_static_terrain` | native PASS at c5eb12f4: exact authored data and Tutorial startup; prototype intent review pending |
| MODE-71 | `mode_native_later_batch::mode71_73_generated_world_starts_homes_and_unattended_guards` | partial: native generation and startup; fresh server seed and World public start absent |
| MODE-72 | none | incomplete: existing exact geography tests retained; no fresh full home-selection qualification in this window |
| MODE-73 | `mode_native_later_batch::mode71_73_generated_world_starts_homes_and_unattended_guards` | native PASS at c5eb12f4: generated guard rosters, hidden actor, alliance and controls; prototype intent review pending |
| MODE-74 | none | incomplete: canonical World phases exist; full fort-transfer capture consequence remains refused |
| MODE-75 | none | incomplete: bounded World elimination exists; full rider/cargo/logistics consequences remain refused |
| MODE-76 | none | incomplete: bounded World victory exists; no fresh native all-threshold outcome test in this window |
| MODE-77 | none | incomplete: complete World snapshot/recovery projection is not exercised here |
| MODE-79 | none | incomplete: existing authored trigger consumers retained; execution in every mode remains unavailable |
| MODE-80 | none | incomplete: only legacy scenario subset represented; general authored validation absent |
| MODE-81 | none | incomplete: general scenario startup validation and public start remain unavailable |
| MODE-82 | none | incomplete: legacy match/time/once subset only; generalized contexts/repeats absent |
| MODE-83 | none | incomplete: legacy time only; remaining condition operators absent |
| MODE-84 | none | incomplete: legacy public Say/Destroy only; generalized actions absent |
| MODE-85 | none | incomplete: general scenario arrivals/reinforcements absent |
| MODE-86 | `mode_native_later_batch::mode86_93_reading_durable_state_does_not_step_or_resample` | partial: read-only getters only; general scenario recipient/objective filtering absent |
| MODE-89 | none | incomplete: closing hold and complete outcome lifecycle unavailable |
| MODE-90 | none | incomplete: typed result components exist; full Horde transport/end lifecycle unavailable |
| MODE-91 | none | incomplete: story components exist; full all-event integration is not exercised here |
| MODE-92 | none | incomplete: fallen-location components exist; full all-mode structure-death endings not exercised here |
| MODE-93 | `mode_native_later_batch::mode86_93_reading_durable_state_does_not_step_or_resample` | partial: real Tutorial tick1/200 sampling only; all-mode finish sampling remains held |
| MODE-94 | none | incomplete: public restart remains guarded; fresh seeds and failed-reload preservation not exercised |
| MODE-95 | none | incomplete: away income branches exist; full server propagation/command lifecycle not exercised here |
| MODE-96 | none | incomplete: complete all-mode snapshot row unavailable |
| MODE-97 | none | incomplete: Unreal HUD owner scope; native-only tests cannot establish playtest presentation |
| MODE-98 | none | incomplete: Unreal boss bar owner scope; native Horde runner absent |
| MODE-99 | none | incomplete: Unreal notices owner scope; several native mode runners absent |
| MODE-100 | none | incomplete: Unreal epilogue owner scope; no client implementation in this checkout |
| MODE-101 | none | incomplete: Unreal report owner scope; no client implementation in this checkout |
| MODE-102 | none | incomplete: Unreal lobby presentation owner scope; native controls alone do not prove rendering |
| MODE-103 | none | incomplete: Unreal objective presentation owner scope; general scenario HUD absent |

## Actual results and review handoff

The second batch ran once at `c5eb12f4b3f6d522f3838361092089521ab55da3`
(Rust `765129ae63640b36c309534c43a1c4470f3ad5b2`): 4 selected,
4 passed, 0 failed, 146 skipped, retries 0. The S3 runner and outer systemd
launcher both returned 0 and released the inherited flock before this report.
Normal output remains at `/tmp/native-feature-batches-0335/mode_impl/native.log`.
The selected functions were:

- `authoritative_maps::authoritative_maps_match_pinned_validation_and_static_terrain`
- `mode_native_later_batch::mode70_tutorial_map_keeps_both_zero_vp_objectives`
- `mode_native_later_batch::mode71_73_generated_world_starts_homes_and_unattended_guards`
- `mode_native_later_batch::mode86_93_reading_durable_state_does_not_step_or_resample`

All 83 assigned IDs are accounted for in the 50- and 33-row windows above.
The six proposed complete-intent candidates are MODE-32, MODE-59, MODE-62,
MODE-65, MODE-70 and MODE-73. All require prototype intent review. The other
77 rows stay held for their listed failure, incomplete native coverage or
Unreal ownership. A passing partial test does not promote its row.

This task's delta begins at `8a5af066`. It changes tests and documentation,
including scoped test registrations. The inherited Tutorial runtime/history
and its still-enabled original full journey are unchanged. That journey's
known tick7081 failure was never selected in these two new batches. Whole
suite or current-main integration success is not claimed. Root owns any
runtime adoption, exact-head Hermes acknowledgment, publication, merged-proof
ledger and later parity status updates. The parity file is byte-identical
to its inherited version. No public action, Source job, new captured fixture,
provenance bundle, semantic repair or semantic retry occurred here.

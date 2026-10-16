# Logistics runtime batch 5

This batch keeps physical logistics below the full match runner and complete
recipient snapshot admission guards. The only shipping change connects manual
convoy retreat to the existing ground-order adapter. It chooses the nearest
active allied source, checks the route before committing the selection, clears
the job and waiting orders, and preserves carried stock. It adds no fields,
commands, events or links.

LOG-39 is exercised by the eight `supply_convoys::manual_tests::manual_*` tests
and the `match_logistics::batch5_tests::intent_log39_*` tests. They cover loaded
and empty trucks, allied completed caches before inventory sync, refused targets,
route checks, queued waypoints, Stop, Resume, malformed orders and retreat.
Supply and Resume use the existing core Match methods. Public command ingress
is unchanged. The old test for unconditional convoy retreat refusal now checks
an atomic missing-source refusal, including a truck beyond the troop cap.

LOG-43 is exercised by the seven `supply_convoys::alert_tests` tests and
`intent_log43_match_scheduler_owner_alerts_stay_below_full_snapshot_guard`.
The component tests cover all alert types, strict shortage thresholds, event
order and counts, per-owner/type cooldowns, retention and scheduler continuation.
The Match adapter proves owner-filtered alerts through the existing internal
logistics projection. This does not open complete logistics snapshots.

LOG-45 and LOG-46 are exercised by `point_supply_intent_tests::intent_log45_*`
and `point_supply_intent_tests::intent_log46_*`. The tests use vehicle corridors,
height steps, enemy control, allied and eliminated spawn sources, capture-square
boundaries, ticks 1/40/41, disabled supply, ordinary ticks and both recipient point rows. They cover
point MP/VP, Classic allied Munitions and territory source exclusion. The existing
`mode_point_services::mode24_radio_accelerates_only_the_uncut_holding_team` and
`mode_point_services::mode24_depot_reinforces_and_repairs_only_allies_within_home_radius`
checks cover radio cooldowns and reinforcement-base exclusion. Non-Conquest
adapters remain guarded. Their refusal tests do not claim playable Horde,
Tutorial, Classic or World logistics matches. The existing
`bases_batch3_economy::base05_supply_runs_on_the_forty_tick_beat_only_in_enabled_point_modes`
checks enabled modes, disabled supply and the Horde/Tutorial/World exclusions
through the scheduling helper now used by the Match adapter.

LOG-20 remains held for Classic/World construction. Core tables list the cache
and engineer builder, but those mode adapters do not execute construction with
physical logistics. No Classic/World construction completion is claimed.

LOG-14 remains held because the native command decoder has no train-boarding
command. Carrier boarding is a different path. The other withdrawal side-effect
guards exist, but partial checks cannot establish the complete row.

Validation results are recorded in the PR body after the checks run.

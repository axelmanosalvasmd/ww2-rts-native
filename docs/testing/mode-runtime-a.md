# MODE runtime A

Baseline: main `9f4685185d1035e019b7d66c8afb49dcc19f2bf5`, prototype
`e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. This branch starts from main.
No private runtime ancestry, recorder, fixture or provenance bundle is imported.

Acknowledged Axel's intent-test decision in issue #5 comment 6206861267.
The previous batch's deferred reasons were read from PR #283's branch before
implementation. The coordinator owns parity status changes, review, CI and merge.

The core now constructs Annihilation and Classic and connects their late rules,
and Assault's existing constructor, to the ordinary deterministic tick runner.
Matching option/Game mode pairs share actor, routing, combat and work phases.
Classic uses queued training and Munitions for support and abilities. Its timer
enters Sudden Death, clears training without refunds, stops construction and
caps repairs while producers decay. Defeat processes players in slot order and
each army in unit-ID order, breaks equal teammate distances by player slot, and
wrecks buildings through the existing terrain owner. Annihilation eliminates before point capture and
income; terminal losing armies remain for the reveal.

World admission stays off. Tests call the core directly. Region capture now
transfers Pillboxes, Towers, Walls and Gates through the canonical unit, spatial
index and matching footprint rows. The transaction includes simultaneous former
owner elimination and finish sampling. The existing gate-tick refusal remains;
this core has no gate timer state. Authored scenario startup and linked carrier, rider, shared-work and enabled
logistics elimination paths remain refused and are not proved by ordinary army
witnesses.

## Intent witnesses

Fourteen rows have passing intent witnesses below. Prototype-intent review and
parity status updates remain with the coordinator.

| Row | Test name |
|---|---|
| MODE-03 | `mode03_each_host_setting_rejects_guests_then_changes_and_broadcasts`; `mode03_factions_belong_to_their_human_and_levels_only_to_ai_seats`; `mode03_invalid_settings_and_mode_specific_restrictions_preserve_the_lobby`; `mode03_attached_match_locks_every_setting_for_host_and_guest` |
| MODE-29 | `mode29_bunker_is_visible_immobile_free_and_never_veteran` |
| MODE-31 | `mode31_assault_runner_cleans_bunker_death_and_freezes_the_decided_clock` |
| MODE-33 | `mode33_annihilation_setup_fortifies_every_player_and_tracks_bunker_maxima` |
| MODE-34 | `mode34_annihilation_defeat_transfers_to_nearest_teammate_and_removes_lone_army` |
| MODE-35 | `mode35_annihilation_terminal_reveal_draw_and_solo_nontermination` |
| MODE-36 | `mode36_out_players_cannot_command_capture_earn_income_or_kill_bounties`; `mode36_world_out_player_commands_income_and_kill_bounties_are_blocked` |
| MODE-37 | `mode37_classic_setup_has_clock_hq_engineers_and_separated_home_fuel_nodes` |
| MODE-38 | `mode38_classic_sudden_death_clears_queues_decays_and_blocks_paid_actions` |
| MODE-39 | `mode39_classic_sites_keep_players_alive_and_defeat_transfers_armies_wrecks_buildings` |
| MODE-40 | `mode40_classic_last_producer_wins_simultaneous_destruction_draws_and_clock_stops` |
| MODE-71 | `mode71_seeded_core_world_creation_supports_both_sizes_and_one_to_six_players` |
| MODE-74 | `mode74_region_capture_contests_locks_drains_and_transfers_all_four_forts`; `mode74_forts_seized_before_former_owner_elimination_survive_the_same_phase` |
| MODE-77 | `mode77_core_metadata_carries_home_goal_cap_regions_stations_and_recovery` |

`elimination_modes_run_connected_ordinary_ticks` additionally exercises the shared
runner on both new modes. `annihilation_tick_eliminates_before_capture_and_income_while_other_teams_play`
and `classic_tick_pays_defeat_tick_income_then_finishes_at_last_production_death`
protect the two late-phase schedules. Additional checks cover recorded purchase,
research and training, Classic retreat and allied recovery, and Munitions costs. `mode30_bunker_support_and_annihilation_howitzer_damage_use_their_reductions`
protects the new howitzer reduction and existing support reduction, but leaves
MODE-30 held because the Horde explosion caller is absent.
`mode75_region_loss_rehomes_allies_and_deletes_armies_without_hq_defeat_or_bounty`
protects the existing ordinary World removal path. MODE-75 remains dropped:
its previously deferred linked-carrier and shared-work removals are still refused.

The World tests retain constructor actors. Explicit unit damage, region ownership
and clock preconditions stage the rule being checked. They do not claim a whole
match, public start, server transport, or client adoption. MODE-71 uses the user's
core-only scope: deterministic seed input, both sizes and 1-6 commanders.

## Dropped rows and differences

| Rows | Reason |
|---|---|
| MODE-01, MODE-02, MODE-16, MODE-17 | Seven-mode construction still needs the separate Horde and Tutorial runtimes. This PR supplies Assault, Annihilation and Classic core paths only. |
| MODE-04 | Needs contract: public start remains unavailable; trusted server attachment remains two-human opposing-team Conquest only. No Assault admission is opened. |
| MODE-06 | All-mode army scaling still depends on Horde and Tutorial constructors. Named lobby settings and Horde Endless restrictions are covered in MODE-03, but do not complete this row. |
| MODE-07 | The trusted server reuses its explicit host seed. Fresh per-match server seed behavior remains unimplemented. |
| MODE-09 | Human-to-Normal-AI handover through leave/handAi is absent. AI difficulty settings alone do not complete the row. |
| MODE-10 | Needs contract: the recorded command boundary has no sandbox Spawn command. Horde wave activation and records belong to the other stream. |
| MODE-11 | Physical logistics and all-mode constructor/server feature defaults remain incomplete. |
| MODE-30 | Horde's live explosion scaling is not connected. This PR fixes Annihilation howitzer damage without claiming the entire row. |
| MODE-75 | Linked rider, cargo and shared-work elimination still needs implementation. The new test covers the existing ordinary removal path and does not resolve those previous-pass objections. |
| MODE-21, MODE-22 | Differs candidates: native recorded purpose RNG streams and explicit phase owners intentionally do not reproduce the prototype's global random draw or unit-ID creation order. No RNG redesign is made here. |

## Validation

The final focused nextest run passed all 60 selected tests, with 626 skipped by
the filter. It covers the new intent checks, both test registries, retained
Assault constructors, ordinary Conquest ticks and recruitment, and World region,
split-phase and decided-result regressions. It used Cargo's `ci-test` profile,
four build jobs, two test workers and zero retries. Formatting and final strict workspace/all-target Clippy both passed.

Qualification used shared S1/S2/S3 locks, their target directories, a 6 GiB memory
cap and zero swap on the Vostro, with local logs retained under
`/tmp/mode-runtime-a-20261016/`. This stream ran no Source process, Node process,
fixture capture, full workspace test run or hosted CI run.

Earlier compilation caught a missing Match initializer field, a queue-projection
type mismatch and an exhaustive support-denial match that needed the new
Munitions arm. The first behavior run found an omitted ordinary-mode field-project
guard and two authored-test errors: a footprint-free building and direct NaN
sentinel equality. Its remaining repeated-generation World test was interrupted
after those failures; it has no passing receipt. The repaired expanded run then
passed 59/60, with an obsolete Assault routing/income refusal assertion left.
Updating that test preserves all constructor comparisons and the Conquest-only
wrapper refusals. The final 60/60 run follows all these corrections.

# MODE-RUNTIME-B2

This batch starts from main `40020946731a2ea17bd85f6ef0ebf8f14e00e493` and reads prototype `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. It imports no code from PR #298.

MODE-83 covers the condition evaluator itself. General authored scenario admission, trigger scheduling, action execution and arrivals remain separate work. This batch does not mark parity tables.

The MODE-83 tests in `scenario_conditions::tests` are:

- `mode83_ownership_capture_and_explicit_side_use_canonical_players`
- `mode83_reserved_region_prefix_does_not_read_a_same_named_point`
- `mode83_loss_counts_and_arrival_state_distinguish_unarrived_and_dead`
- `mode83_area_entry_is_half_open_and_group_scan_charges_twice`
- `mode83_completion_fallback_and_boolean_work_follow_source_short_circuiting`
- `mode83_region_ownership_and_capture_use_region_team_not_vector_index`

These cover every condition branch at `shared/scenarios.js:108-122`, including explicit side overrides, team contexts, stable and reserved region IDs, unarrived versus dead authored units, pending groups, all count comparisons, area boundaries, completion fallback, time thresholds and bounded short-circuit evaluation. Match and authored state remain unchanged by reads.

MODE-91 has partial evidence for the assigned refund and recovery gap:

- `match_construction::tests::mode91_site_refund_updates_wallet_and_spending_without_a_death` creates a paid site, refunds 75 percent, removes its physical sections and metadata, and runs the next ordinary tick without a completion or loss.
- `match_construction::tests::mode91_site_refund_refusals_preserve_paid_site_and_story` checks finished, away, out and decided refusals, plus malformed story and terrain state before publication.
- `mode_runtime_b2::mode91_waiting_refund_reverses_original_mp_charge_and_preserves_active_progress` purchases three actual World jobs, advances the active job, and refunds a waiting identity without resetting active progress. It also checks foreign, active, repeated and post-finish refusals.
- `match_recovery::tests::mode91_recovery_spending_tracks_actual_charges_and_freezes_after_finish` checks paid infantry models and vehicle repair, insufficient funds, real-slot attribution and frozen counters after finish in Conquest.

Prototype anchors: `shared/sim.js:3737-3759` for refunds, `:5662-5668` for recovery, and `shared/story.js:18-24` for counting. These core APIs add no network command admission. The broader seven-counter event matrix and unsupported modes are not qualified by these checks, so full MODE-91 remains held.

Deferred rows:

- MODE-45 and MODE-52 need the Horde runtime under review in #298. Public profile and event projection also needs the agreed consumer boundary.
- MODE-53 needs contract: recipient fog/reveal behavior is excluded from this stream.
- MODE-54 needs the Horde runtime and result lifecycle.
- MODE-81 needs contract: server scenario admission and rollback are excluded.
- MODE-82, MODE-84 and MODE-85 need general scenario state, validated definitions and runtime integration. MODE-84 also includes recipient policy.
- MODE-86 needs contract: objective and announcement recipient filtering is excluded.
- MODE-92 needs the Classic and Annihilation runtimes owned by MODE-RUNTIME-A.
- MODE-93 needs those mode runners plus Horde and Tutorial scheduling.

Validation results are recorded in the PR body after execution. No Source recorder or captured behavior fixture is added.

# RESETS-RESTORE scope and evidence

This branch starts from SAVE-2 #376 at `3767e3e41bad2bcb2ec76bb6c8d8343547677650`.
It composes spectator runtime #372 (`e614103611f044ffb4b32d658402c4c5d353e765`)
and the external AI codec from #392 (`084cbe3d9344e9687a903c001333d8e289d67e97`).
Flight/event delivery #369 (`f1214e12654b98344b6e874a905713cb53f837f0`) is also included so ordinary AI combat can survive recipient snapshots. Their existing claims are dependencies, not new row proof.

## Merge record

The #372 merge was automatic. Its tree equals the precomputed automatic merge tree `6860b85608c6154a55fa247b39ad9df909edfd39`. No hand edits occurred.

The #392 merge had one conflicted path, `docs/contracts/save-format.md`. Both complete appended sections were retained, in order: #376's reconciled ordinary-v1/authored-v2 policy, then #392's external AI memory codec. Only conflict marker lines were removed. The automatic `save_state.rs` union retains #376's elimination compatibility test and #392's static decoder strings and test access. No other merge hunk was hand edited. The later runtime commit updates stale admission prose to describe the composed implementation.

The #369 merge `8332fee594be74893ae0ce4e968b0df788c2f412` has tree `d41c634d0fab49840588d321d0a8fe48916fe6d3`, equal to its automatic `merge-tree --write-tree` result, including all three recipient shot functions and the removal of temporary aircraft restrictions. No hand edits occurred.

## Requested clauses

- NET-25: `net25_seated_takeover_resets_planner_observation_and_helper_without_losing_identity_or_orders` and `net25_restart_recreates_ai_and_human_helpers_and_keeps_spectator_delivery` cover the remaining seated reset and fresh-match helper clauses. Operative identity is already proven by #374; spectator takeover and stale sockets are covered by #378. This branch does not reprove those portions or claim the complete row.
- NET-92: `net92_ai_memory_human_reset_and_surplus_watchers_restore_transactionally_through_victory` and `net92_legacy_ai_defaults_and_spectator_host_pause_survive_load` cover the remaining AI/helper/surplus-spectator clauses. Existing human seat token/fallback matching and raw map/game persistence remain supported by SAVE-2 and the inherited save tests.

No parity status cells are changed. Owner decisions I/J/L and the lossless nested-state/v1 request in #5 comment 6219544899 are acknowledged. Operative coverage in #374/#378 and mission/spectator coverage in #400 are not presented as new proof.

Default authored maps require the compiled `house` tag already repaired in #374. A returning start refreshes recipient routing/fog caches, so the takeover test compares accepted orders, unit state, RNG and tick independently of delivery caches. Callback assertions use the actual simulation snapshot boundary because the first host poll can consume two due ticks. Final validation is reported in the PR body with its producing SHA.

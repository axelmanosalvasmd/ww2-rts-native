# RESETS-RESTORE scope and evidence

This branch starts from SAVE-2 #376 at `3767e3e41bad2bcb2ec76bb6c8d8343547677650`.
It composes spectator runtime #372 (`e614103611f044ffb4b32d658402c4c5d353e765`)
and the external AI codec from #392 (`084cbe3d9344e9687a903c001333d8e289d67e97`).
Their existing claims are dependencies, not new row proof.

## Merge record

The #372 merge was automatic. Its tree equals the precomputed automatic merge tree `6860b85608c6154a55fa247b39ad9df909edfd39`. No hand edits occurred.

The #392 merge had one conflicted path, `docs/contracts/save-format.md`. Both complete appended sections were retained, in order: #376's reconciled ordinary-v1/authored-v2 policy, then #392's external AI memory codec. Only conflict marker lines were removed. The automatic `save_state.rs` union retains #376's elimination compatibility test and #392's static decoder strings and test access. No other merge hunk was hand edited. The later runtime commit updates stale admission prose to describe the composed implementation.

## Requested clauses

- NET-25: `net25_seated_takeover_resets_planner_observation_and_helper_without_losing_identity_or_orders` covers the remaining seated reset clause. Operative identity is already proven by #374; spectator takeover and stale sockets are covered by #378. This branch does not reprove those portions or claim the complete row.
- NET-92: `net92_ai_memory_human_reset_and_surplus_watchers_restore_transactionally_through_victory` and `net92_legacy_ai_defaults_and_spectator_host_pause_survive_load` cover the remaining AI/helper/surplus-spectator clauses. Existing human seat token/fallback matching and raw map/game persistence remain supported by SAVE-2 and the inherited save tests.

No parity status cells are changed. Owner decisions I/J/L and the lossless nested-state/v1 request in #5 comment 6219544899 are acknowledged. Operative coverage in #374/#378 and mission/spectator coverage in #400 are not presented as new proof.

Validation pending.

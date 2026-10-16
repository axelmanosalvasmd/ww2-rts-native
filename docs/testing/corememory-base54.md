# BASE-54 detached fort capture

Prototype: `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`,
`shared/sim.js:1197-1225` and `:1324`; `test-fortress.js:128-151`.
The prototype seizes living hostile Pillboxes, Towers, Walls and Gates inside
the captured region, preserving damage. Gates forget their previous openness
and timer, and footprint ownership changes only where the row still identifies
that fort. Seizure precedes the old nation's elimination.

The explicit `Match::apply_world_regions_with_fort_capture` component uses the
existing transactional region, elimination and finish pipeline. It commits unit
owners, spatial owner membership, matching footprint rows and Gate control reset
after all fallible preparation succeeds. It introduces no persistent field.
Default region APIs refuse captures that need fort transfer before publication.
The World tick preflight, Gate tick guard, snapshots and hosted room admission
retain their existing boundaries.

The detached API uses the complete current unit order and does not charge
income, advance the clock or deliver a recipient snapshot. Its witness calls
cover component behavior only. World rooms remain deferred under owner decision
C. No wire, FFI, C header or recipient shape changes are included.

## Each row literal test mapping

| Row | Test | Behavior |
|---|---|---|
| BASE-54 | `match_world::bases_batch4_fort_tests::base54_world_fort_ranks_are_confined_to_source_shares` | Existing constructor witness: all farthest 40% receive finished Pillboxes toward the nearest home; farthest 15% receive Towers and four Walls on either side. Fixed seed 1821, flattened generated terrain permits every prescribed site. |
| BASE-54 | `match_world::bases_batch4_fort_tests::base54_detached_capture_keeps_damage_and_resets_only_seized_gate_controls` | Actual constructor forts and a canonical Gate transfer with exact damage, completion, position, footprint identity, owner index and Gate reset; friendly, dead and outside-region Gates remain unchanged; repeated phase retains ownership. |
| BASE-54 | `match_world::bases_batch4_fort_tests::base54_detached_capture_survives_previous_nation_elimination` | Captured assets survive the same phase that removes their former owner's remaining army, and finish samples the new ownership. |
| BASE-54 | `match_world::bases_batch4_fort_tests::base54_detached_capture_leaves_replacement_footprint_rows_unchanged` | A footprint row identifying another object retains its owner and position while the captured fort and remaining matching cells transfer. |
| BASE-54 | `match_world::bases_batch4_fort_tests::base54_detached_capture_failure_keeps_gate_owner_and_footprint_atomic` | Invalid story state refuses without changing units, Gate state, footprint rows, regions or wallets. Repairing the story allows capture. |

All tests already register through `match_world`'s existing
`bases_batch4_fort_tests` module. No new registration is required.

## Hand edits to inherited behavior and evidence

The integrated #301 implementation already prepared fort transfers inside the
ordinary region API. Its positive expectations conflicted with #336's retained
refusal contract. The following edits resolve that overlap:

- `match_world_regions.rs`: select fort support explicitly; preserve default
  refusal; validate captured control identity before preparing consequences; reset
  seized Gate controls only on successful publication.
- `bases_batch4_fort_tests.rs`: retain both original tests unchanged and add the
  four detached witnesses above. The default refusal test still checks the damaged
  fort and elimination boundary.
- `mode_runtime_world.rs`: the two existing positive MODE-74 tests select the
  detached API. Other World intent tests keep their original API selection.
- `world_regions.rs` and `world_finish.rs`: honor the fixture's supported marker
  again, removing #301's special exceptions for fort-transfer cases.
- `world_phases.rs`: restore
  `combined_refusal_is_atomic_but_separate_income_is_an_explicit_committed_phase`
  from baseline `9f4685185d1035e019b7d66c8afb49dcc19f2bf5`.
- `world_epilogue.rs`: restore
  `late_fort_refusal_preserves_decided_phase_and_the_original_terminal_owners`
  from the same baseline.

Neither restored refusal assertion is weakened. The removed inherited positive
split-phase and terminal-fort expectations belonged to the default API; positive
fort evidence now names the detached API directly.

## Validation

Execution belongs to the root agent. Requested core filters are `base54_` and
`mode74_`; requested simtest filters are `world_regions`, `world_finish`,
`world_phases` and `world_epilogue`. This subagent has run no Cargo command,
prototype process, recorder, hosted CI, commit, merge or push.

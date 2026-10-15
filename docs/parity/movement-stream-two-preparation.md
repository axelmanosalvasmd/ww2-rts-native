# Movement stream two: prepared checks

`IMPL_MOV_2` prepared this on Vostro in branch `impl/mov-stream-two-de9ab354`,
from baseline `34edf52a7eaa6cb6edba3e802c2190226228276f` and reference
`e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.

This is a source-only checkpoint. No Source recording, Rust compilation or native
test has run. Four native tests and one recorder are prepared for five candidate
rows, with completion pending. The generated fixture stays absent until Root
grants a recorder slot. Its absence fails the new native tests rather than skipping
them. This checkpoint must not be integrated as a passing test change.

The ownership ledger was read before editing. It assigns MOV-41 through MOV-108,
except MOV-98, to this owner. Existing ported rows remain untouched. The companion
`movement-stream-two-predicates.json` copies the existing selected checks for all
67 assigned rows; unselected rows are explicitly not audited in this batch.

## Selected predicates and consumers

| Row | Complete selected predicate | Native consumer prepared | Remaining evidence |
| --- | --- | --- | --- |
| MOV-43 | Original `test.js:83-89`: an editor state with no players or mines finds the open 20 by 20 route, then refuses it after the complete blocking column is set. | `editor_without_players_preserves_both_original_reachability_checks` uses `Pathfinding::find_path` and compares every waypoint against both original calls. | Record the original assertions and run the native test. This is the editor component, not a new Match or Unreal editor interface. |
| MOV-62 | Original `test.js:3511-3531`: all 300 original Massive-scene actors survive all 20 separation passes with the same coordinates, using the original seed 123456 and reference separation oracle. Original `test.js:6677-6690`: the complete 50-rifle dense scene discovers later pairs and preserves its final coordinates after one full Source step. | `complete_crowded_army_and_later_pair_scenes_preserve_every_position` loads each full authored initial state once and feeds native separation output into subsequent passes. It compares every actor ID and exact x/z bits. | Record both complete callbacks and run the native test. The second comparison consumes native separation, not a whole native Match tick; it claims only the selected position predicate. |
| MOV-72 | Complete original `test-transport.mjs:20-35`, including capacity constants, selection of two nearest squads, 100 boarding ticks, cargo flags/count, third-squad refusal, 40 moving ticks, rider position and unload. | No new complete native consumer. Existing `match_carrier_tick` covers related canonical boarding/riding components, but not this entire original purchase/command/snapshot scene. | The recorder retains all original commands, assertions and states. A canonical native purchase/setup adapter plus recipient cargo-flag/count checks is still required. Do not mark this row ported from the recording alone. |
| MOV-79 | Original `generateWorldMap` for all five existing seeds 0, 1, 42, 20261003 and 4294967295, Huge size, six players, teams 0/0/1/1/2/2, scenery false. Compare every station and every line endpoint, ordered cell and ordered region. | `five_original_world_seeds_preserve_complete_ordered_rail_networks` runs `generate_world_from_terrain_seed` and compares full arrays. | Record and run. Existing `original_dressed_geography_and_ordered_rail_paths` and its 20 cases remain unchanged; the new check adds direct cell-array equality instead of relying on line hashes. No train operation or explored-client delivery claim. |
| MOV-83 | Complete post-initialization fighter, attacker and bomber definitions, including nested weapons/AA, plus the whole `CFG.air` object. | `complete_aircraft_definitions_and_air_configuration_match_source` compares the compiled catalog with independently recorded original data, including numeric bits. | Record and run. Static tables do not establish sortie, firing, production or client-model behavior. |

The recorder extracts the original callbacks and their complete helper bodies at
the pin. It does not rewrite them, omit assertions or patch simulation function
bodies. Its wrappers observe imported calls and preserve raw before/after states.
Source text and SHA256 values travel with each named check. Global Source randomness
is fixed at 0.5; the crowded-army callback retains its own unchanged random seed.
The reference checkout remains read-only.

## Missing calls outside this batch

Source inspection found a definite MOV-87 boundary: `stepPlane` selects visible
enemy aircraft when hold fire is off, while native
`match_aviation.rs::preview_aircraft_step` rejects that state with
`aircraft automatic ground-fire actuator is unavailable`. The native routine also
does not select the nearest enemy aircraft as its flight centre. MOV-87 remains
untouched. MOV-84's complete selected check requires actual Classic construction,
purchase and training at two Airfields. Existing direct aircraft construction/base
components do not prove that whole scene. MOV-85/86/88/89 include firing or public
recipient predicates beyond the currently admitted held-aircraft component cases.
These are scope findings, not new gameplay or admission decisions.

## Pending commands and slot requirements

Root must explicitly grant a slot before any command below. At this checkpoint,
the shared ledger grants all three native slots to CMB, MAP and ENG. The worker
did not change the ledger or infer a grant from an idle process.

Run from this branch's isolated worktree. Use Root's granted CPU affinity,
memory/swap limits, process scope and job limits. Recording and native testing
must run serially in that slot.

```sh
WW2_REFERENCE=/home/judiazm/Projects/ww2-rts-native/reference/ww2-rts node harness/movement-existing-checks-fixture.mjs
WW2_REFERENCE=/home/judiazm/Projects/ww2-rts-native/reference/ww2-rts node harness/movement-existing-checks-fixture.mjs --check
cd rust
cargo fmt --all -- --check
cargo test --locked --profile ci-test -p ww2-simtest --test it movement_existing_checks:: -- --test-threads=1
```

The four new native tests include World generation, so Root should grant a heavy
slot for this combined command or split the three smaller tests from the
five-seed generation test under separate grants. The existing ledger's heavy
limits are 8 GiB, zero swap and six Cargo build jobs. Focused limits are 6 GiB,
zero swap and four build jobs. Root must supply the actual CPU set; no set is
reserved here. No full workspace/release/FFI run is requested in this checkpoint.

First failure remains evidence. Do not remove Source assertions, narrow scenes,
inject native expected outputs or relax admission to obtain a pass. Formatting
may require a follow-up edit because no Cargo command has run. Freeze the new
fixture and updated source before the independent reviewer starts. Root retains
separate review, integration, hosted checks and publication.

The local checks performed were JavaScript syntax parsing (`node --check`),
Source snippet/line/assertion inspection, ownership checks and `git diff --check`.
They do not establish native compilation or execution.

## Coordination acknowledgment

Read `docs/agent-coordination.md`, the complete issue #5 comment response, and
PR #16's ABI acknowledgment comments. Acknowledge locally the limits in
`axel-145-ported-readback-129`, the later `julio-restart-150-ported-20261015`
packet at the requested baseline, and the unchanged six-call ABI 2 scope: no new
whole-match, gameplay, spectator/privacy or interface decision is adopted.
No issue or PR comment was posted because Root explicitly reserved publication.

Before a later PR, integrate then-current main with both sides of the shared
movement inventory preserved. Recheck row ownership and status at that boundary.
No existing fixture, assertion, runtime, header, contract, terrain provider,
MOV-09, MOV-98, #199 or #201 changed in this checkpoint.

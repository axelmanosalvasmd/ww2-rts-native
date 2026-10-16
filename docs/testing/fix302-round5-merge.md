# FIX-302 round 5 merge

This merges PR parent `3899de530ce1173bb99fd769b5737b977c726dea` with
main `38c6ba6a8c8f0040e6bd6ea51f812805b7991c8d`, retaining both parents
without rebase or force-push.
The existing PR branch is checked out elsewhere, so this merge uses local branch
`fix/302-round5-merge` and pushes HEAD to `runtime/mov-batch2-20261016`.

Main includes #325, #331, #347, #351, #352, #353 and #356. GitHub reports #341
and #350 OPEN with no merge commit; neither is included in this main parent.

Five textual conflict hunks required hand editing. Locations below refer to
the resolved files. The original conflict starts were DESIGN:1, firing:1158,
and rails:140,356,692.

| Hunk and resolved location | Main side | PR #302 side | Resolution |
| --- | --- | --- | --- |
| 1. `DESIGN.md:1`, `:13` | Seat lifecycle and shared-enemy authored-spawn decisions. | Queued house-entry snapshot compatibility decision. | Retain both main sections, then the complete PR section, with a blank separator. |
| 2. `rust/crates/core/src/match_firing.rs:1158`, `:1266` | Complete Horde `fire_breath`, followed by private `fire_weapon`. | `pub(crate) fire_weapon`, called by the aircraft owner. | Keep main's breath function byte for byte and the PR's crate-visible weapon entry point. The shared body retains both branches' changes. |
| 3. `rust/crates/core/src/match_world_rails.rs:141`, `:149` | `excludes_footprint` checks the signed station yard. | Deterministic open-line `route`. | Keep both methods in `impl WorldRails`; reuse main's extracted yard builder in startup. |
| 4. `rust/crates/core/src/match_world_rails.rs:353`, `:656`, `:1047` | Start of the extracted `rail_yard` helper, interleaved with shared closing braces. | `RailRun::step` and Match train/passenger validation, synchronization and retirement. | Keep the complete PR runtime and its closing braces, then place main's complete helper at module scope before the test module. |
| 5. `rust/crates/core/src/match_world_rails.rs:669`, `:1072` | `rail_yard` returns `out` and closes its function. | Rail departure, unloading and stepping runtime, followed by the gated intent tests. | Retain all PR runtime and the complete original test module. Main's return and closing brace remain in its relocated helper. `#[cfg(test)]` still gates every rail test. |
| 6. `rust/crates/simtest/tests/recipient_effects.rs:190` (semantic merge correction) | The parked, missionless aircraft fixture constructs `AircraftRuntime` without an automatic target cache. | `AircraftRuntime` adds `attack_pick`, initialized to zero for new aircraft. | Add `attack_pick: 0` to the existing fixture initializer. Preserve all recorded inputs, fixture guards and assertions. |

No additional source hunks were hand edited. Byte comparisons confirm that
main's breath function, Gun Pit `weapon_range` and signed `rail_yard` helper
are unchanged, and the PR's complete rail runtime and test module are unchanged.
There is one definition of each shared helper.

The automatic merge retains main's `aircraft::validate`, `aircraft::plan` and
aircraft snapshot tests from #331. It also retains the PR's per-step firing
target clearing, separate `attack_pick`, hold-fire regression and queued Enter
kind 9 with singleton, normalization, finite-coordinate and owner-privacy
guards. No test or guard was dropped or weakened.

Acknowledged locally: `julio-302-ack-3899de53` on issue #5 requests Hermes'
runtime acknowledgment at the old PR parent. The request remains pending;
the historical round-5 ACCEPT does not qualify this merge. No issue post is
made. Public ingress, World hosting, logistics, ABI, header, wire and contract
scope remain as inherited. The 28 claimed and 21 excluded rows are unchanged.

An independent source audit before the fixture-initializer correction found
no concrete integration defect, with zero reviewer Cargo executions, edits or
GitHub writes. The correction adds only the required zero-valued field.

Validation is run from `rust/` on the Vostro under the shared S1/S2/S3 locks,
using only the acquired slot's target directory, MemoryMax=6G and zero swap.
Each command releases its slot when it exits. Free disk space is checked
against the 20 GiB minimum before acquiring a slot.

The first strict Clippy attempt failed before tests with E0599 at
`crates/server/src/room_runtime.rs:106`: the core dependency metadata lacked
`restrict_to_ordinary_snapshots`. The current source defines that public method
outside test gating at `recipient_snapshot.rs:578`. A diagnostic verbose Clippy
attempt reproduced E0599 while reporting core as Fresh and consuming
`libww2_core-e20e574be540a8fa.rmeta`. Workspace-package artifacts were then
cleared inside the acquired slot, preserving third-party dependency artifacts,
before rebuilding. No source edit was made for that diagnostic.

The clean rebuild passed the core/server dependency boundary, confirming stale
metadata caused E0599, then found E0063 at `recipient_effects.rs:184`: main's
aircraft fixture initializer lacked the PR's `attack_pick` field. Hunk 6 fixes
that compile error without changing any test assertion. No tests ran in these
failed Clippy attempts.

The Rust tree under test is `9a28688e885ac1ede0eaeeec0d3dc2e0fdcb4935`.
Only this record and its changelog fragment may change after these checks.

Final commands (from `rust/`):

```text
cargo fmt --all --check
cargo clippy --workspace --all-targets -j4 -- -D warnings
cargo nextest run --locked --cargo-profile ci-test -p ww2-core -p ww2-server -p ww2-simtest -j4 -E 'test(mov) | test(movement) | test(traffic) | test(garrison) | test(carrier) | test(recipient) | test(tutorial) | test(rail) | test(aviation) | test(aircraft) | test(match_tick) | test(firing) | test(horde) | test(room_runtime) | test(commander_ingress)'
cargo nextest run --locked --workspace --cargo-profile ci-test -j2
```

Formatting and strict workspace all-target Clippy pass. Focused tests pass:
`442 tests run: 442 passed (1 slow), 904 skipped` in 110.139 seconds.
The focused run uses only packages touched by #302. The final full workspace
command is invoked once, including FFI, with two jobs. The ten existing ignored
tests retain their annotations; no ignore or test filter was added to that run.

The single final workspace run passed:
`1346 tests run: 1346 passed (1 slow), 10 skipped` in 657.030 seconds.
Both focused and workspace tests used S1. The wrapper released S1 after the
workspace command exited successfully. There were no test failures or retries.
The initial formatting check used S2; Clippy and the final formatting check
used S1. All slots were released after their command batches.

Raw local logs: `/tmp/fix302-focused.log`, `/tmp/fix302-workspace.log`,
`/tmp/fix302-fmt-clippy-final.log`. Diagnostic Clippy logs are preserved in
`/tmp/fix302-clippy.log`, `/tmp/fix302-clippy-verbose.log` and
`/tmp/fix302-clippy-rebuild.log`. No prototype, Unreal, or hosted CI run was
performed by this task. Hermes acknowledgment remains pending.

## Round 6

Merge PR head `8a5c90e199663131e71d333afdd5a3ec56a11df4` with main
`788d911ff00f5a01c1ebc6527166bb51e445f77b`, retaining both parents.
The shared PR branch is checked out elsewhere, so local branch
`fix/302-round6-merge` pushes normally to `runtime/mov-batch2-20261016`.

Three textual conflict hunks required hand editing:

| Resolved location | Main side | PR #302 side | Resolution |
| --- | --- | --- | --- |
| `rust/crates/core/src/match_runtime.rs:656` | `classic_known_buildings` and its team-shared memory comment. | `rail_runs` keyed by train ID. | Keep both fields and the complete memory comment. |
| `rust/crates/core/src/match_runtime.rs:845` | Initialize ordinary match building memory with `Default::default()`. | Initialize ordinary match train runs with `BTreeMap::new()`. | Keep both initializers. |
| `rust/crates/core/src/match_world.rs:317` | Initialize World match building memory with `Default::default()`. | Initialize World match train runs with `BTreeMap::new()`. | Keep both initializers. |

No helper or behavior is replaced in these hunks. Existing tests, guards and
`#[cfg(test)]` gates remain intact. Main's automatic merges retain its combat,
logistics and building-memory changes alongside the movement batch.

Acknowledged locally: `julio-302-ack-3899de53` requests acknowledgment for an
older head. That request and the historical ACCEPT do not qualify this new
merge. Pending capability groups stay closed; this merge adds no ABI, header,
wire, contract, public ingress or privacy policy. No GitHub issue post is made.
The 28 claimed rows and 21 excluded rows are unchanged.

Validation is limited to formatting, strict workspace all-target Clippy and
focused core/simtest tests, from `rust/`, with four jobs. Cargo runs under a
shared S1/S2/S3 lock with MemoryMax=6G, MemorySwapMax=0 and the acquired slot's
target directory. No full workspace test suite is run in Round 6.

The independent read-only source audit found no actionable integration issue.
It performed no edits, Cargo execution or GitHub writes. Comparing the resolved
source with Git's automatic merge tree
`4507aa97d487f8e15cd7ab7989ad0f6e3bad650d` shows only the removal of the nine
conflict-marker lines across the three hunks above. Both sides' source lines
are preserved exactly.

Round 6 validation passed on Rust tree
`23484f7bcaa3e804e570560d1c6fdd7f3a2a8e56`:

```text
cargo fmt --all --check
cargo clippy --workspace --all-targets -j4 -- -D warnings
cargo nextest run --locked --cargo-profile ci-test -p ww2-core -p ww2-simtest -j4 -E 'test(mov) | test(movement) | test(traffic) | test(garrison) | test(carrier) | test(recipient_snapshot) | test(recipient_effects) | test(tutorial) | test(rail) | test(aviation) | test(aircraft) | test(match_tick) | test(world)'
Summary [282.432s] 420 tests run: 420 passed (1 slow), 846 skipped
```

Formatting and strict workspace all-target Clippy passed. The focused run
includes all twelve core Tutorial MODE-59 through MODE-70 checks, 52 tests with
`recipient_snapshot` in their names and both `recipient_effects` tests. These
counts overlap the total. All Cargo commands used S1; the wrapper released the
lock after successful completion. No check failed or needed a retry. Only this
record and the changelog fragment changed after validation.

## Round 7

Merge PR head `82b6b66d78109390c59c27d732da45ed87f6896f` with main
`1bb4601679d94c9bff2c5f953dea04d9bf91cf1e`, retaining both parents.
The clean round-6 worktree and local branch `fix/302-round6-merge` are reused;
the resulting head pushes normally to `runtime/mov-batch2-20261016`.
Main includes #299's aircraft weapons, mixed Attack repair, canonical launch
geometry and single weapon-range helper. Both batches' intended behaviors
remain in the merge.

Acknowledged locally: `axel-302-ack-82b6b66d` covers the previous exact head.
Its content acknowledgment is not reused as qualification of this merge.
The recipient-room aircraft-weapon delivery restriction remains active before
tick and snapshot admission. No pending capability group, public ingress,
World startup, logistics, ABI, header, wire, contract or privacy policy changes.
The 28 claimed rows and 21 excluded rows remain unchanged. No GitHub issue post
is made.

Each hand-edited hunk, including semantic repairs outside conflict markers:

- R7-01: `DESIGN.md:13`: retain main's room-worker supervision section followed by #302's queued-house snapshot compatibility section, with both texts unchanged.
- R7-02: `match_aviation.rs:425`: retain main's catalog-based bomber-salvo validation for Area missions.
- R7-03: `match_aviation.rs:449`: retain main's invalid Attack-target error before mutation.
- R7-04: `match_aviation.rs:575`: retain #302's deterministic visible-enemy fighter pursuit around the mission center.
- R7-05: `match_firing.rs:1384`: reuse main's `weapon_launch_height` helper for ordinary launches.
- R7-06: `match_firing.rs:2385`: retain both the aircraft kind/state consistency guard and main's airborne-only launch guard.
- R7-07: `match_firing.rs:2449`: reuse main's `weapon_launch_height` helper for salvo launches.
- R7-08: `match_tick.rs:405`: retain main's construction refusal before automatic garrison retreat.
- R7-09: `commander_ingress.rs:338`: retain main's complete comment describing the unchanged host stance restriction.
- R7-10: `room_runtime.rs:104`: restrict aircraft weapons before tick validation, retaining #302's ordinary-snapshot restriction call.
- R7-11: `match_aviation.rs:73`: remove main's redundant automatic-target map and removal hook; #302's `AircraftRuntime.attack_pick` remains the sole automatic-selection cache.
- R7-12: `match_aviation.rs:680`: replace #302's duplicate weapon implementation with main's `step_aircraft_weapons`, gated by the recipient-room restriction after movement; preserve per-step visible-target clearing.
- R7-13: `match_aircraft_ticks.rs:138`: remove the second weapon call from the turn wrapper so each aircraft step can emit at most one volley.
- R7-14: `match_aircraft_ticks.rs:206`: use `AircraftRuntime.attack_pick` in main's canonical weapon helper and clear the visible target when the cached choice becomes invalid.
- R7-15: `match_aircraft_ticks/tests.rs:158`: initialize `attack_pick` in main's new fixture and inspect the canonical cache in the existing test; add assertions that an out-of-range target clears while the cached choice remains unchanged.
- R7-16: `match_firing.rs:976`: remove duplicate inline launch-cell and clipped-cell traversal functions, retaining main's identical extracted geometry implementations and existing tests. The first Clippy run exposed E0592/E0034 from this automatic-merge overlap; no tests ran before the repair.


Commands:

- R7-17: Original conflict at lines 1127-1131, shared-target preflight comment. Resolved at line 1127 using main's wording: targets are validated before either command branch. Kept the existing target visibility, alliance and health guards.
- R7-18: Original conflict at lines 1476-1551, late Attack command match arms. Removed HEAD's two late Attack arms. Main already dispatches Attack before this match, so retaining those arms would make the aircraft queue append unreachable and duplicate the ground dispatch. The final match retains main's unreachable Attack classification; Escort and Retreat remain at resolved line 1508.
- R7-19: Semantic collision in the automatically merged early Attack transaction, resolved lines 1240-1310. Retained main's raw ground IDs, selection limit, aircraft classification and canonical `ground_attack_command` call so missing controls, field-work consistency, repeated path capacity and physical ground exits retain canonical preflight. Moved HEAD's aircraft queue append here, after the ground helper accepts the transaction. Queued attacks clear aircraft follow, retain the eight-order cap and store singleton normalized Attack commands with `queue: false` and the validated target coordinates. Retained Needs/QueueFull as nonfatal ground results while rejecting all other ground denials before aircraft writes. Return success when either group accepts; preserve the ground denial when no aircraft queue can accept. Immediate aircraft orders continue through prepared `apply_aircraft_order` only after canonical ground preflight. No second aircraft command implementation or ground helper was added.

Carriers and reactions:

- R7-20: `rust/crates/core/src/match_carrier_tick.rs:52` (original conflict line 52): retain movement's validation and boundary/operative rejection for every existing carrier row, including dead rows. Then retain main's live-parent split, reciprocal cargo checks, cargo-wide `prepare_carrier_ejections(carrier)` when metadata exists, and singleton `prepare_carrier_ejection(id)` only when carrier metadata is absent. The existing-parent validation runs before this split; the redundant later live-parent validation is removed.
- R7-21: `rust/crates/core/src/match_carriers.rs:477` (original conflict line 477): retain movement's existing-carrier validation, boundary guard and live reciprocal relation guard. Keep main's automatically merged cargo-wide `eject_carrier_riders` path, publication of all environment effects and `OnFoot` return. Keep the singleton `Ejected(Hurt)` path when carrier metadata is absent. Naval boarding, shore/cliff checks and explicit manual naval unload rejection remain from movement; main's cargo-wide ejection functions remain intact.
- R7-22: `rust/crates/core/src/match_carriers/tests.rs:627` (original conflict line 627): retain main's metadata-present `OnFoot`, cleared riding/carrier/cargo and 30 percent damage assertions. Preserve movement's `Ejected(Hurt { killed: false, .. })` and cleared-riding assertion in a second setup where the carrier row and metadata have both disappeared; also assert the rider carrier link clears. Both behaviors remain asserted within the existing test.
- R7-23: `rust/crates/core/src/match_reactions.rs:88` (original conflict line 88): retain main's comment describing the shared staged house exit.
- R7-24: `rust/crates/core/src/match_reactions.rs:99` (original conflict line 103): retain main's garrison branch calling the canonical `ground_retreat_command`, propagating errors and denials before returning. Remove the now-unreachable independently implemented garrison exit branch at the original lines 127-185; ordinary retreat uses `reaction_home`, `command_route` and `apply_retreat_route`, matching main. Ground-order identity/capacity checks and staged exit publication remain owned by `match_ground_orders.rs`; air/riding rejection and ordinary field-work/build/path guards remain here.
- R7-25: `rust/crates/core/src/match_reactions/tests.rs:305` (original conflict line 305): keep movement's assertion that rejected retreat preserves `riding`. All test bodies from both parents remain, including movement's wounded-garrison automatic-retreat test. Keep the existing movement test name `reaction_timer_and_unsupported_retreat_preserve_canonical_state`; main names the same test `reaction_timer_and_unsupported_exit_preserve_canonical_state`.

Simtest:

- R7-26: `rust/crates/simtest/tests/match_aircraft_runner.rs:169`, fixture loop and case setup: preserve six original cases, add case 6 using a genuinely published bomber and valid Area mission. Keep case 3's malformed fighter Area rejection from main. This retains both valid Area admission intent and salvo eligibility protection.
- R7-27: `rust/crates/simtest/tests/match_aircraft_runner.rs:218`, original conflict at HEAD lines 214-222: stance enables weapons for the positive patrol and valid bomber Area cases (2 and 6). Malformed fighter Area case 3 keeps main's pre-write refusal.
- R7-28: `rust/crates/simtest/tests/match_aircraft_runner.rs:235`, original conflict at HEAD lines 236-265: combine main's four-tick advancement, unchanged preflight clock/pending queue, no command denials, firing-event drain and fault checks with PR302's exact first command count, next queue tick and persistent mission assertions. All malformed-case no-write assertions remain below.
- R7-29: `rust/crates/simtest/tests/match_aviation.rs:200`, original conflict at HEAD lines 200-227: retain main's invalid fighter Area direct-admission refusal and unchanged snapshot, followed by valid unheld patrol, sortie, ammunition and firing-event assertions. Keep original later malformed AA tests.
- R7-30: `rust/crates/simtest/tests/match_aviation.rs:393`, original conflict at HEAD lines 396-505: retain main's entire mixed Attack canonical-project cancellation and all-selection preflight test, including both selection permutations, every failure case and full no-write comparisons.
- R7-31: `rust/crates/simtest/tests/match_aviation.rs:498`, original conflict at HEAD lines 526-623 and adjacent setup: restore main's full six-command matrix and malformed project/aircraft guard loops. This also retains PR302's mixed Retreat refusal and no-write assertions. Rename combined test to reflect admission plus protection. Main Move/Stop/Attack/Stance and Retreat assertions remain. Case 3 now positively checks admitted mixed queued Move, normalized singleton orders with queue false, unchanged existing orders, complete snapshot equality apart from appended order rows, unchanged reserves and supply state, retained project and no path requests. At line 672 replace main's obsolete start_queued_orders refusal with successful normalized patrol activation, empty consumed queue, exact goal and Out sortie; preserve subsequent empty-queue and prepare calls.
- R7-32: `rust/crates/simtest/tests/match_aviation.rs:883`, original conflict at HEAD lines 847-851: use main's more precise read-only automatic weapon admission label. PR302's positive weapon and queue preflight coverage and all malformed/dead checks remain.
- R7-33: `rust/crates/simtest/tests/match_tick.rs:748`, original conflict at HEAD lines 749-800: retain main's automatic-retreat candidate, reaction home, path generation/path, timers, every tick's snapshot, commands, queue count and fault assertions. Add PR302's explicit validate_tick_state admission, report tick and next_tick assertions. Compare firing/ability cooldowns with the captured pre-tick state (same values as main's literals). Every unsupported actor guard and no-write assertion remains.

- R7-34: `runtime_combat_batch3.rs:653`: initialize `attack_pick: 0` in main's new off-map aircraft fixture; all assertions remain unchanged.
- R7-35: `simtest/tests/match_aviation.rs:198,500`: restore mission-type imports needed by the retained Area-admission and queued-sortie assertions. The second Clippy run reported these missing imports and the fixture initializer; no tests ran before these repairs.

- R7-36: `match_carriers/tests.rs:145,173`: preserve MOV-76's survivor damage, cleared relations and deep-water death assertions through main's canonical cargo-wide producer. Require `OnFoot` plus exactly one published Hurt event with the same survivor/death result. The singleton `Ejected(Hurt)` return remains asserted separately in R7-22. The first focused run stopped here: 52 passed, 1 failed, 334 selected tests unrun. This repairs the event-observation mismatch without changing runtime behavior or reducing coverage.

- R7-37: `match_carriers.rs:631`: dispatch dead cargo preflight for an exact World mode pair and registered rail run to the existing `validate_rail_tick_actor`; all other cargo retains main's `prepare_carrier_ejections`. The existing rail phase ejects train passengers before generic retirement. No global carrier guard or World hosting admission changes. The second focused run stopped at the unchanged MOV-82 wrecked-train regression: 180 passed, 1 failed, 206 selected tests unrun. This repairs the automatic merge's ordinary-carrier preflight crossing the World rail ownership boundary.

- R7-38: `simtest/tests/match_aviation.rs:550`: assert both queued destinations and canonical follow clearing for mixed queued Move before comparing every other snapshot field. The third focused run stopped here: 354 passed, 1 failed, 32 selected tests unrun. Runtime behavior is unchanged; the merged positive queue case now checks the dispatcher's existing follow-clearing contract explicitly.

- R7-39: `simtest/tests/match_aviation.rs:576`: explicitly require forced-supply hold clearing when a mixed queued Move is admitted; keep reserves, supply-escape, project and path-request assertions intact. Current native `match_commands.rs:1363` and pinned prototype `e4e579a` `shared/sim.js:3414` clear this hold for Move regardless of queue. The one-test diagnostic exposed the stale hold expectation before the full selection ran. No runtime change.

Round 7 final validation passed on Rust tree
`b14272e32e1fce153c49f7a9a72c784bfe18bfeb`:

```text
cargo fmt --all --check
cargo clippy --workspace --all-targets -j4 -- -D warnings
cargo nextest run --locked --cargo-profile ci-test -p ww2-simtest -j4 -E 'test(mixed_aircraft_selections_preserve_canonical_supply_and_control_boundaries)'
cargo nextest run --no-fail-fast --locked --cargo-profile ci-test -p ww2-core -p ww2-simtest -p ww2-server -j4 -E 'test(aviation) | test(aircraft) | test(firing) | test(movement) | test(mov) | test(traffic) | test(rail) | test(carrier) | test(garrison) | test(recipient_snapshot) | test(recipient_effects) | test(room_runtime)'
Summary [111.587s] 387 tests run: 387 passed (1 slow), 1068 skipped
```

The selected run passed 326 core, 49 simtest and 12 server tests. The preceding
queue diagnostic passed 1/1 and overlaps the selection. Formatting and strict
workspace all-target Clippy passed. All final commands ran from `rust/` under
S1, with four Cargo jobs, MemoryMax=6G, MemorySwapMax=0 and the matching slot
target directory. The wrapper released S1 after success. No full workspace
test suite ran. The failed compile and test attempts are recorded above,
with raw local logs in `/tmp/fix302-r7-clippy-initial-failure.log`,
`/tmp/fix302-r7-clippy-second-failure.log`,
`/tmp/fix302-r7-test-first-failure.log`,
`/tmp/fix302-r7-test-second-failure.log`,
`/tmp/fix302-r7-test-third-failure.log` and
`/tmp/fix302-r7-queue-diagnostic-failure.log`. The passing receipt is
`/tmp/fix302-r7-checks.log`. No prototype execution, Unreal run or hosted
CI qualification is claimed.

After validation, main advanced only through the status-docs PR #361 to
`97f8ed8a488a5a2474abbc552ace270b175440d8`. A second normal merge keeps
that main head as a parent. It auto-merges without conflicts and changes only
`docs/parity/combat.md`, `docs/parity/server.md` and its existing main changelog
fragment. No manual parity edits are made. The Rust tree remains exactly
`b14272e32e1fce153c49f7a9a72c784bfe18bfeb`, matching all passing checks above.
Only this merge record is appended after that source-tree verification.

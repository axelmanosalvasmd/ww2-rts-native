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

# CMB-61 missing flame components, source-only preparation

## Current result: three exact tests and strict lint pass

Producing head: `25c76d6c0bb5be6384d95ee2db3c63847baed07f`, a clean direct child of `281f4b36`. Rust tree: `ec3b8ac10b09a8090071bd07534c8516ec6206e2`. The authorized metadata-only recursive comparator uses exact f64 bits for numbers, exact object key sets, ordered equal-length arrays and strict nonnumeric values, without mutation. Replay comparisons, every outcome assertion and integer identity checks elsewhere remain byte-identical. Authored inputs, Source, both fixtures and the old firing suite remain exact.

| Granted check | Actual result at producing head | Predicate scope |
| --- | --- | --- |
| Workspace fmt check | PASS, exit 0 | Formatting only |
| `match_firing_flame::cmb61_flame_launch_accuracy_ignores_trench_and_garrison` | 1 passed, 0 failed, 219 filtered | Six launch scenes in Default/Conquest; all 384 state/RNG checkpoints plus the final flame-equal/rifle-different paired velocity assertions |
| `match_firing_flame::cmb61_flame_target_value_has_decisive_one_point_five_preference` | 1 passed, 0 failed, 219 filtered | Ten choice scenes in both modes; all 20 state/RNG checkpoints and final selected-target/no-launch assertions. Covered target at 12 m wins versus 8.1 m open target and loses versus 7.9 m, with open/rifle/unseen/dead controls |
| `every_test_file_is_compiled` | 1 passed, 0 failed, 219 filtered | Actual simtest integration registration guard |
| `cargo clippy --locked --workspace --all-targets -j4 -- -D warnings` | PASS, exit 0 | Fresh granted strict workspace/all-target invocation |

Exactly three unique native test instances ran in this renewal. Source reruns 0; old damage/suppression or firing test repeats 0; full/release/discovery 0. Authentic Source generation/check remains the earlier two successful executions at preparation 7a762f65, frozen in a5411f79 with 16 scenes and 202 checkpoints. Both older metadata test failures remain 0 passed/1 failed on 556a2ad5 and d01d213a. Earlier partial traversal is not reclassified as a test pass.

Raw logs: `/tmp/cmb61-flame-s3-25c76d6c/{fmt,accuracy,choice,registry,clippy}.log`. All commands run serially under S3, CPUs 8-11, 6 GiB memory, swap 0, quota 400%, jobs 4, threads 1 and the current guard's exclusive S3 target/lock with 20 GiB floor. Native dep-info `it-6576708762656383.d` and Clippy dep-info `it-884b87c31258606f.d` include the new consumer. Workspace provenance under key `cde5f99448f48ff551b5cdd7` binds the compiler to this absolute checkout and producing Rust tree.

CMB-61 launch accuracy and target preference now have these focused passes for Root review. Damage 9/13.5 and flame suppression retain their existing producing main 396f component proof, without reruns. Actual CMB-61 status remains in progress; Root still owns whole-row review, any status change and publication. CMB-69 remains only an unapplied Rust/Status proposal. All 160 combat rows and their literal acceptance remain unchanged. Local ack of Hermes 155 preserves the pin, contracts and Root queue 256 then 259; this stream makes no whole-match, runtime or interface decision. The sections below are historical checkpoints.

## Renewed S3 result after the authorized hitAt repair

Tested head: `d01d213a999c2d1cacbc3e50be79e8e7b009e857`, a clean direct child of `0563eb91`. Its Rust tree is `f98be6df380f133240d36cd8acacc9fd539ca29d`. Only the new consumer's metadata comparison changes: every actor's `hitAt` must have equal `as_f64().to_bits()` on both sides, then only those verified cells are canonicalized in local metadata clones for strict whole-JSON equality. Every other field/type remains strict. The entire consumer outside that block, all replay assertions, authored inputs, both Source recorders/fixtures, old tests, registry, parity table and CMB-69 proposal remain exact. No metadata clone initializes native state.

Fmt passed. The exact accuracy test compiled and failed at `match_firing_flame.rs:56` on case `choice-flamer-open-below`: Source `cooldown=1000000000` is an integer, authored `cooldown=1000000000.0` is a float. The `hitAt` checks pass. The failure occurs before the choice group filter, after all six launch scenes have traversed their two-mode replay loops. The helper never returns, so the test's final paired-velocity assertions remain unexecuted. This is partial traversal, not a passing accuracy test or complete row proof. Actual result: 0 passed, 1 failed, 219 filtered out, exit 101.

STOP. No cooldown normalization or other repair is applied. Target-choice, simtest registry and fresh strict workspace/all-target Clippy remain unrun. No Source, old damage/suppression, full/release or discovery execution occurs in this renewal. CMB-61 remains held; CMB-69 stays an unapplied proposal. Raw logs: `/tmp/cmb61-flame-s3-d01d213a/fmt.log` and `accuracy.log`. The earlier failure logs and authentic 16-scene/202-checkpoint fixture remain unchanged.

Dep-info `it-ccf03549903ef5ff.d` includes the flame consumer and authored inputs. The compiler manifest is this checkout's `rust/crates/simtest`, bound to the tested Rust tree above by workspace key `23cb40ba455b7118b8844974`. All granted jobs use S3, CPUs 8-11, 6 GiB memory, swap 0, quota 400%, jobs 4, threads 1 and the current guard's exclusive target/lock and 20 GiB floor.

## Terminal result under the S3 grant

Producing native head: `556a2ad5e24e22b9e5c4736813b0dbf8da978d49`, Rust tree `c30a8070c28d257273acb10f2c54d74b85e35b82`. Authentic fixture checkpoint: `a5411f799a8887cd83bce2b8bba192f5a69d8a95`. Source generation and serial `--check` each passed at preparation `7a762f65`: 16 actual scenes and 202 actual checkpoints. The first fmt check found two whitespace-only wraps; they were corrected with complete non-whitespace token preservation. The fmt recheck passed at `556a2ad5`.

The first exact native test, `match_firing_flame::cmb61_flame_launch_accuracy_ignores_trench_and_garrison`, compiled and failed: 0 passed, 1 failed, 219 filtered out, exit 101. At `simtest/tests/match_firing_flame.rs:44`, `assert_eq!(output["input"], *input)` sees Source integer `hitAt=-1000000000` and authored JSON float `hitAt=-1000000000.0`. This occurs before `common::make_with_mode` or any firing replay. No firing behavior comparison has passed or failed yet. Preserve the inputs, predicate and authentic fixture exactly; no repair or rerun is authorized by this grant.

STOP applies. Target-choice, the simtest registry guard and fresh strict locked workspace/all-target Clippy remain unrun. CMB-61 remains held for the new launch/choice proof; its old damage/suppression evidence remains attributed to main 396f. CMB-69 remains an unapplied proposal. The raw terminal log is `/tmp/cmb61-flame-s3-7a762f65/accuracy-556a2ad5.log`. Source and fmt logs are beside it. No old Source recorder, damage/suppression test, full/release run or discovery was executed.

Dep-info `it-8f64e9b318a856f5.d` names the new flame consumer and authored inputs. Compiler provenance in `/home/judiazm/.t3/handoffs/ww2-native-cargo-wrappers/1fbc157023959f0c051d20e6/compiler-invocations.jsonl` binds the compiled simtest target to this absolute checkout and the frozen Rust tree above. S3 used CPUs 8-11, 6 GiB memory, swap 0, CPU quota 400%, four Cargo jobs and one test thread. The guard enforced the S3 target, exclusive lock and 20 GiB disk floor.

## Granted formatting follow-up

The first S3 formatting check at `a5411f79` failed only on two new-consumer line wraps. Apply those exact whitespace changes under the explicit formatter-only allowance. Static comparison confirms every non-whitespace byte, assertion and literal remains unchanged. Keep the raw failure in `/tmp/cmb61-flame-s3-7a762f65/fmt-check-a5411f79.log`. No compile or behavior test has run at this point.

## Authentic Source capture after preparation

S3 granted generation and a serial byte check at frozen `7a762f6573891a2564cbd7aa4e0ee8566c0ad300`. Both commands exited 0. The authentic new fixture contains 16 scenes and 202 checkpoints, with x64 Node v24.21.0 and Source `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. The recorder and inputs remain exact. Raw logs: `/tmp/cmb61-flame-s3-7a762f65/source-generate.log` and `source-check.log`. Both invocations held the exclusive S3 flock and ran with CPUs 8-11, 6 GiB memory, zero swap and 400% CPU quota. This fixture is frozen before Cargo; native results remain pending. The source-only statements below describe the original preparation checkpoint, not this later capture. CMB-61 stays held and the CMB-69 patch stays unapplied.

Base: `396f6d8a7014bd4a5ebe64f0a12ccd66865bf452`. Original Source: `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. Owner: AUDIT_COMBAT, CMB-61 and the private CMB-69 proposal only. Frozen `e490fb9a37ad307d76f94d5e499809bde85a0b3b` and `11a28e94f126a7bcbcb3d82b00ab649d8cbb8d54` remain on their original branches.

Root's independent review at `/tmp/roster16-reviewer1-cmb61-69/REVIEW.md` holds CMB-61 for launch accuracy and target value. Its earlier damage/suppression proofs remain valid. This checkpoint adds recorder and consumer source, with no generated `rust/crates/simtest/tests/fixtures/match-firing-flame.json.gz`. The consumer fails explicitly when that authentic fixture is absent. No expected observation initializes or repairs the native match. There is no runtime, terrain-rule, guard, interface or randomness-policy change.

## Complete predicate map

| Row / predicate | Original source and native consumer | Evidence or pending check |
| --- | --- | --- |
| CMB-61: flame accuracy ignores cover, specifically open/trench and garrison | Original `shared/sim.js:4161-4174` chooses dispersion through unchanged `fire`; native `core/src/match_firing.rs:1127-1159` is reached by public `apply_unit_firing` | The existing `match_firing::default_and_conquest_targeting_and_launches_match_original_source` already covers plain/wall-cover flame launches at main 396f. New `cmb61_flame_launch_accuracy_ignores_trench_and_garrison`: six independently named launch scenes, 32 readiness pulses each. Flame launch velocity histories must equal across all three target states. Corresponding rifle histories must differ, proving that covered setup affects an ordinary weapon. Every snapshot and RNG state also compares with authentic Source. Recording, compilation and execution pending. |
| CMB-61: flame suppression ignores cover | Original `shared/sim.js:4219-4222`; existing `core/tests/combat.rs` test `suppression_combination_guards_and_write_order_match_reference` | Existing committed Source fixture covers all 64 flame combinations. Already passing at main 396f; preserved exactly, not rerun or claimed as new proof. |
| CMB-61: open damage 9, trench/garrison damage 13.5 | Original `shared/sim.js:4195-4202`; existing `core/tests/combat.rs` test `damage_and_every_scalar_mutation_match_original_functions` | Existing damage entries 737/738 pair open/trench, 739/740 garrison/both. Full scalar mutation and event comparisons already pass at main 396f. Preserved exactly. |
| CMB-61: x1.5 target value for trench/garrison infantry | Original `shared/sim.js:3933-3945` in unchanged `pickTarget`; native `core/src/match_firing.rs:1831-1869` through `apply_unit_firing` | New `cmb61_flame_target_value_has_decisive_one_point_five_preference`: equal rifle targets, farther at 12 m and nearer at 8.1/7.9 m. The same positions are paired across open, trench and garrison states, with house occupants at their actual cell center. Open controls choose nearer. Covered farther target wins below the 1.5 distance-ratio boundary and loses above it. Rifle, unseen and dead-target controls choose the nearer valid target. No shots or Combat draws are allowed in these choice scenes. Recording, compilation and execution pending. |
| CMB-69: literal selected seeded run, same death tick per unit ID | Original `shared/sim.js:5343,5672-5699`; existing `simtest/tests/match_tick.rs:271-281,339-344,470-480` calls actual `advance_tick`, accumulates every retirement and exhausts all checkpoints | Existing `match_tick::connected_tick_phases_match_actual_reference_steps`, seed 701 Combat and environment 305419896. Case 7 runs 800 steps with 15 checkpoints; no deaths through 400, unit 2 dies at 483, retained at 600/800. Reviewer found this eligible for the unchanged selected check. Only an unapplied Rust/Status proposal is stored here. No claim for all death families or whole-match statistics. |

CMB-61 remains `in progress`. Its literal acceptance remains `new port test: flamer vs squad in trench and in open (no JS test exists)`. The new observations cover firing components; they do not introduce a new whole-match or Unreal gate, and the previously passing damage and suppression checks are not weakened or replaced.

## Source schedule and authored setup

`harness/match-firing-flame-inputs.json` contains the map, every initial unit field, target cells, garrison membership and visibility. Both sides read this same authored setup. Source uses ordinary `createGame` and `spawnUnit`, then existing `setCell` for target trench/house authoring. Native uses the unchanged shared firing constructor, existing `World::set_cell`, and authored garrison membership. Garrison occupants stand at the actual house cell center; the recorder asserts that invariant. Initial HP is 66 for a flamer and 100 for a rifle squad. Units start settled for 20 seconds, unsuppressed, with zero veterancy and no recent threat. Equal target kind, HP and facing avoid armor, threat and role differences between choices. Source constructor randomness remains the existing constant setup value; firing randomness uses only the existing seed 701 adapter.

The recorder copies the existing original-block loader, bit snapshots and bounded Combat adapter from `harness/match-firing-fixture.mjs`. It appends an exported wrapper around the unchanged firing block, identified by its original boundaries. It does not translate `fire` or `pickTarget`, alter their bodies, invent outcome rolls or feed observations back into Source. Node must be x64 v24.21.0 and the tracked Source checkout must be clean at the exact pin before and after recording.

For each launch scene, phases 1 through 32 set the tick, clear launch events, apply an authored cooldown-zero readiness pulse, rebuild original claims, subtract 0.05 from the shooter's cooldown/retarget timer and execute the unchanged original block with moving=false and dt=0.05. Flights remain live and unadvanced so only launch accuracy is measured. Each flame scene requires 32 rounds; each rifle control requires 160 rounds. The recorder bounds actual RNG draws to one through three per launched round and compares paired flame RNG end states. Each choice scene runs that same component schedule once, retaining cooldown 1e9, target 0 and no attack order, so the original target picker alone selects the target without a launch or random draw.

The native consumer follows the same authored pulses, `begin_firing_tick`, `advance_firing_timers(1,0.05)` and `apply_unit_firing(1,false,0.05)`. It compares the complete existing firing snapshot and actual Combat stream state at every phase, consumes all 202 Source checkpoints, and repeats each scene in Default and Conquest (404 native comparisons). Snapshot coverage includes units, timers, aim, claims, full flight and launch payloads, environment, terrain values/versions, ammo and salvos. Authored inputs are separately matched to their recording metadata. These counts describe prepared coverage, not executed results.

There is no missing firing provider identified in the inspected source. Authentic Source acceptance of the setup, compilation and actual native comparison remain unverified. Any refusal or mismatch stays a blocker; do not change runtime, terrain or guards to manufacture a pass.

## Preservation and CMB-69 proposal

The ordinary `harness/match-firing-fixture.mjs`, `simtest/tests/fixtures/match-firing.json.gz`, `simtest/tests/match_firing.rs` and shared firing helper are untouched. All 131 old traces, 668 Source checkpoints, 1,336 Default/Conquest comparisons and ordinary test assertions remain exact. Existing damage/suppression/tick fixtures and tests also remain exact. All 160 combat rows, their literal acceptance and actual statuses stay byte-identical to fixed main, including CMB-49 and CMB-69.

`docs/proposals/cmb69-rust-status-only.patch` is an unapplied private proposal. Its only suggested edits are CMB-69 Rust and Status cells; behavior, Source, Unreal, dependencies and literal acceptance cells remain exact. It cites the existing actual runner and [main push run 38740304188](https://github.com/axelmanosalvasmd/ww2-rts-native/actions/runs/38740304188), exact 396f6d8a, success 9/9. Retained reviewer logs show the named tick test PASS at Ubuntu 747, Windows 746 and macOS 752. The existing 563-pass/zero-skipped workspace result belongs to main 396f, not this candidate. No completed gate is rerun here. Publication, any real status promotion, fresh final-head checks and the ordered main integration remain Root-owned.

Local coordination acknowledgment: Hermes 152 and 153 preserve the pin, agreed contracts and queue 255 then 256 then 259. This stream keeps its fixed 396f base and makes no network, ABI, role, privacy or gameplay decisions. No external acknowledgment or publication was sent. Direct CHANGELOG/Unreleased and DESIGN updates follow the explicit task instruction.

## Exact pending commands and slots

No commands below ran. A future Root Source slot must first provide x64 Node 24.21.0 and grant this recorder. Run from the repository root:

```sh
WW2_REFERENCE=/home/judiazm/Projects/ww2-rts-native/reference/ww2-rts node harness/match-firing-flame-fixture.mjs
```

The recorder creates only the supplemental fixture. Its optional `--check` mode compares that same authentic recording without writing; it also requires a Source grant.

A later native slot must use Root's existing shared target assignment, not a per-task target. After authentic fixture capture, run from `rust/`:

```sh
cargo test --locked --profile ci-test -p ww2-simtest --test it match_firing_flame::cmb61_flame_launch_accuracy_ignores_trench_and_garrison -- --exact
cargo test --locked --profile ci-test -p ww2-simtest --test it match_firing_flame::cmb61_flame_target_value_has_decisive_one_point_five_preference -- --exact
cargo test --locked --profile ci-test -p ww2-simtest --test it every_test_file_is_compiled -- --exact
```

All three functions exist in this checkpoint. `simtest/Cargo.toml` has `autotests=false` and an explicit `[[test]] name="it"`; `tests/it/main.rs` adds one real path/module pair for `match_firing_flame`. The existing `every_test_file_is_compiled` guard checks that declaration against on-disk test files. No additional unimplemented function is named as a command. No fmt, discovery, full/release, benchmark, native executable or Source job is authorized by this checkpoint.

New execution count: Source 0, Cargo 0, rustc 0, native/tests 0, fmt 0, discovery 0. Two behavior tests are authored and registered, with zero new passes. CMB-61 stays held and CMB-69 stays an unapplied proposal.

# CMB-61 missing flame components, source-only preparation

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

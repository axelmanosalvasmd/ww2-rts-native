# First targeted aligned attack-move scenario

Private child of exact `5a8ce0e22c5771f3619f7f52fb0705c9afe2fe8a`. The public #261 branch/head, old 600-tick R1 scripts, snapshots, tests, traces and receipts remain unchanged. Preserve MODE-21 checkpoint `34317f3a` without incorporating or revising it. This task covers CMB-91/MOV-49 first. No row status or publication changes.

Local acknowledgment of user15:51 and the full owner decision 6194079987/narrowed proposal 6182315380: aligned inputs and complete row checks are both necessary. The plain-Move control from pinned `test.js:725-741` is required, alongside halt, actual foe death, resume and arrival. The old test's cleared actors, forced positions and 15 HP foe are not reused.

## Exact authored inputs

[The committed scene](../../harness/scenarios/aligned-amove.json) is shared test input, not an expected state. It defines a 64 by 40 flat, open map with spawns (8,10) and (40,32) in cells, one zero-income/zero-VP point at (4,36), and explicit layout [0,1]. Both original layout selectors still execute and consume their original draw before injection. The regular constructor creates its six troops and four base buildings. No actor is deleted, teleported or given artificial health; the HQs remain.

The original R1 Conquest/Standard/Clear/None profile is retained: USA/Germany, A/B, shuffle/tech/point supply/living weather on, logistics/sandbox off. The same private fixed seed and same finite 16,384-value tape restart for each fresh case. The bound is 4,800 ticks (240 seconds) per case, with N10 checkpoints and every intervening tick compared. Exhaustion or unmapped draws fail, without fallback or draw skipping.

Both cases have identical preparation and full-health actors. At tick 1, each owner sends HoldPosition=true, HoldFire=true and AutoRetreat=false for its three troops. At tick 2, rifle 1 receives Move to (21,21) metres and opposing rifle 4 receives Move to (65,21). At tick 400, both receive Stop. At tick 401, owner 0 turns rifle 1 HoldFire=false, then sends the sole differing command: attack-move to (111,21) for case `amove`, ordinary Move to exactly that goal for case `move`. Other troops remain held and hold fire. Staging/full health is verified from actual state, never imposed. Both natural base footprints sit outside the combat lane.

## Actual observations and comparison

[The additive recorder](../../harness/aligned-amove.mjs) uses the existing loader, tape generator and all 14 real random-consumer wrappers. It loads unchanged pinned Source `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`, never a copied engine. Five extra reversible anchors observe the original attack-move arrival/halt/resume-request branches, actual local `moving` result and ordinary dead-unit retirement. A read-only wrapper calls the original `canShoot` for the actor/foe predicate. Exact instrumentation erasure is required. Uninstrumented Math.random still refuses before module initialization.

[The native consumer](../../rust/crates/core/src/aligned_attack_move.rs) constructs actual Match::new_aligned and feeds recorded Stance/Move/Stop/attack-move through Command::from_json, the ordinary queue and advance_tick. New per-match observers are cfg(test) only, at the existing attack-move branch and actual firing `moving` boundary. Their calls and storage compile out of shipping builds. They observe without altering predicates, paths, actors, inputs or RNG. No shipping getter, provider, interface, header or rule changes.

Every paired frame contains:

- Tick, next actor ID, constructor births and subsequent birth/removal ID lists.
- Every live actor's id, owner, kind, x/z, HP and suppression.
- Every path waypoint in order; rot, aim, moveSpeed, vx/vz, travelDir and reverse destination for facing, motion and backing intent.
- Repath timer, stuck counter and still timer, with actual optional values retained.
- Actual ordinary queued orders, targetId, attackId, amove goal, facing order, holdPos, holdFire, autoRetreat, retreating, weapon cooldown and retarget timer.
- Existing player resources, score/winner and point ownership/capture fields.
- Actual canShootFoe, executed attack-move branch records, per-unit actual moving flags, ordinary retirement id/owner/kind/position/HP/last hitter and consumed-draw count.

Undefined Source values remain explicit absence markers. Native mandatory defaults remain actual present values. The original comparison used only the four player[0/1].fuel/mun positive-zero defaults. The explicitly authorized boolean follow-up below adds only holdFire/holdPos absence-to-false on copied unit projections. Every present value survives. No other optional stance/path/motion default is normalized and no tolerance is introduced. A representation mismatch may therefore be the first divergence; it is reported without being mislabeled a gameplay defect. The old raw comparator and canonical module bodies are untouched.

The scenario never queues a waiting ordinary order. The native consumer reads and verifies the actual queue is empty. An unexpected nonempty queue produces a provider failure with its raw native contents, instead of pretending to serialize it or cloning the command algorithm. Full route-budget/bridge/yielding matrices belong to MOV-11 later, not this first scenario. Complete cached routing internals and MODE-21's broad inventory are not comparison gates here.

All source/native frame values remain in private artifacts. The report separately retains first raw and four-currency-canonical differences. Each tick first checks every consumed draw's full consumer/index/bits against the Source prefix and its count, plus constructor layout/startup agreement. Full completion additionally requires equal exhaustion and unused counts. An extra or unmapped consumer fails alignment; it is never skipped.

## Completion predicates

Case `amove` requires actual full-health staging, a halt branch for rifle 1 targeting living rifle 4 that interrupts a nonempty path, empty post-halt path and a stationary following tick; then ordinary retirement of foe 4 with HP at or below zero after that halt; a later resume-request branch; actual post-kill progress; and arrival within **strictly less than 2.5 m**, empty path and cleared amove after the actual arrival branch.

Case `move` repeats the same start and foe setup. It requires passing more than 2.5 m beyond the still-living, actually shootable foe while moving with a nonempty path and no amove; zero attack-move halt branches for the actor; and actual goal arrival within 2.5 m with empty path. A foe death before that pass does not qualify the control. Both engines must separately satisfy the corresponding predicates. Neither order acceptance nor health agreement alone qualifies either row.

PASS requires both cases, every frame/checkpoint equal under only the authorized currency exception, aligned consumed inputs, no denial/fault, and all branch predicates true on both engines. The new test actually fails for MISMATCH, alignment/admission/provider failure or incomplete branches. Stop at the first failure and preserve the complete first-failure artifacts. The control stays explicitly unexecuted on native if attack-move already diverges. No semantic repair, new mapping or rule change follows automatically.

## Exact gates and commands

User15:51's updated grant and central ledger authorize clean 5a8-derived scenario children, with S1 initially head=null. Announce every actual frozen SHA before execution; continue without another review/permission pause. Each process must use S1 flock, CPUs 0-3, memory.max 6442450944, swap.max 0, cpu.max 400000/100000, jobs 4, test threads 1 and the current 20 GiB disk floor. Cargo runs through `/home/judiazm/.t3/handoffs/ww2-native-cargo-guard.py` with WW2_CARGO_SLOT=S1. Source uses the same direct kernel limits and flock. Full tapes, seed and fixtures remain private.

1. `SOURCE_ALIGNED_AMOVE`: one `/usr/bin/node` v24.21.0 invocation records both fresh cases. Keep the unmapped-random trap and freeze the authentic new private trace read-only with its single fixture SHA256 binding before Cargo. Source failure stops the stage before Cargo.
2. `FORMAT_ALIGNED_AMOVE`: one formatting check. A mechanical-only repair may freeze a new child with unchanged test semantics and Source attribution, then recheck.
3. `NATIVE_ALIGNED_AMOVE`: exactly one newly selected ignored test, containing attack-move then plain-Move control; stop first failure.
4. `CLIPPY_ALIGNED_AMOVE`: fresh strict locked core/simtest all-target ci-test Clippy only if the new scenario PASSes.

Payloads run under the allocated systemd scope and current guard, from this checkout:

```sh
/usr/bin/node --experimental-vm-modules harness/aligned-amove.mjs \
  --reference /home/judiazm/Projects/ww2-rts-native/reference/ww2-rts \
  --seed "$R1_MATCH_SEED" --output "$AMOVE_PRIVATE_DIR/source-trace.json"

python3 /home/judiazm/.t3/handoffs/ww2-native-cargo-guard.py \
  +1.99.0 fmt --manifest-path rust/Cargo.toml --all -- --check

# WW2_AMOVE_TRACE, WW2_AMOVE_TRACE_SHA256, WW2_AMOVE_REPORT and
# WW2_AMOVE_NATIVE_HEAD bind private input, output and actual producing head.
python3 /home/judiazm/.t3/handoffs/ww2-native-cargo-guard.py \
  +1.99.0 test --manifest-path rust/Cargo.toml --locked --profile ci-test \
  -p ww2-core --lib -j4 \
  aligned_replay::attack_move::attack_move_and_plain_move_match_with_complete_branches \
  -- --exact --ignored --nocapture --test-threads=1

# Only after PASS:
python3 /home/judiazm/.t3/handoffs/ww2-native-cargo-guard.py \
  +1.99.0 clippy --manifest-path rust/Cargo.toml --locked --profile ci-test \
  -p ww2-core -p ww2-simtest --all-targets -j4 -- -D warnings
```

No old four-test replay, registration guard, full/release/list command or extra format gate is selected. At terminal exit and lock release, transfer S1 immediately. Root reviews the final actual head before any later PR or complete-row proposal for Axel. No row is proposed until both cases PASS; a first divergence is a completed diagnostic deliverable with both rows held.


## Retained first result

Native/formatting head `9819b5c3062e9018727bef9bfb2af1a9389b3a78`: final formatting passes; actual selected test exits101,0passed/1failed/142filtered. First raw and canonical difference is tick0 `$.units[0].holdFire`, Source `{absent:true}` versus native `false`. No engine tick advances. This presence/default representation difference is retained without labeling it a proved gameplay defect or adding a new mapping. Native control remains unexecuted. Comparison count is0/1 observed N10 checkpoints, not0/481 completed checkpoints.

All five consumed constructor draws and startup observations agree, with16,379 native values unused. Full-run alignment is not claimed against Source's9,686/9,694 consumed draws. Source attack-move has actual halt490, natural retirement780, resume781, progress782, arrival branch1092 and empty-path/cleared-goal arrival1102; Source plain Move passes the living shootable foe617 and ends exactly at goal. These are authentic Source observations at13eae85e, separate from the native early-stop result.

Full first failure, both raw frame values, receipts, immutable Source fixture and binding, logs/commands and dependency provenance are private at `/tmp/aligned-amove-d6308bcc/`. Preserve the first formatting failure too. Conditional Clippy is not run after scenario failure. No old test, guard, full/release/list command or public action runs. Both rows remain held; no complete-row proposal is posted. S1 is terminal and its lock released. Final documentation changes do not claim a new execution or alter any predicate, projection, hook, input, map, tape or comparison.


## Authorized two-boolean comparison follow-up

After Root independently verified and posted the first diagnostic as #5/6196700421, the user explicitly authorizes only two additional format defaults: `$.units[*].holdFire` and `$.units[*].holdPos`, independently on each copied comparison projection. Missing keys or **exactly** `{absent:true}` become boolean false. Every present true/false/number/null/string and every marker with extra keys survives unchanged. No falsey, null, numeric-zero, NaN, other-field or global undefined rewrite exists. The old global R1 canonical module is unchanged.

The semantic source is actual pinned `shared/sim.js:1797`, where both fields are declared undefined, and `stanceBits` at3334, whose predicates use boolean truthiness. Native `UnitControl.hold_fire` starts false at `match_commands.rs:252`, and `Unit.hold_pos` starts false at `unit_spawn.rs:347`. Undefined and false both leave those two player stance bits clear. The user authorizes their explicit comparison representation defaults; this does not change engine state or extend to another field.

The two added boolean fields are separate from the existing four player currency paths. Initially all ten actual Source actors have both absence markers, and all ten native actors have both present false values. The initial twenty paths are listed below. After a retirement, array indexes can change; the report therefore counts each mapping by exact path **and actor ID**, separately by side and case over only the compared prefix. Currency mapping counts remain separately named. Every paired raw Source/native frame and the original first raw difference remain in the new private report.

| Actor ID | Initial holdFire path | Initial holdPos path |
| --- | --- | --- |
| 1 | `$.units[0].holdFire` | `$.units[0].holdPos` |
| 2 | `$.units[1].holdFire` | `$.units[1].holdPos` |
| 3 | `$.units[2].holdFire` | `$.units[2].holdPos` |
| 4 | `$.units[3].holdFire` | `$.units[3].holdPos` |
| 5 | `$.units[4].holdFire` | `$.units[4].holdPos` |
| 6 | `$.units[5].holdFire` | `$.units[5].holdPos` |
| 7 | `$.units[6].holdFire` | `$.units[6].holdPos` |
| 8 | `$.units[7].holdFire` | `$.units[7].holdPos` |
| 9 | `$.units[8].holdFire` | `$.units[8].holdPos` |
| 10 | `$.units[9].holdFire` | `$.units[9].holdPos` |

One new focused regression covers both fields and directions, false/missing/exact-marker positives, true versus false, scalar numeric0 and a positive-zero number wrapper, null/string/type changes, an unlisted field and an extra-key marker. It verifies input immutability, every present value and exact mapped path/actor records. The existing scenario test still asserts every checkpoint and complete branch on both engines, and still fails on the first new difference. No Source recorder, scene/order, raw projection, observer hook, provider, RNG, state or input changes. The old first report and a985/9819/13e stay intact; no retrospective pass.

Reuse only `/tmp/aligned-amove-d6308bcc/source-trace.json` and its existing read-only binding. New artifacts go under `/tmp/aligned-amove-bool-d6308bcc/`; no old report is overwritten. Before and after native work, compare the actual Source bytes against that binding and preserve old-report bytes. For the compared old prefix, preserve literal raw native frame bytes as well. No Source invocation is authorized or needed.

The updated S1 grant permits clean bounded children of a985. Freeze and announce the actual head before Cargo. Use the same kernel limits/current guard and20GiB floor. Select only formatting as needed, the following **two unique tests**, then strict core/simtest all-target ci-test Clippy only if both pass. Vectors run first; any test failure stops further execution. No old4, registration guard, list/full/release, Source, public action or MODE-21 work.

```sh
python3 /home/judiazm/.t3/handoffs/ww2-native-cargo-guard.py \
  +1.99.0 test --manifest-path rust/Cargo.toml --locked --profile ci-test \
  -p ww2-core --lib -j4 \
  aligned_replay::attack_move::boolean_defaults_are_symmetric_and_preserve_every_present_value \
  -- --exact --nocapture --test-threads=1

python3 /home/judiazm/.t3/handoffs/ww2-native-cargo-guard.py \
  +1.99.0 test --manifest-path rust/Cargo.toml --locked --profile ci-test \
  -p ww2-core --lib -j4 \
  aligned_replay::attack_move::attack_move_and_plain_move_match_with_complete_branches \
  -- --exact --ignored --nocapture --test-threads=1
```

Execution for this child is pending at its first source freeze. On an early stop, report only the aligned consumed prefix, with actual Source/native counts and checkpoint/tick counts. Root retains fresh final-head review and any later passing-row proposal/publication. Process exit plus flock release immediately returns S1.

## USER16:21: proposed defaults are diagnostic only

This instruction supersedes the earlier implication that the two boolean mappings were owner-approved. Root reports Hermes164/6196736715 explicitly withheld that approval. Root alone sends the one general-rule question to Axel. Until the answer, every proposed-default run is **DIAGNOSTIC_PROPOSED_DEFAULTS**, with ownerApproval=PENDING and parityEligible=false even if all4,800 ticks and both branches match. No row proposal/status/public PR follows from a diagnostic match.

[The explicit per-field registry](aligned-proposed-defaults.json) currently contains only holdFire and holdPos, their exact pinned consuming expressions at sim.js3334, false defaults, declaration evidence, actual native providers and the reason each default leaves its own stance bit clear. It lists every proposed field rather than applying a generic falsey conversion. A new field needs its own actual Source read/default evidence. Present values, types, signed-zero bits, nonfinite values and array order always survive unchanged; no []/{}/empty-string defaults exist. The approved four-currency rule remains a separate unchanged comparison.

The previous two-field attempt remains preserved at6dca6095: vectors1PASS, final formatting0, but the native scope ended OOM-killed at6GiB/swap0 before writing a report or stage-result file. Its tick/checkpoint/first-path/draw-prefix counts are unavailable. No outcome is inferred from that killed attempt. The original13e fixture/binding, a985/9819 first report and all old execution records remain byte-exact. Zero Source reruns.

A bounded test-only successor streams each case's complete raw Source and native frame arrays directly to the final private JSON report, then drops that case's native array before starting the control. It does not clone all frame arrays into nested report Values. Per-tick alignment borrows the unchanged actual draw ledger, checks each newly consumed row exactly once, and retains an explicit checked-prefix count. The original consumers/tape are unchanged. This removes repeated full-receipt clones without omitting a row or altering engine input/state. Full receipts remain in the final report.

The new report separates raw strict, approved currency-only and proposed-diagnostic differences and counts. The test's success means complete diagnostic comparison and branch execution only. First proposed real difference or alignment/admission failure stops execution; it does not cause another mapping/rule/input fix. The corresponding first raw/approved difference remains available even when diagnostic execution proceeds beyond tick0.

Exact new selections, using the same S1 limits/current guard after the announced clean freeze:

```sh
cargo +1.99.0 test --manifest-path rust/Cargo.toml --locked --profile ci-test \
  -p ww2-core --lib -j4 \
  aligned_replay::attack_move::proposed_default_registry_preserves_every_present_value \
  -- --exact --nocapture --test-threads=1
cargo +1.99.0 test --manifest-path rust/Cargo.toml --locked --profile ci-test \
  -p ww2-core --lib -j4 \
  aligned_replay::attack_move::attack_move_and_plain_move_match_with_complete_branches \
  -- --exact --ignored --nocapture --test-threads=1
```

These are payloads for the current S1 guard, not bare-shell authorization. Formatting repair is mechanical only. Conditional fresh core/simtest all-target ci-test strict Clippy runs only if both new diagnostic checks pass. No Source, old vectors/R1 tests, guards/list/full/release, worker, MODE-21 or public action. New private output directory: `/tmp/aligned-amove-diagnostic-d6308bcc/`.


## Completed proposed-default diagnostic, owner decision pending

Actual producing head: `10feb6aaa5acf1ce064c9a02c3891652de88ce73`. The registry regression and scenario check each pass (1 passed, 0 failed, 143 filtered). Final formatting and fresh strict locked core/simtest all-target ci-test Clippy pass at the same head. Earlier format diffs and the 6dca OOM remain preserved; no result is inferred from that killed attempt.

| Case | Actual ticks / frame pairs | Proposed diagnostic N10 matches | Raw strict / approved currency-only N10 matches | Consumed / unused per side |
| --- | --- | --- | --- | --- |
| Attack-move | 4,800 / 4,801 | 481/481 | 0/481 / 0/481 | 9,686 / 6,698 |
| Plain Move control | 4,800 / 4,801 | 481/481 | 0/481 / 0/481 | 9,694 / 6,690 |

The proposed diagnostic first difference is null in each case. Raw strict and approved currency-only both first differ at tick 0 `$.units[0].holdFire`: Source `{absent:true}`, native false. Full alignment covers all actual consumed rows and equal unconsumed suffixes from 16,384 supplied values per case; neither side exhausts its tape. This extent does not replace the historical five-draw-only first attempt.

Both engines complete all unchanged nonvacuity/branch predicates. Attack-move uses full-health natural actors, halts at 490, stays stationary, kills/retires the foe at 780, requests resume at 781 and progresses at 782. The actual arrival branch occurs at 1092, with empty-path/cleared-amove/within2.5m predicate at 1102. The plain-Move control passes the living shootable foe at 617 while moving with a nonempty path and no amove, then arrives at 811 with no attack-move halt. Source and native branch summaries agree exactly.

Each case has 38,420 Source boolean mappings and zero native mappings. Troop actors 1 through 6 map both fields only at tick 0. In the control, headquarters actors 7 through 10 map both fields 4,801 times each. In attack-move those same headquarters map 780 times at their original array indexes, then 4,021 times at indexes shifted by the genuine foe retirement. The private report enumerates every path/actor/default/count separately. Each approved currency path maps 4,801 times on Source and zero times on native. The only proposed fields remain holdFire and holdPos; no additional defaults were needed.

Private evidence: `/tmp/aligned-amove-diagnostic-d6308bcc/native-report.json` retains all raw frames, raw/approved/proposed verdicts, full receipts and the registry. `comparison-summary.json` contains exact per-path/actor counts and branch summaries. `raw-preservation.json` confirms unchanged original Source/binding/first-result bytes and literal initial native-frame bytes equal9819. All commands and producing heads are in the stage-result JSON files. Authentic Source still belongs to 13eae85e; zero Source reruns occurred. The final documentation commit keeps10feb6aa's Rust tree exact and claims no execution at its own head.

The result is **DIAGNOSTIC_PROPOSED_DEFAULTS**, ownerApproval=PENDING, parityEligible=false. Test completion is separate from owner-approved parity. CMB-91 and MOV-49 remain held, with no row proposal, eligibility claim or publication. Root receives the final actual head for independent review and handles the one general-rule owner request. S1 is terminal and its flock is released.

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

Undefined Source values remain explicit absence markers. Native mandatory defaults remain actual present values. The only comparison normalization is the existing symmetric four-path player[0/1].fuel/mun missing-key or exact-absence-marker to positive-zero rule. Every present value survives. No optional stance/path/motion default is silently normalized, no tolerance is introduced, and no field mismatch is removed. A representation mismatch may therefore be the first divergence; it is reported without being mislabeled a gameplay defect. The old raw comparator and canonical module bodies are untouched.

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

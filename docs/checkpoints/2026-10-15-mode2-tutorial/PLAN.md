# MODE second-stream candidate

- Owner: `IMPL_MODE_2`
- Baseline: `34edf52a7eaa6cb6edba3e802c2190226228276f`
- Source: `e4e579a838aabae3b1d9dadbc17a0a87147bc52d` at
  `/home/judiazm/Projects/ww2-rts-native/reference/ww2-rts` on Vostro
- Branch: `impl/mode-2-34edf52-d43da9ba`

## Scope and status

Select nine unported owned rows, MODE-61 through MODE-69. The ownership
ledger was read before edits:
`/home/judiazm/.t3/handoffs/ww2-rts-native-cargo-slots-0e8d6d9b.json`,
`row_ownership.rows`. All nine map to `IMPL_MODE_2`. MODE-78/87 belong to
`MODE_IMPL`; every previously ported row stays untouched.

This candidate adds a standalone core provider of entry data and completion
predicates. The recorder reads original `STEPS` definitions, invokes original
`begin` with declared spawn/command hooks, and invokes the original `done`
callbacks through original `checks`. It exports the private helpers in memory
without replacing any function body or editing Source. Entry hooks are
component inputs, not evidence of real actor creation or command acceptance.
It contains 50 declared component cases. The ten native tests cover one row
each plus malformed internal facts. No recorder, Cargo, rustc, rustfmt or
native job has run for this candidate. The generated oracle file does not
exist yet, and tests deliberately fail with a generation instruction if it
is absent. No expected result has been fabricated in its place.

Rows remain `in progress`. Definitions and predicates are not Tutorial match
execution. Source setup and step-runner behavior outside these nine rows is
not claimed, nor are health application, troop movement, garrison arrival,
manpower payment, actual command acceptance or a finished Tutorial run.

## Complete row predicate map

Every row's entry test compares the whole selected definition: exact goal,
ping in metres, optional manpower floor, ordered spawn kinds and positions,
optional health share, no-auto-retreat flag, and order kind and target.
Every completion test invokes the Rust provider on exactly the facts passed
to the original Source callback. All 50 labels and the Source file hash are
required by the native reader.

| Row | Source predicate and entry data | Candidate test | Missing runtime consumer |
| --- | --- | --- | --- |
| MODE-61 | All living team-0 units have x >= 24 m, including allied slots. Exact/below threshold, live ally behind, dead units ignored, enemy ignored, empty team-0 set true. Hedgerow ping (27,45). | `mode61_hedgerow`, six cases | Tutorial constructor and tick progression from real unit positions |
| MODE-62 | Two rifle spawns at (49,29) and (49,59), health share 0.6, auto-retreat false. All retained spawned IDs dead or absent; unrelated living enemies do not block. | `mode62_orchard`, six cases | Canonical spawn identity, ceil(full HP * 0.6), death/retirement and progression |
| MODE-63 | Point 0 owner belongs to team 0. Neutral/enemy false, either allied slot true, holding point 1 alone false. | `mode63_crossroads`, five cases | Actual objective capture and Tutorial progression |
| MODE-64 | Floor 250 MP, living team-0 AT gun exists. Enemy/dead AT false; own/allied live AT true. | `mode64_recruit`, five cases | Team wallet floor, accepted recruitment and progression |
| MODE-65 | Tank at (107,43), auto-retreat false, attack-move to (83,45). All spawned IDs dead or absent. | `mode65_tank`, six cases | Canonical tank spawn, accepted attack-move, combat death and progression |
| MODE-66 | Rifles at (91,35) and (91,53), auto-retreat false. All spawned IDs dead or absent. | `mode66_trench`, six cases | Authored Tutorial terrain, actual in-trench placement, combat and progression |
| MODE-67 | At least one living team-0 unit retreating. Enemy/dead retreat false, own/allied live retreat true. | `mode67_retreat`, five cases | Accepted Retreat command, real retreat flag and progression |
| MODE-68 | Floor 200 MP; MG at (127,43), rifle at (125,43), both garrison target (127,35), auto-retreat false. Any team-0 artillery cooldown > 0; enemy, zero and negative false. | `mode68_artillery`, six cases | Wallet, accepted garrison and arrival, accepted support command, cooldown and progression |
| MODE-69 | Point 1 held by team 0; neutral/enemy false, either allied slot true, holding point 0 alone false. | `mode69_bridgehead`, five cases | Actual bridgehead capture and `finish(0, 'tutorial', point1)` through the shared finish path |

The provider does not own a second Game or generate IDs. It borrows explicit
facts and returns data. The caller must create and register units, retain the
returned IDs, apply HP through canonical state, publish accepted orders, and
feed current authoritative facts back at the Tutorial phase. No caller is
added in this candidate. Source `stepTutorial` emits queued lines before
testing completion, waits until tick 40 for lines, and finishes only after
step 9. Those MODE-60 behaviors are neither implemented nor claimed here.
No fail state or income policy is selected.

## Coordination readback

Locally acknowledge issue #5 `axel-interface-response-004`, decisions
`axel-decision-ack-waiver-and-ci-085`, `axel-decision-r1-r4-090` and
`axel-decision-r1-profile-lifecycle-097`, and the latest readback
`axel-145-ported-readback-129`, plus Root's
`julio-restart-150-ported-20261015` at comment `6185610295`. R1 remains exactly two human commanders on
opposing teams in trusted Conquest/Default, with its approved temporary
logistics-off profile. The Source component vectors include another allied
slot to test team semantics; they do not widen live R1.

`native_startup.rs::NativeStartup::prepare`, `StartPlan::prepare_profile`
and `Match::validate_tick_input` still reject Tutorial. ABI 2's six calls
and PR #16's bilateral scope remain unchanged. No changes to #199/#201,
contracts, terrain, MOV-09, public hosting, spectators or privacy are made.
Whole-match evidence and new owner decisions remain pending. Because this
candidate contains production core code, do not assume the test-only 085
waiver applies to a later PR. Root owns review and any required owner ack.
No GitHub message, PR, push or merge is authorized in this task.

## Changed files and static checks

- `rust/crates/core/src/tutorial_steps.rs`: new provider.
- `rust/crates/core/src/lib.rs`: one module registration.
- `harness/tutorial-steps-fixture.mjs`: original-Source recorder, unrun.
- `rust/crates/simtest/tests/tutorial_steps.rs`: ten unrun tests.
- `rust/crates/simtest/tests/it/main.rs`: one additive test module registration.
- `docs/parity/modes.md`: only the nine owned rows, all in progress.
- `DESIGN.md`: appended candidate scope and missing consumers.
- `changelog.d/2026-10-15-mode2-tutorial-candidate-d43da9ba.md`: unique fragment.
- `docs/checkpoints/2026-10-15-mode2-tutorial/PLAN.md`: this handoff.

Static byte comparison preserved all 398 existing test/harness/fixture files
outside the additive registration, and preserved that registration file
after removing the exact two added lines. All 894,004 original DESIGN bytes
remain a prefix. A row comparison found exactly MODE-61 through MODE-69
changed and verified each against the preexisting ownership ledger. There
are no new row IDs. The Source tutorial file SHA256 is
`3aaa5e6affd936dae0ae4741d5173eee5d006132a1705d4b553097d3d8fabf42`.
Whitespace and prose checks are local static checks, not compilation or
proof that the recorder runs. All native and original-callback results
remain pending.

## Slot-gated execution plan

S1, S2 and S3 were assigned to CMB, MAP and ENG when this work started.
No slot grant to `IMPL_MODE_2` exists. Root must grant one focused slot for
these serial commands on a frozen head, wrap it with the ledger's actual
kernel limits and verify cleanup before reuse. Requested limits are 6 GiB,
zero swap, four build jobs, one test process at a time, and a Root-assigned
four-CPU set. Requested limits are not measured effective limits.

From the candidate root, after a grant:

```sh
WW2_REFERENCE=/home/judiazm/Projects/ww2-rts-native/reference/ww2-rts /usr/bin/node24.21.0 harness/tutorial-steps-fixture.mjs
WW2_REFERENCE=/home/judiazm/Projects/ww2-rts-native/reference/ww2-rts /usr/bin/node24.21.0 harness/tutorial-steps-fixture.mjs --check
```

Confirm the executable path and x64 Node 24.21.0 before the slot. The
recorder asserts its version, pin and tracked clean state before and after
recording. Review and commit the generated
`rust/crates/simtest/tests/fixtures/tutorial-steps.json` before native proof.
Re-freeze the SHA, then run from `rust/` in the same granted serial scope:

```sh
cargo +1.99.0 fmt --all -- --check
cargo +1.99.0 nextest run --locked --cargo-profile ci-test -p ww2-simtest --test it -j4 --test-threads 1 -E 'test(tutorial_steps::)'
cargo +1.99.0 clippy --locked --profile ci-test -p ww2-core -p ww2-simtest --all-targets -j4 -- -D warnings
```

The native filter should discover ten tests, zero ignored. If formatting
needs correction, preserve the original freeze, fix only owned new files
and re-freeze before claiming results. Preserve the first failure and exact
command/exit rather than retrying or weakening a check. Then Root requests
separate review of the actual candidate. No full/release/list job is
authorized by this plan. Root chooses any subsequent full or platform
gates after review and slot allocation.

Before any PR, integrate then-current main without overwriting either
side's shared parity or DESIGN additions. Recheck ownership, all ported
rows, original tests/assertions/fixtures and exact Source bytes. Re-freeze
and have Root commission separate review. Report actual counts and gaps;
do not promote these nine rows on component proof alone.

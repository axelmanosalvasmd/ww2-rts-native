# CMB-140 aligned Stop diagnostic

Private base: `68bd3ba2326d24fc6f4d77b65c69a7becf083c1c`, identical Rust/harness to independently reviewed `10feb6aa`. Published c8 and merged `55193075` remain untouched. This helper branch is not current main and does not undo its ten accepted components.

Read full `/tmp/aligned-amove-diagnostic-d6308bcc/HANDOFF.md` and `/tmp/review-aligned-10feb-diagnostic-full/REVIEW.md`. Local ACK of Hermes 165/166/167 and owner request 6196930542: technical acknowledgment is not Axel YES. All new default proposals stay diagnostic, pending and ineligible. User16:51 transfers CMB-140 only to CMB_IMPL; CMB-83/91, MODE-21 and MOV-11 remain other streams. No public post or status edit.

## Authored scene and actual providers

`harness/scenarios/aligned-stop.json` retains the authentic parent map/profile/layout. No direct state mutation, fixture purchase, birth, deletion, pose, HP or queue priming is permitted. The 16,384-value shared input is copied verbatim from the existing private authentic tape, retaining the same seed. Actual constructor, weather and combat draw consumers execute with strict finite-tape refusal and no unmapped fallback. Ordinary stances prevent idle reactions and shooting while normal Move commands stage full-health actors. All commands are real Source `command` and native decoded `Command::from_json` plus `Match::execute_command`, followed by `advance_tick`; this is the internal command path, not network admission.

At scheduled tick 401, Source/native frame tick 400:

- A (id 1) receives ordinary attack-move and has nonnull amove immediately before Stop.
- B (id 2) receives actual Attack on visible foe 4 and has attackId 4.
- C (id 3) receives ordinary Move, then queued Move and queued Attack. Its actual typed VecDeque must contain both entries, serialized from their own stored actor, destination, target and optional facing fields.
- Foe 4 receives its own Move and retains a live path. Foreign-only and empty Stops return needs without changing the complete command-boundary frame.
- One Stop ids [1,2,3] clears amove, attackId and orders on every selected actor, plus path, face, worldGoal, retreat, boarding and inactive work fields. The full targetId/firing observation is compared rather than falsely claiming Stop clears Source's automatically selected targetId.
- Subsequent no-own and mixed Stops retain the foreign actor's full actual state. The scene continues through tick 450.

Attack and attack-move deliberately clear each other in pinned sim.js 3451 and 3495-3508. Positive intent is therefore distributed across three actors; no simultaneous-single-actor claim. Command-boundary observations precede movement/firing ticks and prove the actual transition. The complete source and native observations, frames, denials, consumed-draw prefixes and final receipts are retained.

The parent amove consumer and every original refusal/assertion remain unchanged, apart from registering this cfg(test) child. New queue serialization reads real Command::Move/Attack entries and fails for any unrepresented variant. Active dig/entrench without a complete owned project projection is an explicit provider gap, never an empty/null replacement. This scene contains no field-work command, aircraft or public transport path. Those branches are not closed by this ground scenario.

## Comparison modes and new fields

Raw strict and approved four-currency-only results remain separate and may fail at tick zero. Proposed diagnostic copies apply parent holdFire/holdPos false rules plus the new scoped [registry](aligned-stop-proposed-defaults.json). Newly observed raw work/traffic state needs six explicit absent defaults: worldGoal, entrench and traffic to null; build, follow and trafficWait to exact positive zero. Each entry records a unique pinned consumer line/expression and native provider. Source reads distinguish activity from no activity; no unknown or present value is changed. A dedicated new registry vector covers present null/false/true, numeric types and signed-zero/nonfinite wrappers, empty collections, extra-key absence markers, unknown fields and ordered arrays. The old registry and old vector are unchanged and unselected.

OwnerApproval is PENDING and parityEligible is false regardless of diagnostic MATCH. A match can support only a private `/tmp` row-PR draft naming the complete ground Stop acceptance scene, ticks, fields and unproved aircraft/network branches. It cannot apply a status patch or establish owner acceptance.

## Authorized bounded sequence

Use only S3, CPU8-11, systemd 6G memory/swap0/quota400%, jobs4/threads1, current WW2_CARGO_SLOT=S3 guard and exclusive S3 target/flock. Run df immediately before each Cargo, then the guard checks the 20 GiB floor. Freeze all source and direct CHANGELOG/DESIGN before Node. Source gen/check run once each at that frozen head with `/usr/bin/node` v24.21.0 x64 and `--experimental-vm-modules`, new private input/output paths and clean pinned reference. Both captures must satisfy the per-actor genuine branch before native qualification. Freeze read-only authentic capture and input binding, without hashes or sealing, before Cargo.

Proposed Source arguments: `harness/aligned-stop.mjs --reference /home/judiazm/Projects/ww2-rts-native/reference/ww2-rts --input <new-private-shared-input.json> --output <new-private-source-trace.json>`, then the same with `--check`. Inputs are exactly copied sharedInput, not expected observations. The recorder verifies all scoped registry anchors against the pin and exact erasure of every instrumentation edit.

After Source succeeds: fmt check, mechanical new-file-only layout repair if needed with token/literal preservation and a frozen child, then exactly these two new core libtest instances once:

- `aligned_replay::attack_move::stop::stop_registry_preserves_present_values_and_unknown_fields`
- `aligned_replay::attack_move::stop::three_actor_stop_clears_real_intents_and_preserves_foreign_controls` (explicit `--include-ignored`)

Use `cargo +1.99.0 test --locked --profile ci-test -p ww2-core --lib --jobs 4 <exact-name> -- --exact --include-ignored --nocapture --test-threads=1`. Supply WW2_STOP_TRACE, WW2_STOP_INPUT, WW2_STOP_BINDING, WW2_STOP_HEAD and a fresh WW2_STOP_REPORT path. Stop at the first genuine compile/admission/behavior failure. Conditional fresh strict `cargo +1.99.0 clippy --locked -p ww2-core -p ww2-simtest --all-targets --profile ci-test --jobs 4 -- -D warnings` follows only passing new checks. No old R1/amove/registry/guard rerun, list/full/release or standalone component test.

The raw report must show first differences and values, tick/path, frames/checkpoints actually compared, every mapped side/path/expression/default/count, checked input prefix and complete final receipts. Harness completion alone is not a diagnostic match. Raw Source observations are never applied to Match. No new execution is claimed at this source checkpoint.

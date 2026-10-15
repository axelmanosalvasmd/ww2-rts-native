# Test-only R1 aligned replay

Base: `9e618217de2ac1d0a806c6804eda50c1c9da497f`. Reference: `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. This is the 085 test-only successor to #220, with the owner decision in #5 comment 6194079987 and its narrowed input condition in 6182315380. No row moves. The historical result remains 0/61 with a tick-0 x difference of 27 versus 85 metres.

## Input and receipt

The profile, committed Default map, roster, ten orders, 600 actual ticks, N10 checkpoints, exact unit/player/mode/score/objective projection and first-divergence comparator are preserved from #220. The map and every profile field are validated by its original Rust runner. The Source recorder retains its exact projection and command log. Spawn slots [0,1] are authored test input for the two opposing players. Both layout selectors execute their original algorithm and consume the recorded selection draw before the explicit test input supplies the constructor layout. No existing actor is moved or deleted.

Each side consumes from one finite shared input tape. Draws contain index, semantic consumer, context and IEEE bits. Constructor roles are layout shuffle, environment seed32, wind angle, wind strength and first shower. Tick weather roles name the tick. Direct-fire roles name tick, shooter, target, round and accuracy/offset magnitude/offset sign. Environment ignition, spread and rubble draws identify tick and relevant cells. The receipt includes capacity, consumed count, exhaustion, unconsumed count, layout boundary and startup observations: each player spawn, environment state, weather seed and plan, wind, shower, rain, wetness and next actor ID.

All draw rows and counts must agree. So must the layout and startup observations. Unmapped consumers, a shorter/longer draw stream, tape exhaustion or different startup fields fail alignment. State differences do not end either engine. No tolerance, draw skipping, expected-state restoration, fixture filtering or weakened guard is permitted. The private report retains the full native receipt and first draw difference separately from state divergence. Both full state traces remain private.

Rust hooks exist only in cfg(test). No Cargo feature, public ABI/header or ordinary MatchOptions field is added. All test state belongs to the match, shared explicitly with its environment owner and random clones. The normal production generator calls remain the macro expansion in ordinary builds. The Source module loader reversibly instruments call boundaries in memory and checks exact erasure, while every original engine function runs. It never writes the reference checkout. Uninstrumented Math.random fails. The independent weather-plan generator has no consumed draw on this Clear/None profile.

## Private execution

S1 is granted only to this stream and its clean derived seam heads. Every command must use CPUs 0-3, 6 GiB memory, zero swap, 400% quota, four build jobs and one test thread. Source needs the same direct limits and S1 flock. Cargo uses the current coordinator guard and WW2_CARGO_SLOT=S1, with 20 GiB free immediately before each invocation. Report each frozen head before a stage. Freeze the authentic Source trace and complete input receipt before any Cargo invocation.

```sh
/usr/bin/node --experimental-vm-modules harness/r1-aligned-replay.mjs \
  --reference "$WW2_REFERENCE" --seed "$R1_MATCH_SEED" --output "$WW2_ALIGNED_TRACE"
/usr/bin/node --experimental-vm-modules harness/r1-aligned-replay.mjs \
  --reference "$WW2_REFERENCE" --seed "$R1_MATCH_SEED" --output "$WW2_ALIGNED_TRACE" --check

# Payloads below run through the allocated scope and coordinator Cargo guard.
cargo +1.99.0 fmt --manifest-path rust/Cargo.toml --all
cargo +1.99.0 test --manifest-path rust/Cargo.toml --locked --profile ci-test \
  -p ww2-core --lib -j4 aligned_replay::tape_rejects_malformed_inputs_and_records_every_consumption \
  -- --exact --nocapture --test-threads=1
cargo +1.99.0 test --manifest-path rust/Cargo.toml --locked --profile ci-test \
  -p ww2-core --lib -j4 aligned_replay::original_r1_aligned_61_checkpoints \
  -- --exact --ignored --nocapture --test-threads=1
cargo +1.99.0 test --manifest-path rust/Cargo.toml --locked --profile ci-test \
  -p ww2-core --test it -j4 every_test_file_is_compiled -- --exact --test-threads=1
cargo +1.99.0 clippy --manifest-path rust/Cargo.toml --locked --profile ci-test \
  -p ww2-core -p ww2-simtest --all-targets -j4 -- -D warnings
```

WW2_ALIGNED_TRACE, WW2_ALIGNED_REPORT and WW2_ALIGNED_NATIVE_FRAMES name private paths. The replay test is explicitly ignored in normal hosted execution because its private seed and capture are not committed. An explicit focused invocation requires the frozen fixture; it cannot silently pass without it. The finite-tape test runs normally. No full workspace suite, release build, list command or old unaligned replay is required. A compiler, admission or harness failure must be retained and reported before repair.

## Candidate checks

| Row | Actual candidate probe | Remaining full-check limits |
| --- | --- | --- |
| BASE-01 | Failing Conquest currency-presence projection at tick 0 and all checkpoints | Complete Classic starting-currency and owner snapshot check absent; no promotion |
| MODE-22 | Constructor IDs, kinds, owners and positions at tick 0 | Only this two-player Conquest startup; no other modes or complete constructor-order proof |
| CMB-83 | Both accepted holdPos=true commands at tick 1 | Invalid keys/types, foreign selections, other stances and owner-only wire-bit privacy absent |
| CMB-91 | Attack-move at tick 140, Stop at 400, all position/health frames through 600 | Halt/resume, 2.5 m arrival and target eligibility branches need independent witnesses |
| MOV-11 | Move 2, Stop 120, attack-move 140, Stop 400 and positions through 600 | Backing, route budgets, blocked/yielding retries and bridge-group variants absent |

These are partial probes, not complete row checks. No row is eligible for promotion from this prefix alone, even if all 61 checkpoints match. Victory, logistics on, other modes/weather/maps/rosters, vehicles, planes, network delivery and Unreal remain separate.

## Authentic Source freeze

At clean seam head `864d6485f2129d48c2a70635c1505ba16c8c4487`, generation and exact fresh capture check both exited 0 on Vostro Node v24.21.0/x64. Each completed 600 ticks, 352 moving ticks and 61 checkpoints, consuming 1,321 tape values with 15,063 unused. Observed roles include all five startup roles, both tick wind roles and all three direct-fire roles. No Source fault occurred.

The private source trace, shared input and complete source input receipt are frozen read-only under `/tmp/aligned-replay-d6308bcc/`, before the first Cargo invocation. Both Source processes held S1 flock with kernel affinity 0-3, memory.max 6,442,450,944, memory.swap.max 0 and cpu.max 400000/100000. No native alignment or match count is claimed yet. Full logs and stage results retain the producing head.

## First native result, retained before lint cleanup

At `cae6e358f40429bddb5882ab7e6381b694dcc57c`, the finite-tape check and actual 61-checkpoint replay each pass as harness checks. Both engines complete 600 ticks, move during 352 ticks, accept all ten orders, and consume exactly the same 1,321 draw rows with 15,063 unused values. Startup observations match. The differential verdict is MISMATCH, 0/61; first divergence is tick 0, `$.players[0].fuel`, Source absence versus Rust zero bits `0000000000000000`. A successful harness check does not turn that result into parity. No owning row is assigned without separate source investigation.

Immutable first-result copies are `native-report-cae6e358.json` and `native-frames-cae6e358.json` in the private directory. The raw first compiler log retains its test-only private-interface warning. The cleanup makes Replay public only within its private cfg(test) module, and adds the differential verdict to the sanitized stdout summary. Registration, fresh strict Clippy and the final focused-head qualification follow.

## Qualified native head and source investigation

At clean native head `87dfae5c1801ad7d41046e637fd870a8fea1bbb6`, all three focused checks pass: finite tape validation/counting, the original exact-bits comparator regression, and the actual private R1 replay. Both core and simtest registration guards pass. Formatting check and fresh strict locked core/simtest all-target Clippy pass. No failures or ignored tests occur in the explicitly selected focused invocation. All processes use S1 with four jobs, one test thread and the recorded caps.

The actual comparison again completes 600 ticks per engine, 352 moving ticks per engine and all 61 N10 checkpoints. Native emits 128 events. Both accept ten orders. The complete 1,321 draw rows and startup receipt agree; both have 15,063 unconsumed values. Result: MISMATCH, 0/61. Six unchanged unavailable native phases are Convoys, GroundEffects, Production, Rail, SupplyNetwork and UnitReserves. These are listed, not claimed as executed.

Source investigation identifies BASE-01 as the row describing the first field's semantics. At pinned `shared/sim.js:723`, ordinary player construction omits Fuel and Munitions; World construction assigns both at `:1177`. Native `economy_state.rs:59-65` has mandatory f64 fields, and StartPlan uses their Default zeros at `match_runtime.rs:334-350`. The raw diagnostic retains its original unassigned metadata, while this later investigation assigns the resource-representation question to BASE-01. It is a failing partial currency probe, not the complete Classic/snapshot check. No runtime representation or projection is changed.

Local ACK of Hermes 156, comment 6194582243: test-only bounds, no row moves and separate Root review/publication are observed. No external acknowledgment or PR action is performed here.

A final source-review correction moves the unseeded Math.random refusal ahead of module evaluation, covering import-time code and any captured callback. The Rust tree and already frozen shared inputs are unchanged. Check this frozen harness child against the original authentic fixture before final handoff; do not regenerate or replace the first fixture.

## Final handoff

The initialization-guard child `23ad85706130349f773381ddc19c67a625c9624d` passes a fresh Source check against the original immutable capture. All 600 ticks, 61 checkpoints, 1,321 draw records, startup observations and full frames reproduce exactly. The fixture's producing head remains `864d6485`; the final check log names `23ad8570`. The entire Rust tree equals native-tested `87dfae5c`, so no Rust check is repeated for this harness/documentation child. Native evidence remains attributed to that actual producing head.

Private evidence directory: `/tmp/aligned-replay-d6308bcc/`. `source-trace.json`, `shared-input.json` and `source-input-receipt.json` are the pre-Cargo freeze. `native-report.json` and `native-frames.json` retain the final native result; the `-cae6e358` copies retain the first result. Each stage has its raw log and result JSON with head, exact command, exit status and limits. No seed or full trace is published.

Final native command selected `aligned_replay::` with `--include-ignored --nocapture --test-threads=1`, yielding three passed checks and zero failed or ignored. Both registrations selected `every_test_file_is_compiled --exact`. Strict Clippy selected only core and simtest with `--all-targets -- -D warnings`. Formatting passed `--all -- --check`. No full/release/list command or historical unaligned replay ran.

One-line dependency provenance confirms `ww2_core-82f7ed540759d581.d` includes this seam and the compiler record points to this checkout's core manifest and Rust cwd. All invoked processes are terminal and S1 flock is released. Root may reclaim the slot immediately. Root retains independent review and all PR, review and publication actions; none are performed by this implementation stream.

## User14:51 comparison-only successor

The user expressly authorizes a narrow symmetric comparison mapping after raw integration `58a8cb8b`. All earlier no-normalization statements remain the rule for the raw path and historical results; this exception changes copied comparison projections only. The raw strict comparator, original projection, Source capture, complete input tape/receipt and all native baseline frames remain unchanged and available.

| Exact mapped path | Absent input | Canonical comparison default |
| --- | --- | --- |
| `$.players[0].fuel` | Missing key or exact `{absent:true}` | Positive zero, bits `0000000000000000` |
| `$.players[0].mun` | Missing key or exact `{absent:true}` | Positive zero, bits `0000000000000000` |
| `$.players[1].fuel` | Missing key or exact `{absent:true}` | Positive zero, bits `0000000000000000` |
| `$.players[1].mun` | Missing key or exact `{absent:true}` | Positive zero, bits `0000000000000000` |

The same independent function applies to BOTH sides after validation of the original R1 identity/profile. It retains every present value. Absence versus a nondefault, present unequal values, unlisted fields, negative zero, nonfinite values and type changes remain mismatches. No null/false/empty mapping exists. Other modes, logistics-on and an explicit weather seed fail the profile gate. Both players retain their actual slot/team identity. The new single regression check covers both directions, missing keys and explicit absence markers, each whitelist field, both slots, zero-versus-one, unknown fields, signed zero, nonfinite values, malformed markers and primitive/collection type differences.

The actual replay still runs once for600ticks with61 N10 checkpoints. Its unchanged raw frame artifact supplies the additional canonical comparison pass; no canonical value enters an engine or input receipt. The new report adds `canonicalComparison`, including all four paths, exact default, mapping counts and its separate verdict/count/first difference. Raw top-level fields retain their original meaning.

Execution is pending at this source checkpoint. Authorized selection is `aligned_replay:: -- --include-ignored --nocapture --test-threads=1`, exactly four checks, plus formatting and fresh strict locked core/simtest all-target Clippy under S1. Reuse the original read-only authentic Source trace and write fresh native artifacts to a new private directory. Do not rerun Source, either unchanged registry guard, full/release/list commands or public actions. Root retains independent complete-input and new-comparison review.

Both independent immutable-eb15 reviews now return ACCEPT: `/tmp/review-aligned-eb15-production-scope/REVIEW.md` and `/tmp/review-aligned-eb15-all-inputs/REVIEW.md`, each read in full. They independently parsed all1,321 rows and found no shipping behavior defect for that recorded prefix. Their acceptance does not cover this new comparison child, complete derived startup/internal state, executed capacity/exhaustion/label-mismatch negative controls, or any complete row check. The input seam remains unchanged; these limits are preserved, not repaired or reclassified.

### First measured canonical result

Actual four-check producing head: `db854ef7f6568f33bd1179851e0c417b3664a8dd`. Result: four passed, zero failed/ignored. Canonical MATCH61/61,600 stepped ticks equal, first difference null. Raw MISMATCH0/61 remains tick0 `$.players[0].fuel`, Source absence versus Rust positive zero. All1321 consumed rows,15063 unused values and startup observations remain equal. The fresh raw native frame file is byte-identical to the baseline; every old raw report field is retained with its exact old value.

Mapping counts explicitly name all four fields,601 per Source field and zero native mappings. Every present native value remains unchanged. The private canonical report and immutable `-db854ef7` raw native/report copies are under `/tmp/aligned-replay-canonical-d6308bcc/`. The original raw Source and native baseline artifacts are untouched.

The first fresh strict-Clippy command fails with unused `compare_projection_for_test` in the standalone simtest binary's test build. Preserve `strict-clippy-first.log` and result JSON. A comparison-only visibility child changes this new cfg(test) helper from pub(crate) to pub without changing its body or shipping interface. The four-check result remains attributed to db854ef7; do not claim a new native run from the visibility-only repair.

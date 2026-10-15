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
| MODE-22 | Constructor IDs, kinds, owners and positions at tick 0 | Only this two-player Conquest startup; no other modes or complete constructor-order proof |
| CMB-83 | Both accepted holdPos=true commands at tick 1 | Invalid keys/types, foreign selections, other stances and owner-only wire-bit privacy absent |
| CMB-91 | Attack-move at tick 140, Stop at 400, all position/health frames through 600 | Halt/resume, 2.5 m arrival and target eligibility branches need independent witnesses |
| MOV-11 | Move 2, Stop 120, attack-move 140, Stop 400 and positions through 600 | Backing, route budgets, blocked/yielding retries and bridge-group variants absent |

These are partial probes, not complete row checks. No row is eligible for promotion from this prefix alone, even if all 61 checkpoints match. Victory, logistics on, other modes/weather/maps/rosters, vehicles, planes, network delivery and Unreal remain separate.

## Authentic Source freeze

At clean seam head `864d6485f2129d48c2a70635c1505ba16c8c4487`, generation and exact fresh capture check both exited 0 on Vostro Node v24.21.0/x64. Each completed 600 ticks, 352 moving ticks and 61 checkpoints, consuming 1,321 tape values with 15,063 unused. Observed roles include all five startup roles, both tick wind roles and all three direct-fire roles. No Source fault occurred.

The private source trace, shared input and complete source input receipt are frozen read-only under `/tmp/aligned-replay-d6308bcc/`, before the first Cargo invocation. Both Source processes held S1 flock with kernel affinity 0-3, memory.max 6,442,450,944, memory.swap.max 0 and cpu.max 400000/100000. No native alignment or match count is claimed yet. Full logs and stage results retain the producing head.

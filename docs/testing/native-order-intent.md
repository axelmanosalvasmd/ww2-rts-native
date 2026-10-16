# Native order-intent tests

These behavior tests exercise native commands and the ordinary tick runner under [owner decision 6206861267](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6206861267). They use seed 701, an open 64 by 40 cell map, two opposing players and native random streams. The map is defined in the test module, with the same geometry and spawn/objective positions as the old private order scene. No replay adapter, Source capture or fixture generator is added.

All names below have prefix `native_order_intent_tests::` in `rust/crates/core/src/native_order_intent_tests.rs`.

| Row | Native test | Prototype at e4e579a | Behavior |
| --- | --- | --- | --- |
| CMB-83 | `cmb83_stance_validation_owner_flags_and_explicit_attack` | shared/sim.js:3331-3334,3574-3579,5900-5903; test.js:1226-1245 | Invalid key/nonboolean denial, own-only selection, all three stance bits toggled and hidden from enemies, private autocast bit, hold-fire suppression and explicit attack. |
| CMB-91 | `cmb91_mov49_attack_move_halts_kills_resumes_and_finishes` | shared/sim.js:5449-5455; test.js:724-740 | Halts for a shootable target, kills it, resumes and finishes within 2.5 m. Plain Move continues past the enemy. |
| MOV-49 | `cmb91_mov49_attack_move_halts_kills_resumes_and_finishes` | shared/sim.js:5449-5455; test.js:724-740 | The same halt, resume and arrival behavior as CMB-91. |
| CMB-140 | `cmb140_stop_clears_live_orders_and_paid_build_work`; `cmb140_aircraft_stop_patrols_here` | shared/sim.js:3511-3513 | Empty/foreign-only selection denial, mixed-selection ownership, clearing paths/targets/queued orders and paid building/project work without refund, and an airborne plane patrolling its current position. |

## Scope and dropped rows

Only the owned test contribution from `0290fcf6..c431c8ef` was considered. The new branch starts at main `40fe9e29`; it imports no unpublished runtime changes. The accepted stance, attack-move and ground Stop test bodies are preserved. The shared aircraft test keeps its Stop assertions and omits the Retreat portion.

CMB-144 is dropped from this batch. Its historical foreign-construction regression failed with `unrepresented or inconsistent construction activity`; the group test failed an authored home-distance bound below 3 m. That bound alone does not establish a runtime defect. Garrison exit and capture exclusion remain uncovered. Passing aircraft and paid-work components did not complete the row.

CMB-145 is dropped from this batch. Loaded scuttle failed at tick 786 with `thrown carrier ejection tick phase is not connected`. The ordinary-unload component passed but did not complete the row. These diagnostic bodies and their historical results remain on private branch `impl/aligned-ground-reason-992b-20261016-v15` at `c431c8ef`; this PR does not copy them into the active suite.

Prior prototype-intent ACCEPTs for CMB-83/91/140 and MOV-49 were scoped to `17bd0b20`, whose Rust tree matched producing `eddedccd`. That run selected nine tests, with six passing and three failing. Those results do not qualify the new main-based test module. Parity status updates belong to a separate PR after this test PR merges.

## Validation

On the Vostro, Rust tree `8dc9860f9bc815cddbc85509dcf2351e67be0d14` passed all four focused tests (149 unrelated tests skipped) with:

```sh
CARGO_BUILD_JOBS=4 cargo nextest run --locked -p ww2-core --lib --cargo-profile ci-test --test-threads 1 -E 'test(native_order_intent_tests::)'
```

The run used slot S3 under a systemd user scope capped at 6 GiB memory and zero swap. The first command stopped at nextest argument parsing because it specified the test-thread count twice; no compiler or test ran in that invocation. The corrected command passed all four tests without a native-test retry.

`cargo fmt --all --check` passed in slot S2. `cargo clippy --locked --workspace --all-targets -- -D warnings` also passed in slot S2, with four build jobs and the same memory/swap caps. Both slots were released as soon as their Cargo commands exited. Independent read-only source review accepted all four row intents on this main-based extraction, reading the pinned prototype and current runtime providers; the reviewer executed no checks.

Local logs are under `/tmp/orders-intent-batch1-20261016/`.

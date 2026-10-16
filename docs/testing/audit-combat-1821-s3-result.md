# Restored combat S3 qualification and six selected checks

Producing head: `39b004f2e180b334a27a51c9b0ce47db1ff27126`.
Rust tree: `deadbd9f81413f8bc03aa9cd847f715051b8a5ad`.
Own checkout: `/tmp/ww2-audit-combat-cmb65-3fa592e8`.
The producing commit is the authorized mechanical child of
`3ca67777596f7aa989cb33659934ee1e62f77272`; it changes only two command-expression
wraps in Rust, with same-commit CHANGELOG and DESIGN entries.

## Actual granted results

| Job | Producing head | Actual result |
| --- | --- | --- |
| Initial fmt check | 3ca67777 | exit 1, exactly two mechanical expression-wrap diagnostics |
| Renewed fmt check | 39b004f2 | exit 0 |
| Five exact simtest instances | 39b004f2 | exit 0, 5 passed, 0 failed, 224 skipped |
| Strict locked core/simtest all-target ci-test Clippy | 39b004f2 | exit 0, 10.87 s |

The five unique selected native instances were each executed once:

| Test in `ww2-simtest::it` | Result |
| --- | --- |
| `match_aviation::cmb65_mg_flak_and_fighter_hold_then_resume_permitted_anti_air` | PASS, 0.148 s |
| `match_tick::cmb41_four_actual_conscript_recruits_add_three_population` | PASS, 0.028 s |
| `match_tick::cmb69_seeded_retirement_rejects_wrong_tick_identity_and_missing_or_extra_deaths` | PASS, 0.342 s |
| `match_tick::connected_tick_phases_match_actual_reference_steps` | PASS, 2.528 s |
| `vision::cmb39_existing_tower_actor_sees_twelve_then_sixty_when_complete` | PASS, 0.066 s |

No test retry, Source/Node job, CMB42, registry-only test, discovery/list,
full/release suite or CI job ran. The older core CMB64 PASS was deliberately not
rerun. New CMB65 uses the unchanged complete mission-eligible fighter capture,
ordinary Move/Attack dispatch and actual AA providers. No ammo/HP/type/pose/RNG
override, fixture edit, guard relaxation or weakened assertion was added.
Root explicitly accepted this bounded consumer scope; it is not a new
contiguous Source capture of the composed sequence.

## Commands and resource evidence

All jobs ran under the current Root guard and exclusive S3 target
`/tmp/ww2-native-target-S3-0e8d6d9b`. Each job printed actual kernel affinity
[8,9,10,11], memory.max=6442450944, memory.swap.max=0 and
cpu.max=400000 100000. Available space before the jobs exceeded the 20 GiB floor:
initial fmt 53482237952 bytes, renewed fmt 53469057024, native 53480534016,
Clippy 51447091200. Build jobs were 4 and test threads 1.

Payloads from the checkout's `rust/`, wrapped by the verified scope and guard:

```sh
cargo fmt --all -- --check
cargo nextest run --locked -p ww2-simtest --test it --build-jobs 4 --test-threads 1 -E 'test(=match_aviation::cmb65_mg_flak_and_fighter_hold_then_resume_permitted_anti_air) | test(=vision::cmb39_existing_tower_actor_sees_twelve_then_sixty_when_complete) | test(=match_tick::cmb69_seeded_retirement_rejects_wrong_tick_identity_and_missing_or_extra_deaths) | test(=match_tick::connected_tick_phases_match_actual_reference_steps) | test(=match_tick::cmb41_four_actual_conscript_recruits_add_three_population)'
cargo clippy --locked --profile ci-test -p ww2-core -p ww2-simtest --all-targets -- -D warnings
```

All processes exited. A bounded nonblocking exclusive flock acquisition
confirmed S3 free. Root received terminal exit and lock release before this
report or dep-info was written. No PID/cgroup scans, cleanup or seals followed.

Raw logs remain `/tmp/audit-combat-3ca-S3-3fa592e8/`: `fmt-3ca.log`,
`fmt-39b004f2.log`, `sim-five-39b004f2.log`, `clippy-39b004f2.log`.
The guard's own checkout/Rust-tree compiler log is
`/home/judiazm/.t3/handoffs/ww2-native-cargo-wrappers/1f705b411ec5b7e75946ac3c/compiler-invocations.jsonl`.
The one-line dep-info receipt in the raw-log directory binds the actual native
it compilation to this checkout and producing Rust tree.

## Carried evidence and preserved failure

The previous actual S2 result remains unchanged at
`daaa4743bc257235378b561b639641f69f07271b`: core CMB64 PASS, then sim CMB65 FAIL
because the older selected continuous-AA fighter has ammo 0 and cannot admit a
new mission. The other four selected simtests and conditional Clippy were unrun
there. Its logs remain `/tmp/audit-combat-e80-S2-3fa592e8/`; the original
e80/daaa/ccad and separate eb4 source checkpoints remain intact.
The CMB65 correction preserves that failure and uses another existing complete
real-aircraft initialization, rather than modifying the old aircraft's ammo.

The core subtree is unchanged from daaa to this producing head, so the old
single core64 PASS retains its producing-head attribution. It is not reported
as a new execution here. Unchanged complete catalog, startup-table, production,
combat, aviation, vision and support components retain their actual main
`7a7348cbac35924b5876531e318140d13b7a42c1` run
[38776054627](https://github.com/axelmanosalvasmd/ww2-rts-native/actions/runs/38776054627)
evidence. Eight named existing instances each PASS on Ubuntu 24.04, Windows
2025 and macOS 15, for 24 historical named results. Their raw log was read
directly at `/tmp/audit-combat-1821-main-38776054627.log`. The refactored original
connected instance now also has a fresh PASS above.

## Six-row delivery boundary

CMB39/40/41/64/65/69 now have complete selected-check evidence as mapped in the
[whole-check brief](audit-combat-1821-review-brief.md). A documentation child may
update only those six Rust/Status pairs while preserving this tested Rust tree,
all Acceptance cells and all other rows. CMB38/42/62/63/66/67/68 remain held.
Publication still needs Root's lease handover after CMB_IMPL and independent
actual-head review. The user authorizes opening a reviewable row PR once that
lease transfers. No Hermes merge acknowledgment is required merely to open it;
runtime-head/hosted/merge gates remain with Root.

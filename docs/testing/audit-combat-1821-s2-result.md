# AUDIT_COMBAT S2 qualification: stopped at first native failure

Producing native head: `daaa4743bc257235378b561b639641f69f07271b`, authorized
formatting-only child of `e80d369d495c893436740db58155571a9484594a`.
Rust tree: `4c3d8cf4aa92e85921becffab9b8fbba3bae37af`.
Checkout: `/home/judiazm/.t3/worktrees/ww2-rts-native/t3-3fa592e8`.
Branch: `audit/combat-1821-3fa592e8`.

## Actual result

| Job | Producing head | Result |
| --- | --- | --- |
| Initial guarded workspace fmt check | e80d369d | Exit 1, four formatting-only diagnostics. Preserved in fmt-e80.log. |
| Authorized mechanical formatter application | e80d369d inputs | Exit 0; freeze child daaa4743 before subsequent jobs. |
| Renewed guarded fmt check | daaa4743 | Exit 0. |
| Exact core CMB-64 instance | daaa4743 | Exit 0: 1 passed, 0 failed, 138 skipped. |
| Exact five selected simtest instances | daaa4743 | Exit 100: 1 of 5 ran, 0 passed, 1 failed, 224 skipped; other four selected instances did not run because of fail-fast. |
| Strict core/simtest ci-test all-target Clippy | none | Not run: conditional all-six-PASS gate was not met. |

Two unique native instances actually executed across the two invocations:
CMB-64 passed and CMB-65 failed. The following four selected instances are unrun:

- `vision::cmb39_existing_tower_actor_sees_twelve_then_sixty_when_complete`
- `match_tick::cmb69_seeded_retirement_rejects_wrong_tick_identity_and_missing_or_extra_deaths`
- `match_tick::connected_tick_phases_match_actual_reference_steps`
- `match_tick::cmb41_four_actual_conscript_recruits_add_three_population`

CMB-64 exact name:
`match_combat::cmb64_actual_retirement_pays_enemy_cost_bounty_and_excludes_friendly_death`.
CMB-65 exact name:
`match_aviation::cmb65_mg_flak_and_fighter_hold_then_resume_permitted_anti_air`.

## First native failure and inspected input

The CMB-65 function panics at
`rust/crates/simtest/tests/match_aviation.rs:692`. Its assertion requires
send_aircraft_mission(owner 0, shooter 1000, Attack target 1001) to return true;
the actual supported caller returns `Ok(false)`.

Read-only inspection after stopping identifies the concrete input conflict:
`fighter-continuous-aa` in the existing authentic aviation fixture supplies
fighter 1000 in station state with air.ammo=0. This old component scene proves
continuous AA independent of a sortie's weapon ammo. The new test then tries
to admit a new Attack mission from that same empty-ammo actor. The unchanged
validate_aircraft_mission in match_aviation.rs:390-391 refuses a mission when
ammo <= 0. The new test makes no ammo changes before that call, and its six held
AA calls consume no aircraft weapon ammo. This explains the observed refusal;
it is not evidence that the admission guard should be weakened or that the AA
runtime is defective.

The earlier mg/flak loops have been traversed before the fighter assertion,
but the function failed. They are not counted as separate test passes. No
assertion/input/fixture/runtime repair, ammo refill, reset or retry was made.
The missing CMB-65 witness is a mission-eligible actual fighter for the required
held-to-explicit-target transition. Any corrected scene or ordinary rearming
sequence needs a separate frozen checkpoint and Root's exact new grant.

## Scope, slot and provenance

Every Cargo job used the current guard, exclusive S2 target and a fresh disk
preflight. Kernel readings inside each systemd scope confirmed affinity
[4,5,6,7], memory.max=6442450944, memory.swap.max=0 and cpu.max="400000 100000".
Cargo build jobs=4 and test threads=1. Disk remained 56-57 GiB free, above the
20 GiB floor. No Source, Node, old cbb3/45d1 run, list/discovery command, full or
release suite, CI, sniper-child execution or public action occurred.

Cargo/nextest exited. A bounded nonblocking S2 flock acquisition succeeded and
was immediately released. Root received terminal/slot-free notification before
this report or dep-info was written. No process/cgroup scans, cleanup receipts,
sealing or all-input manifests were used. Root alone updates the ledger.

Compiler records bind both own-checkout test binaries to daaa4743's Rust tree.
The concise dep-info line is in depinfo.txt; compiler provenance is
`/home/judiazm/.t3/handoffs/ww2-native-cargo-wrappers/49dd342171fb57c98b8b6586/compiler-invocations.jsonl`.
Raw logs beside this report are fmt-e80.log, fmt-apply-e80.log, fmt-daaa.log,
core-cmb64-daaa.log and sim-five-daaa.log. Historical hosted-main passes retain
their original producing head. The first-five status patch remains private and
unapplied; all actual parity cells stay unchanged. The separate eb4e97b3 sniper
child is preserved unrun. No first-five whole-check completion is claimed.

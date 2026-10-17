# Final integration qualification, 2026-10-17

## Qualified and landed source

- Main integration: `4ff2e3daed31d10dfb27276a8c1916525f658af0`.
- Rust tree: `174d378c316cc98d136c41a1a577f37cab6c681b`.
- Frozen base: `61947c5b8a8438eb6ccbefa02e7d269c4e032472`.
- Final fetch and no-ff merge of origin/main reported already up to date. The
  tested head landed unchanged, without force-push, squash or rebase.
- GitHub confirmed all 43 included exact-head PRs merged; superseded #438 closed.
  Unrelated #199 and #201 remain untouched. Status sweeps are docs-only.
- Reference remains `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`; World admission
  stays closed. Unreal's FFI pin remains `9e618217` until Axel repins.

## Full local qualification

Nextest run `e3e8dfd2-4772-4056-9c6d-067ce83bc7af` ran all 2,448 enabled
workspace tests: 2,448 passed, zero failed, 16 existing ignored/opt-in checks,
1,383.592 seconds. Formatting, strict workspace Clippy, workspace doctests,
release static `ww2-ffi`, strict Linux C11 linking and the original formation
fixture also passed. The C fixture had 32 cases and 215 spots, zero differences.
HEAD, Rust tree and clean worktree were verified unchanged before and after.

Commands, from `rust/`, used the locked Rust 1.99 toolchain:

- `cargo test --locked --workspace --profile ci-test --no-run -j1`
- `cargo nextest run --locked --workspace --cargo-profile ci-test --profile ci --tool-config-file cloud:<local-resource-config> -j4`
- `cargo fmt --all --check`
- `cargo clippy --locked --workspace --all-targets -- -D warnings`
- `cargo test --locked --workspace --profile ci-test --doc`
- `cargo build --locked -p ww2-ffi --release -j1`
- Strict `cc -std=c11 -Wall -Wextra -Werror -pedantic` link of the existing
  `crates/ffi/examples/link_check.c`, then the unchanged formation fixture.

### Cloud resource accommodation

The Linux x86_64 cloud host had 10,206,508 KiB RAM and no swap. Four nextest
slots were retained, with all slots reserved for `match_support::`,
`engineering_consumers::support_selected::`, `movement_existing_checks::`,
`operative_tests::timing::`, and the `trusted_room_process` binary. This was a
local tool configuration, not a repository CI change. No retry, newly ignored
test, timeout, assertion or test filter was used to obtain the full pass.

An earlier same-head full diagnostic had 2,446 passes and two support-fixture
SIGKILLs. The unchanged fighter-cover and bombing tests passed alone at
2,397,388 and 2,397,492 KiB per-child peak RSS. Four decoded copies nearly
exhaust this host's memory. Kernel OOM records were unavailable, so SIGKILL is
not labeled a proven OOM. With the whole family reserved, both passed inside
the single green full run (1.872 and 1.765 seconds respectively).

The compressed shared fixture is byte-identical to frozen main: blob
`1598069711ac42550c1db6c3831a206cc3b31fc5`, 924,650 bytes. It is neither new nor
enlarged. The globally enabled preserve_order also changes the decoded map
representation; no main-versus-union allocation comparison is claimed.

The complete compressed IF-28 replay passed inside the full run in 121.449s.
Its isolated, unchanged optimized binary passed in 124.400s at **335,808 KiB**
peak RSS measured by per-child wait4. Binary SHA-256:
`5c6df895f6ab16e73f7f2fff28da3366f0ea6d07253a4732e3583c8a60bc398a`.
The earlier uncompressed isolated diagnostic was killed at 6,943,104 KiB.
An intermediate cumulative RUSAGE_CHILDREN measurement included prior child
peaks and is excluded. **Runtime memory footprint vs main: not established.**
The same test does not exist at the frozen main revision.

## Witness preservation and expectation repairs

[integrate-all-witness-map-20261017.json](integrate-all-witness-map-20261017.json)
records all 2,130 distinct test names from the independently qualified rooms,
logistics and save chains, mapped to actual optimized executable registrations
and full-run passes. Renamed/expanded witnesses are explicit. Legacy detached
comparison-owner tests remain labeled as such, not production-owner evidence.
The AI chain's historical 1,994-pass receipt is not represented as a separately
rerun chain here.

The [35-entry expectation inventory](integrate-all-expectation-map-20261017.json)
and repair changelog record changed fixtures, superseded behavior and exact
source heads. NET-92's deterministic comparator defect was fixed between runs:
complete restored policy equals an independent fresh-commander restore,
including private RNG, while exact serialized strategy and refreshed opening
memory remain checked. Its old pause-after-start expectation was replaced by
#423's exact lobby, host pause, then one start per role. It passed in the full
suite in 1.356s. No RNG field or ordered message check was removed.

The expanded operative lifecycle checks first-spawn and respawn reserve
publication and retired-body reserve cleanup with logistics disabled/enabled,
retaining all earlier lifecycle assertions. The combined-role production queue
witness covers ordinary AI, Assault, Horde defenders and the internal Horde
owner and asserts zero production inputs from the cfg(test) legacy owner.

The component construction split retains 1,917 complete runner turns and
separately checks atomic full-runner refusal plus exact construction-component
behavior for two deliberately invalid disabled-logistics reserve turns.
Null unassigned-build normalization matches the original full comparator at
[PR #411 head, lines 415-416](https://github.com/axelmanosalvasmd/ww2-rts-native/blob/ea9aa761636f5a3ed2a983232958633bed9e1e68/rust/crates/simtest/tests/construction_tick.rs#L415-L416).
The Classic-logistics admission refusal and synthetic missing-Horde-owner guard
remain separately checked.

JSON object order changes under preserve_order; public map arrays retain lexical
order. No production save/view/WebSocket digest depends on that object order.
World metadata digest keys sort explicitly by UTF-16; terrain hashes, custom
compiled startup-table golden bytes, password hashes and lobby entropy hashing
are unaffected. All fixture hashes and golden values remain unchanged.

## Ignored and opt-in inventory

No previously enabled test was newly ignored. Ten checks were already ignored
in the qualified chains; six new live-reference comparisons arrived with #436
at `6f4107054129e6404c39907dd61067549b6dfc74` as explicit opt-ins. The integration
repairs added no ignore attributes. No fresh live-reference result is claimed.

- `ww2-core aligned_replay::original_r1_aligned_61_checkpoints` (pre-existing ignored check).
- `ww2-core aligned_replay::startup_state::complete_r1_create_game_state` (pre-existing ignored check).
- `ww2-core movement::mov_audit::mov11_bridge_groups_finish_all_widths_and_order_variants_with_deferred_recovery` (pre-existing ignored check).
- `ww2-core movement::mov_audit::mov23_powered_vehicle_displacement_respects_reduced_step_budget` (pre-existing ignored check).
- `ww2-core pathfinding::mov_audit::mov30_bank_step_back_restores_walkable_first_leg_and_arrival` (pre-existing ignored check).
- `ww2-core pathfinding::mov_audit::mov33_score_only_heap_ties_and_road_heuristic_lower_bound` (pre-existing ignored check).
- `ww2-core pathfinding::mov_audit::mov35_smoothing_preserves_roads_wire_mines_and_bunker_clearance` (pre-existing ignored check).
- `ww2-core pathfinding::mov_audit::mov37_coarse_routes_update_from_history_and_survive_cache_eviction` (pre-existing ignored check).
- `ww2-server room_ai::seeded_tests::ai65_annihilation_outcomes_against_pinned_javascript` (live reference opt-in from #436; not freshly executed).
- `ww2-server room_ai::seeded_tests::ai65_assault_outcomes_against_pinned_javascript` (live reference opt-in from #436; not freshly executed).
- `ww2-server room_ai::seeded_tests::ai65_classic_outcomes_against_pinned_javascript` (live reference opt-in from #436; not freshly executed).
- `ww2-server room_ai::seeded_tests::ai65_conquest_outcomes_against_pinned_javascript` (live reference opt-in from #436; not freshly executed).
- `ww2-server room_ai::seeded_tests::ai65_horde_outcomes_against_pinned_javascript` (live reference opt-in from #436; not freshly executed).
- `ww2-server room_ai::seeded_tests::ai65_seed617_natural_outcome_against_pinned_javascript` (live reference opt-in from #436; not freshly executed).
- `ww2-simtest::it mov_world_distance::original_public_world_300_plus_40_plus_40_distance` (pre-existing ignored check).
- `ww2-simtest::it mov_world_distance::physical_world_march_calm_boundary_200_and_201` (pre-existing ignored check).

## GitHub Actions on the landed head

[Main run 39037257975](https://github.com/axelmanosalvasmd/ww2-rts-native/actions/runs/39037257975).

- Rust (ubuntu-24.04): **success**, job `117409490434`.
- Release library (ubuntu-24.04): **success**, job `117409490568`.
- Release library (windows-2025): **success**, job `117409490579`.
- Lint (ubuntu-24.04): **success**, job `117409490594`.
- Release library (macos-15): **success**, job `117409490605`.
- Rust (windows-2025): **success**, job `117409490610`.
- Lint (windows-2025): **success**, job `117409490637`.
- Lint (macos-15): **success**, job `117409490652`.
- Rust (macos-15): **success**, job `117409490670`.

Each platform ran all 2,448 enabled tests with 16 existing ignored/opt-in checks:
Linux 1,069.804s, macOS 1,527.580s and Windows 2,168.723s for nextest.
All nine jobs completed successfully on the same landed integration head. No
platform-specific source changes or job reruns were needed.

The chronological merge receipt's provisional/pending statements describe earlier
stages and are superseded by this final qualification, without changing Rust.

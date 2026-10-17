# MODE-STARTUP-2

This stream starts from MODE-STARTUP PR #395 at
`fbd7ecbf8496452c917a9446c016a0d1b02217e2`, with prototype
`e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. The parent owns its MODE-06,
BASE-02 and BASE-52 non-World claims. Its unchanged #373 serialization fix
remains inherited. This stream does not change parity status tables.

## Row evidence

| Row | Test | Scope |
| --- | --- | --- |
| MODE-01 | `mode01_non_world_mode_rows_match_the_prototype_public_fields` | All six non-World constructors and exact public mode component rows; native Default shares Conquest's no-mode result |
| MODE-01 | `mode01_horde_row_omits_unset_profile_boss_and_active_next_profile` | Initial absent fields, active profile/boss, active hidden next profile and boss null after wave zero |
| MODE-01 | `mode01_mode_rows_round_timers_and_refuse_invalid_values_or_mode_pairs` | Timer clamp/ceil, invalid values and mismatched typed modes |
| MODE-01 | `mode01_lobby_whitelists_seven_kinds_and_rejects_invalid_or_foreign_selection` | Existing seven-name server whitelist, invalid input and host authority; World selection does not open a World match |
| MODE-02 | `mode02_ineligible_horde_and_tutorial_start_conquest_and_deliver_every_seat` | Missing Horde defend and six-player Horde/Tutorial fallback, unchanged roster, six ordinary ticks and every seat's complete recipient snapshot |
| MODE-02 | `mode02_fallback_preserves_options_except_requested_mode_feature_gates` | Caller settings survive except the original Horde/Tutorial logistics gate and Tutorial tech gate |
| MODE-02 | `mode02_eligible_horde_and_tutorial_keep_shared_enemy_startup` | One and five eligible humans retain the shared enemy and original feature restrictions |
| MODE-02 | `mode02_fallback_retains_option_and_mode_guards` | Invalid mode, roster, teams, weather and eligible Horde Endless remain refused |
| MODE-02 | `mode02_short_spawn_maps_do_not_open_world_startup` | Ordinary typed maps reject fewer than two spawns; the typed one-spawn World route remains refused |
| MODE-04, MAP-06 | `mode04_map06_non_world_capacity_and_start_readiness_follow_authored_spawns` | All 33 maps across six non-World modes, counts 0 through 7, both Assault teams and loaded spawn filtering |
| MODE-04 | `mode04_trusted_start_checks_capacity_and_assault_sides_before_r1_policy` | Actual trusted Start consumes capacity/team readiness before its separate R1 policy, without mutation on refusal |
| MAP-06 | `map06_ordinary_validation_requires_two_universal_open_spawns_and_at_most_six` | Ordinary 2-6 limit, two universal spawns, boolean Assault flags, open terrain and the exact universal-spawn error |
| MAP-06 | `map06_lobby_map_selection_reaches_conquest_layout_and_every_recipient` | Bastogne and Stalingrad Factory lobby selection reaches canonical Conquest layout; Assault-only spawns are excluded from Conquest and used by the Assault constructor; both Start envelopes and six ticks deliver to every seat without a fault |

The MODE-01 unit tests are registered through
`match_construction::native_intent::mode_startup` in
`src/base_native_intent_tests.rs`. MODE-02 is registered in
`tests/it/main.rs` because the core has `autotests = false`. Server tests are
registered as `lobby::mode_startup2_tests` in `src/lobby.rs`.

## Prototype readback and limits

`shared/sim.js:587-590,626` owns spawn validation/filtering.
`server.js:332,336,659,682` owns Assault readiness, capacity, mode whitelist
and Start's necessary conditions. Core fallback is in
`shared/sim.js:686-689`; logistics still depends on the requested mode at
`:729`, and Tutorial remains excluded from tech at `:799` even on fallback.
The projection contract and mode-specific setup references are in
`docs/contracts/mode-startup.md`.

MODE-02 and MAP-06 have complete non-World evidence in this stream. MODE-01
and MODE-04 have the component and predicate evidence listed above; their full
rows remain held. Complete non-Conquest room startup and recipient composition
remain guarded, and Horde's ordinary full tick still needs integration. These
are non-World delivery gaps, separate from the World deferral. A public mode row
alone does not establish whole-frame delivery or hosted mode support.

World-only exclusions are the generated World constructor and public mode row
(MODE-01), six-seat World match startup (MODE-04), and World 1-6-spawn map-to-room
startup (MAP-06). Tutorial's fewer-than-two-spawns case in MODE-02 cannot reach
an ordinary `AuthoritativeMap`; its one-spawn typed map is World-only and stays
closed. World map validation components are not changed or promoted here.

Conquest has no `g.mode` in the prototype. Its `modeRow` result is undefined,
which JSON serialization omits. The native component represents that result with
null internally while actual Conquest snapshots keep omitting the field. No new
state owner, command, wire meaning, privacy rule, FFI or header is introduced.

The coordination requests to preserve the prototype shapes and keep World
closed are addressed using the confirmed owner decision in issue #5 comment
6218552386. Validation must include the inherited #373 serialization repair and
current main before publication. Hermes' exact-head runtime acknowledgement,
hosted checks and merging remain the orchestrator's responsibility. This stream
posts no issue comment.

## Validation

The PR body records the producing head and actual validation results. Focused
checks use Rust 1.99.0 on the Vostro, a shared S1/S2/S3 lock with its matching
target directory, the `ci-test` profile, four build jobs and four test workers,
MemoryMax=6G and MemorySwapMax=0. Formatting and strict workspace Clippy use the
same slot protocol. No Source recorder or fixture capture is part of this stream.

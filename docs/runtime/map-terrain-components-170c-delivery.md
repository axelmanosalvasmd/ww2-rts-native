# MAP terrain component row evidence

The complete selected Rust checks for MAP-25, MAP-31, MAP-47, MAP-48 and
MAP-50 pass at producing head `170cfe002af0e9f787282a8ed0c8d83ff4cba8de`,
Rust tree `e3d7a7e39b916154fe8a66c4dee40318f169aeba`. This documentation child
changes only the five Rust/Status pairs and records the evidence. Every
Acceptance cell remains literal, including the other 135 MAP rows.

| Row | Unchanged selected Acceptance | Actual consumer and result |
| --- | --- | --- |
| MAP-25 | per-char groundMul table (MAP-47) | Production World mutation and ground_mul over all 19 characters, wear 0/0.5/1, wet 0/1, levels 0/-1 and both classes: 456 exact-bit comparisons PASS. Open ground has LAND only. |
| MAP-31 | port the check ("you can see across water") | Original river/bridge/ford rows, MOVE-only water flags and full production LOS ray (5,25) to (35,25): PASS. |
| MAP-47 | groundMul for every char at wear 0, 0.5, 1 and wet 0, 1 equals JS | Full 456-value ground matrix PASS. Nine additional production speed_mul footprint cases PASS. |
| MAP-48 | port "Slope" | Both complete original two-second Slope outcomes PASS through 160 continuous production movement calls. Six additional exact returned-speed samples PASS. |
| MAP-50 | port the three checks | All eight clauses from the complete original LOS, ridge and Depressions checks PASS, including the two-level cliff path refusal. |

The native consumers are in
[map_terrain_components.rs](../../rust/crates/simtest/tests/map_terrain_components.rs).
They use production terrain, World mutation, unit initialization, pathfinding,
movement and LOS providers. No expected state initializes native observations.

The nine footprint cases retain ordered centre/east/west/south/north samples,
wall and water exclusion, empty fallback, naval bypass, off-map exclusion,
the 0.6-radius offset, infantry with three repeated centre-cell samples, and
MOVE-only filtering over worn VBLOCK rubble. Zero wear after M/F mutation
is an authored input matching the pure Source helper. The rubble case
authors wear 1. No provider or expected output is repaired.

The water scene authors one neutral objective at cell (2,2), VP 0 and MP 0,
on known open ground before ordinary admission. The original blank point
at (10,10) overlaps river water. The original river, bridge, ford, water
flags and LOS coordinates stay intact, and the shipping admission guard
still rejects water points. The pure Source sight query does not read
objective metadata. The failed parent `92c18323` remains an admission FAIL
before World construction or LOS; its actual LOS result is UNREACHED.

Slope retains the original tanks, coordinates, orientations, goals, real
paths and 40 continuous steps per scene. The uphill x deficit exceeds 0.5
and downhill x difference stays below 0.3. Returned-speed samples cover
infantry and vehicles at grade 0.625, grade 1.25 clamped to 1, and grade
-1.25 clamped to 0. No Source pose, random reset or snapshot drives the journey.

MAP-50 retains both original building/wall LOS clauses, three ridge clauses
and three Depressions clauses. The last clause requires the two-level
cliff path to be empty. The separate MAP-49 cliff selection remains separate.

The unchanged [Source checker](../../harness/map-terrain-components-check.mjs)
passed once at `a99c2b385e4f14db77a5339dc308f5736b964153`: all 456 ground
values, nine footprint cases, four complete original check bodies and the
water sight assertion. It exposes the pinned private helpers without
rewriting their bodies and evaluates the original helpers and bodies intact.
Its bytes are unchanged at the native producing head. No Source or Node
process runs during this new qualification or documentation step.

Actual qualification

The new Root S1 grant executes exactly three jobs, each once from the assigned
physical checkout root with explicit `+1.99.0` and `--manifest-path rust/Cargo.toml`:

1. `cargo +1.99.0 fmt --manifest-path rust/Cargo.toml --all -- --check`: exit 0.
2. Locked ci-test nextest, ww2-simtest test binary `it`, exact seven-function
   selection below, build jobs 4, test threads 1 and fail-fast: seven unique
   PASS, zero FAIL and 231 skipped.
3. `cargo +1.99.0 clippy --manifest-path rust/Cargo.toml --locked --profile ci-test -p ww2-core -p ww2-simtest --all-targets -- -D warnings`: exit 0, conditional on all seven passes.

The exact selected functions are:

- `map_terrain_components::map25_map47_every_character_wear_and_wetness_reach_ground_mul`
- `map_terrain_components::map47_five_samples_use_radius_passability_and_empty_fallback`
- `map_terrain_components::map31_original_water_sight_assertion_uses_full_los`
- `map_terrain_components::map50_all_original_los_ridge_and_depression_assertions`
- `map_terrain_components::map48_complete_original_slope_journey_uses_production_movement`
- `vision::rows::cmb128_line_of_sight_matches_original_source`
- `movement::movement_matches_the_reference_call_by_call`

The retained vision gate compares all 18 authentic LOS rays. The retained
movement gate checks every original vehicle profile and the complete replay
of 3 ticks, 636 calls and 1,362 observations. Neither supporting gate promotes
a CMB or MOV row. The original Source bodies remain pinned to e4e579a.

Every job verifies actual CPU affinity 0-3, memory.max 6442450944,
memory.swap.max 0, cpu.max 400000/100000, jobs 4, threads 1, the current
Root guard, external S1 target and inherited exclusive flock. Each records
free -g and disk above 20GiB. Guard-checked available bytes are
100036075520, 100034265088 and 99728048128 for fmt, native and Clippy.
All processes are terminal; each stage immediately confirms release of the
same inherited S1 flock. Root receives the final release notification before
the result report. Host swap usage is distinct from this scope's swap0 limit.

Compiler provenance binds the actual binary to this checkout and Rust tree.
Binary `it-a48c46bd1cc72bf7` SHA256:
`f5edb9df162edeb126c29adf5e2cdbf660b3e5a334b53289a4517c4fd3cb7568`.
The selection file SHA256 is
`e2e771301e791ca6c2fc4dc5ff31d738a4542193159279260ff935160940c272`.
The private raw command, resource, log, compiler and terminal receipts are
retained in `/tmp/map-native-170cfe-52b4776c`. The first outer taskset launch
failed before the runner, Cargo or any slot lock. Its original log is
preserved; Root approved only the argument-separation correction. No project
job or product predicate was retried.

Scope and preservation

All incoming fixtures, registries, providers, admission guards, FFI, wire,
contracts and reference remain exact. Both held World test bodies and their
fixture retain their explicit ignore reasons. Six incoming MOV ported rows
remain exact. The original failed parent, old native qualification at 7f7
and historical Source at a99 retain their producing-head attribution.
The docs child makes no additional execution claim.

Open-ground buildability/burning, other bridge destruction/path/broadcast
clauses, the full separate MAP-49 selection, editor history, World and
recipient delivery, whole-match outcomes and Unreal acceptance remain outside
these selected checks. Root owns final incoming-main preservation, fresh
exact-child review, applicable hosted gates, owner readback and publication.

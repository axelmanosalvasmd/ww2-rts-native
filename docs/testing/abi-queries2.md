# ABI-QUERIES-2

This stream starts at #418 `1ce78e1231bf0bf4c1b825d447eb95e10251f64b` and stacks on #418 (#389). Reference remains `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. Owner message `julio-373-merged-decisions-20261016` authorizes additive queries and retains signature review before Unreal adoption. The later owner message `julio-merge-without-ack-20261017` ([6225924617](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6225924617)) removes exact-head Hermes acknowledgement as a merge gate; the orchestrator owns CI and integration, and this stream still stops after publication. Existing C declarations, scalar major 2, minor 1, layouts and error conventions remain unchanged; no function is added.

## Literal row mapping

IF-12, additional ordinary fairness and detached-preview witnesses through the public C entry points:

- `fairness_finishes_real_ai_matches_and_credits_actual_shuffled_starts`: completed matches, mirrored authored bias, winner/team/VP consistency and exact native replay.
- `fairness_moves_beyond_home_capture_range_before_winning`: no VP at tick 80, followed by a normal completed match that requires AI movement and repeats deterministically.
- `sixty_seeded_c_matches_reproduce_the_editor_unfair_verdict_by_authored_spawn`: seeds 7 through 66, 60 completed matches, actual shuffled-start attribution and the editor's percentage/verdict calculation. Expected source outcome is 60 wins at authored start 0, zero timeouts, percentages `[100,0]`, verdict `unfair`.
- `every_committed_map_builds_a_detached_conquest_preview_through_c`: every shipped map constructs detached ordinary setup with complete dimensions, terrain rows, players and units. Large results use the C capacity/retry path.
- `preview_exposes_classic_resources_and_horde_roles`: ready HQs, both resource marker kinds, shared defenders, internal Horde seat, offset gates, bunker and break clock.
- `preview_assault_and_annihilation_preserve_roles_forts_and_point_filtering`: defender starts/teams, forts, authored Assault clock, bunker counts and omitted VP-only points.
- `horde_preview_without_defenders_keeps_the_prototype_conquest_fallback`: unavailable Horde selection retains ordinary setup instead of throwing or admitting logistics.

The prior twelve authoring witnesses, twelve placement witnesses and ABI/header/error checks from #418 remain registered. IF-12 is held until the complete non-World startup and match surface is supported. Explicit authored scenarios, active non-Conquest fairness and logistics are still refused. World runtime is deferred under the owner decision. No parity status file is edited and no complete-row claim is made from these bounded witnesses.

## Prototype comparison and execution

Direct Node v26.11.1/x64 execution on Vostro runs the unchanged pinned `playMatch`, including its full human AI schedule, using the test documents and seeds. No Source recorder, captured behavior fixture or provenance bundle is created. The three near-home `vp:5,mp:0` points yield a source win at tick 1760; the remote-point document at seed 77 yields a source win at tick 2032. Both credit the favored authored spawn. Finish clocks are diagnostics, not exact native assertions.

The first Rust build diagnosed a missing QueueError conversion. The next focused run reached real matches and exposed the base's ShotsAndFlights refusal, and a stale gate assertion. Those failed runs do not qualify the final tree. Projectile/event delivery comes from #369, whose inherited claims remain with that PR. The PR records final committed-head checks; the complete merge record follows.

## Integration record

- Authoring implementation commit: `b184d7c3`.
- #369 source head: `161bbe3880f0ac06187c64bfe3ff2dab0856841c`.
- Merge commit: `06879f503fadefd8b1b4c119033eb95d80fcba4a`.
- The actual merge tree is `8bc5ea51a13ee2f06b2a257bfa2f7e82939ec361`, exactly matching `git merge-tree --write-tree b184d7c3 161bbe38`.
- Both automatic merges (`DESIGN.md` and `match_tutorial/tests.rs`) retained their parent changes. There were no conflicts, manual resolutions or hand-edited merge hunks. Incoming registrations and runtime changes remain inherited #369 work.

- Main `9e5bf12439cead3ede90fdf7e62b73fcb7ed1881` merged at `fc8ee37d7b2b478cbb5aa84edfe243304b86ab31`.
- This merge had exactly two conflict hunks. In `commander_tactics.rs`, retain the entire existing ground-army/front/medic-rule block and append main's `world_free` initialization before the shared unit loop. This preserves medic regrouping and garrison shelling from #349 and World free-unit collection from #401. In `core/src/lib.rs`, retain all four exports: `authoring_queries`, `recipient_placement`, `commander_field_policy`, `commander_world_policy`.
- The resolved main-merge tree `67c6451bf1e6600b442fb2ce69d12ecace41ae0f` differs from the automatic conflict tree only in those two files, solely by removing markers and retaining both alternatives. No other manual merge hunk exists. Chores, field-policy, field-assault and field-review test registrations are retained.
- Main then advanced to `47ecd597`. Merge `1bdf5b171476ffb8a59f5bb3d152783ef66c108e` has actual tree `a5b69f63c944bca1823afb8de991ad1c428fd943`, exactly equal to the automatic merge-tree result. It has no conflicts or manual hunks.
- Independent source review accepted the authoring increment and both main integrations at `1bdf5b17`. That review ran no tests and does not substitute for runtime qualification. Hermes' post-merge review remains welcome under the updated owner process.

## Required qualification

Run on Vostro from `rust/`, with Rust 1.99.0, a shared S1/S2/S3 lock and matching target, `MemoryMax=6G`, `MemorySwapMax=0` and `-j4`: formatting check; all `ww2-ffi` tests (including generated header, unchanged layouts/signatures, buffer errors and C-entry fairness); relevant commander chore/field tests for the two-block merge; strict locked workspace/all-target Clippy; release static library build and strict C11 link/replay. No full workspace test run or Unreal adoption is claimed. The PR gives the actual producing head and results.

The first post-main qualification stopped at formatting: rustfmt required the four retained module exports in alphabetical order. The follow-up changes only that declaration order; all four exports and every test registration remain present. No test or Clippy result is claimed from that stopped attempt.

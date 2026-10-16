# MISC-RUNTIME-2

Baseline: main `ead962f6`. Prototype: `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.

CMB-02: `combat_enemy_faction::cmb02_horde_and_tutorial_choose_first_unpicked_faction_or_germany` calls both canonical constructors for ten faction rosters each. Explicit expected factions follow prototype `shared/sim.js:690-693`: Germany, USSR, USA priority, then Germany when all three are picked. Cases include UK, repeated factions and five human seats. The test also checks that human identities survive and the appended enemy belongs to the opposing team. Existing runtime code already implements the behavior. This is a test-only PR.

Dropped rows:

- MOV-01: waits on #302; complete phase composition also needs reserve and convoy ticks, refused by `match_tick.rs`.
- MOV-03: waits on #302; the full unit sequence needs logistics reserve and driving-fuel integration.
- MOV-04: waits on #302; enabled logistics is refused before the tick, so a complete reserve speed-cap test is unavailable.
- MOV-42: waits on #302; complete World route legs and owner movement/plan delivery remain unproved. World admission and recipient changes are outside this batch.
- CMB-59: beam firing is explicitly refused in `match_firing.rs`; Horde is excluded from ordinary ground ticking in `match_actor.rs`. The terrain-deduplication component alone does not prove the breath row. #299 also drops this row.
- BASE-02: Classic and Annihilation startup profiles are absent from `match_runtime.rs`. Existing Conquest, Assault, Horde and World wallet slices cannot prove every named mode; World admission stays guarded.
- AI-43: `commander_tactics.rs` skips aircraft, delivered facts lack base readiness, attacker commands are refused in `match_aircraft_commands.rs`, and active aviation is refused by recipient composition. This needs aviation and recipient work beyond the batch. Neither #342 nor #343 proves this row.

No differs candidates. No parity status, runtime, reference, contract or admission changes.

The focused `ww2-core` nextest run on the Vostro passed both selections: the CMB-02 behavior test and `every_test_file_is_compiled` (781 tests skipped). It used the `ci-test` profile, `-j4`, S2's target directory, a 6 GiB memory cap and zero swap. The slot was released after completion. An independent source reviewer accepted the test's coverage against the pinned prototype.

After refreshing `origin/main` (still `ead962f6`), `cargo fmt --all --check` and `cargo clippy --locked --workspace --all-targets -j4 -- -D warnings` passed under the same S2 limits. No workspace tests ran. Hosted checks and merge remain with the orchestrator.

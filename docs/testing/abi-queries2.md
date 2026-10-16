# ABI-QUERIES-2

This stream starts at #418 `1ce78e1231bf0bf4c1b825d447eb95e10251f64b` and stacks on #418 (#389). Reference remains `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. Owner message `julio-373-merged-decisions-20261016` authorizes additive queries and retains exact-head runtime review before Unreal adoption. Existing C declarations, scalar major 2, minor 1, layouts and error conventions remain unchanged; no function is added.

## Literal row mapping

IF-12, additional ordinary fairness and detached-preview witnesses through the public C entry points:

- `fairness_finishes_real_ai_matches_and_credits_actual_shuffled_starts`: completed matches, mirrored authored bias, winner/team/VP consistency and exact native replay.
- `fairness_moves_beyond_home_capture_range_before_winning`: no VP at tick 80, followed by a normal completed match that requires AI movement and repeats deterministically.
- `sixty_seeded_c_matches_reproduce_the_editor_unfair_verdict_by_authored_spawn`: seeds 7 through 66, 60 completed matches, actual shuffled-start attribution and the editor's percentage/verdict calculation. Expected source outcome is 60 wins at authored start 0, zero timeouts, percentages `[100,0]`, verdict `unfair`.
- `preview_exposes_classic_resources_and_horde_roles`: ready HQs, both resource marker kinds, shared defenders, internal Horde seat, offset gates, bunker and break clock.
- `preview_assault_and_annihilation_preserve_roles_forts_and_point_filtering`: defender starts/teams, forts, authored Assault clock, bunker counts and omitted VP-only points.
- `horde_preview_without_defenders_keeps_the_prototype_conquest_fallback`: unavailable Horde selection retains ordinary setup instead of throwing or admitting logistics.

The prior twelve authoring witnesses, twelve placement witnesses and ABI/header/error checks from #418 remain registered. IF-12 is held until the complete non-World startup and match surface is supported. Explicit authored scenarios, active non-Conquest fairness and logistics are still refused. World runtime is deferred under the owner decision. No parity status file is edited and no complete-row claim is made from these bounded witnesses.

## Prototype comparison and execution

Direct Node v26.11.1/x64 execution on Vostro runs the unchanged pinned `playMatch`, including its full human AI schedule, using the test documents and seeds. No Source recorder, captured behavior fixture or provenance bundle is created. The three near-home `vp:5,mp:0` points yield a source win at tick 1760; the remote-point document at seed 77 yields a source win at tick 2032. Both credit the favored authored spawn. Finish clocks are diagnostics, not exact native assertions.

The first Rust build diagnosed a missing QueueError conversion. The next focused run reached real matches and exposed the base's ShotsAndFlights refusal, and a stale gate assertion. Those failed runs do not qualify the final tree. Projectile/event delivery comes from #369, whose inherited claims remain with that PR. Final committed-head checks and the complete merge record are recorded below before publication.
